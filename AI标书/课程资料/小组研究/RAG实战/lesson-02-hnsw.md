# Day 2：近似最近邻搜索 — 从手工推演到简化 HNSW Demo

> Day 1 你把文本变成了 1024 维向量。今天你要在 100 万条向量中找到最近的 10 条——不能逐个比较，必须在 1 毫秒内完成。

---

## 学习目标

1. 理解维度诅咒及其对 ANN 算法的根本限制
2. 用小规模 Demo 理解 HNSW 插入、分层搜索和选邻居策略
3. 实现 IVF 索引作为对比基准
4. 在 100K 法规向量上对比 BruteForce / IVF / HNSW 的 Recall vs QPS

---

## 核心概念

### 1. 维度诅咒 — 为什么高维搜索这么难

#### 实验：随机向量的 pairwise 距离

```python
# 在 d 维空间中生成 10000 个随机向量，计算所有 pairwise 距离
# 你会发现：
d=2:    最近邻距离/最远邻距离 ≈ 0.01   → 距离差异很大，"近"有明确含义
d=10:   最近邻距离/最远邻距离 ≈ 0.30
d=100:  最近邻距离/最远邻距离 ≈ 0.80
d=1024: 最近邻距离/最远邻距离 ≈ 0.98   → 所有距离都差不多！
```

这被称为"维度诅咒"（Curse of Dimensionality）——当维度足够高时，任意两点间的距离趋于常数。这意味着：

- 没有任何索引结构能有效区分"近"和"远"——所有点都是"差不多远"
- 当距离比趋近 1.0 时，任何 ANN 算法都退化为 Brute Force
- 1024 维在这个边界上——还能用 ANN，但已经很接近极限了

这个实验你必须亲手跑一遍。不亲手看到数据，你不会真正理解为什么 HNSW 的参数要这样设计。

#### 为什么 ANN 仍然有效

真实数据（标书条款的 Embedding）不是均匀分布的随机向量。它们聚类——同一类法规的向量集中、不同类法规的向量远离。ANN 算法利用的就是这种聚类结构。

---

### 2. IVF-PQ — 工业界第一代方案

#### IVF（Inverted File）

```
构建：
  1. 对所有向量做 K-means 聚类（K = nlist 个聚类中心）
  2. 每个向量分配到最近的中心 → 形成 nlist 个倒排列表
  3. 倒排列表_i = {id_x, id_y, ...}（落在第 i 个聚类中的所有向量 ID）

搜索：
  1. 计算 query 到 nlist 个聚类中心的距离
  2. 选最近的 nprobe 个聚类（如 nprobe = 10）
  3. 只在这 nprobe 个聚类的倒排列表中做暴力搜索
  4. 返回 Top-K
```

搜索复杂度：O(nprobe * N / nlist)，远小于 O(N) 的暴力搜索。

K-means 的 Lloyd 算法迭代：
```
1. 随机初始化 K 个质心
2. 分配：每个向量分配给最近的质心
3. 更新：每个质心 = 其所有成员向量的均值
4. 重复 2-3 直到收敛（质心移动 < 阈值）或达到最大迭代次数
```

#### PQ（Product Quantization）

IVF 减少了搜索的向量数量。PQ 进一步压缩每个向量的存储。

```
原始向量：[1024 个 f32] = 4096 bytes
PQ 压缩：
  1. 将 1024d 等分为 m 段，每段 1024/m 维（如 m=8，每段 128d）
  2. 对每段独立做 K-means（K = 256 个码本）
  3. 每段的向量用最近码本的 ID（1 byte）表示
  4. 整个向量 = [code_1, code_2, ..., code_8] = 8 bytes
  
  压缩比 = 4096 / 8 = 512×（实际通常用 m=8, 压缩 32-64×）
```

搜索时的非对称距离计算（ADC）：
- Query：保持 FP32（不量化），计算 query 每段与所有码本的距离 → 距离查找表
- Database 向量：查表累加 → `Σ lookup_table[code_i]`
- 不需要反量化，直接查表 O(m) 完成

---

### 3. HNSW — 当前 SOTA 的图导航方法

#### 核心直觉：分层导航

想象你要从北京开车到广州：
- **顶层**（国家级高速）：G4 京港澳高速 → 从北京直接到广州方向。这条层的图很稀疏（每个城市只连几个其他城市），但你很快找到大方向。
- **中层**（省级公路）：到了湖南 → 选择去长沙还是郴州。这层图更密（每个节点连十几个其他节点），你在大方向内找具体目的地。
- **底层**（市县级道路）：到了广州 → 找具体街道。这层图最密（每个节点连几十个邻居），你在最终目的地附近精确定位。

HNSW 就是把这个直觉形式化为算法。

#### 原论文关键段落实读（Malkov & Yashunin, 2016）

**§3.1 层级随机化**

每个插入元素的最高层级 l 按指数分布随机抽样：

