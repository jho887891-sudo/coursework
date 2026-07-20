use lesson_06_evaluation::*;

// ---------------------------------------------------------------------------
// FakeSystem：脚本化的被测系统，用于确定性验收测试
// ---------------------------------------------------------------------------

/// FakeSystem 通过输入文本控制行为，实现可复现的确定性测试。
///
/// 支持的输入关键字：
/// - `"risk"` → prediction = Some(true)
/// - `"safe"` → prediction = Some(false)
/// - `"timeout"` → prediction = None, error = "timeout after 5000ms"
/// - `"parse_error"` → prediction = None, error = "parse error: invalid JSON"
/// - `"refusal"` → prediction = None, error = "model refused to answer"
/// - 其他 → prediction = Some(false)（默认）
struct FakeSystem;

impl SystemUnderTest for FakeSystem {
    fn run(&mut self, input: &str) -> RunResult {
        match input {
            "risk" => RunResult {
                prediction: Some(true),
                error: None,
                trace_path: Some("traces/risk.jsonl".into()),
                model_calls: 1,
                tool_calls: 2,
                latency_millis: 5,
            },
            "safe" => RunResult {
                prediction: Some(false),
                error: None,
                trace_path: Some("traces/safe.jsonl".into()),
                model_calls: 1,
                tool_calls: 1,
                latency_millis: 3,
            },
            "timeout" => RunResult {
                prediction: None,
                error: Some("timeout after 5000ms".into()),
                trace_path: None,
                model_calls: 0,
                tool_calls: 0,
                latency_millis: 5000,
            },
            "parse_error" => RunResult {
                prediction: None,
                error: Some("parse error: invalid JSON".into()),
                trace_path: Some("traces/parse_error.jsonl".into()),
                model_calls: 1,
                tool_calls: 0,
                latency_millis: 10,
            },
            "refusal" => RunResult {
                prediction: None,
                error: Some("model refused to answer".into()),
                trace_path: Some("traces/refusal.jsonl".into()),
                model_calls: 1,
                tool_calls: 0,
                latency_millis: 8,
            },
            _ => RunResult {
                prediction: Some(false),
                error: None,
                trace_path: Some(format!("traces/{input}.jsonl")),
                model_calls: 1,
                tool_calls: 0,
                latency_millis: 1,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// 测试辅助函数
// ---------------------------------------------------------------------------

fn version() -> VersionStamp {
    VersionStamp {
        model: "fake-v1".into(),
        prompt_hash: "p1".into(),
        data_version: "d1".into(),
        code_version: "c1".into(),
    }
}

fn make_case(id: &str, input: &str, expected: bool) -> EvalCase {
    EvalCase {
        id: id.into(),
        input: input.into(),
        expected,
        tags: vec![],
    }
}

// ===========================================================================
// Checkpoint A：指标计算 — 手算验证
// ===========================================================================

#[test]
fn computes_known_confusion_matrix() {
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
fn perfect_predictions_is_one() {
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
fn all_wrong_is_zero() {
    let m = evaluate(
        &[true, true, false, false],
        &[Some(false), Some(false), Some(true), Some(true)],
    );
    assert!((m.accuracy - 0.0).abs() < 1e-9);
    assert!((m.f1 - 0.0).abs() < 1e-9);
}

#[test]
fn timeout_counts_as_wrong() {
    // None 留在分母：2 个样本中 1 个正确、1 个 None
    let m = evaluate(&[true, false], &[Some(true), None]);
    assert!((m.accuracy - 0.5).abs() < 1e-9);
}

#[test]
fn all_timeouts_is_zero_accuracy() {
    let m = evaluate(&[true, false, true], &[None, None, None]);
    assert!((m.accuracy - 0.0).abs() < 1e-9);
    assert!((m.precision - 0.0).abs() < 1e-9);
    assert!((m.recall - 0.0).abs() < 1e-9);
    assert!((m.f1 - 0.0).abs() < 1e-9);
}

#[test]
fn no_positive_predictions_is_defined() {
    // 全部判负，Precision 分母为 0 → 0.0
    let m = evaluate(&[false, false], &[Some(false), Some(false)]);
    assert!((m.precision - 0.0).abs() < 1e-9);
    assert!((m.recall - 0.0).abs() < 1e-9);
}

#[test]
fn no_positive_ground_truth_is_defined() {
    // 没有正例 → Recall 分母为 0 → 0.0，但 Accuracy 应为 1.0
    let m = evaluate(&[false, false], &[Some(false), Some(false)]);
    assert!((m.accuracy - 1.0).abs() < 1e-9);
    assert!((m.recall - 0.0).abs() < 1e-9);
}

#[test]
fn empty_input_is_defined() {
    let m = evaluate(&[], &[]);
    assert!((m.accuracy - 0.0).abs() < 1e-9);
    assert!((m.f1 - 0.0).abs() < 1e-9);
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

// ===========================================================================
// Checkpoint B：Runner — 所有 case 都被执行
// ===========================================================================

#[test]
fn runner_never_silently_skips_failure() {
    let cases = vec![
        make_case("a", "risk", true),
        make_case("b", "timeout", false),
        make_case("c", "parse_error", true),
    ];
    let records = run_cases(&mut FakeSystem, &cases, &version());
    assert_eq!(records.len(), 3);
    assert_eq!(records[1].failure_class, "runtime_failure");
    assert_eq!(records[2].failure_class, "runtime_failure");
}

#[test]
fn runner_count_matches_input_len() {
    let cases: Vec<EvalCase> = (0..20)
        .map(|i| make_case(&format!("c{i}"), "risk", true))
        .collect();
    let records = run_cases(&mut FakeSystem, &cases, &version());
    assert_eq!(records.len(), 20);
}

#[test]
fn runner_empty_cases_is_ok() {
    let records = run_cases(&mut FakeSystem, &[], &version());
    assert!(records.is_empty());
}

// ===========================================================================
// Checkpoint C：版本与 Trace — 每条 record 可追溯
// ===========================================================================

#[test]
fn every_record_has_version_and_trace() {
    let cases = vec![make_case("a", "risk", true)];
    let records = run_cases(&mut FakeSystem, &cases, &version());
    assert_eq!(records[0].version.prompt_hash, "p1");
    assert_eq!(records[0].version.model, "fake-v1");
    assert!(records[0].result.trace_path.is_some());
}

#[test]
fn version_changes_are_reflected() {
    let v1 = VersionStamp {
        prompt_hash: "old".into(),
        ..version()
    };
    let v2 = VersionStamp {
        prompt_hash: "new".into(),
        ..version()
    };
    let cases = vec![make_case("a", "risk", true)];
    let r1 = run_cases(&mut FakeSystem, &cases, &v1);
    let r2 = run_cases(&mut FakeSystem, &cases, &v2);
    assert_eq!(r1[0].version.prompt_hash, "old");
    assert_eq!(r2[0].version.prompt_hash, "new");
}

// ===========================================================================
// Checkpoint D：错误分类
// ===========================================================================

#[test]
fn classifies_all_four_categories() {
    assert_eq!(classify_failure(Some(true), true), "");
    assert_eq!(classify_failure(Some(false), false), "");
    assert_eq!(classify_failure(Some(true), false), "false_positive");
    assert_eq!(classify_failure(Some(false), true), "false_negative");
    assert_eq!(classify_failure(None, true), "runtime_failure");
    assert_eq!(classify_failure(None, false), "runtime_failure");
}

#[test]
fn record_carries_failure_class() {
    let cases = vec![make_case("fp", "risk", false)]; // 系统判 risk=true，实际 false → FP
    let records = run_cases(&mut FakeSystem, &cases, &version());
    assert_eq!(records[0].failure_class, "false_positive");
}

// ===========================================================================
// Checkpoint E：变形测试（五类关系）
// ===========================================================================

// 1. 同义改写（SamePrediction）
#[test]
fn irrelevant_text_keeps_prediction() {
    // SamePrediction: 相同 → true, 不同 → false, 有 None → false
    assert!(check_relation(
        Some(true),
        Some(true),
        MetamorphicRelation::SamePrediction
    ));
    assert!(!check_relation(
        Some(true),
        Some(false),
        MetamorphicRelation::SamePrediction
    ));
    assert!(!check_relation(
        Some(true),
        None,
        MetamorphicRelation::SamePrediction
    ));
}

// 2. 无关段落插入（SamePrediction — 与上面同义）
#[test]
fn paragraph_order_swap_keeps_prediction() {
    // 交换段落顺序不应改变预测
    assert!(check_relation(
        Some(false),
        Some(false),
        MetamorphicRelation::SamePrediction
    ));
}

// 3. 证据删除（MustAbstain）
#[test]
fn deleted_evidence_must_abstain() {
    assert!(check_relation(
        Some(true),
        None,
        MetamorphicRelation::MustAbstain
    ));
    assert!(!check_relation(
        Some(true),
        Some(false),
        MetamorphicRelation::MustAbstain
    ));
    // 即使 base 也为 None，transformed 为 None 时 MustAbstain 仍成立
    assert!(check_relation(None, None, MetamorphicRelation::MustAbstain));
}

// 4. 数字反事实（MustFlip）
#[test]
fn numeric_counterfactual_must_flip() {
    assert!(check_relation(
        Some(true),
        Some(false),
        MetamorphicRelation::MustFlip
    ));
    assert!(check_relation(
        Some(false),
        Some(true),
        MetamorphicRelation::MustFlip
    ));
    assert!(!check_relation(
        Some(true),
        Some(true),
        MetamorphicRelation::MustFlip
    ));
    assert!(!check_relation(
        None,
        Some(false),
        MetamorphicRelation::MustFlip
    ));
}

// 5. Prompt Injection（MustAbstain — 系统被注入不应产生有效输出）
#[test]
fn prompt_injection_causes_abstention_or_safe() {
    // Prompt Injection 后系统不应继续输出"有风险"的判定
    // 要么拒答（None），要么判 safe（false）——但不能仍判 risk（true）
    let base = Some(true); // 原始输入被判风险
    let after_injection = None; // 注入后系统崩溃/拒答
    assert!(check_relation(
        base,
        after_injection,
        MetamorphicRelation::MustAbstain
    ));
}

// ===========================================================================
// Checkpoint F：消融实验
// ===========================================================================

#[test]
fn delta_reports_variant_minus_baseline() {
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
}

#[test]
fn delta_positive_when_variant_better() {
    let baseline = Metrics {
        accuracy: 0.5,
        precision: 0.5,
        recall: 0.5,
        f1: 0.5,
    };
    let variant = Metrics {
        accuracy: 0.9,
        precision: 0.9,
        recall: 0.9,
        f1: 0.9,
    };
    let d = metric_delta(&baseline, &variant);
    assert!(d.accuracy > 0.0);
    assert!(d.f1 > 0.0);
}

// ===========================================================================
// Checkpoint G：汇总与稳定性
// ===========================================================================

#[test]
fn summarize_counts_correctly() {
    let v = version();
    let records = vec![
        EvalRecord {
            case_id: "a".into(),
            expected: true,
            result: RunResult {
                prediction: Some(true),
                error: None,
                trace_path: Some("t1".into()),
                model_calls: 1,
                tool_calls: 1,
                latency_millis: 10,
            },
            failure_class: "".into(),
            version: v.clone(),
        },
        EvalRecord {
            case_id: "b".into(),
            expected: false,
            result: RunResult {
                prediction: Some(true),
                error: None,
                trace_path: Some("t2".into()),
                model_calls: 2,
                tool_calls: 1,
                latency_millis: 20,
            },
            failure_class: "false_positive".into(),
            version: v.clone(),
        },
        EvalRecord {
            case_id: "c".into(),
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
            version: v,
        },
    ];
    let s = summarize(&records);
    assert_eq!(s.total_cases, 3);
    assert_eq!(s.total_runs, 3);
    assert_eq!(s.runtime_failures, 1);
    // error class counts: runtime_failure=1, false_positive=1
    let rf = s
        .error_class_counts
        .iter()
        .find(|(c, _)| c == "runtime_failure")
        .map(|(_, n)| *n);
    let fp = s
        .error_class_counts
        .iter()
        .find(|(c, _)| c == "false_positive")
        .map(|(_, n)| *n);
    assert_eq!(rf, Some(1));
    assert_eq!(fp, Some(1));
}

#[test]
fn stability_all_consistent() {
    let v = version();
    // 同一 case 跑 3 次，每次 prediction 一致且正确
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
            version: v.clone(),
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
    let v = version();
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
            version: v.clone(),
        },
        EvalRecord {
            case_id: "a".into(),
            expected: true,
            result: RunResult {
                prediction: Some(false),
                error: None,
                trace_path: Some("t2".into()),
                model_calls: 2,
                tool_calls: 0,
                latency_millis: 12,
            },
            failure_class: "false_negative".into(),
            version: v,
        },
    ];
    let s = compute_stability(&records);
    assert!((s.flip_rate - 1.0).abs() < 1e-9);
    assert!((s.consensus_rate - 0.0).abs() < 1e-9);
}

#[test]
fn stability_pass_at_n_succeeds_with_retry() {
    let v = version();
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
            version: v.clone(),
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
            version: v,
        },
    ];
    let s = compute_stability(&records);
    assert!((s.pass_at_1 - 0.0).abs() < 1e-9);
    assert!((s.pass_at_n - 1.0).abs() < 1e-9);
}

// ===========================================================================
// Checkpoint H：混淆矩阵
// ===========================================================================

#[test]
fn confusion_counts_none_as_error() {
    let c = compute_confusion(
        &[true, false, true, false],
        &[Some(true), None, Some(false), None],
    );
    // idx0: Some(true),true → TP
    // idx1: None,false → FP
    // idx2: Some(false),true → FN
    // idx3: None,false → FP
    assert_eq!(c.tp, 1);
    assert_eq!(c.fp, 2);
    assert_eq!(c.fn_, 1);
    assert_eq!(c.tn, 0);
    assert_eq!(c.total(), 4);
}

#[test]
fn confusion_perfect_is_clean() {
    let c = compute_confusion(
        &[true, true, false, false],
        &[Some(true), Some(true), Some(false), Some(false)],
    );
    assert_eq!(c.tp, 2);
    assert_eq!(c.tn, 2);
    assert_eq!(c.fp, 0);
    assert_eq!(c.fn_, 0);
}

// ===========================================================================
// Checkpoint I：Worked Example — PPT 演示验证
// ===========================================================================

#[test]
fn system_a_all_positive() {
    // 10 个条款，3 个有风险。系统 A 全部判风险。
    let expected = [
        true, true, true, false, false, false, false, false, false, false,
    ];
    let predicted = [Some(true); 10];
    let m = evaluate(&expected, &predicted);
    // TP=3, FP=7, FN=0
    assert!((m.recall - 1.0).abs() < 1e-9);
    assert!((m.precision - 0.3).abs() < 1e-4);
}

#[test]
fn system_b_conservative() {
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
    // TP=2, FP=1, FN=1
    assert!((m.recall - 2.0 / 3.0).abs() < 1e-4);
    assert!((m.precision - 2.0 / 3.0).abs() < 1e-4);
}

// ===========================================================================
// Checkpoint J：无失败样本的汇总
// ===========================================================================

#[test]
fn summarize_no_failures_empty_error_classes() {
    let v = version();
    let records: Vec<EvalRecord> = (0..5)
        .map(|i| EvalRecord {
            case_id: format!("c{i}"),
            expected: true,
            result: RunResult {
                prediction: Some(true),
                error: None,
                trace_path: Some(format!("t{i}")),
                model_calls: 1,
                tool_calls: 0,
                latency_millis: 10,
            },
            failure_class: "".into(),
            version: v.clone(),
        })
        .collect();
    let s = summarize(&records);
    assert_eq!(s.total_cases, 5);
    assert_eq!(s.total_runs, 5);
    assert_eq!(s.runtime_failures, 0);
    assert!(s.error_class_counts.is_empty());
}
