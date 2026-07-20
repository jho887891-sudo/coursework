# 第6课：Agent Evaluation — 怎样证明它真的有效？

> 单次 Demo 只能证明系统曾经成功过。Evaluation 要回答它在一类任务上有多可靠、付出多少代价、会怎样失败。

---

## 学习目标

1. 区分单元测试、契约测试、轨迹测试和端到端评测；
2. 构建 train/dev/test 与隐藏测试；
3. 计算 Precision、Recall、F1、完成率、成本和稳定性；
4. 使用重复运行与变形测试评估非确定性；
5. 做 baseline、消融和错误分类。

---

## 本 Lesson 必须实现

工程：`配套代码/lesson-06-evaluation`。

1. 完成 `evaluate`，实现 accuracy/precision/recall/F1 和除零语义；
2. timeout、parse error、refusal 必须作为结果留在分母；
3. 实现 EvalCase runner、结果版本记录和错误分类；
4. 对一个前置 Lesson 系统做重复实验、五类变形测试和一次单变量消融；
5. 输出可从失败样本跳转到 Trace 的报告。

```powershell
cargo test -p lesson-06-evaluation --test acceptance -- --ignored
```

本 Lesson 必须自己实现指标，而不是只调用评测库；只有手算、单测与程序结果一致后，才能相信报告。

---

## 1. 四层测试

| 层级 | 测什么 | 示例 |
|---|---|---|
| Unit | 纯函数和状态更新 | 预算扣减、依赖图判环 |
| Contract | 模型/工具边界 | 参数校验、超时、错误结构 |
| Trajectory | 行为过程 | 失败后是否换策略、是否死循环 |
| End-to-End | 最终任务 | 是否正确发现风险并引用证据 |

只测最终输出，出错时很难知道是检索、决策、工具还是汇总失败。

---

## 2. 分类任务指标

对于风险发现：

```text
TP：正确发现的风险
FP：把正常条款误判为风险
FN：漏掉的真实风险

Precision = TP / (TP + FP)
Recall    = TP / (TP + FN)
F1        = 2PR / (P + R)
```

只奖励检出率会鼓励“全部判高风险”。必须同时计算 Precision。

还要分开测：

- risk detection；
- severity classification；
- evidence retrieval；
- citation support；
- abstention。

---

## 3. 非确定性与重复运行

真实模型在相同输入上可能产生不同轨迹。至少记录：

- 每个样本运行次数；
- 成功次数；
- 平均值与最差结果；
- 不同失败类型；
- 模型、Prompt、工具和数据版本。

对于入门课程，关键样本至少运行 5 次。不要只保留最好的一次。

---

## 4. 变形测试

无法为每个自然语言输入写精确答案时，可以检查应保持的性质：

- 同义改写不应改变事实结论；
- 交换无关段落顺序不应改变风险等级；
- 把 5% 改成 1% 后，保证金超限结论应消失；
- 添加无关文本不应引发额外工具权限；
- 删除证据后，置信度不应提高；
- 工具返回冲突证据时，系统不得继续声称“证据一致”。

这类关系称为 metamorphic relation。

---

## 5. 评测数据纪律

数据至少分为：

- public/train：可以查看，用于开发；
- dev：可以反复跑，用于调参；
- hidden/test：提交前不可见；
- red-team：攻击与分布变化。

每个样本保留：

- 输入；
- 标签与标签定义；
- 证据；
- 是否有歧义；
- 标注者；
- 版本。

老师的直觉不是天然 ground truth。存在争议的样本应标记 ambiguous 或经过复核。

---

## 6. Worked Example：为什么只看 Recall 会被骗

测试集有 10 个条款，其中 3 个有风险、7 个正常。

系统 A 把所有条款都判风险：

```text
TP = 3
FP = 7
FN = 0
Recall = 3 / 3 = 100%
Precision = 3 / 10 = 30%
F1 ≈ 46%
```

系统 B 找到 2 个风险，误报 1 个：

```text
TP = 2
FP = 1
FN = 1
Recall = 2 / 3 ≈ 67%
Precision = 2 / 3 ≈ 67%
F1 ≈ 67%
```

如果只奖励 Recall，系统 A 看起来满分，却会制造大量人工复核负担。

---

## 7. Eval Harness 的最小数据结构

```rust
#[derive(Deserialize)]
struct EvalCase {
    case_id: String,
    input: String,
    expected_label: String,
    evidence_refs: Vec<String>,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct RunResult {
    case_id: String,
    predicted_label: String,
    evidence_refs: Vec<String>,
    success: bool,
    model_calls: u32,
    tool_calls: u32,
    latency_ms: u64,
    termination_reason: String,
    trace_path: String,
}
```

评测循环：

