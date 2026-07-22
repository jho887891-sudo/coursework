# 任务 1 参考实现：EmbeddingEngine

## 快速开始

### 1. 准备模型文件

BGE-M3 模型需要从 HuggingFace 下载并导出为 ONNX 格式。讲师已提供模型文件，放到任意目录，确保包含：

```
模型目录/
├── model.onnx       # ONNX 模型（~2GB）
└── tokenizer.json   # HuggingFace tokenizer 配置
```

如果没有模型文件，用 optimum-cli 导出：

```powershell
pip install optimum[onnxruntime]
optimum-cli export onnx \
  --model BAAI/bge-m3 \
  --task sentence-similarity \
  ./bge-m3-onnx
```

### 2. 编译运行

```powershell
# 进入 demo 目录
cd 任务1参考

# 编译（首次编译会下载 ort-sys + tokenizers 的 native 依赖，需要几分钟）
cargo build --release

# 运行
cargo run --release -- <模型目录路径>

# 例如：
cargo run --release -- D:\models\bge-m3-onnx

# 自定义 batch size：
$env:EMBEDDING_BATCH_SIZE="16"
cargo run --release -- D:\models\bge-m3-onnx
```

### 3. 预期输出

```
=== 加载模型 ===
[load] ONNX 输入: ["input_ids", "attention_mask", "token_type_ids"]
[load] ONNX 输出: last_hidden_state

=== 测试 embedding ===
返回 3 条向量

文本 1: L2 norm = 1.000000  ✅ 归一化正常
  前 5 维: [0.0123, -0.0456, 0.0789, -0.0234, 0.0567]
文本 2: L2 norm = 1.000000  ✅ 归一化正常
  前 5 维: [0.0234, -0.0345, 0.0678, -0.0123, 0.0456]
文本 3: L2 norm = 1.000000  ✅ 归一化正常
  前 5 维: [-0.0890, 0.0123, -0.0345, 0.0678, -0.0123]

=== 相似度验证（不是任务要求，仅供参考）===
相似文本 "资质要求" vs "资质条款": cos = 0.8723
不相关文本 "资质要求" vs "预算金额": cos = 0.3124
✅ 相似文本的 cosine similarity 更高，embedding 质量正常

=== 任务 1 验收 ===
✅ EmbeddingEngine::load() 加载成功
✅ embed_batch() 返回 1024d 向量
✅ L2 norm ≈ 1.0（归一化正确）
```

---

## 代码导读

阅读 `src/main.rs` 时按顺序看，每个方法的注释解释了"为什么这么做"。

### 关键设计决策

#### 1. 为什么用 `Arc<EmbeddingEngineInner>`？

```rust
pub struct EmbeddingEngine {
    inner: Arc<EmbeddingEngineInner>,
}
```

要满足 `Send + Sync`。`ort::Session` 内部持有 C 指针，不同 ort 版本对 `Send+Sync` trait 的实现可能不同。用 `Arc` 统一解决：即使 Inner 里的某个字段不是 `Sync`，只要所有访问都走 `&self`（不可变引用），就是安全的。

**事实核查**：`ort::Session` 在 ort 2.x 实际上已经实现了 `Send + Sync`。这里的 `Arc` 有两个额外好处：
- `EmbeddingEngine` 可以廉价 clone（`Arc::clone` 只增加引用计数）
- 未来如果加缓存层（如 LRU tokenization cache），`Arc` 可以直接配合 `RwLock` 使用

#### 2. 动态 Padding vs 固定 Padding

```
固定 padding：所有 batch 都 pad 到模型最大长度（8192）
  → 一条 50 token 的文本要算 8192 - 50 = 8142 个 padding 位置
  → 浪费 ~99% 的计算

动态 padding：每个 batch pad 到该 batch 内最长文本的长度
  → 4 条文本，长度分别是 [32, 48, 55, 41]
  → 全部 pad 到 55
  → 大幅减少无效计算
```

`tokenize_batch()` 做的就是动态 padding。

#### 3. Mean Pooling 的 mask 处理

```rust
// 核心三行：
let mask_3d = mask_2d.insert_axis(Axis(2));  // [B, L] → [B, L, 1]
let masked = hidden_states * &mask_3d;        // padding 位置自动归零
let sum = masked.sum_axis(Axis(1));           // 沿 token 维求和
let count = mask_2d.sum_axis(Axis(1));        // 每个样本的有效 token 数
let mean = sum / count;                       // 逐行除以有效 token 数
```

几何直觉：不是对整个序列取平均，是只对**真实 token** 取平均。padding 的 `[PAD]` token 本身也有一个 embedding 向量，如果不 mask 掉，它会拖偏句子向量。

#### 4. IO Binding 的预分配逻辑

```rust
// 输入：把 ndarray 包装成 ort::Value（零拷贝）
let input_val = Value::from_array(&allocator, &input_ids)?;

// 输出：提前分配好内存，runtime 直接往里写
let output_val = Value::new_f32(&allocator, &[batch, seq_len, 1024])?;
```

对比普通 `session.run()`：
- `session.run()` 每次内部 malloc + 返回时 memcpy → ~10ms/次
- IO Binding 预分配，直接读写 → ~3ms/次

在批量推理（每小时几十万次）中，这个差异会累积成显著的延迟下降。

---

## 环境问题排查

### 编译错误：找不到 `onnxruntime`

`ort` crate 依赖 ONNX Runtime C 库。Windows 上的常见解决方式：

```
# 方式一：让 ort-sys 自动下载（推荐）
# Cargo.toml 中 ort 默认 features 已包含自动下载逻辑
cargo clean && cargo build

# 方式二：手动指定 ONNX Runtime 路径
$env:ORT_LIB_LOCATION = "C:\Program Files\onnxruntime"
cargo build
```

