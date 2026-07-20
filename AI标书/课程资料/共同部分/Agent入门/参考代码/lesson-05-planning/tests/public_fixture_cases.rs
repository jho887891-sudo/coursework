use lesson_05_planning::{
    compare_strategies, run_travel_case, StrategyKind, TravelCase, SHARED_STRATEGY_LIMITS,
};

const CASES: &str = include_str!("../../../公开数据/lesson-05-travel/cases.jsonl");

fn load_cases() -> Vec<TravelCase> {
    CASES
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("public case must be valid JSONL"))
        .collect()
}

#[test]
fn all_thirty_public_cases_match_expected_outcomes() {
    let cases = load_cases();
    assert_eq!(cases.len(), 30);
    for case in cases {
        let result = run_travel_case(&case, StrategyKind::PlanExecuteReplan);
        assert_eq!(
            result.outcome, case.expected,
            "case {} produced an unexpected outcome",
            case.id
        );
        assert_eq!(result.metrics.constraint_violations, 0);
    }
}

#[test]
fn all_strategies_use_the_same_case_set_without_constraint_violations() {
    let cases = load_cases();
    for strategy in [
        StrategyKind::Workflow,
        StrategyKind::React,
        StrategyKind::PlanExecuteReplan,
    ] {
        let runs: Vec<_> = cases
            .iter()
            .map(|case| run_travel_case(case, strategy))
            .collect();
        assert_eq!(runs.len(), cases.len());
        assert!(runs.iter().all(|run| run.limits == SHARED_STRATEGY_LIMITS));
        assert!(runs.iter().all(|run| {
            run.metrics.model_calls <= run.limits.max_model_calls
                && run.metrics.tool_calls <= run.limits.max_tool_calls
                && run.metrics.replans <= run.limits.max_replans
        }));
        assert!(runs
            .iter()
            .all(|run| run.metrics.constraint_violations == 0));
    }
}

#[test]
fn invalid_plans_are_rejected_before_tool_calls() {
    for case in load_cases()
        .into_iter()
        .filter(|case| case.expected == "validator_reject")
    {
        let result = run_travel_case(&case, StrategyKind::PlanExecuteReplan);
        assert_eq!(result.metrics.tool_calls, 0, "case {}", case.id);
    }
}

#[test]
fn comparison_reports_all_required_metrics_on_the_same_thirty_cases() {
    let summaries = compare_strategies(&load_cases());
    assert_eq!(summaries.len(), 3);
    assert!(summaries.iter().all(|summary| summary.case_count == 30));
    assert!(summaries
        .iter()
        .all(|summary| summary.constraint_violation_rate == 0.0));
    assert!(summaries
        .iter()
        .all(|summary| summary.average_model_calls >= 0.0
            && summary.average_tool_calls >= 0.0
            && summary.average_latency_millis >= 0.0));
    let planner = summaries
        .iter()
        .find(|summary| summary.strategy == StrategyKind::PlanExecuteReplan)
        .unwrap();
    assert_eq!(planner.unnecessary_replan_rate, 0.0);
}
