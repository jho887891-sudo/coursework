# Day 3：自动化评测 Pipeline + 统计显著性

> Day 1-2 你学会了算指标、造 Benchmark。今天你把这一切自动化——每次 Prompt 变更自动跑全量评测，统计检验告诉你"这个改进是真的还是运气好"。

---

## 学习目标

1. 搭建 CI 集成的自动化评测 Pipeline
2. 用 Bootstrap CI 判定"版本 B 显著优于版本 A"
3. 理解并实现多重比较校正（Bonferroni / Holm / Benjamini-Hochberg）
4. 计算效应量（Cohen's d）和统计功效，区分"显著"与"有用"

---

## 核心概念

### 1. 自动化评测 Pipeline 架构

```
┌─────────────────────────────────────────────────────────┐
│                    触发条件                               │
│  - Git PR: prompts/ 目录有文件变更                        │
│  - 手动触发: POST /eval/run?agent=judge_agent             │
│  - 定时触发: 每天 02:00 全量巡检（检测数据漂移）            │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│                    Step 1: 准备                            │
│  - 读旧版本 Prompt (A) + 新版本 Prompt (B)                 │
│  - 加载 Benchmark 50 条 query + Gold Standard              │
│  - 初始化 LLM 客户端（复用连接池）                          │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│                    Step 2: 并行推理                        │
│  - 50 query × 30 次重复 = 1500 次推理                     │
│  - 缓存: HashMap<(prompt_hash, query), Vec<output>>       │
│  - 并发: tokio::spawn × semaphore(最大并发=10)             │
│  - 预计耗时: 1500 × 500ms / 10 ≈ 75 秒                    │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│                    Step 3: 评测                            │
│  - 每条 output vs Gold Standard → P/R/F1                  │
│  - 得到 A: [score_1, ..., score_1500]                     │
│       B: [score_1, ..., score_1500]                       │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│                    Step 4: 统计分析                        │
│  - Bootstrap CI (10000 次) → (lower, upper)               │
│  - 回归检测: 逐 query 对比 → 找出退化 query                │
│  - 效应量: Cohen's d                                      │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│                    Step 5: 决策                            │
│  - CI_lower > 0 → B 显著更好 ✅ 允许合并                   │
│  - CI 包含 0 → 差异不显著 ⚠️ 需要更多数据或放弃             │
│  - CI_upper < 0 → B 显著更差 ❌ 阻止合并                   │
│  - 有任何 query 退化 → 审查是否可以接受                     │
└─────────────────────────────────────────────────────────┘
```

### 响应缓存策略

```rust
struct ResponseCache {
    // Key: (prompt_content_hash, query_text)
    // Value: Vec<String> — 该 query 在该 Prompt 下的 30 次输出
    cache: HashMap<(u64, String), Vec<String>>,
}

impl ResponseCache {
    fn get_or_compute(
        &mut self,
        prompt: &str,
        query: &str,
        repetitions: usize,
        llm: &LlmClient,
    ) -> Vec<String> {
        let key = (hash(prompt), query.to_string());
        if let Some(cached) = self.cache.get(&key) {
            if cached.len() >= repetitions {
                return cached[..repetitions].to_vec();  // 缓存命中！
            }
        }
        // 缓存未命中或不足 → 补跑
        let outputs = llm.chat_repeatedly(prompt, query, repetitions);
        self.cache.insert(key, outputs.clone());
        outputs
    }
}
```

缓存省 80% 的 LLM 调用——相同的 Prompt+Query 组合在不同 A/B 测试之间复用。但注意：不同 temperature 的输出不能复用！

---

### 2. 回归检测 — 进步的代价

整体 F1 提升了，但某个 query 变差了——这叫回归。

```rust
fn regression_detection(
    scores_a_per_query: &HashMap<&str, Vec<f64>>,
    scores_b_per_query: &HashMap<&str, Vec<f64>>,
) -> Vec<RegressionReport> {
    let mut regressions = vec![];
    
    for (query_id, scores_a) in scores_a_per_query {
        let scores_b = &scores_b_per_query[query_id];
        let (lower, upper) = bootstrap_ci(scores_a, scores_b, 10000, 0.05);
        
        if upper < 0.0 {
            // B 在这个 query 上显著更差
            let effect = cohens_d(scores_a, scores_b);
            regressions.push(RegressionReport {
                query_id: query_id.to_string(),
                ci: (lower, upper),
                effect_size: effect,
            });
        }
    }
    
    regressions
}
```

CI 全部在 0 以下 = 在这个 query 上显著退化。需要人工审查——这个退化是否可接受？是否可以针对性地修复？

---

### 3. 多重比较校正 — 为什么不能跑 10 次取最好的

如果你跑了 10 个 Prompt 变体的 A/B 测试，每次用 α=0.05 判断"显著"：

```
P(至少一个假阳性) = 1 - (1 - 0.05)^10 = 1 - 0.599 = 0.401

→ 40% 的概率，你会声称至少一个变体"显著更好"——但那是随机波动！
```

这就是 **Family-Wise Error Rate (FWER)** 膨胀。

