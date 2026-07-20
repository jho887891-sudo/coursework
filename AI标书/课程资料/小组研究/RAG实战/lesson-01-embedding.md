# Day 1：Embedding 原理深挖 — 从 Token 到 1024 维向量

> 计算机怎么"理解""建筑工程施工总承包资质"和"建筑业企业资质要求"是同一个意思？把文字变成向量，让相似的文本在空间中靠近。

---

## 学习目标

1. 理解 BPE Tokenizer 的训练算法和中文适配机制
2. 理解 Transformer 从 Token 到 Embedding 的完整数据流
3. 掌握三种 Pooling 策略的几何差异和适用场景
4. 实现带 IO Binding 优化的 ONNX 推理引擎
5. 在标书领域文本对上验证语义表征能力（Spearman 相关系数）

---

## 核心概念

### 1. Tokenizer — 文本怎么变成数字

#### 为什么需要 Tokenizer

LLM 不能直接"读"文字。它只能处理数字。Tokenizer 的工作就是把任意文本 → 整数序列。

```
"建筑工程" → [5632, 1897]  // 两个 token ID
"资质"     → [8945]        // 一个 token ID
```

#### BPE（Byte Pair Encoding）训练算法

BPE 从字节级开始，统计所有相邻 token pair 的出现频率，合并最高频的 pair，重复直到 vocab 达到目标大小。

```
训练流程（简化）：
初始 vocab: {a, b, c, ..., 建, 筑, 工, 程, ...}
第1轮: "建"+"筑" 出现 10000 次 → 合并为 "建筑"
第2轮: "建筑"+"工" 出现 8000 次 → 合并为 "建筑工"
第3轮: "建筑工"+"程" 出现 8000 次 → 合并为 "建筑工程"
...直到 vocab_size = 250000（BGE-M3 的实际 vocab 大小）
```

这解释了为什么"建筑工程"是一个 token：它在训练语料中出现频率极高，BPE 自动把它合并了。

#### SentencePiece 与中文

大多数 LLM 用 SentencePiece 实现 BPE。中文的特殊性：没有天然空格分隔。

- 英文："The construction project" → `["The", " construction", " project"]`（天然按空格分）
- 中文："建筑工程施工" → 如果直接 BPE，需要自己发现词的边界

SentencePiece 直接在 Unicode 字符级做 BPE，不依赖预分词（pre-tokenization），因此对中文更友好。

#### 现场验证：Tokenization 的不可逆性

```rust
let text = "建筑工程施工总承包二级资质";
let tokens = tokenizer.encode(text).get_ids();
let decoded = tokenizer.decode(&tokens, true)?;
// decoded 应该是 "建筑工程施工总承包二级资质"（理想情况）
// 但有时候 encode → decode → encode ≠ 原来的 tokens
// 这是 Unicode NFKC 归一化的坑！例如全角数字 １ → 半角 1
```

#### 为什么你不需要手写 Tokenizer

HuggingFace 的 `tokenizers` crate 已经实现了完整的 BPE + SentencePiece。你只需要：

```rust
use tokenizers::Tokenizer;

let tokenizer = Tokenizer::from_file("bge-m3/tokenizer.json")?;
let encoding = tokenizer.encode("建筑工程施工总承包", false)?;
// encoding.get_ids() → [5632, 1897, 8945, 3456, ...]
// encoding.get_attention_mask() → [1, 1, 1, 1, ...]
```

但理解 BPE 的原理，让你知道为什么同一个词可能有多种 tokenization（不同的上下文里切分方式不同）。

---

### 2. Transformer Embedding — Token 向量怎么变成句子向量

#### 第一步：三种 Embedding 相加

每个输入 token 的初始表示 = 三种 Embedding 逐元素相加：

```
Token Embedding:    每个 token ID 对应的可学习向量 (1024d)
Position Embedding: 第 0 个 token → pos_0 向量，第 1 个 → pos_1 向量...
Segment Embedding:  segment A → seg_A 向量，segment B → seg_B 向量（BGE-M3 不用）

Input_i = TokenEmb(token_i) + PositionEmb(i)
```

为什么加法可行？高维空间中，近似正交的向量加法不破坏各自的信息。1024 维空间中，两个随机向量的 cosine similarity 期望接近 0——它们几乎正交。

#### 第二步：Self-Attention — 让 token 看到彼此

```
Q = Input × W_Q    （Query：我在找什么）
K = Input × W_K    （Key：我有什么）
V = Input × W_V    （Value：我的内容是什么）

Attention(Q, K, V) = softmax(Q·K^T / √d_k) × V
```

