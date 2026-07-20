# Day 3：Qdrant 内核级实战

> Day 2 你手写了 HNSW。今天你深入 Qdrant——生产环境真正在跑的向量数据库。理解它的存储引擎、量化策略、Payload 索引和混合检索，然后跑 192 组实验找到标书场景的最优配置。

---

## 学习目标

1. 理解 Qdrant 的 Segment 存储架构和 WAL 机制
2. 对比三种量化策略（SQ/PQ/BQ）的存储/速度/精度 trade-off
3. 掌握 Payload 索引的过滤时机（Pre vs Post filtering）
4. 实现 Dense + Sparse 混合检索，对比四种融合策略
5. 通过 192 组参数实验找到标书场景的 Pareto 最优配置

---

## 核心概念

### 1. Qdrant 存储引擎

#### Segment 架构 — 类 LSM-Tree

Qdrant 借鉴了 LSM-Tree 的思想：

```
┌─────────────────────────────────────┐
│  WAL (Write-Ahead Log)              │  ← 写入先落这里，幂等恢复
├─────────────────────────────────────┤
│  Mutable Segment (可写段)            │  ← 当前写入目标，小且可修改
├─────────────────────────────────────┤
│  Immutable Segment #1 (已优化只读)   │  ← HNSW 索引已构建，不可修改
│  Immutable Segment #2                │
│  ...                                 │
└─────────────────────────────────────┘

后台优化线程：
  当 Mutable Segment 达到阈值（默认 200MB 或 100K vectors）
  → 转为 Immutable → 构建 HNSW 索引 → 原子替换
  → 多个小 Immutable Segment 定期合并 → 重建 HNSW → 减少碎片
```

你的写入不会立即"可见"于搜索——这也是为什么 Qdrant 有 `wait` 参数：插入后可以 `wait=true` 等待索引可搜索。

#### Mmap vs RAM — 存储模式的选择

```
on_disk = true:
  向量数据存在磁盘 → mmap 到虚拟地址 → OS page cache 自动管理冷热
  优点：内存需求低（可以存 100GB 数据但只用 8GB 内存）
  缺点：冷数据初次访问触发缺页中断 → P50=2ms, P99=200ms（冷）

on_disk = false:
  向量数据全部在 RAM
  优点：稳定低延迟（P50=P99≈1ms）
  缺点：数据量受限于内存大小
```

对于标书场景（500-5000 条法规，1024d，< 50MB 向量数据），`on_disk=false` 完全可行。但你需要在 Benchmark 中证明这个选择。

#### Vector Storage 的内存布局

```rust
// Qdrant 内部：Vec<[f32; 1024]> 连续存储
// 遍历时的 cache line 利用率：
//   一行 L1 cache = 64 bytes = 16 个 f32
//   每个向量 = 1024 个 f32 = 64 条 cache line
//   连续存储 → 遍历时 CPU prefetcher 提前加载 → cache miss 率低
```

这个内存布局细节解释了为什么 Qdrant 的 Brute-Force 扫描比你自己写的 `Vec<Vec<f32>>` 快——指针跳转的 cache miss 惩罚远大于连续访问。

---

### 2. 量化策略

#### Scalar Quantization（SQ）

```rust
// 每维独立量化
for dim in 0..1024 {
    let min = database_vectors.iter().map(|v| v[dim]).min();
    let max = database_vectors.iter().map(|v| v[dim]).max();
    // 存 min/max + 量化后的 u8
    // 搜索时：query[dim] (FP32) × dequantize(database_u8[dim])
}

// 存储：1024d × 4B = 4096B → 1024d × 1B = 1024B → 压缩 4×
// 精度损失：< 1% Recall@10（因为 1024 维的误差被平均化）
```

#### Product Quantization（PQ）

Day 2 讲过原理。Qdrant 的 PQ 实现特点：在 Segment 级别独立训练码本。这意味着合并 Segment 时需要重新训练 PQ。

#### Binary Quantization（BQ）

```rust
// 最简单的量化：只保留符号位
// h_i = sign(vector[i])  // 1 if ≥0 else 0
// 1024d → 1024 bits = 128 bytes → 压缩 32×

// 搜索用 Hamming 距离（popcount(diff)）
// 通常作为候选粗筛：BQ 海选 Top-200 → 精确距离 Top-10
```

#### 量化选择决策树

