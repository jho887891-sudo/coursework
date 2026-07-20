# Day 4：Prompt 评测 + A/B 测试框架

> "我改了一行 Prompt，效果变好了"——你怎么知道？凭感觉？今天你学会用数据证明。

---

## 学习目标

1. 理解 LLM-as-Judge 的可靠性边界——Judge 在什么情况下会出错
2. 搭建 Prompt A/B 测试自动化 Pipeline
3. 用 Bootstrap CI 判定"版本 B 显著优于版本 A"
4. 实现回归检测——新 Prompt 有没有让某些 query 变差

---

## 核心概念

### 1. LLM-as-Judge — 用 LLM 评估 LLM

#### 为什么需要 LLM-as-Judge

标书审核的 Prompt 评测无法自动化——没有标准答案。

```
问题："招标文件要求投标人注册资本不低于 1 亿元。是否存在合规风险？"

LLM 输出 A: "存在排斥性条款风险。依据《政府采购法》第 22 条..."
LLM 输出 B: "该要求合理。大型工程项目需要确保投标人财务实力..."

哪个更好？→ 需要领域知识判断。人工评审慢（50 条 × 30 次重复 = 1500 次人工评审 → 不可行）
→ 用 LLM 当 Judge。
```

#### Pairwise Comparison vs Single-Point Scoring

```
Pairwise: "A 和 B 哪个更好？" →  A > B / B > A / tie
  → 相对判断准确率 ~85%（人类水平）
  → 但只能判断"相对好坏"，不能判断"多少分"

Single-Point: "给 A 打分（1-5 分）" →  3.2 / 4.1 / ...
  → 绝对评分的一致性低（同一个输出两次打分可能差 1 分以上）
  → 需要 calibration set（人工标注 20 条 → 校准评分尺度）
```

#### Judge 的 System Prompt 设计

```
Judge Prompt:
  "你是一个标书合规审核质量的评估专家。
   请对比以下两个 Agent 的审核结论，从以下维度评估：
   
   1. 准确性：审核结论是否符合法规要求（0-10 分）
   2. 完整性：是否覆盖了所有可发现的问题（0-10 分）
   3. 可追溯性：每条结论是否引用了具体的法规条款（0-10 分）
   4. 实用性：修改建议是否具体、可执行（0-10 分）
   
   对于每个维度，先给出 A 的评分，再给出 B 的评分，再给出对比理由。
   最后输出 JSON：{\"维度名\": {\"A\": 分数, \"B\": 分数, \"winner\": \"A\"|\"B\"|\"tie\"}}"
```

**Judge 的校准方法**：
1. 人工标注 20 条 Agent 输出（人工给出"哪个更好"的判断）
2. Judge 对同样的 20 对做评估
3. 计算 Pearson r（人类评分 vs Judge 评分）→ 应 > 0.8
4. 如果 r < 0.6 → Judge 不理解领域 → 加领域解释到 Judge Prompt

---

### 2. A/B 测试 — 科学地比较两个 Prompt

#### 实验设计

```
Prompt A (v1.0.0 当前线上)   vs   Prompt B (v1.0.1 候选)
        │                                │
        ▼                                ▼
  同一批 50 条评测 query                  同一批 50 条评测 query
        │                                │
        ▼                                ▼
  每条 query 重复 30 次推理               每条 query 重复 30 次推理
  (Day 1 已证明：temperature=0 也不确定)    (为什么 30 次：LLM-as-Judge 的评分方差)
        │                                │
        ▼                                ▼
  Judge 评分 → 50 × 30 = 1500 个数据点    Judge 评分 → 1500 个数据点
```

#### 为什么需要 30 次重复

LLM 的输出有随机性（Day 1）。如果有随机性，Judge 对"同一个 Prompt 同一个 query"的评分也会波动。30 次重复让你得到每个 Prompt 在每个 query 上的**平均表现 + 方差**。

```
Prompt A on query_17 的 30 次评分： [3.2, 3.5, 2.8, 3.1, ..., 3.4]
  → mean = 3.24, std = 0.31

Prompt B on query_17 的 30 次评分： [3.8, 3.6, 3.9, 3.5, ..., 3.7]
  → mean = 3.68, std = 0.28

差值 = 3.68 - 3.24 = 0.44 → B 在 query_17 上平均好 0.44 分
但这是"显著"的吗？还是随机波动导致？
→ Bootstrap CI
```

#### Bootstrap 置信区间

```rust
fn bootstrap_ci(
    scores_a: &[f64],  // A 的 1500 个评分（50 query × 30 次）
    scores_b: &[f64],  // B 的 1500 个评分
    n_bootstrap: usize, // 10000
    alpha: f64,         // 0.05 → 95% CI
) -> (f64, f64) {
    let mut diffs = Vec::with_capacity(n_bootstrap);
    let mut rng = rand::thread_rng();
    
    for _ in 0..n_bootstrap {
        // 有放回地重采样（保持 query 级别的聚类结构！）
        let sample_a: Vec<f64> = scores_a.choose_multiple(&mut rng, scores_a.len()).copied().collect();
        let sample_b: Vec<f64> = scores_b.choose_multiple(&mut rng, scores_b.len()).copied().collect();
        
        let mean_a = sample_a.iter().sum::<f64>() / sample_a.len() as f64;
        let mean_b = sample_b.iter().sum::<f64>() / sample_b.len() as f64;
        diffs.push(mean_b - mean_a);
    }
    
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lower = diffs[(n_bootstrap as f64 * alpha / 2.0) as usize];
    let upper = diffs[(n_bootstrap as f64 * (1.0 - alpha / 2.0)) as usize];
    (lower, upper)
}

// 解读：
// CI = [0.12, 0.38] → CI_lower > 0 → B 显著优于 A（95% 置信度）
// CI = [-0.05, 0.25] → 包含 0 → 差异不显著（再跑更多数据可能就反转了）
// CI = [0.45, 0.89] → B 显著优于 A，且效应量 > 0.5 → 大效应
```

