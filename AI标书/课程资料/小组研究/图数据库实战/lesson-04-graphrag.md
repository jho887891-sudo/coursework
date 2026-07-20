# Day 4：GraphRAG 深度 — 图增强检索

> 向量检索找到了"语义相似的法规"。但法律知识有结构——法规引用法规，条款体现规则，案例揭示风险。今天你把向量检索和图书检索结合，并深入微软 GraphRAG 论文的算法内核。

---

## 学习目标

1. 理解 Microsoft GraphRAG 的 Local→Global Search 双通道架构
2. 手写 Leiden 社区检测，理解模块度优化的数学原理
3. 实现社区摘要生成和 Map-Reduce 全局搜索
4. 对比 Pure Vector / Pure Graph / GraphRAG 三种策略的检索质量

---

## 核心概念

### 1. GraphRAG — 当向量不够用时

#### 一个向量回答不了的问题

```
问题："这份招标文件的资质条款是否存在隐性的地方保护倾向？"

向量检索能找到：
  - "资质要求"相关条款的文本
  - "地方保护"相关的法规

但回答不了：
  - 这些条款在整体招标文件中的比例（需要知道全文档的条款分布）
  - 这些条款引用的法规是否被某地方法规"实际上放宽了"（需要图遍历）
  - 类似案例中这种条款模式导致了什么后果（需要 Risk → Case → Article 图链）
```

GraphRAG 的核心洞察：**全局性问题需要全局性视野**。向量检索提供"点状"证据，图提供"网状"结构。

#### Microsoft GraphRAG 的完整架构

```
┌──────────────────────────────────────────────────────────┐
│                     Phase 1: Indexing (离线)               │
│                                                            │
│  Raw Documents                                              │
│      │                                                      │
│      ▼                                                      │
│  Entity + Relation Extraction (LLM)                         │
│  → Entities (nodes) + Relations (edges)                     │
│      │                                                      │
│      ▼                                                      │
│  Build Entity Graph                                         │
│      │                                                      │
│      ▼                                                      │
│  Leiden Community Detection (层次化社区划分)                  │
│  → Level 0 (leaf): ~10-100 entities per community           │
│  → Level 1 (intermediate): ~50-500 entities                 │
│  → Level 2 (root): ~100-1000 entities                      │
│      │                                                      │
│      ▼                                                      │
│  Community Summarization (LLM per community)                │
│  → 结构化摘要：entities, themes, key_numbers, relationships  │
│      │                                                      │
│      ▼                                                      │
│  Index into Vector Store (entities + community summaries)   │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│                     Phase 2: Querying (在线)               │
│                                                            │
│  ┌─ Local Search (具体问题)                                  │
│  │   Query → Vector search on entities                     │
│  │   → Load entity + 1-hop neighbors                       │
│  │   → Load community summaries for those entities        │
│  │   → LLM synthesize                                      │
│  │                                                          │
│  └─ Global Search (宏观问题)                                │
│      Query → Map(每个社区摘要独立回答) → Reduce(聚合答案)    │
│      → LLM synthesize final global answer                  │
└──────────────────────────────────────────────────────────┘
```

**Local Search 对标书场景**："招标投标法第 22 条在哪些案例中被引用？"

**Global Search 对标书场景**："这份招标文件是否存在排斥外地企业的系统性倾向？"

---

### 2. Leiden 社区检测 — 图的自动分区

#### 模块度（Modularity）的数学形式

图分区的好坏由模块度 Q 量化：

$$Q = \frac{1}{2m}\sum_{ij}\left[A_{ij} - \frac{k_i k_j}{2m}\right]\delta(c_i, c_j)$$

其中：
- $A_{ij}$ = 节点 i 和 j 之间的实际边权重
- $k_i$ = 节点 i 的度（所有边权重之和）
- $m$ = 图中所有边权重之和
- $\frac{k_i k_j}{2m}$ = 随机图中节点 i 和 j 之间的期望边权重（Null Model）
- $\delta(c_i, c_j)$ = 1 如果 i 和 j 在同一社区，否则 0