#### Bonferroni 校正（最保守）

$$\alpha_{\text{adjusted}} = \frac{\alpha}{m}$$

跑了 10 个测试 → adjusted α = 0.05/10 = 0.005。每个测试必须满足 p < 0.005 才声称显著。

代价：统计功效降低（更难检测到真实差异）。

#### Holm-Bonferroni（逐步拒绝，更 powerful）

```
1. 对 m 个 p-value 从小到大排序: p_(1) ≤ p_(2) ≤ ... ≤ p_(m)
2. 对于 j=1: 如果 p_(1) < 0.05/m → 拒绝 H_0(1)（最显著的那个）
3. 对于 j=2: 如果 p_(2) < 0.05/(m-1) → 拒绝 H_0(2)
4. ...
5. 对于 j=k: 如果 p_(k) < 0.05/(m-k+1) → 拒绝 H_0(k)
6. 第一次不满足时停止——剩下的都不显著
```

比 Bonferroni 更 powerful（拒绝更多 null hypothesis），但仍然控制 FWER。

#### Benjamini-Hochberg (FDR 控制)

控制 False Discovery Rate——假阳性占所有阳性的比例。适合探索性分析（"我们不是要绝对确定每个都显著，而是控制发现的错误率"）。

---

### 4. 效应量与统计功效

#### Cohen's d

$$d = \frac{\bar{X}_B - \bar{X}_A}{s_{\text{pooled}}}$$

```
d = 0.2: 小效应（如 F1 从 0.800 提升到 0.820——用户可能感觉不到差异）
d = 0.5: 中效应（F1 从 0.800 到 0.840——明显改善）
d = 0.8: 大效应（F1 从 0.800 到 0.870——质变）
```

Bootstrap CI 告诉你"差异是否真实存在"。Cohen's d 告诉你"差异有多大"。两个都要报告。

**CI 通过 + d=0.05** → 统计显著但实际无意义——样本量太大导致即使极小的差异也被检测为显著。**CI 不通过 + d=0.8** → 效应大但样本不足——再加数据就能显著。这是两个最经典的误解。

#### 统计功效

功效 = P(检测到真实差异 | 差异确实存在)。

功效取决于三个因素：
- 样本量 n（n↑ → 功效↑）
- 效应量 d（d↑ → 功效↑）
- α 水平（α↑ → 功效↑，但假阳性率也↑）

在你的评测中：50 条 query × 30 次重复 = 1500 个数据点。如果效应量 d=0.2（小效应），功效可能是 0.6——意味着即使真的变好了，也只有 60% 的概率检测出来。

---

## 动手

### 任务 1：搭建自动化评测 Pipeline

```rust
struct EvalPipeline {
    benchmark: Benchmark,       // Day 2 的 Gold Standard
    llm_client: LlmClient,
    cache: ResponseCache,
}

impl EvalPipeline {
    async fn run_ab_test(&mut self, prompt_a: &str, prompt_b: &str) -> ABTestReport;
    async fn detect_regression(&self, report: &ABTestReport) -> Vec<RegressionReport>;
}
```

### 任务 2：实现多重比较校正

```rust
fn bonferroni_correct(p_values: &[f64]) -> Vec<f64>;       // 乘以 m
fn holm_bonferroni(p_values: &[f64]) -> Vec<bool>;          // 逐步拒绝
fn benjamini_hochberg(p_values: &[f64], q: f64) -> Vec<bool>; // FDR 控制
```

### 任务 3：效应量与功效

```rust
fn cohens_d(sample_a: &[f64], sample_b: &[f64]) -> f64;
fn hedges_g(sample_a: &[f64], sample_b: &[f64]) -> f64;     // 小样本修正版
fn statistical_power(d: f64, n: usize, alpha: f64) -> f64;  // 后验功效分析
```

---

## 验收标准

- [ ] Pipeline 完整可运行：两个 Prompt → 评测报告（含 CI + 回归检测 + 效应量）
- [ ] 多重比较校正三种方法正确实现
- [ ] 功效分析：能回答"50 query × 30 rep 能否检测到 d=0.2 的效应"
- [ ] 响应缓存有效：相同 Prompt+Query 第二次跑直接命中

---

## 思考题

1. 如果 CI 通过（差异显著）但效应量 d=0.05——这意味着什么？你会合并这个 Prompt 变更吗？
2. Holms-Bonferroni 和 Benjamini-Hochberg 的区别是什么时候用哪个？（提示：验证性分析 vs 探索性分析）
3. 回归检测中，某个 query 显著退化但整体 F1 提升。在什么情况下你可以接受这个回归？

---

## 与标书审核项目的关系

G4↔G5 反馈闭环的引擎就是你今天的 Pipeline。G5 改了一行 Prompt——你的 Pipeline 自动跑全量评测——Bootstrap CI 说"不显著"→ G5 继续改。CI 说"显著更好 + no regression"→ 合并。CI 说"显著更好 but 3 queries 退化"→ 审查退化。

没有这套 Pipeline，G4 和 G5 就是在黑暗中对骂——一个说"我的评测说你差了"，一个说"你的评测不准"。
