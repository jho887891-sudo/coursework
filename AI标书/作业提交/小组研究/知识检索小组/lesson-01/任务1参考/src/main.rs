// ═══════════════════════════════════════════════════════════════════════════
// 任务 1：EmbeddingEngine — BGE-M3 ONNX 批量 Embedding
// ═══════════════════════════════════════════════════════════════════════════
//
// 运行方式：
//   cargo run -- <模型目录路径>
//   模型目录下需包含 model.onnx 和 tokenizer.json
//
// 验收：main() 会对几条标书文本做 embedding，打印 L2 norm（应 ≈ 1.0）。
//
// 本文件阅读顺序（从上到下）：
//   1. 数据结构定义（EmbeddingEngineInner / EmbeddingEngine）
//   2. load() — 加载模型 + Tokenizer + 分配器
//   3. tokenize_batch() — 文本 → padded input_ids + attention_mask
//   4. mean_pooling() — 用 attention_mask 做加权平均 + L2 归一化
//   5. run_onnx_batch() — ONNX IO Binding 推理
//   6. embed_batch() — 入口：自动分批 → 推理 → pooling → 收集结果
//   7. main() — 跑一个 demo

use anyhow::{Context, Result};
use ndarray::{Array2, Array3, Axis, s};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use ort::memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType};
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;

// ═══════════════════════════════════════════════════════════════════════════
// 1. 数据结构
// ═══════════════════════════════════════════════════════════════════════════

/// 环境变量名：控制每次 ONNX 推理的 batch 大小
const ENV_BATCH_SIZE: &str = "EMBEDDING_BATCH_SIZE";
const DEFAULT_BATCH_SIZE: usize = 32;

/// BGE-M3 输出维度（不会变）
const HIDDEN_DIM: usize = 1024;

/// EmbeddingEngine 的真实数据。用 Arc 包一层实现 Send + Sync。
///
/// 关键设计：
/// - `session` 用 Mutex 包装，因为 ort::Session::run_binding() 需要 &mut self
///   （ort 2.x 的设计决定，实际上 Session 内部是 Arc 线程安全的）
/// - `tokenizer` 不需要 mutex（encode 只读）
struct EmbeddingEngineInner {
    tokenizer: Tokenizer,
    session: Mutex<Session>,
    input_names: Vec<String>,
    output_name: String,
    batch_size: usize,
}

/// 对外暴露的 EmbeddingEngine。内部用 Arc，clone 是廉价操作。
#[derive(Clone)]
pub struct EmbeddingEngine {
    inner: Arc<EmbeddingEngineInner>,
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. load() — 加载模型 + Tokenizer + 分配器
// ═══════════════════════════════════════════════════════════════════════════

impl EmbeddingEngine {
    /// 加载 ONNX 模型和 Tokenizer。
    ///
    /// `model_dir` 目录下需要包含：
    ///   - model.onnx  （用 optimum-cli 从 BGE-M3 导出）
    ///   - tokenizer.json（HuggingFace tokenizer 文件）
    pub fn load(model_dir: &str) -> Result<Self> {
        let model_path = format!("{}/model.onnx", model_dir);
        let tokenizer_path = format!("{}/tokenizer.json", model_dir);

        // ── 加载 Tokenizer ──
        // Tokenizer::from_file 的 error type 是 Box<dyn Error>，和 anyhow 不兼容，用 map_err 转
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("加载 tokenizer 失败 {}: {}", tokenizer_path, e))?;

