//! Lesson 6: Agent Evaluation Harness
//!
//! 实现分类指标手算、评测 runner、变形测试和消融报告。
//! 核心原则：失败样本不得从统计中消失，除零必须有显式定义。
//!
//! ## 设计原则
//!
//! 1. **失败留在分母** — timeout / parse error / refusal 全部作为错误参与指标计算
//! 2. **除零显式定义** — 分母为 0 时 Precision / Recall / F1 返回 0.0
//! 3. **每条记录可追溯** — 携带 VersionStamp 和 trace_path
//! 4. **消融单变量** — `metric_delta` 逐字段报告 baseline 与 variant 的差异
//!
//! ## 使用示例
//!
//! ```rust
//! use lesson_06_evaluation::*;
//!
//! let metrics = evaluate(
//!     &[true, true, false, false],
//!     &[Some(true), Some(false), Some(true), Some(false)],
//! );
//! assert!((metrics.accuracy - 0.5).abs() < 1e-9);
//! ```

// ---------------------------------------------------------------------------
// 核心数据结构
// ---------------------------------------------------------------------------

/// 二分类评测指标。
///
/// 本课约定：分母为 0 时，对应指标定义为 0.0。
///
/// # 计算式
///
/// ```text
/// Accuracy  = (TP + TN) / (TP + FP + FN + TN)
/// Precision = TP / (TP + FP)
/// Recall    = TP / (TP + FN)
/// F1        = 2PR / (P + R)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

impl Metrics {
    /// 全零指标 —— 当输入为空或无法计算时的默认返回值。
    pub fn zeros() -> Self {
        Self {
            accuracy: 0.0,
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
        }
    }
}

/// 混淆矩阵的四个格子。
///
/// - `tp`：预测为正、实际为正
/// - `fp`：预测为正、实际为负
/// - `fn_`：预测为负、实际为正（`fn` 是 Rust 关键字，故用 `fn_`）
/// - `tn`：预测为负、实际为负
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfusionCounts {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
    pub tn: usize,
}

impl ConfusionCounts {
    /// 从 raw predictions 构造混淆矩阵计数。
    ///
    /// `None` 预测视为错误 —— 不是 tp 也不是 tn：
    /// - expected = true,  pred = None → FN（漏报）
    /// - expected = false, pred = None → FP（误报）
    ///
    /// # Panics
    ///
    /// 当 `expected` 和 `predicted` 长度不等时 panic。
    pub fn from_pairs(expected: &[bool], predicted: &[Option<bool>]) -> Self {
        assert_eq!(
            expected.len(),
            predicted.len(),
            "expected and predicted must have the same length"
        );

        let mut counts = Self::default();
        for (exp, pred) in expected.iter().zip(predicted.iter()) {
            match (pred, exp) {
                (Some(true), true) => counts.tp += 1,
                (Some(true), false) => counts.fp += 1,
                (Some(false), true) => counts.fn_ += 1,
                (Some(false), false) => counts.tn += 1,
                // None 预测：系统未能产生有效输出。
                // 若期望为正 → 漏报；若期望为负 → 误报。
                (None, true) => counts.fn_ += 1,
                (None, false) => counts.fp += 1,
            }
        }
        counts
    }

    /// 总样本数 = TP + FP + FN + TN。
    ///
    /// 由于 `None` 被分配到 FP 或 FN，total 始终等于输入长度。
    pub fn total(&self) -> usize {
        self.tp + self.fp + self.fn_ + self.tn
    }
}

/// 单条评测用例。
#[derive(Debug, Clone)]
pub struct EvalCase {
    /// 用例唯一标识
    pub id: String,
    /// 输入文本
    pub input: String,
    /// 期望标签（true = 有风险 / 正例）
    pub expected: bool,
    /// 用例标签（如 "critical", "ambiguous"）
    pub tags: Vec<String>,
}

/// 单次运行结果。
///
/// `prediction` 为 `None` 表示系统未能产生有效输出
/// （timeout / parse error / refusal）。
/// 此时 `error` 必须为非空说明。
#[derive(Debug, Clone)]
pub struct RunResult {
    /// 预测标签，None 表示运行时失败
    pub prediction: Option<bool>,
    /// 失败原因的说明（prediction 为 None 时必须非空）
    pub error: Option<String>,
    /// Trace 文件路径，用于跳转分析失败根因
    pub trace_path: Option<String>,
    /// 本次运行调用的模型次数
    pub model_calls: usize,
    /// 本次运行调用的工具次数
    pub tool_calls: usize,
    /// 延迟（毫秒）
    pub latency_millis: u64,
}

/// 可复现性版本戳。每次评测运行必须携带。
#[derive(Debug, Clone)]
pub struct VersionStamp {
    /// 模型名称 / 版本
    pub model: String,
    /// Prompt 内容的 hash（如 SHA256 前 8 位）
    pub prompt_hash: String,
    /// 数据集版本标识
    pub data_version: String,
    /// 代码版本（git commit hash 或等价标识）
    pub code_version: String,
}

/// 一条完整的评测记录：case + 预期 + 实际结果 + 错误分类 + 版本。
#[derive(Debug, Clone)]
pub struct EvalRecord {
    /// 对应的 case id
    pub case_id: String,
    /// 期望标签
    pub expected: bool,
    /// 实际运行结果
    pub result: RunResult,
    /// 错误分类（空字符串表示正确）
    pub failure_class: String,
    /// 可复现性版本戳
    pub version: VersionStamp,
}

/// 变形关系：应保持的性质类型。
///
/// 用于 metamorphic testing，验证系统在输入变形后是否保持应有的行为。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetamorphicRelation {
    /// 变形前后预测应一致（如同义改写、无关段落插入）
    SamePrediction,
    /// 变形后系统必须拒答（如删除唯一证据后应为 insufficient_evidence）
    MustAbstain,
    /// 变形后预测应翻转（如把 5% 改成 1% 后风险结论应改变）
    MustFlip,
}