```
l = floor(-ln(uniform(0, 1)) * m_L)

其中 m_L = 1 / ln(M)，M 是每节点最大邻居数。

这个公式的含义：
  - 大部分元素只在底层（l=0）：概率 ≈ 1 - 1/M
  - 少数元素在高层（l ≥ 1）：概率 ≈ 1/M
  - 指数衰减：层级越高，元素越少

例如 M=16, m_L ≈ 0.36：
  l=0 概率: 69.8%
  l=1 概率: 21.2%
  l=2 概率:  6.4%
  l=3 概率:  1.9%
  ...
```

**§3.2 插入算法**

```
INSERT(hnsw, q, M, M_max, ef_construction, m_L):
  l ← floor(-ln(uniform(0,1)) * m_L)     # 随机选层级
  
  # 从顶层开始找 entry point
  ep ← entry_point
  L ← top_level
  for lc ← L down to l+1:                 # 从顶层下降到目标层的上一层
    ep ← SEARCH_LAYER(q, ep, ef=1, lc)[0]  # 每层只找最近的一个
  
  # 在目标层及以下逐层插入
  for lc ← min(L, l) down to 0:
    W ← SEARCH_LAYER(q, ep, ef_construction, lc)  # 在 lc 层找 ef_construction 个最近邻
    neighbors ← SELECT_NEIGHBORS(q, W, M, lc)     # 从候选里选 M 个建边
    在 lc 层连接 q 和 neighbors                     # 双向加边
    for each n in neighbors:
      if n的连接数 > M_max:
        n.neighbors ← SELECT_NEIGHBORS(n, n.neighbors ∪ {q}, M_max)  # 收缩邻居列表
    ep ← W[0]  # 下一层从最近的开始
```

**§4.1 搜索算法**

```
SEARCH_LAYER(q, ep, ef, lc):
  v ← ep           # 从 entry point 开始
  C ← {ep}         # 候选集
  W ← {ep}         # 已访问集（结果集）
  
  while C 非空:
    c ← C 中离 q 最近的
    f ← W 中离 q 最远的
    if distance(c, q) > distance(f, q):
      break        # 候选集中最近的都比结果集中最远的还远 → 停止
    
    for each neighbor ∈ c.get_neighbors(lc):
      if neighbor ∉ W:
        W.add(neighbor)
        C.add(neighbor)
        if |W| > ef: W.remove_farthest()  # W 只保留 ef 个最近邻
  
  return W 的前 K 个
```

这是一个贪心搜索：从 entry point 开始，总是跳到当前节点的邻居中离 query 更近的，直到再也找不到更近的为止。

#### 什么决定了 HNSW 的参数

| 参数 | 控制 | 增大影响 |
|------|------|----------|
| `M` | 每节点最大邻居数 | 图更密 → Recall ↑、内存 ↑、构建时间 ↑ |
| `M_max` | 顶层最大邻居数 | 通常 = 2*M |
| `ef_construction` | 插入时的候选集大小 | 建图更精细 → 搜索 Recall ↑、构建时间 ↑↑ |
| `ef` | 搜索时的候选集大小 | Recall ↑、搜索时间 ↑。可运行时动态改！ |
| `m_L` | 层级分布 | 通常 = 1/ln(M)，不需要调 |

`ef` 是最灵活的——索引构建完成后，你可以每条查询用不同的 ef。重要查询用 ef=512 求高召回，普通查询用 ef=64 求低延迟。

#### 选邻居的启发式剪枝

这是 HNSW 最关键也最容易写错的代码：

```rust
fn select_neighbors(
    q: &[f32],           // 查询向量
    candidates: &[ScoredId],  // SEARCH_LAYER 返回的 ef_construction 个候选
    m: usize,            // 需要选出的邻居数
) -> Vec<usize> {
    let mut result = Vec::new();
    
    for candidate in candidates.sorted_by_distance_to(q) {
        if result.len() >= m {
            break;
        }
        
        // 启发式：如果 candidate 离 q 比离 result 中已有邻居更近，就保留
        // 这意味着 candidate 位于 q 的一个"未被覆盖"的方向
        let is_covered = result.iter().any(|&r| {
            distance(candidate.vec, r.vec) < distance(candidate.vec, q.vec)
        });
        
        if !is_covered || result.is_empty() {
            result.push(candidate);
        }
    }
    
    result
}
```

直觉：纯贪心策略（只选离 q 最近的 M 个）会导致图在某个方向极度密集、其他方向稀疏——搜索时可能陷入"死胡同"。启发式策略保留"不在已有邻居覆盖区"的候选，保证图在新方向也有连通性。

---

## 动手

### 任务 1：简化 HNSW 实验 Demo（小组必做）

