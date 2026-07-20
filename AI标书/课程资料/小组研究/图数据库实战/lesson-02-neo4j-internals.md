# Day 2：Neo4j 内核深挖

> Day 1 你写了 Cypher。今天你钻到 Neo4j 存储引擎里——用 hexdump 看磁盘文件，用手写查询计划解释器理解为什么图遍历是 O(1) per hop，用 Page Cache 实验观察内存对性能的影响。

---

## 学习目标

1. 理解 Neo4j 四种存储文件的固定大小记录布局，能 hexdump 解析节点和关系
2. 解释免索引邻接（Index-Free Adjacency）的物理实现——为什么指针追踪比 JOIN 快
3. 理解 Page Cache 的 LRU-K 驱逐策略和预热机制
4. 手写简化版 Cypher 查询计划解释器
5. 跑 Page Cache 命中率实验，分析冷/热查询性能差异

---

## 核心概念

### 1. 存储引擎 — 磁盘上的图

#### 四个核心文件

Neo4j 社区版的数据目录（`/data/databases/neo4j/`）下有四个核心存储文件，每个记录固定大小：

```
neostore.nodestore.db          — 节点存储   15 bytes/record
neostore.relationshipstore.db  — 关系存储   34 bytes/record
neostore.propertystore.db      — 属性存储   可变（键+值，链表）
neostore.relationshipgroupstore.db — 关系组存储（密集节点优化）
```

**为什么固定大小记录？**

因为 Neo4j 通过记录 ID 直接定位——给定 Node ID=42，它的物理位置在 `nodestore.db` 的偏移 `42 × 15 = 630`。这不需要任何一个索引查找——将 ID 乘以记录大小，`fseek` 到那个偏移，读 15 字节。这就是 O(1)。

#### 节点记录的 15 字节

```
偏移  大小  字段
0     1     inUse(1 bit) + flags(7 bits)
1     4     firstRelId — 该节点的第一条关系在 relationshipstore 中的 ID
5     4     firstPropId — 该节点的第一个属性在 propertystore 中的 ID  
9     4     labelStore — 标签位图或指向标签存储的指针
13    1     reserved

// flags 的含义：
// bit 0: 是否使用（1=在用，0=已删除可回收）
// bit 1: 是否是密集节点（dense node — 有大量关系的节点）
// bits 2-7: 保留

// 如果一个节点的度 > 阈值（默认 50），Neo4j 标记为"密集节点"
// 密集节点的关系不存为单链表，而是按类型分组存在 relationshipgroupstore 中
// 这避免了从 10000 条关系中筛选特定类型的 O(degree) 扫描
```

#### 关系记录的 34 字节 — 图遍历的核心

```
偏移  大小  字段
0     1     inUse + flags
1     4     firstNodeId — 起始节点的 ID
5     4     secondNodeId — 终止节点的 ID
9     4     relType — 关系类型的整数编码
13    4     firstPrevRelId — 起始节点的"上一条关系"指针
17    4     firstNextRelId — 起始节点的"下一条关系"指针
21    4     secondPrevRelId — 终止节点的"上一条关系"指针
25    4     secondNextRelId — 终止节点的"下一条关系"指针
29    4     firstPropId — 属性链指针
33    1     reserved
```

**双链表结构**是关键：

```
Node(id=42).firstRelId = 103

rel_103: firstPrevRelId=-1, firstNextRelId=215  → Node(id=42) 的关系链表头
rel_215: firstPrevRelId=103, firstNextRelId=387 → 中间关系
rel_387: firstPrevRelId=215, firstNextRelId=-1  → Node(id=42) 的关系链表尾
```

遍历 Node(id=42) 的所有关系的物理过程：

```
1. nodestore.db[42*15..42*15+4] → firstRelId = 103
2. fseek relationshipstore.db 到 103*34
3. 读 34 字节 → 解析出起止节点、关系类型、下一个关系的指针
4. 如果 firstNextRelId != -1 → fseek → 读下一条
5. 重复直到 firstNextRelId == -1

每次迭代 = 1 次 fseek + 34 字节 read = 1 次磁盘 IO（或 Page Cache hit）= O(1) per hop
```