/// 待测系统接口。每个被评测的 Agent 都需要实现此 trait。
pub trait SystemUnderTest {
    /// 对给定输入执行一次推理，返回运行结果。
    ///
    /// 即使系统内部 panic 或超时，也应以 `RunResult { prediction: None, error: Some(...), ... }` 的形式返回，
    /// 不得让 panic 传播到评测框架。
    fn run(&mut self, input: &str) -> RunResult;
}

/// 稳定性指标（多次重复运行后计算）。
///
/// 评估非确定性系统在相同输入上多次运行的行为一致性。
#[derive(Debug, Clone, Copy, Default)]
pub struct StabilityMetrics {
    /// 第一次就成功的比例（首次运行 prediction == expected）
    pub pass_at_1: f64,
    /// N 次运行中至少成功一次的比例
    pub pass_at_n: f64,
    /// N 次运行全部给出相同 prediction 的比例
    pub consensus_rate: f64,
    /// 存在至少一对运行结果不同的比例
    pub flip_rate: f64,
}

/// 评测汇总：指标 + 运行统计 + 错误分布。
#[derive(Debug, Clone)]
pub struct EvalSummary {
    /// 核心分类指标
    pub metrics: Metrics,
    /// 唯一 case 数（按 case_id 去重）
    pub total_cases: usize,
    /// 总运行次数（含重复运行）
    pub total_runs: usize,
    /// 运行时失败次数（failure_class = "runtime_failure"）
    pub runtime_failures: usize,
    /// 平均模型调用次数
    pub avg_model_calls: f64,
    /// 平均工具调用次数
    pub avg_tool_calls: f64,
    /// 平均延迟（毫秒）
    pub avg_latency_ms: f64,
    /// 错误分类统计（按出现次数降序排列）
    pub error_class_counts: Vec<(String, usize)>,
}

// ---------------------------------------------------------------------------
// 核心函数实现
// ---------------------------------------------------------------------------

/// 手算 Accuracy / Precision / Recall / F1。
///
/// # 除零约定
///
/// - `TP + FP = 0` → Precision = 0.0
/// - `TP + FN = 0` → Recall = 0.0
/// - `Precision + Recall = 0` → F1 = 0.0
///
/// # 输入约束
///
/// - `expected` 和 `predicted` 长度必须相等，否则 panic。
/// - `None` 预测视为错误（既不算 tp 也不算 tn），留在分母。
/// - 空输入返回全零 `Metrics`。
///
/// # 示例
///
/// ```
/// # use lesson_06_evaluation::evaluate;
/// // TP=1, FP=1, FN=1, TN=1 → 所有指标 = 0.5
/// let m = evaluate(
///     &[true, true, false, false],
///     &[Some(true), Some(false), Some(true), Some(false)],
/// );
/// assert!((m.accuracy - 0.5).abs() < 1e-9);
/// assert!((m.f1 - 0.5).abs() < 1e-9);
/// ```
pub fn evaluate(expected: &[bool], predicted: &[Option<bool>]) -> Metrics {
    assert_eq!(
        expected.len(),
        predicted.len(),
        "expected and predicted must have the same length"
    );

    if expected.is_empty() {
        return Metrics::zeros();
    }

    let counts = ConfusionCounts::from_pairs(expected, predicted);

    let accuracy = if counts.total() == 0 {
        0.0
    } else {
        (counts.tp + counts.tn) as f64 / counts.total() as f64
    };

    // 除零约定：分母为 0 时返回 0.0
    let precision = if counts.tp + counts.fp == 0 {
        0.0
    } else {
        counts.tp as f64 / (counts.tp + counts.fp) as f64
    };

    let recall = if counts.tp + counts.fn_ == 0 {
        0.0
    } else {
        counts.tp as f64 / (counts.tp + counts.fn_) as f64
    };

    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };

    Metrics {
        accuracy,
        precision,
        recall,
        f1,
    }
}

/// 从预测和标签构造混淆矩阵计数。
///
/// 这是 `evaluate` 的底层函数，单独暴露以便测试和报告。
/// 内部委托给 [`ConfusionCounts::from_pairs`]。
///
/// # Panics
///
/// 当 `expected` 和 `predicted` 长度不等时 panic。
pub fn compute_confusion(expected: &[bool], predicted: &[Option<bool>]) -> ConfusionCounts {
    ConfusionCounts::from_pairs(expected, predicted)
}

/// 把单个预测结果归类为错误类型。
///
/// 返回空字符串 `""` 表示预测正确。
/// 本课必须至少区分以下四类：
///
/// - `"runtime_failure"`：系统未能产生有效输出（prediction = None）
/// - `"false_positive"`：系统判 true，实际为 false
/// - `"false_negative"`：系统判 false，实际为 true
/// - `""`（空字符串）：预测正确
pub fn classify_failure(prediction: Option<bool>, expected: bool) -> &'static str {
    match (prediction, expected) {
        (None, _) => "runtime_failure",
        (Some(true), false) => "false_positive",
        (Some(false), true) => "false_negative",
        // (Some(true), true) 或 (Some(false), false)
        _ => "",
    }
}

