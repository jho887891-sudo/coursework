use lesson_05_planning::*;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn rich_validator_rejects_duplicate_missing_unknown_and_cycle() {
    let catalog = ToolCatalog::travel();

    let mut duplicate = build_travel_plan("duplicate");
    duplicate.steps[1].id = duplicate.steps[0].id.clone();
    assert!(matches!(
        validate_executable_plan(&duplicate, &catalog, 100),
        Err(PlanValidationError::DuplicateId(_))
    ));

    let mut missing = build_travel_plan("missing");
    missing.steps[3].depends_on.push("not-found".to_owned());
    assert_eq!(
        validate_executable_plan(&missing, &catalog, 100),
        Err(PlanValidationError::MissingDependency(
            "not-found".to_owned()
        ))
    );

    let mut unknown = build_travel_plan("unknown");
    unknown.steps[0].action.tool = "shell".to_owned();
    assert_eq!(
        validate_executable_plan(&unknown, &catalog, 100),
        Err(PlanValidationError::UnknownTool("shell".to_owned()))
    );

    let mut cycle = build_travel_plan("cycle");
    cycle.steps[0].depends_on.push("reserve".to_owned());
    assert_eq!(
        validate_executable_plan(&cycle, &catalog, 100),
        Err(PlanValidationError::Cycle)
    );
}

#[test]
fn rich_validator_checks_arguments_criteria_attempts_and_cost() {
    let catalog = ToolCatalog::travel();

    let mut invalid_arguments = build_travel_plan("args");
    invalid_arguments.steps[0].action.arguments = json!({});
    assert!(matches!(
        validate_executable_plan(&invalid_arguments, &catalog, 100),
        Err(PlanValidationError::InvalidArguments(_))
    ));

    let mut no_criteria = build_travel_plan("criteria");
    no_criteria.steps[0].success_criteria.clear();
    assert!(matches!(
        validate_executable_plan(&no_criteria, &catalog, 100),
        Err(PlanValidationError::EmptySuccessCriteria(_))
    ));

    let mut zero_attempts = build_travel_plan("attempts");
    zero_attempts.steps[0].max_attempts = 0;
    assert!(matches!(
        validate_executable_plan(&zero_attempts, &catalog, 100),
        Err(PlanValidationError::InvalidAttemptLimit(_))
    ));

    assert_eq!(
        validate_executable_plan(&build_travel_plan("cost"), &catalog, 4),
        Err(PlanValidationError::CostBudgetExceeded {
            estimated: 5,
            budget: 4
        })
    );
}

#[test]
fn executor_runs_ready_steps_and_verifies_the_goal_separately() {
    let plan = build_travel_plan("base");
    let mut environment = TravelEnvironment::new(&[]);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 8 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 3_000,
            require_reservation: true,
        },
    )
    .unwrap();
    assert!(report.goal.satisfied);
    assert_eq!(report.tool_calls, 5);
    assert_eq!(environment.reservation_count(), 1);
    assert_eq!(report.termination_reason, "goal_satisfied");
}

#[test]
fn all_steps_can_succeed_while_goal_verifier_rejects_overspend() {
    let mut plan = build_travel_plan("overspend");
    plan.steps.retain(|step| step.id != "reserve");
    let changes = vec!["price_increase".to_owned()];
    let mut environment = TravelEnvironment::with_budget(&changes, 2_500);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 8 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 2_500,
            require_reservation: false,
        },
    )
    .unwrap();
    assert!(report
        .records
        .iter()
        .all(|record| record.status == StepStatus::Succeeded));
    assert!(!report.goal.satisfied);
    assert_eq!(report.termination_reason, "goal_verifier_rejected");
}

#[test]
fn irreversible_reservation_is_guarded_by_the_budget_constraint() {
    let plan = build_travel_plan("guarded-reservation");
    let changes = vec!["price_increase".to_owned()];
    let mut environment = TravelEnvironment::with_budget(&changes, 2_500);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 8 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 2_500,
            require_reservation: true,
        },
    )
    .unwrap();
    let reserve = report
        .records
        .iter()
        .find(|record| record.step_id == "reserve")
        .unwrap();
    assert_eq!(reserve.status, StepStatus::Failed);
    assert_eq!(
        reserve.error.as_ref().unwrap().kind,
        ActionErrorKind::PermissionDenied
    );
    assert_eq!(environment.reservation_count(), 0);
}