```
你的数据量 < 100K vectors？
  ├─ 是 → on_disk=false + No Quantization（直接上 FP32）
  └─ 否 → 100K-1M: on_disk=false + SQ（平衡方案）
          1M+: on_disk=true + PQ（大容量方案）
          10M+: on_disk=true + BQ 粗筛 + 精确重排
```

对 G3 组当前阶段（初始 500 条法规，Phase 4 扩展到 100K），**不需要量化**。但你需要验证这个结论——跑 Benchmark 证明无量化方案在 100K 规模下仍然最佳。

---

### 3. Payload Indexing

Qdrant 不是纯向量数据库。它允许你在每个 Point 上附加任意 JSON Payload，并对 Payload 建索引用于过滤。

```
Point {
  id: 42,
  vector: [0.023, -0.145, ...],   // 1024d
  payload: {
    "law_name": "建筑业企业资质管理规定",
    "article": "第三条",
    "category": "法规",
    "effective_date": "2015-03-01",
    "keywords": ["资质", "建筑业", "施工总承包"],
  }
}
```

#### 索引类型

| 索引类型 | 适用 Payload 类型 | 数据结构 | 查询示例 |
|----------|-------------------|----------|----------|
| Keyword | String | 倒排索引 | `category = "法规"` |
| Integer | i64 | B-Tree 范围索引 | `article_number >= 20 AND article_number <= 30` |
| Float | f64 | B-Tree 范围索引 | `budget >= 1000000.0` |
| Geo | geo coordinates | R-Tree | `location near (116.4, 39.9) within 10km` |
| Full-text | String (长文本) | 分词 + BM18 | `text LIKE "%供应商资格%"` |
| Datetime | ISO 8601 string | B-Tree | `effective_date >= "2020-01-01"` |

#### 过滤时机：Pre-filtering vs Post-filtering

```rust
// Pre-filtering（Qdrant 默认）
// 1. 先对 Payload 条件求值 → 得到候选 Point ID 集合
// 2. 只在候选集合的向量上做 ANN 搜索
// 适合：过滤后结果集占总量 < 10%

// Post-filtering
// 1. 先对所有向量做 ANN 搜索 → 得到 Top-K
// 2. 再过滤掉不满足 Payload 条件的
// 适合：过滤后结果集占总量的 50%+，或过滤条件很宽松

// Qdrant 的选择逻辑（简化）：
// if filtered_ratio < threshold（默认 0.1）:
//     pre-filtering  ← 过滤掉了大部分，ANN 成本大幅降低
// else:
//     post-filtering ← 过滤没省多少，不如先搜再看
```

你需要实验验证：在 100K 法规中过滤 `category=法规`（匹配 50K，ratio=0.5）→ 对比 Pre vs Post QPS。

---

### 4. 稀疏向量与混合检索

#### BGE-M3 的 Sparse Embedding

BGE-M3 不仅输出 1024d Dense 向量，还输出 Sparse 向量：

```json
// Dense: [0.023, -0.145, 0.891, ...]  ← 1024 个 f32
// Sparse: {
//   "建筑工程": 0.85,      ← token → weight
//   "资质": 0.92,
//   "总承包": 0.78,
//   "二级": 0.65,
//   ...
// }
```

Sparse 向量的本质是"学出来的 TF-IDF"——不是人工设计的关键词权重，而是模型端到端学出来的词重要性。

#### 四种融合策略

```
Dense Only:     只用向量相似度            ← 语义匹配强，专有名词弱
Sparse Only:    只用关键词匹配（BM25 风格） ← 专有名词强，不会做语义扩展
RRF:            Σ 1/(k + rank_i)          ← 不需归一化，robust，业界标配
Linear:         α*dense + (1-α)*sparse    ← 需要调 α，但更灵活
```

**RRF（Reciprocal Rank Fusion）** 是当前最广泛使用的融合策略：

```
为什么 k=60？
  - k 越小，排名靠前的文档权重越大（排序位置差异更显著）
  - k 越大，排序位置差异的影响越小（更趋近均匀）
  - k=60 是 Cormack et al. (2009) 的经典值，在实践中表现最稳定
  
你的实验：
  跑 k=1, 10, 60, 300 对比同一批 query 的 Recall@10
```

---

## 动手

### 任务 1：Qdrant 部署与数据入库

```bash
docker run -p 6333:6333 -p 6334:6334 \
  -v $(pwd)/qdrant_storage:/qdrant/storage:z \
  qdrant/qdrant
```