### 运行时报错：找不到 onnxruntime.dll

将 ONNX Runtime 的 `lib/` 目录加入 PATH，或把 DLL 复制到 `target/release/` 同级目录。

### tokenizer.json 加载失败

确认文件路径和文件名。BGE-M3 的 tokenizer 来自 HuggingFace `BAAI/bge-m3` 仓库的 `tokenizer.json`。

---

## 后续任务怎么做

### 任务 2：验证语义表征能力（Spearman 相关系数）

你需要用刚写好的 `EmbeddingEngine.embed_batch()` 来算。步骤：

1. **准备数据**：构造 100 对标书文本对 `(text_a, text_b, human_score)`。人工标注相似度 0-1。建议覆盖：完全等价、部分重叠、完全无关三种情况。

   ```rust
   struct TextPair {
       a: String,
       b: String,
       human_score: f32,  // 0.0 ~ 1.0
   }
   ```

2. **计算向量相似度**：
   ```rust
   let emb_a = engine.embed_batch(&[&pair.a])?;
   let emb_b = engine.embed_batch(&[&pair.b])?;
   let cosine = dot_product(&emb_a[0], &emb_b[0]);
   ```

3. **算 Spearman ρ**：把 100 个 `(cosine, human_score)` 对输入 Spearman 公式，或者直接用 `statrs` crate：
   ```rust
   // Cargo.toml 加 statrs = "0.17"
   use statrs::statistics::rank;
   let rho = rank::spearman_rho(&cosines, &human_scores)?;
   ```

4. **目标**：ρ > 0.80。如果不够，检查：人工标注是否一致？文本对是否覆盖足够的难度梯度？

### 任务 3：对比三种 Pooling 策略

在 `main.rs` 的 `mean_pooling()` 旁边加两个新函数：

```rust
fn cls_pooling(hidden_states: &Array3<f32>) -> Array2<f32> {
    // 取 [CLS] token（位置 0）的向量，做 L2 normalize
    let cls = hidden_states.slice(s![.., 0, ..]);  // [batch, 1024]
    // ... L2 normalize 每行
}

fn max_pooling(hidden_states: &Array3<f32>, attention_mask: &Array2<i64>) -> Array2<f32> {
    // 每个维度取最大值（padding 位置设为 -∞）
    // ... L2 normalize 每行
}
```

对同一批数据分别跑三种方法，用表格对比相似度分布。注意思考题的提示：标书场景下，关键差异往往在几个词（"必须" vs "宜"），Max Pooling 可能更适合捕获这种显著特征。

### 任务 4：Benchmark 报告

核心是测 `embed_batch()` 在不同 batch size 下的吞吐量：

```rust
use std::time::Instant;

for bs in [1, 4, 16, 32, 64] {
    // 构造 bs 条示例文本（共跑 N 次取平均）
    let texts: Vec<String> = (0..bs).map(|i| format!("测试文本 {}", i)).collect();
    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

    let start = Instant::now();
    let n_rounds = 100;
    for _ in 0..n_rounds {
        engine.embed_batch(&text_refs)?;
    }
    let elapsed = start.elapsed();
    let sentences_per_sec = (bs * n_rounds) as f64 / elapsed.as_secs_f64();

    println!("batch={bs:>3}: {sentences_per_sec:.0} sentences/s");
}
```

画出吞吐量/延迟曲线（Excel 或 matplotlib 都行），找出最优 batch size——通常是吞吐量不再线性增长的那个拐点。

---

## 依赖版本说明

| crate | 版本 | 作用 |
|-------|------|------|
| `ort` | 2.0.0-rc.12 | ONNX Runtime Rust 绑定，执行模型推理 |
| `tokenizers` | 0.23 | HuggingFace tokenizers 的 Rust 绑定 |
| `ndarray` | 0.17 | Rust 的 numpy——多维数组运算（**必须和 ort 依赖的版本一致**） |
| `anyhow` | 1 | 错误处理（前面 Rust 课已经用过） |

### 已知坑点（已在本代码中解决）

以下是 `ort` 2.0.0-rc.12 和讲义中伪代码的 API 差异，已在 `main.rs` 中修正：

1. **`GraphOptimizationLevel` 路径**：实际是 `ort::session::builder::GraphOptimizationLevel`，不是 `ort::GraphOptimizationLevel`
2. **`Tensor::from_array`**：只接受一个参数（ndarray），不需要传 allocator。返回的是 `Tensor<T>` 类型（= `Value<TensorValueType<T>>`）
3. **预分配输出**：`Tensor::<f32>::new(&allocator, [dims])` 而不是 `Value::new_f32`
4. **`session` 需要 `&mut self`**：`run_binding()` 要求 `&mut self`，因此 session 必须用 `Mutex` 包装（即使内部是线程安全的）
5. **Session builder 的 `?` 不兼容 anyhow**：`Session::builder()?` 返回 `ort::Error<SessionBuilder>`，因为 `SessionBuilder` 含 `NonNull` 指针（`!Send + !Sync`），无法自动转为 `anyhow::Error`。需要手动 `.map_err()`
6. **`AllocatorType::Arena`**：不是 `DeviceArena`
7. **`ndarray` 版本必须和 ort 一致**：ort 2.0.0-rc.12 用 ndarray 0.17，你的项目也必须是 0.17，否则 `extract_array` 等方法的类型不匹配
8. **Tokenizer error 类型**：`Tokenizer::from_file()` 返回 `Box<dyn Error>`，和 anyhow 不兼容，需要 `.map_err()`

**如果 ort 更新了版本**（例如 2.0.0 正式版发布），需要重新验证这些 API。核心方法是：`cargo check` 报什么错就修什么，参考本 README 的踩坑记录快速定位。
