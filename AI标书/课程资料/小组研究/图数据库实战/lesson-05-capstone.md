# Day 5：知识生命周期独立实验

> 前 4 天你理解了图数据库内核、知识图谱和 GraphRAG。今天在独立小图中验证知识去重、来源、时效和检索机制；完整双库 Pipeline 作为骨干选做。

---

## 目标

深入 Curator Pipeline 每一步的算法原理。不追求完整实现——追求每一个模块你能回答"为什么选这个算法、它的复杂度是多少、在什么条件下退化"。

---

## 架构总览

Curator Pipeline 是一个后台异步任务，在每次审核完成后触发。输入是本课程自带的法规/案例数据（或从审核结果中提取的新实体），输出是更新后的 Neo4j 图 + 可选的图嵌入索引。

```
审核结果 / 新法规数据（本课程自带的 JSON 文件）
         │
         ▼
┌──────────────────────────────────────────────────────┐
│               Curator Pipeline（后台异步）              │
│                                                        │
│  Step 1: Dedup  ── 四层去重漏斗                          │
│    ① SHA256 exact match (O(1) per item)                │
│    ② Embedding cosine ≥ 0.95 (O(n) pairwise, 可优化)    │
│    ③ SimHash 海明距离 < 3 (O(1) per item)               │
│    ④ LLM final judgment (可选, O(1) per ambiguous pair) │
│                                                        │
│  Step 2: Graph Build  ── 实体抽取 → MERGE 写 Neo4j      │
│    ②a: Entity + Relation Extraction                     │
│    ②b: Deterministic ID + MERGE                         │
│                                                        │
│  Step 3: Freshness  ── 时效性判定                        │
│    ③a: effective_date 检查                              │
│    ③b: AMENDED_BY 链遍历 → 找到最新版本                   │
│    ③c: 过期标记（is_deprecated=true, 而非删除节点）       │
│                                                        │
│  Step 4: Embed & Index  ── 图嵌入（可选）                │
│    ④a: Node2Vec on Law nodes → 128d 向量                │
│    ④b: 可存入本地 JSON 文件或任意向量存储                 │
└──────────────────────────────────────────────────────┘
```

> 注：Step 4 的向量存储是可选的。如果你学过向量数据库，可以接入 Qdrant；如果没学过，用本地 JSON 文件 + 简单的余弦相似度计算即可——本课不依赖外部向量数据库。

---

## 模块 1：去重算法 — 四层漏斗

### 为什么需要四层

```
审核后每天可能产生 ~100 条新的 curation_mark 实体。
但其中很多与已有知识库中的实体重复。
如果重复实体被重新创建 → Neo4j 中出现多个代表同一实体的节点
→ 图查询的连通性被破坏（有些关系连到旧节点，有些连到新节点）

去重的目标：最大化去重率，最小化误去重率。

为什么用四层：
  第①层：SHA256 → 极快(O(1))、零误判、但只能发现完全相同的实体
  第②层：Embedding → 能发现高度相似但文本略有差异的实体
  第③层：SimHash → Embedding 的 O(n²) 计算太慢，SimHash 用哈希桶预筛选
  第④层：LLM → 对于第②③层判定为"边界"的 pair，LLM 做最终裁决
```

### 第①层：SHA256 Exact Match

```rust
fn dedup_exact(candidates: &[Entity], existing_set: &HashSet<String>) -> (Vec<Entity>, usize) {
    let mut new_entities = Vec::new();
    let mut dup_count = 0;
    
    for entity in candidates {
        let id = entity.deterministic_id();  // SHA256(name + version + date)[:8]
        if existing_set.contains(&id) {
            dup_count += 1;  // 完全相同的实体 → 跳过
        } else {
            new_entities.push(entity);
        }
    }
    
    (new_entities, dup_count)
}
```

### 第②层：Embedding Cosine Dedup

```rust
// 对第①层筛选后的新实体，与已有实体库做 embedding similarity 对比
// 但如果已有库有 100K 实体 → O(100K × new_count) → 太慢

// 优化：只与同 Label、同 category 的已有实体做对比
// Law 只与 Law 比，Case 只与 Case 比
// 每个类别 ~10K → O(10K × new_count) → 可接受
```

### 第③层：SimHash — 亚线性去重

SimHash 的核心思想：把高维向量压缩为固定长度的指纹（fingerprint），用汉明距离近似余弦距离。

```rust
fn simhash(vector: &[f32; 1024]) -> u64 {
    let mut hash: u64 = 0;
    for (i, &val) in vector.iter().enumerate() {
        // 用一个与 i 绑定的随机向量（seed = i）来分散 hash 位
        let weight = val.to_bits() as u64;
        // 如果 val > 0，hash 的每位的贡献加 1；如果 < 0，减 1
        for bit in 0..64 {
            let bit_mask = 1u64 << bit;
            let random_bit = (weight.wrapping_mul(bit_mask)) >> 63;
            if val > 0.0 {
                hash += random_bit;
            } else {
                hash -= random_bit;
            }
        }
    }
    // 最终：每位 > 0 → 1，< 0 → 0
    hash
}

// 汉明距离 < 3 → 可能是重复
// 为什么快：simhash 是 u64 → 汉明距离比较 = popcount(a ^ b)
// 100K 条 simhash 的 pairwise 比较 = 100K × 64bit XOR + popcount
// vs 100K 条 1024d 向量的 pairwise cosine = 100K × 1024 MAC
// ~5000 倍加速
```