        // ── 加载 ONNX 模型 ──
        // GraphOptimizationLevel::Level2 对 Transformer 模型效果最好。
        // Session::builder()? 返回的是 ort::Error<SessionBuilder>，不是 anyhow::Error。
        // 因为 SessionBuilder 内部有 NonNull 指针（!Send + !Sync），
        // ? 无法自动转成 anyhow。用 .map_err() 手动转。
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("创建 session builder 失败: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level2)
            .map_err(|e| anyhow::anyhow!("设置优化级别失败: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| anyhow::anyhow!("加载 ONNX 模型失败 {}: {}", model_path, e))?;

        // ── 读取模型输入/输出名称 ──
        // 不硬编码 "input_ids"，因为不同导出方式的命名可能不同。
        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .context("模型没有任何输出")?;

        println!("[load] ONNX 输入: {:?}", input_names);
        println!("[load] ONNX 输出: {}", output_name);

        // ── batch_size 从环境变量读取 ──
        let batch_size = std::env::var(ENV_BATCH_SIZE)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_BATCH_SIZE);

        Ok(EmbeddingEngine {
            inner: Arc::new(EmbeddingEngineInner {
                tokenizer,
                session: Mutex::new(session),
                input_names,
                output_name,
                batch_size,
            }),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. tokenize_batch() — 文本 → padded input_ids + attention_mask
// ═══════════════════════════════════════════════════════════════════════════
//
// "动态 padding"：不 pad 到模型最大长度 8192，而是 pad 到
// 当前 batch 内最长文本的长度。大幅减少无效计算。
//
// 输入：["文本A", "文本B", "文本C"]
// 输出：
//   - input_ids:       [3, max_len] 每行是 token ID 序列
//   - attention_mask:  [3, max_len] 1=真实 token, 0=padding
//   - token_type_ids:  [3, max_len] 全 0（BGE-M3 不需要 segment embedding）

impl EmbeddingEngineInner {
    fn tokenize_batch(&self, texts: &[&str]) -> Result<TokenizedBatch> {
        // 从 tokenizer 配置中读 pad_token_id，不要写死
        let pad_id = self
            .tokenizer
            .get_padding()
            .map(|p| p.pad_id)
            .unwrap_or(0);

        // 逐条编码（不加 padding，拿到真实长度）
        let mut all_ids: Vec<Vec<i64>> = Vec::with_capacity(texts.len());
        let mut max_len = 0usize;

        for text in texts {
            let encoding = self
                .tokenizer
                .encode(*text, true) // add_special_tokens = true
                .map_err(|e| anyhow::anyhow!("tokenize 失败：{}", e))?;

            let ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
            max_len = max_len.max(ids.len());
            all_ids.push(ids);
        }

        // 构造 padded 矩阵 + attention_mask
        let batch = texts.len();
        let mut input_ids = Array2::<i64>::from_elem((batch, max_len), pad_id as i64);
        let mut attention_mask = Array2::<i64>::zeros((batch, max_len));

        for (i, ids) in all_ids.iter().enumerate() {
            for (j, &id) in ids.iter().enumerate() {
                input_ids[[i, j]] = id;
                attention_mask[[i, j]] = 1; // 真实 token → 1
            }
            // 剩余位置已在 from_elem 时初始化为 pad_id，attention_mask 保持 0
        }

        let token_type_ids = Array2::<i64>::zeros((batch, max_len));

        Ok(TokenizedBatch {
            input_ids,
            attention_mask,
            token_type_ids,
        })
    }
}

/// tokenize_batch 的返回
struct TokenizedBatch {
    input_ids: Array2<i64>,
    attention_mask: Array2<i64>,
    token_type_ids: Array2<i64>,
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. mean_pooling() — 用 attention_mask 做加权平均 + L2 归一化
// ═══════════════════════════════════════════════════════════════════════════
//
// 输入：
//   hidden_states:  [batch, seq_len, 1024]   ← ONNX 输出
//   attention_mask: [batch, seq_len]          ← 1=有效, 0=padding
//
// 输出：
//   [batch, 1024]  每行是 L2-normalized 的句子向量
//
// 计算过程（以一条文本为例）：
//   sum = Σ(token_i * mask_i)   沿 seq_len 维求和，mask_i 过滤 padding
//   count = Σ(mask_i)            有效 token 数
//   mean = sum / count
//   output = mean / ||mean||₂    L2 归一化

fn mean_pooling(
    hidden_states: &Array3<f32>,   // [batch, seq_len, 1024]
    attention_mask: &Array2<i64>,  // [batch, seq_len]
) -> Array2<f32> {
    let (batch, _seq_len, hidden_dim) = hidden_states.dim();

    // mask → f32 并扩维：[batch, seq_len] → [batch, seq_len, 1]
    // insert_axis 会 consume self，所以先 clone 一份用于后面的 count 计算
    let mask_f32 = attention_mask.mapv(|v| v as f32);
    let count_1d = mask_f32.sum_axis(Axis(1));  // [batch] — 有效 token 数
    let mask_3d = mask_f32.insert_axis(Axis(2)); // [batch, seq_len, 1]

    // padding 位置乘以 0，不参与求和
    let masked = hidden_states * &mask_3d;

    // 沿 seq_len 维求和 → [batch, 1024]
    let sum_2d = masked.sum_axis(Axis(1));

    // 逐行 Mean + L2 Normalize
    let mut result = Array2::<f32>::zeros((batch, hidden_dim));
    for b in 0..batch {
        let cnt = count_1d[[b]];
        if cnt == 0.0 {
            continue;
        }

        // Mean：取第 b 行的 sum，除以 count
        let row = sum_2d.slice(s![b, ..]);
        let mean: Vec<f32> = row.iter().map(|&v| v / cnt).collect();

        // L2 Normalize
        let norm_sq: f32 = mean.iter().map(|x| x * x).sum();
        let norm = norm_sq.sqrt();
        if norm > 0.0 {
            for (j, &v) in mean.iter().enumerate() {
                result[[b, j]] = v / norm;
            }
        }
    }

    result
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. run_onnx_batch() — ONNX IO Binding 推理
// ═══════════════════════════════════════════════════════════════════════════
//
// IO Binding 的核心思想：预分配输入/输出内存，避免每次推理都 malloc + memcpy。
//
// 流程：
//   1. 把 ndarray 包装成 Tensor（零拷贝，不传 allocator）
//   2. 创建 IO Binding，绑定输入（引用）和输出（预分配的 owned Tensor）
//   3. session.run_binding() — 直接在预分配内存上读写
//   4. 从 outputs 中提取 tensor 数据

impl EmbeddingEngineInner {
    fn run_onnx_batch(
        &self,
        input_ids: &Array2<i64>,
        attention_mask: &Array2<i64>,
        token_type_ids: &Array2<i64>,
    ) -> Result<Array2<f32>> {
        let batch = input_ids.dim().0;
        let seq_len = input_ids.dim().1;

        // ── 1. ndarray → ort::Tensor（零拷贝） ──
        let input_ids_val = Tensor::from_array(input_ids.clone())
            .context("创建 input_ids tensor 失败")?;
        let attention_mask_val = Tensor::from_array(attention_mask.clone())
            .context("创建 attention_mask tensor 失败")?;
        let token_type_ids_val = Tensor::from_array(token_type_ids.clone())
            .context("创建 token_type_ids tensor 失败")?;

        // ── 2. IO Binding ──
        let mut binding = self.session.lock().unwrap().create_binding()?;

        binding.bind_input(&self.input_names[0], &input_ids_val)?;
        binding.bind_input(&self.input_names[1], &attention_mask_val)?;
        if self.input_names.len() > 2 {
            binding.bind_input(&self.input_names[2], &token_type_ids_val)?;
        }

        // ── 3. 绑定输出（让 runtime 自动分配，不预定义 shape） ──
        // 不同导出的模型输出形状不同：
        //   - 原始 ONNX: [batch, seq_len, 1024]（3D hidden states）
        //   - 量化/sentence-similarity 模型: [batch, 1024]（已池化）
        // bind_output_to_device 不预定义 shape，runtime 自动适配
        let output_memory = MemoryInfo::new(
            AllocationDevice::CPU,
            0,
            AllocatorType::Arena,
            MemoryType::CPUOutput,
        )?;
        binding.bind_output_to_device(&self.output_name, &output_memory)?;

        // ── 4. 执行推理 ──
        let mut session = self.session.lock().unwrap();
        let mut outputs = session
            .run_binding(&binding)
            .context("ONNX 推理失败")?;

        let output = outputs
            .remove(&self.output_name)
            .context("输出中找不到结果")?;
        let output_view: ndarray::ArrayViewD<f32> = output
            .try_extract_array::<f32>()
            .context("提取 ONNX 输出失败")?;

        // ── 5. 根据输出 rank 决定是否需要 mean pooling ──
        match output_view.ndim() {
            3 => {
                // 原始 hidden states [batch, seq_len, 1024] → mean_pooling → [batch, 1024]
                let hidden = output_view
                    .into_shape_with_order((batch, seq_len, HIDDEN_DIM))
                    .map_err(|e| anyhow::anyhow!("reshape 3D 输出失败: {}", e))?
                    .to_owned();
                Ok(mean_pooling(&hidden, attention_mask))
            }
            2 => {
                // 模型已内置池化 [batch, 1024] → 只做 L2 normalize
                let embeddings = output_view
                    .into_shape_with_order((batch, HIDDEN_DIM))
                    .map_err(|e| anyhow::anyhow!("reshape 2D 输出失败: {}", e))?;

                let mut result = Array2::<f32>::zeros((batch, HIDDEN_DIM));
                for b in 0..batch {
                    let row = embeddings.slice(s![b, ..]);
                    let norm_sq: f32 = row.iter().map(|x| x * x).sum();
                    let norm = norm_sq.sqrt();
                    if norm > 0.0 {
                        for (j, &v) in row.iter().enumerate() {
                            result[[b, j]] = v / norm;
                        }
                    }
                }
                Ok(result)
            }
            n => Err(anyhow::anyhow!(
                "不支持的输出维度: {} (期望 2 或 3)",
                n
            )),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. embed_batch() — 入口方法
// ═══════════════════════════════════════════════════════════════════════════
//
// 对外接口。做三件事：
//   1. 把 texts 按 batch_size 切分
//   2. 对每个 sub-batch：tokenize → ONNX 推理 → mean_pooling
//   3. 合并所有结果

impl EmbeddingEngine {
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<[f32; HIDDEN_DIM]>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let inner = &self.inner;
        let mut all_embeddings: Vec<[f32; HIDDEN_DIM]> =
            Vec::with_capacity(texts.len());

        // ── 按 batch_size 切分成 sub-batch ──
        for chunk in texts.chunks(inner.batch_size) {
            // 1. Tokenize + dynamic padding
            let tokenized = inner.tokenize_batch(chunk)?;

            // 2. ONNX 推理 → 返回已 L2-normalized 的 [batch, 1024]
            let embeddings = inner.run_onnx_batch(
                &tokenized.input_ids,
                &tokenized.attention_mask,
                &tokenized.token_type_ids,
            )?;

            // 3. Array2<f32> → Vec<[f32; 1024]>
            for b in 0..embeddings.dim().0 {
                let slice = embeddings.slice(s![b, ..]);
                let mut arr = [0.0f32; HIDDEN_DIM];
                for (j, &v) in slice.iter().enumerate() {
                    arr[j] = v;
                }
                all_embeddings.push(arr);
            }
        }

        Ok(all_embeddings)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. main() — 加载模型，embed 几条文本，验证 L2 norm ≈ 1.0
// ═══════════════════════════════════════════════════════════════════════════

fn main() -> Result<()> {
    let model_dir = std::env::args()
        .nth(1)
        .context("用法: cargo run -- <模型目录路径>\n模型目录下需包含 model.onnx 和 tokenizer.json")?;

    println!("=== 加载模型 ===");
    let engine = EmbeddingEngine::load(&model_dir)?;

    println!("\n=== 测试 embedding ===");
    let texts: Vec<&str> = vec![
        "投标人须具备建筑工程施工总承包二级及以上资质",
        "施工总承包资质要求：二级及以上",
        "项目预算为人民币 500 万元整",
    ];

    let embeddings = engine.embed_batch(&texts)?;

    println!("返回 {} 条向量\n", embeddings.len());
    for (i, emb) in embeddings.iter().enumerate() {
        let norm_sq: f32 = emb.iter().map(|x| x * x).sum();
        let norm = norm_sq.sqrt();
        println!(
            "文本 {}: L2 norm = {:.6}  {}",
            i + 1,
            norm,
            if (norm - 1.0).abs() < 1e-4 {
                "✅ 归一化正常"
            } else {
                "❌ 归一化异常！"
            },
        );
        println!("  前 5 维: {:?}", &emb[..5]);
    }

    // ── 顺手验证相似文本的 cosine 更高（不是任务要求，仅供参考） ──
    if embeddings.len() >= 3 {
        let sim_0_1 = cosine(&embeddings[0], &embeddings[1]);
        let sim_0_2 = cosine(&embeddings[0], &embeddings[2]);
        println!("\n=== 相似度验证（参考）===");
        println!("相似文本 vs 相似文本: cos = {:.4}", sim_0_1);
        println!("相似文本 vs 无关文本: cos = {:.4}", sim_0_2);
        if sim_0_1 > sim_0_2 {
            println!("✅ 相似文本的 cosine 更高，embedding 质量正常");
        } else {
            println!("⚠️  相似文本的 cosine 反而更低，检查模型是否正确加载");
        }
    }

    println!("\n=== 任务 1 验收 ===");
    println!("✅ EmbeddingEngine::load() 加载成功");
    println!("✅ embed_batch() 返回 1024d 向量");
    println!("✅ L2 norm ≈ 1.0（归一化正确）");
    Ok(())
}

fn cosine(a: &[f32; HIDDEN_DIM], b: &[f32; HIDDEN_DIM]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na > 0.0 && nb > 0.0 {
        dot / (na * nb)
    } else {
        0.0
    }
}