**对比 MySQL 的等值 JOIN**：

```
SELECT * FROM articles a JOIN cases c ON a.id = c.article_id WHERE a.law_id = 42;

MySQL 执行：
1. B-Tree 索引查找 articles.law_id = 42 → O(log N) IO
2. 对于找到的每条 article：
   B-Tree 索引查找 cases.article_id = a.id → O(log M) IO per article
总 IO = O(log N) + K × O(log M)

Neo4j 执行：
1. B-Tree 索引查找 Law(id=42) → O(log N) IO（这是唯一的索引查找！）
2. 指针追踪：Law → HAS_ARTICLE → Article → CITED_IN → Case
   = K × O(1) 指针解引用（可能在 Page Cache 中命中）
总 IO = O(log N) + K × 0（Page Cache 命中）或 O(log N) + K × O(1)（磁盘）
```

**所以图数据库比关系型快 1000 倍不是因为 Cypher 比 SQL 好——是因为指针链表替代了重复的 B-Tree 查找。**

#### 密集节点的优化

当一个节点有 > 50 条关系时，Neo4j 标记它为"密集节点"（dense node）。对于密集节点，关系不再存为单链表，而是**按类型分组**存在 `relationshipgroupstore.db` 中：

```
// 非密集节点：
Node.firstRelId → [Rel_A, Rel_B, Rel_A, Rel_C, Rel_A, ...]  // 混合所有类型

// 密集节点：
Node.firstRelId → Group_A → [Rel_A1, Rel_A2, ...]  // 特定类型的关系
                  Group_B → [Rel_B1, Rel_B2, ...]
                  Group_C → [Rel_C1, ...]

// 当遍历 MATCH (n)-[:CITED_BY]->() 时，不需要扫描所有关系
// 直接跳到 :CITED_BY 的关系组 → 只遍历相关类型
// degree=10000, 但只找 10 条 :CITED_BY → 只需要 10 次追踪而非 10000
```

---

### 2. Page Cache — 内存在图数据库中的角色

#### 架构

```
┌──────────────────────────────────────┐
│          Neo4j 进程                   │
│  ┌──────────────────────────────┐    │
│  │    Page Cache (堆外内存)       │    │
│  │    dbms.memory.pagecache.size │    │
│  │    默认 512MB                  │    │
│  └──────────┬───────────────────┘    │
│             │ cache miss              │
│             ▼                         │
│  ┌──────────────────────────────┐    │
│  │  操作系统 Page Cache (FS)     │    │
│  └──────────┬───────────────────┘    │
│             │                         │
│             ▼                         │
│  ┌──────────────────────────────┐    │
│  │      磁盘 (SSD/HDD)          │    │
│  └──────────────────────────────┘    │
└──────────────────────────────────────┘
```

Neo4j 维护自己的 Page Cache（堆外内存），独立于 OS 的 Page Cache。原因：OS Page Cache 是通用的 LRU 或 LFU，Neo4j 可以用图遍历的模式信息做更智能的预热（例如：加载一个节点时，预加载它的关系链表所在页）。

#### LRU-K 驱逐策略

普通 LRU：只看"最近访问时间"。一次全表扫描会把所有热点冲掉。

LRU-K（K=2）：不仅要看最近一次访问，还要看倒数第 K 次访问时间。如果一个页被访问了 1 次（全表扫描），`访问间隔 = ∞`（没有第 2 次访问）→ 低优先级 → 不会被保留在 cache 中。如果一个页被频繁访问（1ms 前、5ms 前、10ms 前），`访问间隔短` → 高优先级 → 保留。

#### Page Cache 命中率实验

你要用实验回答三个问题：

1. **数据量 vs 命中率**：当图大小（节点数）从 1K 增长到 1M 时，Page Cache 512M 的命中率怎么降？
2. **冷 vs 热**：Neo4j 重启后，第一次查询（冷）和第六次查询（热）的延迟差多少？
3. **预热策略**：手动跑一次全图扫描（`MATCH (n) RETURN n`）后，后续查询的延迟变化