为什么不用 MinHash：SimHash 适用于 "是否高度相似" 的二分类，MinHash 更适用于 Jaccard 相似度估算（集合场景）。文本去重通常用 SimHash。

### 误去重分析

去重后必须手动检查。随机抽 20 对被标记为"重复"的实体：
- 如果 19 对确实是重复 → 精确率 95% → 可以接受
- 如果 5 对不是重复 → 精确率 75% → 阈值太宽松，需要提高 cosine threshold 到 0.97

输出"去重率"和"误去重率"——这是 Curator Pipeline 的核心质量指标。

---

## 模块 2：Node2Vec 图嵌入

### 为什么需要图嵌入

BGE-M3 文本嵌入捕获的是"内容相似性"——两段文字语义接近。图嵌入捕获的是"结构相似性"——两个节点在图中的位置/角色接近。

在法律图中：
- 内容相似：两个法规都提到了"资质等级" → 文本嵌入相似
- 结构相似：两个法规被同一批案例引用 → 图嵌入相似（即使内容完全不同）

这两种相似性**互补**——有些法律问题需要结构相似性（"哪些法规在实际判例中经常一起出现"），文本嵌入无法回答。

### Node2Vec 的随机游走

```rust
struct Node2Vec {
    p: f64,  // Return parameter: 控制"往回走"的概率
    q: f64,  // In-out parameter: 控制 BFS vs DFS
    walk_length: usize,
    num_walks: usize,
}

impl Node2Vec {
    fn random_walk(&self, graph: &Graph, start: usize) -> Vec<usize> {
        let mut walk = vec![start];
        let mut current = start;
        
        for _ in 0..self.walk_length {
            let neighbors = graph.neighbors(current);
            if neighbors.is_empty() { break; }
            
            let next = if walk.len() == 1 {
                // 第一步：均匀随机选邻居
                neighbors[rand::random::<usize>() % neighbors.len()]
            } else {
                // 后续步：根据 p, q 偏置
                let prev = walk[walk.len() - 2];
                let biased = self.biased_sample(current, prev, neighbors);
                biased
            };
            
            walk.push(next);
            current = next;
        }
        
        walk
    }
    
    fn biased_sample(&self, current: usize, prev: usize, neighbors: &[usize]) -> usize {
        // 对于每个邻居，计算 transition probability 权重
        let weights: Vec<f64> = neighbors.iter().map(|&n| {
            if n == prev {
                1.0 / self.p  // 往回走 → 被 p 控制
            } else if graph.has_edge(n, prev) {
                1.0           // 离 prev 距离 1 → 正常权重
            } else {
                1.0 / self.q  // 离 prev 距离 2 → 被 q 控制
            }
        }).collect();
        
        // 按权重采样
        weighted_sample(neighbors, &weights)
    }
}
```

#### p 和 q 参数的几何直觉

```
p < 1: "鼓励回头" → 游走在局部区域来回
      → 捕获结构等价性（structural equivalence）
      → 两个资质管理规定，即使在不同行业，结构角色相似 → 图嵌入接近

p > 1: "避免回头" → 游走不会立即返回
      → 鼓励探索新的区域

q < 1: "向外探索" → 游走倾向于远离起点 → DFS 式探索
      → 捕获同质性（homophily）
      → 同一法规体系内的节点 → 图嵌入接近

q > 1: "留在附近" → 游走倾向于近邻 → BFS 式探索
      → 捕获结构等价性（local structure）

经典设置：
  Node2Vec 论文推荐 p=1, q=1 作为 baseline（等同于 DeepWalk）
  结构等价性任务：p=0.5, q=2
  同质性任务：p=1, q=0.5
```

#### 从游走到嵌入

```rust
// 对所有节点生成 num_walks × walk_length 条游走路径
let walks: Vec<Vec<usize>> = (0..graph.node_count())
    .flat_map(|start| {
        (0..num_walks).map(move |_| node2vec.random_walk(&graph, start))
    })
    .collect();

// 把游走路径当作"句子"，节点 ID 当作"词"
// 用 Word2Vec (SkipGram) 训练节点嵌入
let node_embeddings = word2vec_skipgram(
    &walks,
    dim=128,           // 嵌入维度（比 BGE-M3 的 1024 低很多）
    window=10,         // 游走窗口大小
    negative=5,        // 负采样数
    epochs=5,
);

// 结果：每个 Law 节点 → 128d 向量
// 这个向量捕获的是"结构角色"，不是"语义内容"
```