#[test]
fn permanent_failure_blocks_descendants() {
    let plan = build_travel_plan("sold-out");
    let changes = vec!["transport_sold_out".to_owned()];
    let mut environment = TravelEnvironment::new(&changes);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 8 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 3_000,
            require_reservation: true,
        },
    )
    .unwrap();
    let statuses: HashMap<_, _> = report
        .records
        .iter()
        .map(|record| (record.step_id.as_str(), record.status))
        .collect();
    assert_eq!(statuses["transport"], StepStatus::Failed);
    assert_eq!(statuses["cost"], StepStatus::Blocked);
    assert_eq!(statuses["reserve"], StepStatus::Blocked);
}

#[test]
fn only_transient_errors_are_retried_within_step_budget() {
    let plan = build_travel_plan("transient");
    let changes = vec!["two_transient_timeouts".to_owned()];
    let mut environment = TravelEnvironment::new(&changes);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 10 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 3_000,
            require_reservation: true,
        },
    )
    .unwrap();
    let transport = report
        .records
        .iter()
        .find(|record| record.step_id == "transport")
        .unwrap();
    assert_eq!(transport.status, StepStatus::Succeeded);
    assert_eq!(transport.attempts, 3);
    assert_eq!(environment.call_count("search_transport"), 3);
}

#[test]
fn timeout_is_not_retried_automatically() {
    let plan = build_travel_plan("timeout");
    let changes = vec!["transport_timeout".to_owned()];
    let mut environment = TravelEnvironment::new(&changes);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 10 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 3_000,
            require_reservation: true,
        },
    )
    .unwrap();
    let transport = report
        .records
        .iter()
        .find(|record| record.step_id == "transport")
        .unwrap();
    assert_eq!(transport.status, StepStatus::Failed);
    assert_eq!(transport.attempts, 1);
    assert_eq!(environment.call_count("search_transport"), 1);
}

#[test]
fn response_lost_after_reservation_does_not_duplicate_side_effect() {
    let changes = vec!["reserve_success_response_lost".to_owned()];
    let mut environment = TravelEnvironment::new(&changes);
    let action = build_travel_plan("lost-response")
        .steps
        .into_iter()
        .find(|step| step.id == "reserve")
        .unwrap()
        .action;
    assert_eq!(
        environment.execute(&action).unwrap_err().kind,
        ActionErrorKind::Timeout
    );
    assert_eq!(environment.reservation_count(), 1);
    assert!(environment.execute(&action).is_ok());
    assert_eq!(environment.reservation_count(), 1);
}

#[test]
fn replanning_preserves_only_compatible_successful_results() {
    let old_plan = build_travel_plan("old");
    let mut new_plan = build_travel_plan("new");
    new_plan.steps[1].action.arguments["date"] = json!("2026-08-02");
    let results = HashMap::from([
        ("transport".to_owned(), json!({"price":900})),
        ("hotel".to_owned(), json!({"price":900})),
    ]);
    let mut controller = ReplanController::new(1);
    let preserved = controller
        .record(&old_plan, &new_plan, "user changed date", &results)
        .unwrap();
    assert_eq!(preserved, vec!["transport"]);
    assert_eq!(controller.records[0].trigger, "user changed date");
    assert_eq!(
        controller.record(&new_plan, &old_plan, "again", &results),
        Err("max replans reached".to_owned())
    );
}

#[test]
fn tool_budget_stops_before_an_extra_action() {
    let plan = build_travel_plan("budget");
    let mut environment = TravelEnvironment::new(&[]);
    let report = execute_executable_plan(
        &plan,
        &ToolCatalog::travel(),
        10,
        ExecutionBudget { max_tool_calls: 2 },
        &mut environment,
        &CriteriaVerifier,
        &TravelGoalVerifier {
            budget: 3_000,
            require_reservation: true,
        },
    )
    .unwrap();
    assert_eq!(report.tool_calls, 2);
    assert_eq!(report.termination_reason, "tool_budget_exhausted");
    assert_eq!(environment.reservation_count(), 0);
}