直觉：如果实际边数比随机期望多很多 → 这两个节点"故意连在一起"→ 同一社区。

#### Leiden 算法三阶段

**阶段 1：局部移动（Local Moving）**

```
for each node v:
    current_community = community[v]
    best_community = current_community
    best_delta_Q = 0
    
    for each neighbor_community of v:
        delta_Q = compute_delta_Q(v, current_community, neighbor_community)
        if delta_Q > best_delta_Q:
            best_delta_Q = delta_Q
            best_community = neighbor_community
    
    if best_community != current_community:
        move v to best_community  // 可能增加某些社区的模块度、减少另一些

// 重复直到没有节点移动能增加 Q
```

与 Louvain 的区别：Leiden 的节点移动是**非贪心的**——即使当前最佳移动增加 Q，Leiden 也会继续寻找更好的移动。

**阶段 2：精炼（Refinement）— Leiden 独有的步骤**

```
// Louvain 的问题：
// 一次"局部移动"后，有些社区可能内部不连通：
//   Community C: {A--B, B--C, D--E} → D-E 与 A-B-C 不连通
//   但 Louvain 直接进入聚合阶段 → 把整个 C 压缩为一个节点
//   → 之后 C 内部的分割永远无法被修正

// Leiden 的精炼：
for each community from Phase 1:
    从社区内随机选一个节点作为"种子"
    用 BFS 从种子出发构建子社区（保证内部连通）
    将社区内剩余未分配的节点分配到最近的子社区
    → 保证：精炼后的每个子社区都是 well-connected

// 精炼后可能产生空社区 → 移除
```

**阶段 3：聚合（Aggregation）**

```
将精炼后的每个社区压缩为一个"超节点"：
   超节点的权重 = 社区内所有节点权重之和
   超节点 A 和 B 之间的边权重 = 原图中 A 内节点与 B 内节点间的所有边权重之和

在聚合图上重复阶段 1-3 → 形成层次化社区结构
```

#### Leiden 在标书法律图上的直观解释

```
500 条法规 → Leiden 社区检测：

Level 0 (leaf communities, ~10-30 法规/社区):
  社区 1: {招标投标法, 招标投标法实施条例, 建筑工程施工招标投标管理办法, ...}
  社区 2: {政府采购法, 政府采购法实施条例, 政府采购货物和服务招标投标管理办法, ...}
  社区 3: {安全生产法, 建设工程安全生产管理条例, ...}
  社区 4: {建筑法, 建筑工程质量管理条例, ...}

Level 1 (intermediate, ~30-100 法规):
  社区 A: {社区 1 + 社区 4}  →  "建筑招投标法规体系"
  社区 B: {社区 2 + ...}     →  "政府采购法规体系"
  社区 C: {社区 3 + ...}     →  "安全生产法规体系"

Level 2 (root):
  社区 X: 全部 500 条法规    →  "中国招标采购法规体系"
```

当 Agent 需要"全局视野"来回答"这份招标文件是否系统性地偏袒本地企业"时——它从 Level 2 的社区摘要开始，逐步下钻到具体条款。

---

### 3. 社区摘要与 Map-Reduce 全局搜索

#### 社区摘要的结构化格式

社区摘要不是"一段话总结"。它必须结构化——保留法条编号、案例名称、关键数字：

```json
{
  "community_id": "comm_042",
  "level": 0,
  "name": "建筑招投标资质要求法规群",
  "entities": [
    {"law_id": "law_042", "name": "建筑业企业资质管理规定", "key_articles": ["3", "5", "8"]},
    {"law_id": "law_103", "name": "建筑工程施工招标投标管理办法", "key_articles": ["12", "15"]}
  ],
  "key_themes": ["施工资质等级", "资质证书有效期", "联合体资质认定"],
  "key_numbers": ["二级及以上", "5年有效期", "3年审查周期"],
  "typical_scenarios": [
    "招标文件要求投标人具备二级资质但未区分专业类别",
    "联合体投标中各方资质等级不一致的认定争议"
  ],
  "related_risks": ["资质等级门槛过高排除中小企业", "资质专业不对口导致合同无效"]
}
```

