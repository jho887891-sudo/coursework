use lesson_06_evaluation::*;
struct FakeSystem;
impl SystemUnderTest for FakeSystem {
    fn run(&mut self, input: &str) -> RunResult {
        RunResult {
            prediction: if input == "timeout" {
                None
            } else {
                Some(input == "risk")
            },
            error: if input == "timeout" {
                Some("timeout".into())
            } else {
                None
            },
            trace_path: Some(format!("traces/{input}.jsonl")),
            model_calls: 1,
            tool_calls: 0,
            latency_millis: 5,
        }
    }
}
fn version() -> VersionStamp {
    VersionStamp {
        model: "fake".into(),
        prompt_hash: "p1".into(),
        data_version: "d1".into(),
        code_version: "c1".into(),
    }
}
#[test]
#[ignore = "compute metrics"]
fn computes_known_confusion_matrix() {
    let m = evaluate(
        &[true, true, false, false],
        &[Some(true), Some(false), Some(true), Some(false)],
    );
    assert_eq!(m.accuracy, 0.5);
    assert_eq!(m.precision, 0.5);
    assert_eq!(m.recall, 0.5);
    assert_eq!(m.f1, 0.5);
}
#[test]
#[ignore = "keep failures in denominator"]
fn timeout_counts_as_wrong() {
    assert_eq!(evaluate(&[true, false], &[Some(true), None]).accuracy, 0.5);
}
#[test]
#[ignore = "define zero division"]
fn no_positive_predictions_is_defined() {
    assert_eq!(evaluate(&[false], &[Some(false)]).precision, 0.0);
}
#[test]
#[ignore = "run every case"]
fn runner_never_silently_skips_failure() {
    let cases = vec![
        EvalCase {
            id: "a".into(),
            input: "risk".into(),
            expected: true,
            tags: vec![],
        },
        EvalCase {
            id: "b".into(),
            input: "timeout".into(),
            expected: false,
            tags: vec![],
        },
    ];
    let records = run_cases(&mut FakeSystem, &cases, &version());
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].failure_class, "runtime_failure");
}
#[test]
#[ignore = "persist reproducibility metadata"]
fn every_record_has_version_and_trace() {
    let cases = [EvalCase {
        id: "a".into(),
        input: "risk".into(),
        expected: true,
        tags: vec![],
    }];
    let r = run_cases(&mut FakeSystem, &cases, &version());
    assert_eq!(r[0].version.prompt_hash, "p1");
    assert!(r[0].result.trace_path.is_some());
}
#[test]
#[ignore = "metamorphic relations"]
fn irrelevant_text_keeps_prediction() {
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
}
#[test]
#[ignore = "single-variable ablation"]
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
}
