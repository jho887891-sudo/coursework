use lesson_05_planning::*;
use std::collections::HashMap;
fn step(id: &str, deps: &[&str]) -> Step {
    Step {
        id: id.into(),
        tool: "search".into(),
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        success_condition: "result exists".into(),
    }
}
struct FakeRunner {
    results: HashMap<String, Result<String, String>>,
    order: Vec<String>,
}
impl StepRunner for FakeRunner {
    fn run_step(&mut self, s: &Step) -> Result<String, String> {
        self.order.push(s.id.clone());
        self.results
            .remove(&s.id)
            .unwrap_or_else(|| Ok("ok".into()))
    }
}
#[test]
#[ignore = "topological validation"]
fn valid_plan_is_sorted() {
    let p = Plan {
        steps: vec![step("b", &["a"]), step("a", &[])],
        goal_condition: "done".into(),
    };
    assert_eq!(validate_and_sort(&p, &["search"]).unwrap(), vec!["a", "b"]);
}
#[test]
#[ignore = "reject missing dependency"]
fn missing_dependency_is_rejected() {
    let p = Plan {
        steps: vec![step("b", &["missing"])],
        goal_condition: "done".into(),
    };
    assert_eq!(
        validate_and_sort(&p, &["search"]),
        Err(PlanError::MissingDependency("missing".into()))
    );
}
#[test]
#[ignore = "reject unknown tools"]
fn unknown_tool_is_rejected() {
    let mut s = step("a", &[]);
    s.tool = "shell".into();
    let p = Plan {
        steps: vec![s],
        goal_condition: "done".into(),
    };
    assert_eq!(
        validate_and_sort(&p, &["search"]),
        Err(PlanError::UnknownTool("shell".into()))
    );
}
#[test]
#[ignore = "detect cycles"]
fn cycle_is_rejected() {
    let p = Plan {
        steps: vec![step("a", &["b"]), step("b", &["a"])],
        goal_condition: "done".into(),
    };
    assert_eq!(validate_and_sort(&p, &["search"]), Err(PlanError::Cycle));
}
#[test]
#[ignore = "execute ready steps in dependency order"]
fn executor_uses_topological_order() {
    let p = Plan {
        steps: vec![step("b", &["a"]), step("a", &[])],
        goal_condition: "done".into(),
    };
    let mut r = FakeRunner {
        results: HashMap::new(),
        order: vec![],
    };
    let _ = execute_plan(&p, &["search"], &mut r);
    assert_eq!(r.order, vec!["a", "b"]);
}
#[test]
#[ignore = "block descendants after failure"]
fn failed_step_blocks_dependents() {
    let p = Plan {
        steps: vec![step("a", &[]), step("b", &["a"])],
        goal_condition: "done".into(),
    };
    let mut r = FakeRunner {
        results: [("a".into(), Err("timeout".into()))].into(),
        order: vec![],
    };
    let out = execute_plan(&p, &["search"], &mut r).unwrap();
    assert!(out
        .records
        .iter()
        .any(|x| x.step_id == "b" && x.status == StepStatus::Blocked));
}
#[test]
#[ignore = "separate goal from steps"]
fn successful_steps_do_not_imply_goal() {
    let p = Plan {
        steps: vec![step("a", &[])],
        goal_condition: "total<=100".into(),
    };
    assert!(!goal_satisfied(
        &p,
        &["step a success".to_string(), "total=120".to_string()]
    ));
}
#[test]
#[ignore = "bounded replan"]
fn replan_stops_at_limit() {
    let out = ExecutionResult {
        records: vec![],
        goal_satisfied: false,
        failure_reason: Some("sold out".into()),
    };
    assert_eq!(
        should_replan(&out, false, 0, 1),
        Some(ReplanReason::StepFailed)
    );
    assert_eq!(should_replan(&out, false, 1, 1), None);
}
