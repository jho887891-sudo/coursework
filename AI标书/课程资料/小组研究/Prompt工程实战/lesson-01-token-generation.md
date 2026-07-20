# Day 1：LLM 推理本质 — 概率空间与采样策略

> "为什么同一个 Prompt 每次输出都不一样？"——因为 LLM 每次推理都是在 250000 面的骰子上掷一次。今天你理解这个骰子是怎么造的。

---

## 学习目标

1. 理解 LLM 自回归推理的本质：每个 token 是一次条件概率抽样
2. 手写 temperature / top-k / top-p 三种采样器，理解三者的叠加顺序
3. 可视化不同参数组合下 token 概率分布的变化
4. 解释"为什么 temperature=0 时输出也不完全确定"

---

## 核心概念

### 1. LLM 不是"写"文字，是"掷骰子"

#### 自回归解码循环

```
输入: "投标人须具备"
         │
         ▼
    ┌─────────┐
    │  LLM    │ → 前向传播 → logits (vocab_size 维的向量)
    └─────────┘
         │
         ▼
    logits: [2.3, -0.5, 5.1, ..., 1.8]  ← 250000 个 token 各有一个分数
         │
         ▼
    softmax → 概率分布
    [0.001, 0.0001, 0.15, ..., 0.003]  ← 和 = 1.0
         │
         ▼
    从分布中抽样 → token_id = 8945 ("建筑")
         │
         ▼
    新的输入: "投标人须具备建筑"
         │
         ▼
    循环...直到抽到 <EOS> 或达到 max_tokens
```

这就是"自回归"（Auto-Regressive）——用自己刚生成的 token 作为下一步的输入。每个 token 的选择不是"写"出来的，是从概率分布中"抽"出来的。

#### softmax 函数

$$P(token_i) = \frac{e^{logit_i}}{\sum_{j=1}^{V} e^{logit_j}}$$

其中 V = vocab_size（BGE-M3 是 250000，qwen 是 152064）。

直觉：logit 大的 token 对应更大的概率，但不是线性的——softmax 通过指数函数放大了差异。logit 从 2.0 变到 3.0，概率不是翻 1.5 倍而是翻 e≈2.7 倍。

#### 为什么不能用 argmax

```python
# 如果每次总是选概率最高的 token：
def greedy_decode(logits):
    return argmax(logits)  # 永远选 #1

# → 结果: "投标人须具备建筑工程施工总承包资质。投标人须具备建筑..."
#   LLM 陷入循环——"资质"后面最可能再跟一个"投标人"

# 抽样引入了多样性：
def sample_decode(logits):
    probs = softmax(logits)
    return random.choice(vocab, p=probs)  # 偶尔选 #2、#3
```

argmax 导致"确定性死循环"——这就是为什么需要采样。

---

### 2. 采样三参数：Temperature / Top-k / Top-p

#### Temperature（温度）

$$P_T(token_i) = \frac{e^{logit_i / T}}{\sum_{j} e^{logit_j / T}}$$

```
T = 0.0:   退化为 argmax（理论上的，实际浮点有 bug）
T = 0.5:   logits 差异被放大 → 高分 token 概率更高 → 输出更"集中"（保守）
T = 1.0:   原始 softmax（中性）
T = 1.5:   logits 差异被压缩 → 分布更扁平 → 输出更多样（大胆）
T → ∞:    均匀分布——纯随机

示例（2 个 token 竞争）：
logits: [4.0, 2.0]
T=0.5: prob_dist = [0.98, 0.02]  ← 几乎确定选 #1
T=1.0: prob_dist = [0.88, 0.12]  ← 原始分布
T=1.5: prob_dist = [0.79, 0.21]  ← #2 有机会
```

#### Top-k 采样

```rust
fn sample_top_k(logits: &[f32], k: usize) -> usize {
    // 1. 找到概率最高的 k 个 token 的索引
    let mut indexed: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    indexed.truncate(k);
    
    // 2. 对 k 个候选 softmax → 概率
    let probs = softmax(&indexed.iter().map(|(_, l)| *l).collect::<Vec<_>>());
    
    // 3. 从 k 个候选中按概率抽样
    sample_from_distribution(&indexed.iter().map(|(i, _)| *i).collect::<Vec<_>>(), &probs)
}
```

- k=1：退化为 greedy decode
- k=10：在 10 个最可能的 token 中选
- k=vocab_size：无过滤（等价于纯 temperature 采样）

Top-k 的问题：固定 k 不适应不同置信度的场景。在当前位置很确定时（如"施工总承包"后面的词极可能是"资质"），k=50 引入了过多噪声。在当前位置不确定时（如一段话的开头），k=50 可能不够。

#### Top-p (Nucleus Sampling)

```rust
fn sample_top_p(logits: &[f32], p: f64) -> usize {
    // 1. 按概率从高到低排序
    let probs = softmax(logits);
    let mut indexed: Vec<(usize, f64)> = probs.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    
    // 2. 累积概率直到 ≥ p
    let mut cumsum = 0.0;
    let mut cutoff = 0;
    for (i, (_, prob)) in indexed.iter().enumerate() {
        cumsum += prob;
        cutoff = i;
        if cumsum >= p { break; }
    }
    
    // 3. 只保留累积概率 ≥ p 的最小候选集 → 重新归一化 → 抽样
    let truncated = &indexed[..=cutoff];
    let renormalized = softmax(&truncated.iter().map(|(_, l)| *l as f32).collect::<Vec<_>>());
    sample_from_distribution(...)
}
```