几何直觉：
- Q·K^T 是"投票矩阵"——第 i 行第 j 列是 token_i 对 token_j 的相关性打分
- softmax 把打分变成"注意力权重"（和为 1）
- 乘 V 是"按注意力权重聚合信息"

经过 12 层（BGE-M3 是 BERT-base，12 层 Transformer）的反复 Attention，浅层学习局部句法，深层学习全局语义。最后一层的 Hidden State 包含了最丰富的语义信息，因此用它做 Embedding。

#### 第三步：Pooling — 把 token 向量汇聚成句子向量

最后一层输出是 `[CLS] [tok_1] [tok_2] ... [tok_N] [SEP]`，每个 token 一个 1024 维向量。Pooling 把这些向量合并为一个句子向量。

**三种策略的几何含义**：

```
Mean Pooling（BGE-M3 使用）:
  embedding = mean(h[1], h[2], ..., h[N])  // 去掉 [CLS] 和 [SEP]，对有效 token 取均值
  几何：所有 token 向量的质心。最稳定，不偏向任何特定 token。

CLS Token（BERT 原始设计）:
  embedding = h[0]  // [CLS] token 的向量
  几何：训练时 [CLS] 被设计为"聚合整个句子信息"的哨兵 token。
  注意：MLM 预训练目标下 CLS 可能方向不明确（它不直接参与 masked token 预测）

Max Pooling:
  embedding_j = max(h_1j, h_2j, ..., h_Nj)  // 每个维度取最大值
  几何：每个维度保留最显著的特征。优点是保留最突出的信号，缺点是丢失细粒度信息。
  适用场景：关键词匹配任务（如"建筑资质"这个短语的显著特征会被捕获）
```

#### 第四步：L2 Normalize — 投影到单位超球面

```
embedding_normalized = embedding / ||embedding||_2
```

为什么要做这一步？因为归一化后：

```
cosine_similarity(u, v) = (u·v) / (||u|| * ||v||) = u_normalized · v_normalized
```

余弦相似度退化为点积——计算更快（少除一次），且向量数据库（Qdrant）默认用 DotProduct。

---

### 3. ONNX Runtime 推理优化

#### 为什么用 ONNX

PyTorch 模型 → ONNX 导出 → 跨框架推理。ONNX Runtime 的优化比直接用 PyTorch 推理快 2-5 倍（Graph Optimization + 内存池 + CPU 指令集加速）。

#### Graph Optimization Levels

```
Level 1 (Basic):      常量折叠、冗余节点消除
Level 2 (Extended):   算子融合（如 Conv+BN+ReLU → 单个融合算子）
Level 99 (All):       Layout Optimization（NHWC → NCHW 内存布局优化）+ 所有拓展优化
```

BGE-M3 的 ONNX 模型用 Level 2 即可，Level 99 的 Layout Optimization 主要针对 CNN，对 Transformer 效果有限。

#### INT8 量化原理

```
FP32:  value ∈ [-3.4e38, 3.4e38], 32 bit per value
INT8:  value ∈ [-128, 127], 8 bit per value

量化公式：q = round((v - zero_point) / scale)
反量化：  v' = q * scale + zero_point

量化的关键是找 scale 和 zero_point：
  对称量化：zero_point = 0, scale = max(|v_max|, |v_min|) / 127
  非对称量化：zero_point = round(-v_min / scale), scale = (v_max - v_min) / 255
```

大模型的参数分布接近正态分布，INT8 量化引入的噪声（量化误差 max = scale/2）在 1024 维向量中通过维度的平均效应被稀释。这就是为什么 BGE-M3 INT8 精度损失 < 0.5%——量级上，0.5% 的余弦相似度偏差在 Recall@10 上几乎不可见。

#### IO Binding：内存预分配

```rust
// ❌ 每次推理都分配新内存
let outputs = session.run(inputs)?;  // malloc + memcpy 每次 ~10ms

// ✅ IO Binding 预分配
let io_binding = session.create_binding();
io_binding.bind_input("input_ids", &input_ids_buffer)?;      // 预分配的 buffer
io_binding.bind_output("last_hidden_state", &mut output_buffer)?;  // 预分配
session.run_with_binding(&io_binding)?;  // 直接读写预分配内存，~3ms
```

在批量推理中，IO Binding 减少的 malloc 开销尤为显著。你的 EmbeddingEngine 必须用 IO Binding。

---

## 动手

### 任务 1：实现 EmbeddingEngine