#### 回归检测

```rust
fn regression_detection(
    scores_per_query_a: &HashMap<String, Vec<f64>>,  // query → 30 次评分
    scores_per_query_b: &HashMap<String, Vec<f64>>,
) -> Vec<RegressionAlert> {
    let mut alerts = vec![];
    
    for (query_id, scores_a) in scores_per_query_a {
        let scores_b = &scores_per_query_b[query_id];
        let (lower, upper) = bootstrap_ci(scores_a, scores_b, 10000, 0.05);
        
        // B 比 A 差，且 CI 全部在 0 以下 → 回归！
        if upper < 0.0 {
            alerts.push(RegressionAlert {
                query_id,
                mean_diff: scores_b.iter().sum::<f64>() / scores_b.len() as f64
                         - scores_a.iter().sum::<f64>() / scores_a.len() as f64,
                ci_lower: lower,
                ci_upper: upper,
            });
        }
    }
    
    alerts
}

// 判定规则：
//  如果任何 query 的 CI 全部在 0 以下 → 版本 B 在这个 query 上显著变差
//  → 团队审核：这个 regression 是否可以接受？是否可以针对性修复？
```

---

### 3. 自动化评测 Pipeline

```
┌──────────────────────────────────────────────────────┐
│              Prompt 变更触发评测                         │
│                                                        │
│  Git PR: prompts/judge_agent.md changed                 │
│      │                                                  │
│      ▼                                                  │
│  CI Job: "prompt-eval"                                  │
│      │                                                  │
│      ├─ Step 1: 读取变更前后的两个 Prompt 版本            │
│      ├─ Step 2: 并行跑 50 × 30 = 1500 次推理            │
│      │         (缓存：相同 input → 复用 output)           │
│      ├─ Step 3: LLM-as-Judge 评分                       │
│      ├─ Step 4: Bootstrap CI → 显著性判定                │
│      ├─ Step 5: 回归检测 → 逐 query 分析                  │
│      └─ Step 6: 生成评测报告 → 写入 PR comment            │
│                                                        │
│  ┌─────────────────────────────────────────┐           │
│  │ 📊 Prompt 变更评测报告                     │           │
│  │                                          │           │
│  │ Agent: JudgeAgent                        │           │
│  │ Version: v1.0.0 → v1.0.1                 │           │
│  │                                          │           │
│  │ 综合评分: 3.24 → 3.68 (+13.6%)            │           │
│  │ Bootstrap 95% CI: [+0.18, +0.52] ✅ 显著  │           │
│  │                                          │           │
│  │ 回归检测: 0 条 query 显著退化              │           │
│  │ 进步最多: query_17 (+0.44), query_31 (+0.38)│         │
│  │                                          │           │
│  │ 结论: ✅ 可以合并                          │           │
│  └─────────────────────────────────────────┘           │
└──────────────────────────────────────────────────────┘
```

---

## 动手

### 任务 1：实现 Bootstrap CI

```rust
fn bootstrap_ci(samples_a: &[f64], samples_b: &[f64], n: usize, alpha: f64) -> (f64, f64);
```

验证：用已知正态分布的数据（`N(3.0, 0.5)` vs `N(3.3, 0.5)`）测试，CI 应不包含 0。

### 任务 2：搭建 A/B 测试框架

```rust
struct ABTest {
    prompt_a: String,
    prompt_b: String,
    queries: Vec<String>,
    repetitions: usize,  // 30
    judge: JudgeConfig,
}

impl ABTest {
    async fn run(&self) -> ABTestReport;         // 跑全量推理 + Judge 评分
    fn bootstrap_analysis(&self, data: &[Score]) -> BootstrapResult;
    fn regression_check(&self, data: &[Score]) -> Vec<RegressionAlert>;
}
```

### 任务 3：验证 LLM-as-Judge 可靠性

人工标注 20 对 Agent 输出（哪个更好）。Judge 对同样的 20 对评分。计算 Pearson r。分析 r 最低的 3 对——Judge 为什么判断错了？

---

## 验收标准

- [ ] Bootstrap CI 正确实现（验证：已知分布的测试数据）
- [ ] A/B 测试框架可运行：输入两个 Prompt → 输出评测报告（含 CI + 回归检测）
- [ ] LLM-as-Judge 校准：Pearson r > 0.7
- [ ] 至少发现 1 个真实的 Prompt 改进案例（新版本 Bootstrap CI > 0）

---

## 思考题

1. 如果评测集有 50 条 query，每条重复 30 次 → 1500 次 LLM 推理 ≈ 1500 × 500ms ≈ 12.5 分钟。如果 11 个 Agent 都要跑呢？怎么优化？（提示：只跑变更的 Agent + 缓存）
2. LLM-as-Judge 的 Pairwise 比较默认"A 和 B 独立评估，然后比较分数"。如果改用"A 和 B 放一起，Judge 直接比较"——会不会更准？（提示：position bias——Prompt 中先出现的信息 Judge 可能更重视）
3. Bootstrap 的假设是样本独立同分布。30 次重复之间真的是独立的吗？（提示：同一 LLM 实例的后续推理可能受 KV cache 热状态影响）