```rust
pub struct HnswIndex {
    layers: Vec<Layer>,          // Vec<Vec<usize>>，每层存储该层的节点 ID
    nodes: Vec<HnswNode>,
    entry_point: usize,
    config: HnswConfig,
}

struct HnswNode {
    id: u64,
    vector: Vec<f32>,             // 1024d
    neighbors: Vec<Vec<usize>>,   // neighbors[layer] = 该层邻居的 node_index 列表
    max_layer: usize,
}

pub struct HnswConfig {
    pub m: usize,                  // 默认 16
    pub m_max: usize,              // 默认 32 (= 2*M)
    pub ef_construction: usize,    // 默认 200
    pub ml: f64,                   // 默认 1.0 / (M as f64).ln()
}
```

Demo 至少实现或模拟以下方法中的核心路径：
- `insert(&mut self, vector: &[f32], id: u64)` — 随机选层级 → 逐层搜索 → 选邻居 → 双向加边
- `search(&self, query: &[f32], k: usize, ef: usize) -> Vec<ScoredId>` — 顶层 descent → 底层优先队列
- `save(&self, path: &str)` / `load(path: &str)` — 序列化（JSON 或 bincode）
- 距离函数支持 `Cosine`、`Euclidean`、`DotProduct` 三种（BGE-M3 用 DotProduct = Cosine Similarity）

### 任务 2：实现 IVF 索引

```rust
pub struct IvfIndex {
    centroids: Vec<Vec<f32>>,         // nlist 个聚类中心向量
    inverted_lists: Vec<Vec<usize>>,  // 每个聚类的向量索引列表
    vectors: Vec<Vec<f32>>,           // 所有向量
}
```

骨干选做：实现 K-means Lloyd 迭代、分配和 `search(query, k, nprobe)`，观察 `nprobe` 对召回和延迟的影响。其他成员可以用伪代码和手工样本解释 IVF 搜索过程。

### 任务 3：Benchmark 全量对比

用 Day 1 的 `EmbeddingEngine` 生成 100K 条法规的 1024 维向量（讲师提供预生成文件）。

对比方案：
- Brute-Force（基线：100% Recall，QPS 作为 baseline）
- IVF（nlist=100/500/1000, nprobe=5/10/20）
- HNSW（M=8/16/32/64, ef_construction=100/200/400/800）

输出：
1. **Recall@10 vs QPS 散点图**（每组参数一个点）
2. **构建时间 vs 搜索 Recall 散点图**（ef_construction 参数敏感性）
3. **正确性验证**：随机 100 条 query，HNSW Top-10 与 Brute-Force Top-10 的交集大小——目标 > 85%

### 任务 4：找出 Pareto 最优

从 Benchmark 数据中找出 Pareto 前沿——那些"不存在另一个配置同时有更高 Recall 和更高 QPS"的点。这些就是生产环境的候选配置。

---

## 验收标准

- [ ] 能画出 HNSW 的分层搜索和插入过程
- [ ] 独立 Demo 能在小规模向量上比较精确搜索与近似搜索
- [ ] 改变 `M` 或 `ef` 后，能解释召回率和访问节点数的变化
- [ ] 完整 HNSW 的 Save/Load、启发式剪枝和 IVF 实现为骨干选做
- [ ] 100K 向量 HNSW 搜索 P99 < 10ms（ef=128 时）
- [ ] HNSW Top-10 与 Brute-Force Top-10 交集 ≥ 85%
- [ ] Benchmark 报告含 Recall-QPS 图 + 构建时间敏感性分析 + Pareto 前沿
- [ ] 维度诅咒实验重现：d=2/10/100/1024 的距离比曲线

---

## 思考题

1. `select_neighbors` 的启发式剪枝中，"if `dist(c, existing) < dist(c, q)`" 这个条件直觉上很奇怪——它保留了"离已有邻居比离 query 更近"的候选。为什么这样能保证图的连通性？
2. HNSW 论文发表已经 9 年了。它有什么已知缺陷？最新的替代方案（如 DiskANN、FreshDiskANN）在什么场景下优于 HNSW？
3. 如果 100K 数据每天新增 1K 条，HNSW 的增量插入会导致图质量退化吗？你怎么检测退化？

---

## 进阶挑战

- 实现 SIMD 加速的距离计算：用 `std::arch::x86_64` 的 AVX2 指令并行计算 8 个 f32 的乘加
- 每次插入 1000 条后做一次 Recall 抽样检测，画出"插入量 vs 召回率衰减"曲线
- 实现并行 HNSW 构建：多个线程同时插入，用 RwLock 保护共享的 layers/nodes 结构
- 用 `pprof` 或 `perf` 做 CPU profiling，定位距离计算的热点，尝试优化

---

## 与标书审核项目的关系

G3 组的 Qdrant 内部用的就是 HNSW。你现在理解了 Qdrant 的 `m`、`ef_construct`、`ef` 参数——不是你背下来的，是你**亲手实现过**的。

当你调 `ef=128` 时，你知道这意味着底层优先队列保留 128 个候选，每个候选的每个邻居都要计算距离——计算量是 ef × M。当你说"先降低 ef 到 64 提升 QPS"时，你知道代价是 Recall 可能从 95% 降到 90%。这不是玄学，是Trade-off。