/// 跑完所有 case，返回与输入一一对应的记录列表。
///
/// # 合约
///
/// - 返回的 `Vec` 长度必须等于 `cases.len()`。
/// - 每个 record 必须携带传入的 `version`。
/// - timeout / parse error / refusal 也必须生成 `EvalRecord`
///   （prediction = None），不得静默丢弃。
/// - 错误分类自动通过 [`classify_failure`] 计算。
pub fn run_cases(
    system: &mut impl SystemUnderTest,
    cases: &[EvalCase],
    version: &VersionStamp,
) -> Vec<EvalRecord> {
    let mut records = Vec::with_capacity(cases.len());

    for case in cases {
        let result = system.run(&case.input);
        let failure_class = classify_failure(result.prediction, case.expected).to_string();
        records.push(EvalRecord {
            case_id: case.id.clone(),
            expected: case.expected,
            result,
            failure_class,
            version: version.clone(),
        });
    }

    records
}

/// 验证变形关系是否成立。
///
/// 三种关系：
///
/// - `SamePrediction`：base 和 transformed 必须都是 `Some` 且相等。
///   任一为 `None`（系统崩溃）时返回 false（无法比较）。
///
/// - `MustAbstain`：transformed 必须为 `None`（系统应拒答）。
///   base 值不影响结果。
///
/// - `MustFlip`：base 和 transformed 必须都是 `Some` 且不相等。
///   任一为 `None` 时返回 false（无法确认翻转）。
///
/// # 示例
///
/// ```
/// # use lesson_06_evaluation::{check_relation, MetamorphicRelation};
/// assert!(check_relation(Some(true), Some(true), MetamorphicRelation::SamePrediction));
/// assert!(check_relation(Some(true), None, MetamorphicRelation::MustAbstain));
/// assert!(check_relation(Some(true), Some(false), MetamorphicRelation::MustFlip));
/// ```
pub fn check_relation(
    base: Option<bool>,
    transformed: Option<bool>,
    relation: MetamorphicRelation,
) -> bool {
    match relation {
        MetamorphicRelation::SamePrediction => match (base, transformed) {
            (Some(b), Some(t)) => b == t,
            // 任一为 None → 无法比较，视为不成立
            _ => false,
        },
        MetamorphicRelation::MustAbstain => {
            // 变形后必须拒答（prediction = None）
            transformed.is_none()
        }
        MetamorphicRelation::MustFlip => match (base, transformed) {
            (Some(b), Some(t)) => b != t,
            // 任一为 None → 无法确认翻转
            _ => false,
        },
    }
}

/// 计算 variant − baseline 的逐字段差值。
///
/// 每个字段独立相减，用于消融实验报告。
/// 正差值表示 variant 优于 baseline，负差值表示变差。
///
/// # 示例
///
/// ```
/// # use lesson_06_evaluation::{Metrics, metric_delta};
/// let baseline = Metrics { accuracy: 0.8, precision: 0.7, recall: 0.6, f1: 0.64 };
/// let variant  = Metrics { accuracy: 0.7, precision: 0.6, recall: 0.5, f1: 0.55 };
/// let delta = metric_delta(&baseline, &variant);
/// assert!(delta.accuracy < 0.0); // variant 更差
/// ```
pub fn metric_delta(baseline: &Metrics, variant: &Metrics) -> Metrics {
    Metrics {
        accuracy: variant.accuracy - baseline.accuracy,
        precision: variant.precision - baseline.precision,
        recall: variant.recall - baseline.recall,
        f1: variant.f1 - baseline.f1,
    }
}

/// 从多条 records 汇总评测摘要。
///
/// 按 `case_id` 去重计算 case 数，汇总所有运行的平均调用/延迟统计，
/// 并计算错误分类分布。
///
/// # 注意
///
/// - 如果同一 case 有多次重复运行，每一次都算作独立的 run。
/// - `total_cases` 按唯一 `case_id` 计数，`total_runs` 按 record 总数计数。
pub fn summarize(records: &[EvalRecord]) -> EvalSummary {
    let total_runs = records.len();

    // 按 case_id 去重得到唯一 case 数
    let mut case_ids: Vec<&str> = records.iter().map(|r| r.case_id.as_str()).collect();
    case_ids.sort();
    case_ids.dedup();
    let total_cases = case_ids.len();

    // 运行时失败数
    let runtime_failures = records
        .iter()
        .filter(|r| r.failure_class == "runtime_failure")
        .count();

    // 收集预测和标签用于计算指标
    let expected: Vec<bool> = records.iter().map(|r| r.expected).collect();
    let predicted: Vec<Option<bool>> = records.iter().map(|r| r.result.prediction).collect();
    let metrics = evaluate(&expected, &predicted);

    // 平均运行统计
    let (sum_model, sum_tool, sum_latency) =
        records
            .iter()
            .fold((0usize, 0usize, 0u64), |(sm, st, sl), r| {
                (
                    sm + r.result.model_calls,
                    st + r.result.tool_calls,
                    sl + r.result.latency_millis,
                )
            });
    let n = if total_runs == 0 { 1 } else { total_runs } as f64;
    let avg_model_calls = sum_model as f64 / n;
    let avg_tool_calls = sum_tool as f64 / n;
    let avg_latency_ms = sum_latency as f64 / n;

    // 错误分类统计（按频次降序排列）
    let mut class_counts: Vec<(String, usize)> = Vec::new();
    for record in records {
        if record.failure_class.is_empty() {
            continue;
        }
        if let Some(existing) = class_counts
            .iter_mut()
            .find(|(c, _)| c == &record.failure_class)
        {
            existing.1 += 1;
        } else {
            class_counts.push((record.failure_class.clone(), 1));
        }
    }
    class_counts.sort_by_key(|b| std::cmp::Reverse(b.1));

    EvalSummary {
        metrics,
        total_cases,
        total_runs,
        runtime_failures,
        avg_model_calls,
        avg_tool_calls,
        avg_latency_ms,
        error_class_counts: class_counts,
    }
}