#### Map-Reduce 全局搜索

```
Query: "这份招标文件在资质要求方面是否存在不合理的排斥性条款？"

Global Search:

Map Phase (并行):
  Level 1 communities:
    comm_A "建筑招投标法规":  摘要 + query → "该社区要求在资质方面不得设定与项目
                             规模不匹配的等级门槛。招标文件要求'总承包一级'，
                             但项目为2000万工程，一级可能过度。"
    comm_B "政府采购法规":    摘要 + query → "政府采购强调不得以注册资本、资产总额
                             等规模条件限制供应商。招标文件无此问题。"
    comm_C "安全生产法规":    摘要 + query → "安全许可证要求是硬性规定，不构成排斥。
                             招标文件的安全要求属于常规范围。"
    ...

Reduce Phase:
  聚合所有 community 的部分答案：
    "综合建筑招投标和政府采购法规体系分析：
     1. 资质等级要求（总承包一级）对2000万工程可能过高 → 建议降低到二级（建筑法规社区）
     2. 未发现基于注册资本的限制 → 符合政府采购法要求（采购法规社区）
     3. 安全要求属于常规范围，不构成排斥（安全法规社区）
     结论：存在1处潜在的排斥性风险，来源于资质等级要求超出项目合理需求。"

  来源可追溯：每条结论都标注来自哪个社区、该社区包含哪些法规
```

---

### 4. 图增强检索的混合架构

> 注：关于向量检索，本节自带 10 分钟原理说明——不需要前置学过向量数据库或 RAG 课程。

**向量检索是什么（10 分钟速览）**

向量检索的核心思想：把文本变成固定长度的数字向量（如 1024 个 float），使得"语义相似的文本在向量空间中距离更近"。

```
文本 → 嵌入模型（Embedding Model）→ [0.023, -0.145, 0.891, ..., -0.032]（1024 维）
两条文本的语义相似度 = 它们向量的余弦相似度
```

对于本课，你不需要自己跑嵌入模型。两种选择：
- A) 使用本课程提供的预计算向量文件（法规文本 → 1024d 向量，已算好）
- B) 使用简单的关键词匹配（TF-IDF 或 BM25）作为"伪向量"——也能做语义级别的粗略匹配

**三层检索架构**

```
Query: "投标人须具备建筑工程施工总承包二级及以上资质"

Layer 1: 语义匹配（向量或关键词）
  → Top-20 语义相关的法规 Chunk
  → 确定"种子节点"：Top-3 chunk 对应的 Law/Article 节点 ID

Layer 2: 图扩展（Neo4j，Day 1-3 的核心能力）
  从种子节点出发，2-hop 图扩展：
    Law → HAS_ARTICLE → Article
    Article → CITED_IN → Case
    Law → OVERLAPS_WITH → Law
    Law → AMENDED_BY → Law (旧版→新版)
  → 收集扩展节点的文本

Layer 3: 合并 & 排序
  将 Layer 1 的语义结果 + Layer 2 的图扩展结果 → 合并去重
  → 按图遍历距离 + 语义相似度综合排序 → Top-10 最终证据
  标注来源：source = "semantic" | "graph_traversal"
```

---

## 动手

### 任务 1：Leiden 局部移动实验

小组必做：用 Rust 实现或模拟 Leiden 的局部移动阶段，并在 10～50 个节点的小图上观察模块度变化。完整三阶段实现作为骨干选做：