```rust
pub struct EmbeddingEngine {
    tokenizer: Tokenizer,          // HuggingFace tokenizers
    session: Session,              // ONNX Runtime
    allocator: OrtAllocator,       // IO Binding 用
}

impl EmbeddingEngine {
    /// 加载 ONNX 模型 + Tokenizer，配置 IO Binding
    pub fn load(model_path: &str) -> Result<Self>;

    /// 批量 embedding，返回 L2-normalized 的 1024d 向量
    /// - texts: 输入文本列表
    /// - 内部自动 batch（batch_size 从环境变量读取，默认 32）
    /// - 自动 padding 到 batch 内最长文本
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<[f32; 1024]>>;
}
```

关键实现细节：
1. **Mean Pooling 的 mask 处理**：padding token 位置不参与平均。你需要用 `attention_mask` 来过滤
2. **L2 Normalize**：Mean Pooling 后手动 normalize，不要依赖模型的输出（模型输出可能已归一化，但加一层显式 normalize 更安全）
3. **线程安全**：`EmbeddingEngine` 需要 `Send + Sync`。用 `Arc<EmbeddingEngineInner>` 或直接确保 `Session` 和 `Tokenizer` 都是 Send+Sync

### 任务 2：验证语义表征能力

构建 100 对标书领域文本对，人工标注相似度（0-1）：

```
标书条款对示例：
  "投标人须具备建筑工程施工总承包二级及以上资质"  vs  "施工总承包资质要求：二级及以上"  → 0.95
  "投标人须具备建筑工程施工总承包二级及以上资质"  vs  "项目预算为人民币 500 万元整"    → 0.05
```

用你的 `EmbeddingEngine` 计算每对文本的 cosine similarity，与人工标注计算 Spearman 相关系数。目标：ρ > 0.80。

### 任务 3：对比三种 Pooling 策略

用同一份数据，分别跑 Mean / CLS / Max Pooling，对比 Recall@10（需要 Day 4 的评测框架，这里只需要输出原始 similarity 数据即可）。

### 任务 4：Benchmark 报告

| Batch Size | Sentences/s | P50 Latency | P99 Latency | Memory (MB) |
|------------|------------|-------------|-------------|-------------|
| 1          | ?          | ?           | ?           | ?           |
| 4          | ?          | ?           | ?           | ?           |
| 16         | ?          | ?           | ?           | ?           |
| 32         | ?          | ?           | ?           | ?           |
| 64         | ?          | ?           | ?           | ?           |

画出吞吐量/延迟曲线，找出最优 batch size。

---

## 验收标准

- [ ] `EmbeddingEngine::load()` 成功加载 BGE-M3 ONNX 模型，IO Binding 配置正确
- [ ] `embed_batch()` 输出 1024 维 L2-normalized 向量（验证：`||v||_2 ≈ 1.0`）
- [ ] 100 对标书文本对 Spearman ρ > 0.80
- [ ] Benchmark 报告含吞吐量/延迟曲线 + 最优 batch size 分析
- [ ] 对比报告：三种 Pooling 策略的相似度分布差异

---

## 思考题

1. BGE-M3 的 Position Embedding 最大支持 8192 个 token。如果一篇标书的条款超过这个长度会发生什么？怎么处理长文本？（提示：想想 Chunking 和位置编码的扩展方案，如 RoPE 的 extrapolation）
2. 你对比了 Mean/CLS/Max Pooling。在标书检索场景下，哪种可能最好？为什么？（提示：标书条款的关键差异往往在几个关键词上——"必须"vs"宜"、"二级"vs"一级"）
3. INT8 量化在 1024 维向量上精度损失 < 0.5%。如果维度是 128，损失会变大还是变小？为什么？（提示：大数定律——维度越多，量化误差越分散）

---

## 进阶挑战

- 实现动态批处理（Dynamic Batching）：不等攒够 batch_size，最多等 5ms 就执行推理
- 对比 INT8 量化前后的精度损失（Spearman ρ 和 P99 latency 的 trade-off 曲线）
- 实现 Tokenization 缓存：相同文本不重复 tokenize（LRU cache，capacity=10000）
- 用 Rayon 并行批量 embedding：`texts.par_chunks(batch_size).map(|batch| engine.embed_batch(batch))`

---

## 与标书审核项目的关系

你刚写的 `EmbeddingEngine` → G3 知识检索组的向量化模块。G3 的每一步检索都始于：用户查询 → EmbeddingEngine.embed() → 1024d 向量 → Qdrant 搜索。

**标书场景的特殊性**：
- 条款编号（"第22条"）的 Tokenization 必须一致——如果"第22条"有时被切成 `[第, 22, 条]` 有时是 `[第22条]`，同一条款的语义匹配就废了
- 数字的嵌入质量直接影响金额验证——"500万元"和"5000000元"的向量应该接近（但 BGE-M3 可能做不到，这就是为什么后面需要 Sparse 检索和规则引擎来兜底）