用 Rust SDK 或 REST API（`POST /collections/{name}/points`）将 500+ 条法规向量 + Payload 入库。

### 任务 2：量化对比实验

创建 4 个 Collection：`laws_fp32` / `laws_sq` / `laws_pq` / `laws_bq`

同一份 100K 数据 → 各入库 → 同一批 50 条查询 → 对比：
- 存储大小（`GET /collections/{name}` 的 `points_count × vector_size`）
- P50/P99 QPS
- Recall@10（以 FP32 的 Top-10 为 ground truth）

### 任务 3：HNSW 参数扫描

固定数据 100K FP32，变化：
- M ∈ [8, 16, 32, 64]
- ef_construct ∈ [100, 200, 400]
- ef_search ∈ [64, 128, 256, 512]

4 × 3 × 4 = 48 组实验。每组跑 50 条查询 × 3 次取均值。输出 Recall@10 vs QPS CSV。

### 任务 4：混合检索对比

对同一批查询跑 4 种策略：
1. Dense only
2. Sparse only
3. RRF (k=60)
4. Linear (α=0.3, 0.5, 0.7)

输出每种策略的 Recall@10 并分析：
- 哪些查询 Dense 更好？（语义模糊但关键词明确 → Sparse 更好）
- 哪些查询 RRF 显著优于两者？
- RRF vs Linear 在什么情况下有差异？

### 任务 5：自动化 Benchmark 脚本

```rust
// benchmark.rs
// 读 experiments.csv → 遍历每组参数
//   → 创建/重建 Collection
//   → 插入数据（如果参数变化需要重建）
//   → 跑 50 条查询 × 3 次
//   → 记录：Recall@10, P50/P99 QPS, 存储大小
//   → 追加到 results.csv

// 然后 analysis.rs：
// → 从 results.csv 读数据
// → 画 Recall-QPS 散点图（终端 ASCII art 或 输出 JSON 给外部绘图）
// → 输出 Pareto 前沿配置列表
```

---

## 验收标准

- [ ] Qdrant Docker 启动 + 500 条法规数据入库（含完整 Payload）
- [ ] 量化对比报告：FP32/SQ/PQ/BQ 在 100K 规模下的存储/速度/精度
- [ ] HNSW 参数扫描报告：48 组实验数据 + 最优配置推荐
- [ ] 混合检索对比：4 种策略的 Recall@10 对比 + 按查询类别分析差异
- [ ] 自动化 Benchmark 脚本可复现全部实验
- [ ] Pareto 前沿分析：推荐 3 个配置（高性价比 / 高精度 / 高吞吐）

---

## 思考题

1. 你的 HNSW 参数扫描中，M=64 和 M=32 的 Recall 差异有多大？这个差异值不值得多花 2 倍内存？
2. Qdrant 的 Pre-filtering 在 `filtered_ratio = 0.01`（过滤后只剩 1% 数据）时 QPS 很高。但在标书审核场景，`category=法规` 可能匹配 50% 的数据——Pre-filtering 还有优势吗？
3. BGE-M3 的 Sparse 向量是在 Dense 的基础上额外输出的（同一个前向传播）。这意味着 Sparse 嵌入是"免费的"——为什么很多人仍然只用 Dense？（提示：想想 Qdrant 的 Sparse Vector 支持成熟度 + 维护两套索引的成本）

---

## 进阶挑战

- 实现"自适应 ef"：简单查询（query 短、关键词明确）用低 ef → 高 QPS；复杂查询（query 长、语义模糊）用高 ef → 高 Recall
- 用 `perf` 或 `flamegraph` 分析 Qdrant 搜索的热点函数，验证 Day 2 讲的 HNSW 计算瓶颈
- 对比 Qdrant 的 HNSW 实现和 Day 2 你的手写 HNSW 在相同数据下的性能差异（QPS、Recall、构建时间）

---

## 与标书审核项目的关系

G3 组的检索 API 默认使用你今天的实验结论——哪个 HNSW 配置最合适、Dense+Sparse 用什么融合策略。你今天跑的 Benchmark 报告，明天会成为 G3 组的调参文档。

**但场景在变**：初始 500 条法规 → Phase 3 扩展到 5000 条 → Phase 4 扩展到 100K 条。你在 500 条规模下的最优配置在 100K 规模下可能**不再最优**——M 需要增大（图密度跟随数据量），ef 需要降低（候选多了计算量更大）。跑全量 Benchmark 验证它。