```rust
struct LeidenCommunityDetection {
    graph: SparseAdjacencyMatrix,
    node_weights: Vec<f64>,
    edge_weights: HashMap<(usize, usize), f64>,
}

impl LeidenCommunityDetection {
    fn local_moving(&mut self) -> Vec<usize>;    // 社区分配结果
    fn refine(&self, partition: &[usize]) -> Vec<usize>;  // 精炼
    fn aggregate(&self, refined: &[usize]) -> Self;       // 聚合
    fn run(&mut self, max_levels: usize) -> Vec<Vec<usize>>;  // 多层级结果
}
```

在人工构建的 50 节点测试图上验证：能正确分出预期社区。

### 任务 2：社区摘要生成

对 Leiden 产出的每个 Level 0 社区，用 LLM 生成结构化摘要（JSON 格式，含 entities/themes/numbers/scenarios）。

### 任务 3：构建对比评测

10 条需要图遍历才能完整回答的标书法律查询。每条查询标注"标准答案需要覆盖的知识点"——其中至少 3 个知识点只能从图关系中获取。

```
示例查询：
  "哪些地方规章在资质要求上与《建筑业企业资质管理规定》存在重叠？
   这些重叠是否导致了执法冲突？"

标准答案需要的知识点：
  1. 建筑业企业资质管理规定的核心条款 [Vector 可获取]
  2. 地方规章列表中"资质管理"相关内容 [Vector 可获取]
  3. 地方规章与国家法规的 OVERLAPS_WITH 关系 [需要 Graph]
  4. 重叠引起的执法冲突案例 [需要 Graph — Case→Article 图遍历]
  5. 冲突发生的时间线 [需要 Graph — 案例的时间聚合]
```

### 任务 4：对比三种策略

| 策略 | Recall@10 | MRR | 答案完整性 (0-1) |
|------|-----------|-----|-----------------|
| Pure Vector | ? | ? | ? |
| Pure Graph | ? | ? | ? |
| GraphRAG (Hybrid) | ? | ? | ? |

---

## 验收标准

- [ ] 能手工解释一次节点移动为什么提高或降低模块度
- [ ] 小图 Demo 能输出社区划分和模块度变化
- [ ] Leiden 三阶段完整实现为骨干选做
- [ ] 社区摘要生成：至少 5 个 Level 0 社区的结构化摘要
- [ ] 10 条图依赖评测查询的标注完成
- [ ] 三种策略对比报告：含 Recall@10 + MRR + Bootstrap CI

---

## 思考题

1. 模块度 Q 的最大值是多少？在所有节点都孤立成单一社区和所有节点都在同一社区两种极端情况下，Q 分别是多少？
2. Leiden 的精炼阶段保证了 well-connected communities——这对法律知识图谱有什么实际意义？（提示：一个"内部不连通的社区"在摘要生成时会有什么问题？）
3. Global Search 的 Reduce 阶段依赖 LLM 做信息聚合。如果社区数 >100，Reduce 的输入会超过 LLM 的上下文窗口。GraphRAG 论文的解决方案是什么？（提示：论文提到了一种自底向上的聚合策略）

---

## 进阶挑战

- 对比 Leiden 和 Louvain：在同一个图上运行两种算法，对比 Q 值 + 迭代轮数 + 社区内部连通性
- 实现自适应社区层级选择：短查询 → 用低层级社区（精、多），长查询 → 用高层级社区（粗、少）
- Global Search 的 Reduce 如果不调 LLM 而是用 TF-IDF 聚合——效果会怎样？做实验验证

---

## 与标书审核项目的关系

G3 组的 GraphRAG 混合检索直接基于你今天的选择——用什么社区层级、摘要格式是什么、Map-Reduce 怎么配置。

当 Agent 被问到"这份招标文件是否存在系统性问题"时——它不是一条一条查条款，而是触发 Global Search → 加载社区摘要 → 从宏观到微观层层递进。你今天写的 Map-Reduce，就是那个让 Agent 拥有"全局视野"的引擎。
