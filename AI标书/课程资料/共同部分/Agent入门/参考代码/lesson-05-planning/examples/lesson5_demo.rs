use lesson_05_planning::*;
use serde_json::json;
use std::collections::HashMap;

const CASES: &str = include_str!("../../../公开数据/lesson-05-travel/cases.jsonl");

fn main() {
    let scenario = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "graph".to_owned());
    match scenario.as_str() {
        "graph" => graph_demo(),
        "blocked" => blocked_demo(),
        "goal-failed" => goal_failed_demo(),
        "replan" => replan_demo(),
        "compare" => compare_demo(),
        other => {
            eprintln!("未知场景：{other}；可选 graph、blocked、goal-failed、replan、compare");
            std::process::exit(2);
        }
    }
}

fn graph_demo() {
    let mut plan = build_travel_plan("cycle-demo");
    plan.steps[0].depends_on.push("reserve".to_owned());
    let result = validate_executable_plan(&plan, &ToolCatalog::travel(), 10);
    println!("输入是合法 JSON，但 transport → ... → reserve → transport 构成环。");
    println!("VALIDATION={result:?}");
}

fn blocked_demo() {
    let plan = build_travel_plan("blocked-demo");
    let mut environment = TravelEnvironment::new(&["transport_sold_out".to_owned()]);
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
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn goal_failed_demo() {
    let mut plan = build_travel_plan("goal-demo");
    plan.steps.retain(|step| step.id != "reserve");
    let mut environment = TravelEnvironment::with_budget(&["price_increase".to_owned()], 2_500);
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
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    println!("注意：每个 Step 都是 Succeeded，但 GoalVerifier 拒绝超预算结果。");
}

fn replan_demo() {
    let old_plan = build_travel_plan("plan-v1");
    let mut new_plan = build_travel_plan("plan-v2");
    new_plan.steps[1].action.arguments["date"] = json!("2026-08-02");
    let old_results = HashMap::from([
        ("transport".to_owned(), json!({"price":900})),
        ("hotel".to_owned(), json!({"price":900})),
    ]);
    let mut controller = ReplanController::new(1);
    let preserved = controller
        .record(&old_plan, &new_plan, "user_date_changed", &old_results)
        .unwrap();
    println!("PRESERVED={preserved:?}");
    println!(
        "{}",
        serde_json::to_string_pretty(&controller.records).unwrap()
    );
    println!("酒店日期改变，因此只保留仍兼容的 transport 结果。");
}

fn compare_demo() {
    let cases: Vec<TravelCase> = CASES
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&compare_strategies(&cases)).unwrap()
    );
}