实验设计：

```
1. 创建 100K 节点的图（随机连边，平均度 5-20）
2. 冷查询：刚重启 Neo4j → 跑 50 条随机 2-hop 查询 → 记录每次 P50/P99
3. 热查询：同一批查询跑第 6 遍 → 记录 P50/P99（此时大部分数据在 Page Cache 中）
4. 调整 pagecache.size：128M / 512M / 2G → 重复步骤 2-3
5. 画图：X 轴=Page Cache Size / 数据总大小，Y 轴=P99 延迟
```

---

### 3. 查询编译与执行

#### 完整编译 Pipeline

```
Cypher String
  │
  ▼
Lexer + Parser (ANTLR 生成的解析器)
  │  产出：AST
  ▼
Semantic Analysis
  │  检查：变量作用域、类型匹配（不能 MATCH (n) WHERE n.name + 42）
  ▼
Normalization (逻辑优化)
  │  规则：谓词下推、常量折叠、重复模式消除
  ▼
Logical Plan (关系代数算子树)
  │  算子：NodeIndexSeek / Expand / Filter / Projection / Sort / Limit
  ▼
Cost-Based Optimizer
  │  - 基数估算：DBMS 采样统计（每个 Label 的节点数、索引的选择性）
  │  - 算子重排：先做选择性高的过滤
  ▼
Physical Plan
  │  具体策略：IndexSeekByRange / ExpandInto / ExpandAll
  ▼
Runtime Execution (Pipeline / Volcano 迭代模型)
```

#### 关键算子

```
AllNodesScan:          扫描 nodestore.db 的每一个记录 → 最慢，避免
NodeByLabelScan:       扫描特定 Label 的所有节点 → 利用 Label 索引
NodeIndexSeek:         B-Tree 索引精准查找 → 最快，用于起点
NodeIndexRangeSeek:    B-Tree 索引范围查找
ExpandAll:             从一个节点出发，遍历所有关系 → 不知道终点
ExpandInto:            已知起止节点，验证关系是否存在 → 双向遍历，比单向快 2 倍
OptionalExpandAll:     LEFT OUTER JOIN 的图版本
```

#### 代价优化——基数估算为何重要

```
查询：MATCH (l:Law)-[:HAS_ARTICLE]->(a:Article)<-[:CITED_IN]-(c:Case)
       WHERE l.level = '法律' AND c.date > '2024-01-01'

Cost Model 决策：
  方案 A: 从 Law 开始 → 找到 level='法律' 的法规 → Expand HAS_ARTICLE →
          Expand CITED_IN → Filter date
  方案 B: 从 Case 开始 → Filter date → Expand CITED_IN → Expand HAS_ARTICLE →
          Filter Law.level

哪个更快取决于：
  - (Law, level='法律') 的选择性：假设 300 部法律，5 部是 level='法律' → 高选择性 → 好起点
  - (Case, date > '2024-01-01') 的选择性：假设 200 案例，50 个是 2024 年后 → 中等选择性
  
Neo4j 用基数估算（Cardinality Estimation）来比较：
  estimated_rows(A) = |Law_level_legal| × avg_articles_per_law × avg_cases_per_article
  estimated_rows(B) = |Case_2024| × 1 × avg_articles_per_case
  = 5 × 20 × 3 = 300     vs     50 × 1 × 1 = 50

代价优化器选方案 B（从 Case 开始）。
```

但基数估算可能出错——如果统计信息过时、数据倾斜（平均值不能代表实际分布）。`EXPLAIN` 看到的 `EstimatedRows` 和 `PROFILE` 看到的 `Rows` 差距大 → 执行计划选错了。这就是为什么生产环境定期跑 `CALL db.stats.collect()` 更新统计信息。

---

## 动手

### 任务 1：hexdump 解析存储文件

用 Rust 写一个工具，读取 Neo4j 数据目录下的 `nodestore.db` 和 `relationshipstore.db`，解析并打印前 10 个节点和前 10 条关系：