- p=0.9：保留累积概率 ≥ 90% 的最少 token 集
- p=1.0：无过滤
- Top-p 的优雅之处：置信度高时（top-1 prob 已经 0.95），p=0.9 时 k≈1。置信度低时（top-1 prob 只有 0.05），k 自动增大到几十甚至几百。

#### 三者的叠加顺序（重要！）

```
原始 logits
  → 除以 Temperature (T)
  → softmax 得到概率
  → Top-k 截断（只保留 k 个）
  → Top-p 截断（累积概率 ≥ p）
  → 重新归一化
  → 从最终分布中抽样
```

**顺序不能换**。先 softmax 再 top-k 才有意义——在概率空间截断。先 top-k 再 softmax 讨论的是 logit 空间，含义不同。

---

### 3. 为什么 temperature=0 也不确定

```
理论上：T=0 → 1/T = ∞ → exp(∞) = ∞，但 exp(-∞) = 0
  → 只有最大 logit 的 token 有非零概率
  → 等同于 argmax

实际上：
  ① 浮点溢出：exp(large_logit / 0.001) 可能溢出为 Inf
     Inf / Inf = NaN → 无法归一化 → 行为未定义
  
  ② 浮点非结合性：GPU 的 FP16/FP32 混合精度计算中
     matmul 的结果因 CUDA core 调度不同而相差 1 ULP
     → 两个"相等"的 logits 在不同推理中可能略微不同
     → 微小的差异经 softmax 放大后 → 不同的 argmax → 不同的 token
  
  ③ Prefill vs Decode 的差异：
     Prefill（首次输入）并行计算，结果相对确定
     Decode（逐 token）每次只算一个新 token，非确定性累积
```

工程实践：要完全确定性 → `temperature=0` + `seed=42` + 固定 batch_size=1 + 同一 GPU 型号 + 同一 CUDA 版本。即便如此，跨框架（vLLM vs HuggingFace）仍不能保证。

---

## 动手

### 任务 1：手写采样器

```rust
pub struct Sampler {
    temperature: f64,
    top_k: usize,
    top_p: f64,
}

impl Sampler {
    /// 从 logits 中采样一个 token_id
    /// 执行顺序：/T → softmax → top-k → top-p → re-normalize → sample
    pub fn sample(&self, logits: &[f32]) -> usize;
}

pub fn weighted_random_choice(probs: &[f64]) -> usize;
pub fn softmax(logits: &[f32]) -> Vec<f64>;
```

### 任务 2：概率可视化工具

用 DashScope API（`logprobs=true`），对同一个 Prompt，分别可视化：
- 下一个 token 的 Top-20 概率分布（终端 ASCII bar chart）
- 不同 temperature 下概率分布的变化

```
Prompt: "投标人须具备建筑工程施工总承包"
Next token prediction (T=1.0):
  资质     ████████████████████████████████ 0.62
  二级     ████████ 0.15
  一级     ██████ 0.11
  三级     ███ 0.05
  ...
```

### 任务 3：参数冻结实验

```
固定 Prompt: "招标文件中的资质要求应当"
  跑 temperature = [0.1, 0.5, 1.0, 1.5, 2.0]
  每个 T 跑 50 次
  统计：输出唯一性（50 次中有多少种不同的输出）vs T 的曲线
  统计：输出长度 vs T 的曲线
```

### 任务 4：解释为什么同一个 Prompt 不同答案

从你的可视化数据中找 3 个具体例子，说明"在某个 token 位置，概率分布很平坦 → 抽样时可能选到不同的 token → 后续自回归累积放大 → 最终输出完全不同"。

---

## 验收标准

- [ ] `Sampler::sample()` 正确实现三种截断的叠加
- [ ] 概率可视化能正确展示 top-20 token 分布
- [ ] 参数冻结实验报告：含唯一性曲线 + 长度曲线
- [ ] 能解释至少一个"发散案例"的 token 级别因果链

---

## 思考题

1. 为什么 top-p 比固定 top-k 更合理？在标书审核场景，"确定"位置和"不确定"位置分别是什么？（提示：公式化条款 vs 主观判断）
2. 如果 batch_size=8 时做推理，batch 内不同序列的长度不同——padding 位置会影响其他序列的采样吗？（提示：attention_mask + 浮点非确定性）
3. 你在 Day 1 学到了"为什么 temperature=0 也不确定"。在生产环境中，需要"完全可复现"的审核结论——你会怎么解决？

---

## 与标书审核项目的关系

G5 组的 Agent 调用 LLM 时，采样参数不是随便设的。你的实验数据直接决定了：
- JudgeAgent 用 T=0.1（需要一致性——同一份标书审两次，结论应该接近）
- BrainstormAgent 用 T=0.7（需要多样性——发现更多潜在风险）
- 为什么？你今天的"参数冻结实验"就是答案。