```rust
for case in cases {
    for repeat in 0..repeat_count {
        let result = run_agent(&case).await;
        save_run_result(&case.case_id, repeat, result)?;
    }
}
let metrics = aggregate(all_results);
write_report(metrics)?;
```

任何 panic、timeout、解析失败都要生成 `RunResult`，不能直接从统计中消失。

---

## 8. 怎样写变形测试

### 数字反事实

```rust
let risky = "保证金比例为 5%";
let safe = "保证金比例为 1%";

assert_eq!(classify(risky), Risk);
assert_ne!(classify(safe), Risk);
```

自然语言系统不一定适合精确 `assert_eq!` 所有文本，但可以断言结构性质：

- 风险结论应改变；
- 引用仍应定位；
- 置信度不应无理由上升；
- 不应增加越权动作。

### 无关段落不变性

给输入加入天气、学校介绍等无关内容，关键事实不变时核心结论应保持。

### 删除证据

移除唯一支持来源后，系统应转为 `insufficient_evidence`，而不是保持同等置信度。

---

## 9. 错误分析怎么写

不要只写：

> “模型不够聪明，所以答错了。”

要沿 Trace 定位：

```text
Case: dev-017
Expected: insufficient_evidence
Predicted: risk

Retrieval: 找到主题相关但不支持的段落
Decision: 模型把“相关”误当“支持”
Validator: 只检查 ref 存在，没有检查 entailment
Root cause: evidence validation coverage gap
Regression test: 删除直接证据后必须拒答
```

---

## 10. 跟做实验

### Checkpoint A：10 条固定结果

先不用 Agent，手写 10 个 prediction，验证 Precision/Recall/F1 计算正确。

### Checkpoint B：接入 Agent

把失败、timeout 和无结果都保存下来。确认总运行数等于 cases × repeats。

### Checkpoint C：版本记录

每次运行保存：模型名、参数、Prompt hash、数据版本、代码 commit 或等价版本标识。

### Checkpoint D：变形测试

实现数字、无关段落、证据删除、顺序交换和 Prompt Injection 五类关系。

### Checkpoint E：消融

关闭一个组件，例如重复检测、检索路由或 PlanValidator。只改变这一项并比较。

---

## 11. 本课自测

1. timeout 是否应该从指标分母删除？
2. dev 集反复调到 100% 有什么风险？
3. Precision 高但 Recall 低意味着什么？
4. 为什么要保存 trace_path？
5. 消融实验为什么一次只改一个主要变量？

---

## 12. 延伸学习

- confusion matrix；
- bootstrap confidence interval；
- property-based / metamorphic testing；
- benchmark contamination；
- inter-annotator agreement；
- error taxonomy 与回归测试集维护。

---

## 13. AI 协作：让 AI 写评测管道，你负责实验是否可信

**本课起点**：从课程根目录使用 `配套代码/lesson-06-evaluation`，再把前五课的公开数据接入同一个 runner。

可以交给 AI：指标计算、报告排版、runner 样板和测试候选生成。必须由你决定：数据如何切分、ground truth 如何定义、变形关系为何应成立、消融只改变哪个变量、失败属于哪类根因。

推荐 Prompt：

```text
请补全评测 runner 和指标计算；timeout、parse error、refusal 都必须留在分母中。
对除零、重复样本和空数据集写测试。不要替我解释实验结论，
只输出原始结果、计算公式与可能的混杂变量。
```

**AI 代码审查任务**：检查 AI 是否静默丢弃失败样本、是否在 test 集上调参、是否把同一次随机运行当成稳定结论；至少植入并抓住其中一个问题。

---

## 作业：为前五课系统建立 Eval Harness

### 必做

1. 统一 `EvalCase` 与 `RunResult`；
2. 自动运行公开数据集；
3. 计算任务指标和运行指标；
4. 保存模型、Prompt、数据和代码版本；
5. 输出每个失败样本的 Trace 路径；
6. 至少实现 5 个变形测试；
7. 对一个组件做消融实验；
8. 建立错误分类表。

### 错误分类示例

```text
retrieval_miss
unsupported_claim
wrong_tool
invalid_arguments
budget_exhausted
premature_stop
format_error
unsafe_action_blocked
```

### 报告要求

报告必须包含：

- 你的假设；
- baseline；
- 数据集说明；
- 指标定义；
- 结果表；
- 至少 5 个失败样本；
- 局限性；
- 下一步实验。

---

## 验收标准

- [ ] 一条命令完成评测并生成报告；
- [ ] Precision 与 Recall 同时存在；
- [ ] 关键样本重复运行；
- [ ] 失败结果不会被测试脚本静默跳过；
- [ ] 能从结果跳转到 Trace；
- [ ] 至少一个“增加组件但指标下降”的真实或构造案例。

---

## 思考题

> 如果你连续修改 Prompt，直到 dev 集达到 100%，为什么这不能证明 Agent 已经可靠？