```rust
// 解析一个节点记录（15 bytes）
fn parse_node_record(bytes: &[u8; 15]) -> NodeRecord {
    NodeRecord {
        in_use: bytes[0] & 1 == 1,
        is_dense: bytes[0] & 2 == 2,
        first_rel_id: i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]),
        first_prop_id: i32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]),
    }
}

// 解析一条关系记录（34 bytes）
fn parse_rel_record(bytes: &[u8; 34]) -> RelRecord {
    RelRecord {
        first_node_id: i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]),
        second_node_id: i32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]),
        rel_type: i32::from_be_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]),
        first_prev_rel_id: i32::from_be_bytes([bytes[13], bytes[14], bytes[15], bytes[16]]),
        first_next_rel_id: i32::from_be_bytes([bytes[17], bytes[18], bytes[19], bytes[20]]),
        second_prev_rel_id: i32::from_be_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]),
        second_next_rel_id: i32::from_be_bytes([bytes[25], bytes[26], bytes[27], bytes[28]]),
        first_prop_id: i32::from_be_bytes([bytes[29], bytes[30], bytes[31], bytes[32]]),
    }
}
```

验证：用你的 hexdump 工具找到 Node(id=0) 的 `firstRelId`，然后跳到 relationshipstore 的对应偏移，验证 `firstNodeId` 或 `secondNodeId` 中有一个是 0。

### 任务 2：Page Cache 实验

```
实验条件：
  数据：100K 节点 + 500K 关系
  查询：50 条随机 2-hop 遍历
  配置：dbms.memory.pagecache.size = 128M / 512M / 2G
  
需要记录：
  每组的 P50/P99/P999 延迟
  Page Cache 命中率（Neo4j 的 query.log 或 JMX 监控）

分析：
  当 cache size / data_size < 1 时，命中率下降 → 延迟增加多少？
```

### 任务 3：手写查询计划解释器

读 5 条不同查询的 `EXPLAIN` 输出，手写解释每个算子的选择和顺序。对于每条查询，回答：
- 为什么选这个索引而不选另一个？
- 为什么用 ExpandAll 而不是 ExpandInto？
- 如果调换 WHERE 子句的顺序，执行计划会不会变？

---

## 验收标准

- [ ] hexdump 解析器能正确解析 nodestore 和 relationshipstore 的前 10 条记录
- [ ] Page Cache 实验报告：含三种 cache size 下的 P50/P99 延迟 + 命中率
- [ ] 5 条查询的 EXPLAIN 手写解读，每条 150 字以上
- [ ] 冷查询 vs 热查询的延迟对比数据

---

## 思考题

1. Neo4j 用固定大小记录。如果一条关系需要存储大量属性（如一个 200 字符的 `citation_context`），34 字节不够怎么办？Neo4j 的 propertystore 是怎么扩展的？
2. 密集节点的优化（关系分组）在什么场景下反而是性能损失？（提示：如果 MATCH 不指定关系类型——`(n)-->(m)`——需要遍历所有分组）
3. 如果 Page Cache 大小 = 数据大小，理论上命中率是 100%。但实际上可能只有 95%。为什么？（提示：Page Cache 不只存节点和关系数据——索引页、属性页也竞争 cache 空间）

---

## 进阶挑战

- 用 procfs（Linux）/ perf 工具监控 Neo4j 的磁盘 IO 模式——随机读 vs 顺序读的比例
- 对比 Neo4j Community vs Enterprise 在 Page Cache 管理和查询并行度上的差异（读文档即可）
- 手动触发 Neo4j checkpoint（`CALL db.checkpoint()`），观察 checkpoint 对查询延迟的瞬时影响

---

## 与标书审核项目的关系

你在 G3/G6 组面对的 Neo4j 实例会包含 5000+ 法规、50000+ 条款、10000+ 案例。如果你不理解 Page Cache 的工作方式——当 Agent 的 `graph_traverse` 工具首次查询时可能花费 500ms（冷），第二次只需 5ms（热）。你需要能解释这个差异，并能设计预热策略让生产环境的第一条查询也是热的。