/// 从多次重复运行的 records 中计算稳定性指标。
///
/// records 应按 case_id 分组：同一 case_id 的多条 record 代表多次重复运行。
///
/// 对于每个 case：
/// - `pass_at_1`：第一次运行就成功的比例（prediction == Some(expected)）
/// - `pass_at_n`：N 次中至少一次成功的比例
/// - `consensus_rate`：N 次全部给出相同 prediction 的比例
/// - `flip_rate`：存在至少一对运行结果不同的比例
///
/// # 空输入
///
/// 空 records 返回全零 `StabilityMetrics`。
pub fn compute_stability(records: &[EvalRecord]) -> StabilityMetrics {
    // 按 case_id 分组
    let mut groups: Vec<Vec<&EvalRecord>> = Vec::new();
    let mut seen_ids: Vec<&str> = Vec::new();

    for record in records {
        if let Some(pos) = seen_ids
            .iter()
            .position(|id| *id == record.case_id.as_str())
        {
            groups[pos].push(record);
        } else {
            seen_ids.push(record.case_id.as_str());
            groups.push(vec![record]);
        }
    }

    if groups.is_empty() {
        return StabilityMetrics::default();
    }

    let total_groups = groups.len() as f64;
    let mut pass_at_1_count = 0usize;
    let mut pass_at_n_count = 0usize;
    let mut consensus_count = 0usize;
    let mut flip_count = 0usize;

    for group in &groups {
        // pass_at_1: 第一次运行就成功
        if let Some(first) = group.first() {
            if first.result.prediction == Some(first.expected) {
                pass_at_1_count += 1;
            }
        }

        // pass_at_n: N 次中至少成功一次
        let any_success = group
            .iter()
            .any(|r| r.result.prediction == Some(r.expected));
        if any_success {
            pass_at_n_count += 1;
        }

        // consensus: 全部 prediction 一致
        if let Some(first) = group.first() {
            let all_same = group
                .iter()
                .all(|r| r.result.prediction == first.result.prediction);
            if all_same {
                consensus_count += 1;
            }
        }

        // flip: 存在不一致的 prediction
        let mut predictions: Vec<Option<bool>> =
            group.iter().map(|r| r.result.prediction).collect();
        // 去重：先按 Some(false) < Some(true) < None 排序
        predictions.sort_by(|a, b| match (a, b) {
            (Some(va), Some(vb)) => va.cmp(vb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        predictions.dedup();
        if predictions.len() > 1 {
            flip_count += 1;
        }
    }

    StabilityMetrics {
        pass_at_1: pass_at_1_count as f64 / total_groups,
        pass_at_n: pass_at_n_count as f64 / total_groups,
        consensus_rate: consensus_count as f64 / total_groups,
        flip_rate: flip_count as f64 / total_groups,
    }
}

// ---------------------------------------------------------------------------
// 报告生成（可选扩展）
// ---------------------------------------------------------------------------

/// 生成 Markdown 格式的评测报告。
///
/// 包含：指标表、运行统计、错误分类、失败样本详情。
/// 每个失败样本附带 `trace_path` 链接以便跳转分析。
///
/// 本函数为便利函数，不是验收测试的必需项。
#[allow(dead_code)]
pub fn generate_report(summary: &EvalSummary, failed_records: &[EvalRecord]) -> String {
    let mut report = String::new();

    // 标题
    report.push_str("# Evaluation Report\n\n");

    // 指标表
    report.push_str("## Metrics\n\n");
    report.push_str("| Metric    | Value |\n");
    report.push_str("|-----------|-------|\n");
    report.push_str(&format!(
        "| Accuracy  | {:.4} |\n",
        summary.metrics.accuracy
    ));
    report.push_str(&format!(
        "| Precision | {:.4} |\n",
        summary.metrics.precision
    ));
    report.push_str(&format!("| Recall    | {:.4} |\n", summary.metrics.recall));
    report.push_str(&format!("| F1        | {:.4} |\n", summary.metrics.f1));
    report.push('\n');

    // 运行统计
    report.push_str("## Run Statistics\n\n");
    report.push_str(&format!("- Total cases: {}\n", summary.total_cases));
    report.push_str(&format!("- Total runs: {}\n", summary.total_runs));
    report.push_str(&format!(
        "- Runtime failures: {}\n",
        summary.runtime_failures
    ));
    report.push_str(&format!(
        "- Avg model calls: {:.2}\n",
        summary.avg_model_calls
    ));
    report.push_str(&format!(
        "- Avg tool calls: {:.2}\n",
        summary.avg_tool_calls
    ));
    report.push_str(&format!(
        "- Avg latency (ms): {:.2}\n",
        summary.avg_latency_ms
    ));
    report.push('\n');

    // 错误分类
    if !summary.error_class_counts.is_empty() {
        report.push_str("## Error Distribution\n\n");
        report.push_str("| Error Class      | Count |\n");
        report.push_str("|------------------|-------|\n");
        for (class, count) in &summary.error_class_counts {
            report.push_str(&format!("| {:<16} | {:>5} |\n", class, count));
        }
        report.push('\n');
    }

    // 失败样本
    if !failed_records.is_empty() {
        report.push_str("## Failed Samples\n\n");
        for (i, record) in failed_records.iter().enumerate() {
            report.push_str(&format!("### {}. Case `{}`\n\n", i + 1, record.case_id));
            report.push_str(&format!("- **Expected**: {}\n", record.expected));
            report.push_str(&format!(
                "- **Predicted**: {}\n",
                record
                    .result
                    .prediction
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "None (failure)".to_string())
            ));
            report.push_str(&format!("- **Failure class**: {}\n", record.failure_class));
            if let Some(ref err) = record.result.error {
                report.push_str(&format!("- **Error**: {}\n", err));
            }
            if let Some(ref trace) = record.result.trace_path {
                report.push_str(&format!("- **Trace**: [{}]({})\n", trace, trace));
            }
            report.push_str(&format!(
                "- **Model calls**: {}, **Tool calls**: {}, **Latency**: {}ms\n",
                record.result.model_calls, record.result.tool_calls, record.result.latency_millis
            ));
            report.push('\n');
        }
    }

    report
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // scaffold
    // -----------------------------------------------------------------------

    #[test]
    fn scaffold_compiles() {
        let m = Metrics::zeros();
        assert_eq!(m.accuracy, 0.0);
        assert_eq!(m.f1, 0.0);
    }

    // -----------------------------------------------------------------------
    // ConfusionCounts 单元测试
    // -----------------------------------------------------------------------

    #[test]
    fn confusion_all_correct() {
        let c = ConfusionCounts::from_pairs(
            &[true, false, true, false],
            &[Some(true), Some(false), Some(true), Some(false)],
        );
        assert_eq!(c.tp, 2);
        assert_eq!(c.tn, 2);
        assert_eq!(c.fp, 0);
        assert_eq!(c.fn_, 0);
    }

    #[test]
    fn confusion_with_none_predictions() {
        // None 期望 true → 漏报（FN）；None 期望 false → 误报（FP）
        let c = ConfusionCounts::from_pairs(&[true, false], &[None, None]);
        assert_eq!(c.tp, 0);
        assert_eq!(c.tn, 0);
        assert_eq!(c.fp, 1); // expected=false, pred=None → FP
        assert_eq!(c.fn_, 1); // expected=true,  pred=None → FN
    }

    #[test]
    fn confusion_total_matches_input_len() {
        let expected = vec![true, false, true, false, true];
        let predicted = vec![Some(true), Some(false), None, Some(false), Some(true)];
        let c = ConfusionCounts::from_pairs(&expected, &predicted);
        assert_eq!(c.total(), expected.len());
    }

    #[test]
    fn confusion_counts_none_as_error_not_ignored() {
        // expected:  [true,  false, true,  false]
        // predicted: [Some(true), None, Some(false), None]
        //
        // idx0: Some(true), true    → TP
        // idx1: None,       false   → FP (None on negative expected = false alarm)
        // idx2: Some(false), true   → FN (miss)
        // idx3: None,       false   → FP (None on negative expected = false alarm)
        //
        // Result: TP=1, FP=2, FN=1, TN=0, total=4
        let c = ConfusionCounts::from_pairs(
            &[true, false, true, false],
            &[Some(true), None, Some(false), None],
        );
        assert_eq!(c.tp, 1);
        assert_eq!(c.fp, 2);
        assert_eq!(c.fn_, 1);
        assert_eq!(c.tn, 0);
        assert_eq!(c.total(), 4);
    }

    #[test]
    fn confusion_perfect_is_clean() {
        let c = ConfusionCounts::from_pairs(
            &[true, true, false, false],
            &[Some(true), Some(true), Some(false), Some(false)],
        );
        assert_eq!(c.tp, 2);
        assert_eq!(c.tn, 2);
        assert_eq!(c.fp, 0);
        assert_eq!(c.fn_, 0);
    }

    // -----------------------------------------------------------------------
    // evaluate 单元测试
    // -----------------------------------------------------------------------

    #[test]
    fn perfect_predictions() {
        let m = evaluate(
            &[true, false, true, false],
            &[Some(true), Some(false), Some(true), Some(false)],
        );
        assert!((m.accuracy - 1.0).abs() < 1e-9);
        assert!((m.precision - 1.0).abs() < 1e-9);
        assert!((m.recall - 1.0).abs() < 1e-9);
        assert!((m.f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn all_wrong() {
        let m = evaluate(
            &[true, true, false, false],
            &[Some(false), Some(false), Some(true), Some(true)],
        );
        assert!((m.accuracy - 0.0).abs() < 1e-9);
        assert!((m.precision - 0.0).abs() < 1e-9);
        assert!((m.recall - 0.0).abs() < 1e-9);
        assert!((m.f1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn all_positive_predictions() {
        // 全部判 true：Recall = 1.0，Precision = 正例比例
        let m = evaluate(&[true, true, false], &[Some(true), Some(true), Some(true)]);
        assert!((m.recall - 1.0).abs() < 1e-9);
        assert!((m.precision - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn all_negative_predictions() {
        // 全部判 false：没有判正的，Precision 分母为 0 → 0.0
        let m = evaluate(
            &[true, false, false],
            &[Some(false), Some(false), Some(false)],
        );
        assert!((m.precision - 0.0).abs() < 1e-9);
        assert!((m.recall - 0.0).abs() < 1e-9);
    }

    #[test]
    fn known_confusion_matrix() {
        // TP=1, FP=1, FN=1, TN=1
        let m = evaluate(
            &[true, true, false, false],
            &[Some(true), Some(false), Some(true), Some(false)],
        );
        assert!((m.accuracy - 0.5).abs() < 1e-9);
        assert!((m.precision - 0.5).abs() < 1e-9);
        assert!((m.recall - 0.5).abs() < 1e-9);
        assert!((m.f1 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn timeout_counts_as_wrong() {
        // None 留在分母：2 个样本中 1 个正确、1 个 None → accuracy = 0.5
        let m = evaluate(&[true, false], &[Some(true), None]);
        assert!((m.accuracy - 0.5).abs() < 1e-9);
    }

    #[test]
    fn empty_input_is_defined() {
        let m = evaluate(&[], &[]);
        assert!((m.accuracy - 0.0).abs() < 1e-9);
        assert!((m.precision - 0.0).abs() < 1e-9);
        assert!((m.recall - 0.0).abs() < 1e-9);
        assert!((m.f1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn all_none_predictions() {
        let m = evaluate(&[true, false, true], &[None, None, None]);
        assert!((m.accuracy - 0.0).abs() < 1e-9);
        assert!((m.precision - 0.0).abs() < 1e-9);
        assert!((m.recall - 0.0).abs() < 1e-9);
        assert!((m.f1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn no_positives_in_ground_truth() {
        // 没有正例时：TP+FN=0，Recall 分母为 0 → 0.0
        let m = evaluate(&[false, false], &[Some(false), Some(false)]);
        assert!((m.accuracy - 1.0).abs() < 1e-9);
        assert!((m.precision - 0.0).abs() < 1e-9); // TP+FP=0
        assert!((m.recall - 0.0).abs() < 1e-9); // TP+FN=0
        assert!((m.f1 - 0.0).abs() < 1e-9);
    }

    #[test]
    #[should_panic(expected = "same length")]
    fn mismatched_lengths_panic() {
        evaluate(&[true, false], &[Some(true)]);
    }

    #[test]
    fn all_timeouts_is_zero_accuracy() {
        let m = evaluate(&[true, false, true], &[None, None, None]);
        assert!((m.accuracy - 0.0).abs() < 1e-9);
    }

    #[test]
    fn no_positive_predictions_is_defined() {
        // 全部判负，Precision 分母为 0 → 0.0
        let m = evaluate(&[false, false], &[Some(false), Some(false)]);
        assert!((m.precision - 0.0).abs() < 1e-9);
    }

    #[test]
    fn all_positive_has_perfect_recall() {
        let m = evaluate(
            &[true, true, false, false],
            &[Some(true), Some(true), Some(true), Some(true)],
        );
        assert!((m.recall - 1.0).abs() < 1e-9);
        assert!((m.precision - 0.5).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // classify_failure 单元测试
    // -----------------------------------------------------------------------

    #[test]
    fn correct_prediction_is_empty_string() {
        assert_eq!(classify_failure(Some(true), true), "");
        assert_eq!(classify_failure(Some(false), false), "");
    }

    #[test]
    fn runtime_failure_on_none() {
        assert_eq!(classify_failure(None, true), "runtime_failure");
        assert_eq!(classify_failure(None, false), "runtime_failure");
    }

    #[test]
    fn false_positive_and_negative() {
        assert_eq!(classify_failure(Some(true), false), "false_positive");
        assert_eq!(classify_failure(Some(false), true), "false_negative");
    }

    // -----------------------------------------------------------------------
    // check_relation 单元测试
    // -----------------------------------------------------------------------

    #[test]
    fn same_prediction_holds() {
        assert!(check_relation(
            Some(true),
            Some(true),
            MetamorphicRelation::SamePrediction,
        ));
        assert!(check_relation(
            Some(false),
            Some(false),
            MetamorphicRelation::SamePrediction,
        ));
    }

    #[test]
    fn same_prediction_fails_on_mismatch() {
        assert!(!check_relation(
            Some(true),
            Some(false),
            MetamorphicRelation::SamePrediction,
        ));
    }

    #[test]
    fn same_prediction_fails_on_any_none() {
        assert!(!check_relation(
            Some(true),
            None,
            MetamorphicRelation::SamePrediction,
        ));
        assert!(!check_relation(
            None,
            Some(true),
            MetamorphicRelation::SamePrediction,
        ));
    }

    #[test]
    fn must_abstain_succeeds_on_none() {
        assert!(check_relation(
            Some(true),
            None,
            MetamorphicRelation::MustAbstain,
        ));
    }

    #[test]
    fn must_abstain_fails_on_some() {
        assert!(!check_relation(
            Some(true),
            Some(false),
            MetamorphicRelation::MustAbstain,
        ));
    }

    #[test]
    fn must_abstain_holds_even_when_base_is_none() {
        // 即使 base 也为 None，transformed 为 None 时 MustAbstain 仍成立
        assert!(check_relation(None, None, MetamorphicRelation::MustAbstain));
    }

    #[test]
    fn must_flip_holds() {
        assert!(check_relation(
            Some(true),
            Some(false),
            MetamorphicRelation::MustFlip,
        ));
        assert!(check_relation(
            Some(false),
            Some(true),
            MetamorphicRelation::MustFlip,
        ));
    }

    #[test]
    fn must_flip_fails_on_same() {
        assert!(!check_relation(
            Some(true),
            Some(true),
            MetamorphicRelation::MustFlip,
        ));
    }

    #[test]
    fn must_flip_fails_on_none() {
        assert!(!check_relation(
            Some(true),
            None,
            MetamorphicRelation::MustFlip,
        ));
        assert!(!check_relation(
            None,
            Some(false),
            MetamorphicRelation::MustFlip,
        ));
    }

    // -----------------------------------------------------------------------
    // metric_delta 单元测试
    // -----------------------------------------------------------------------

    #[test]
    fn delta_subtracts_fieldwise() {
        let b = Metrics {
            accuracy: 0.8,
            precision: 0.7,
            recall: 0.6,
            f1: 0.64,
        };
        let v = Metrics {
            accuracy: 0.7,
            precision: 0.6,
            recall: 0.5,
            f1: 0.55,
        };
        let d = metric_delta(&b, &v);
        assert!((d.accuracy + 0.1).abs() < 1e-9);
        assert!((d.precision + 0.1).abs() < 1e-9);
        assert!((d.recall + 0.1).abs() < 1e-9);
        assert!((d.f1 + 0.09).abs() < 1e-9);
    }

    #[test]
    fn delta_positive_when_variant_better() {
        let b = Metrics {
            accuracy: 0.5,
            precision: 0.5,
            recall: 0.5,
            f1: 0.5,
        };
        let v = Metrics {
            accuracy: 0.9,
            precision: 0.9,
            recall: 0.9,
            f1: 0.9,
        };
        let d = metric_delta(&b, &v);
        assert!(d.accuracy > 0.0);
        assert!(d.precision > 0.0);
        assert!(d.recall > 0.0);
        assert!(d.f1 > 0.0);
    }

    // -----------------------------------------------------------------------
    // run_cases + summarize 单元测试
    // -----------------------------------------------------------------------

    struct ConstantSystem {
        prediction: Option<bool>,
        calls: usize,
    }

    impl SystemUnderTest for ConstantSystem {
        fn run(&mut self, _input: &str) -> RunResult {
            self.calls += 1;
            RunResult {
                prediction: self.prediction,
                error: if self.prediction.is_none() {
                    Some("timeout".into())
                } else {
                    None
                },
                trace_path: Some(format!("traces/{}.jsonl", self.calls)),
                model_calls: 1,
                tool_calls: 0,
                latency_millis: 10,
            }
        }
    }

    fn test_version() -> VersionStamp {
        VersionStamp {
            model: "fake".into(),
            prompt_hash: "abc123".into(),
            data_version: "v1".into(),
            code_version: "c1".into(),
        }
    }

    #[test]
    fn runner_returns_exact_case_count() {
        let cases: Vec<EvalCase> = (0..5)
            .map(|i| EvalCase {
                id: format!("c{i}"),
                input: "test".into(),
                expected: true,
                tags: vec![],
            })
            .collect();
        let mut system = ConstantSystem {
            prediction: Some(true),
            calls: 0,
        };
        let records = run_cases(&mut system, &cases, &test_version());
        assert_eq!(records.len(), 5);
        assert_eq!(system.calls, 5);
    }

    #[test]
    fn runner_preserves_version_on_every_record() {
        let cases = vec![EvalCase {
            id: "a".into(),
            input: "test".into(),
            expected: true,
            tags: vec![],
        }];
        let mut system = ConstantSystem {
            prediction: Some(true),
            calls: 0,
        };
        let records = run_cases(&mut system, &cases, &test_version());
        assert_eq!(records[0].version.prompt_hash, "abc123");
        assert_eq!(records[0].version.code_version, "c1");
    }

    #[test]
    fn runner_handles_runtime_failure() {
        let cases = vec![
            EvalCase {
                id: "ok".into(),
                input: "test".into(),
                expected: true,
                tags: vec![],
            },
            EvalCase {
                id: "fail".into(),
                input: "timeout".into(),
                expected: false,
                tags: vec![],
            },
        ];
        let mut system = ConstantSystem {
            prediction: None,
            calls: 0,
        };
        let records = run_cases(&mut system, &cases, &test_version());
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].failure_class, "runtime_failure");
        assert_eq!(records[1].failure_class, "runtime_failure");
        assert!(records[0].result.error.is_some());
    }

    #[test]
    fn runner_empty_cases_is_ok() {
        let mut system = ConstantSystem {
            prediction: Some(true),
            calls: 0,
        };
        let records = run_cases(&mut system, &[], &test_version());
        assert!(records.is_empty());
    }

    #[test]
    fn summarize_counts_correctly() {
        let version = test_version();
        let records = vec![
            EvalRecord {
                case_id: "a".into(),
                expected: true,
                result: RunResult {
                    prediction: Some(true),
                    error: None,
                    trace_path: Some("t1".into()),
                    model_calls: 2,
                    tool_calls: 1,
                    latency_millis: 100,
                },
                failure_class: "".into(),
                version: version.clone(),
            },
            EvalRecord {
                case_id: "b".into(),
                expected: true,
                result: RunResult {
                    prediction: None,
                    error: Some("timeout".into()),
                    trace_path: None,
                    model_calls: 0,
                    tool_calls: 0,
                    latency_millis: 5000,
                },
                failure_class: "runtime_failure".into(),
                version: version.clone(),
            },
            EvalRecord {
                case_id: "c".into(),
                expected: false,
                result: RunResult {
                    prediction: Some(true),
                    error: None,
                    trace_path: Some("t3".into()),
                    model_calls: 3,
                    tool_calls: 2,
                    latency_millis: 200,
                },
                failure_class: "false_positive".into(),
                version,
            },
        ];
        let summary = summarize(&records);
        assert_eq!(summary.total_cases, 3);
        assert_eq!(summary.total_runs, 3);
        assert_eq!(summary.runtime_failures, 1);
        // 验证错误分类统计
        let fp = summary
            .error_class_counts
            .iter()
            .find(|(c, _)| c == "false_positive")
            .map(|(_, n)| *n);
        assert_eq!(fp, Some(1));
        let rf = summary
            .error_class_counts
            .iter()
            .find(|(c, _)| c == "runtime_failure")
            .map(|(_, n)| *n);
        assert_eq!(rf, Some(1));
    }

    // -----------------------------------------------------------------------
    // compute_stability 单元测试
    // -----------------------------------------------------------------------

    #[test]
    fn stability_all_consistent() {
        let version = test_version();
        let records: Vec<EvalRecord> = (0..3)
            .map(|_| EvalRecord {
                case_id: "a".into(),
                expected: true,
                result: RunResult {
                    prediction: Some(true),
                    error: None,
                    trace_path: Some("t".into()),
                    model_calls: 1,
                    tool_calls: 0,
                    latency_millis: 10,
                },
                failure_class: "".into(),
                version: version.clone(),
            })
            .collect();
        let s = compute_stability(&records);
        assert!((s.pass_at_1 - 1.0).abs() < 1e-9);
        assert!((s.pass_at_n - 1.0).abs() < 1e-9);
        assert!((s.consensus_rate - 1.0).abs() < 1e-9);
        assert!((s.flip_rate - 0.0).abs() < 1e-9);
    }

    #[test]
    fn stability_detects_flips() {
        let version = test_version();
        let records = vec![
            EvalRecord {
                case_id: "a".into(),
                expected: true,
                result: RunResult {
                    prediction: Some(true),
                    error: None,
                    trace_path: Some("t1".into()),
                    model_calls: 1,
                    tool_calls: 0,
                    latency_millis: 10,
                },
                failure_class: "".into(),
                version: version.clone(),
            },
            EvalRecord {
                case_id: "a".into(),
                expected: true,
                result: RunResult {
                    prediction: Some(false),
                    error: None,
                    trace_path: Some("t2".into()),
                    model_calls: 1,
                    tool_calls: 0,
                    latency_millis: 10,
                },
                failure_class: "false_negative".into(),
                version,
            },
        ];
        let s = compute_stability(&records);
        assert!((s.flip_rate - 1.0).abs() < 1e-9);
        assert!((s.consensus_rate - 0.0).abs() < 1e-9);
    }

    #[test]
    fn stability_pass_at_n_succeeds_with_retry() {
        let version = test_version();
        let records = vec![
            EvalRecord {
                case_id: "a".into(),
                expected: true,
                result: RunResult {
                    prediction: Some(false), // 第一次失败
                    error: None,
                    trace_path: Some("t1".into()),
                    model_calls: 1,
                    tool_calls: 0,
                    latency_millis: 10,
                },
                failure_class: "false_negative".into(),
                version: version.clone(),
            },
            EvalRecord {
                case_id: "a".into(),
                expected: true,
                result: RunResult {
                    prediction: Some(true), // 第二次成功
                    error: None,
                    trace_path: Some("t2".into()),
                    model_calls: 2,
                    tool_calls: 0,
                    latency_millis: 12,
                },
                failure_class: "".into(),
                version,
            },
        ];
        let s = compute_stability(&records);
        assert!((s.pass_at_1 - 0.0).abs() < 1e-9);
        assert!((s.pass_at_n - 1.0).abs() < 1e-9);
    }

    #[test]
    fn stability_empty_input_returns_default() {
        let s = compute_stability(&[]);
        assert!((s.pass_at_1 - 0.0).abs() < 1e-9);
        assert!((s.pass_at_n - 0.0).abs() < 1e-9);
        assert!((s.consensus_rate - 0.0).abs() < 1e-9);
        assert!((s.flip_rate - 0.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // VersionStamp 变化测试
    // -----------------------------------------------------------------------

    #[test]
    fn version_changes_are_reflected() {
        let v1 = VersionStamp {
            prompt_hash: "old".into(),
            ..test_version()
        };
        let v2 = VersionStamp {
            prompt_hash: "new".into(),
            ..test_version()
        };
        let cases = vec![EvalCase {
            id: "a".into(),
            input: "test".into(),
            expected: true,
            tags: vec![],
        }];
        let mut system = ConstantSystem {
            prediction: Some(true),
            calls: 0,
        };
        let r1 = run_cases(&mut system, &cases, &v1);
        let r2 = run_cases(&mut system, &cases, &v2);
        assert_eq!(r1[0].version.prompt_hash, "old");
        assert_eq!(r2[0].version.prompt_hash, "new");
    }

    // -----------------------------------------------------------------------
    // 综合测试：F1 计算验证（Lesson PPT 中的 Worked Example）
    // -----------------------------------------------------------------------

    #[test]
    fn worked_example_system_a_all_positive() {
        // 10 个条款，3 个有风险，7 个正常。系统 A 全部判风险。
        let expected = [
            true, true, true, false, false, false, false, false, false, false,
        ];
        let predicted = [
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
            Some(true),
        ];
        let m = evaluate(&expected, &predicted);
        // TP=3, FP=7, FN=0, TN=0
        assert!((m.recall - 1.0).abs() < 1e-9); // 3/3 = 100%
        assert!((m.precision - 0.3).abs() < 1e-4); // 3/10 = 30%
        let expected_f1 = 2.0 * 1.0 * 0.3 / (1.0 + 0.3);
        assert!((m.f1 - expected_f1).abs() < 1e-4);
    }

    #[test]
    fn worked_example_system_b_conservative() {
        // 10 个条款，3 个有风险。系统 B 找到 2 个风险，误报 1 个。
        let expected = [
            true, true, true, false, false, false, false, false, false, false,
        ];
        let predicted = [
            Some(true),
            Some(true),
            Some(false), // 漏掉第 3 个
            Some(true),  // 误报
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
            Some(false),
        ];
        let m = evaluate(&expected, &predicted);
        // TP=2, FP=1, FN=1, TN=6
        assert!((m.recall - 2.0 / 3.0).abs() < 1e-4); // ≈ 67%
        assert!((m.precision - 2.0 / 3.0).abs() < 1e-4); // ≈ 67%
        let expected_f1 = 2.0 * (2.0 / 3.0) * (2.0 / 3.0) / ((2.0 / 3.0) + (2.0 / 3.0));
        assert!((m.f1 - expected_f1).abs() < 1e-4); // ≈ 67%
    }

    // -----------------------------------------------------------------------
    // 综合测试：generate_report 不 panic
    // -----------------------------------------------------------------------

    #[test]
    fn report_generation_includes_failures() {
        let version = test_version();
        let records = vec![EvalRecord {
            case_id: "case-1".into(),
            expected: true,
            result: RunResult {
                prediction: None,
                error: Some("timeout".into()),
                trace_path: Some("traces/case-1.jsonl".into()),
                model_calls: 0,
                tool_calls: 0,
                latency_millis: 5000,
            },
            failure_class: "runtime_failure".into(),
            version,
        }];
        let summary = summarize(&records);
        let report = generate_report(&summary, &records);
        assert!(report.contains("case-1"));
        assert!(report.contains("traces/case-1.jsonl"));
        assert!(report.contains("runtime_failure"));
    }
}