---

## 模块 3：知识保鲜

### 法规的时效性

```
一条法规可能：
  1. 仍然有效（最新版）
  2. 已被修订（旧版被新版取代）
  3. 已废止（但历史案例仍引用它）
  4. 事实上被替代（没有正式废止，但实践中不再适用）

处理策略：
  - 被修订的法规 → 不删除！新旧版本共存，边 AMENDED_BY 连接
  - 已废止的法规 → 标记 is_deprecated=true，图查询时可过滤
  - 历史案例仍引用旧版 → 这是正确的——案例引用的是当时的有效版本
```

### 保鲜检查算法

```rust
fn check_freshness(law: &Law, graph: &Graph) -> FreshnessStatus {
    // 1. 检查当前时间是否超过 effective_date
    let now = chrono::Utc::now().date();
    
    // 2. 沿 AMENDED_BY 链找到最新版本
    let latest = follow_amendment_chain(law, graph);
    
    if latest.law_id != law.law_id {
        // 存在更新版本
        FreshnessStatus::Amended {
            current_version: latest.law_id,
            current_effective_date: latest.effective_date,
        }
    } else if law.effective_date < now - Duration::days(365 * 5) {
        // 超过 5 年未更新 → 可能需要复查
        FreshnessStatus::Stale {
            last_updated: law.effective_date,
            years_without_update: (now - law.effective_date).num_days() / 365,
        }
    } else {
        FreshnessStatus::Fresh
    }
}
```

---

## 动手

### P0：小型知识图 + 查询 Demo（小组必做）

1. 构建完整知识图谱（≥ 500 Law + 2000 Article + 200 Case + 100 Risk）
2. 实现 `POST /api/v1/knowledge/graph-search`：
   ```json
   {
     "seed_law_id": "law_042",
     "traversal_depth": 2,
     "relation_types": ["HAS_ARTICLE", "CITED_IN", "OVERLAPS_WITH"],
     "limit": 20
   }
   ```
3. 评测报告：10 个查询的路径完整性和延迟

### P1：Curator 内核（加分项）

1. 实现去重漏斗（至少第①+②层）
2. 实现 Node2Vec 图嵌入（游走 + Word2Vec）
3. 实现保鲜检查
4. 评测：手动插入 10 个重复/过期实体 → 验证去重率和保鲜准确率

### P2：图 + 语义融合检索（挑战项）

```
两路融合（自包含版，不依赖外部向量数据库）：
  ① 文本语义匹配：TF-IDF 或预计算余弦相似度（本课程提供预计算向量文件）
  ② 图嵌入匹配：Node2Vec 128d → 余弦相似度

融合方式（RRF，不依赖 Qdrant）：
  score = 1/(60 + rank_text) + 1/(60 + rank_graph)

评测：
  两路 RRF 融合 vs 纯文本匹配 vs 纯图嵌入
  → 10 条评测查询的 Recall@10 + MRR
```

> 如果你已学过向量数据库，可以接入 Qdrant 升级为三路融合。本课程不强制要求。

---

## 验收标准

| 验收项 | 权重 | 怎么测 |
|--------|------|--------|
| 图规模达标 | 15% | Neo4j `MATCH (n) RETURN labels(n), count(*)` |
| graph-search API 可用 | 15% | 发 5 条查询验证响应格式正确 |
| 去重算法（≥ 2 层） | 15% | 插入已知重复实体 → 验证去重率 |
| Node2Vec 嵌入可用 | 15% | t-SNE 可视化看到社区聚类效果 |
| 保鲜检查可用 | 10% | 插入已知过期法规 → 验证检测正确 |
| 评测报告有深度 | 15% | 含对比数据 + Bootstrap CI + 设计决策 |
| 设计决策文档 | 15% | 每个模块的选择理由 + trade-off 分析 |

---

## 设计决策文档（必写）

1. **去重阈值为什么选 0.95 而不是 0.98？** — 跑阈值敏感性分析证明
2. **Node2Vec 的 p/q 参数怎么选？** — 针对结构等价性 vs 同质性的目标选择
3. **图嵌入为什么用 128d 而不是 1024d？** — 维度过高导致下游融合权重不平衡
4. **保鲜为什么不删除旧法规？** — 历史案例引用旧版，删除会破坏引用链

---

## 与标书审核项目的关系

这个实验用于理解知识沉淀引擎的 Curator Pipeline 和图检索接口，不直接写入项目知识库。

你今天的去重算法 → G6 Curator Step 1。你的 Node2Vec → 图结构相似性检索（与语义检索互补）。你的保鲜检查 → G6 CuratorFreshAgent。你的 graph-search API → G3/G4/G5 共享的知识检索基础设施。

> 本课程产出的 Neo4j 图 + graph-search API 是独立可运行的服务。如果后续学了 RAG 课程，可以将两者的检索结果融合——图提供关系推理，向量提供语义匹配，形成完整的知识检索能力。
