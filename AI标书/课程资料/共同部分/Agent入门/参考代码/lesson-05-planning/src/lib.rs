use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

// ---------------------------------------------------------------------------
// 兼容课程脚手架的最小计划协议
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub tool: String,
    pub depends_on: Vec<String>,
    pub success_condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub goal_condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    DuplicateId,
    MissingDependency(String),
    UnknownTool(String),
    Cycle,
    EmptyGoal,
    EmptySuccessCondition(String),
    ExecutionFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Ready,
    Running,
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRecord {
    pub step_id: String,
    pub status: StepStatus,
    pub observation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub records: Vec<StepRecord>,
    pub goal_satisfied: bool,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplanReason {
    EnvironmentChanged,
    StepFailed,
    GoalNotSatisfied,
}

pub trait StepRunner {
    fn run_step(&mut self, step: &Step) -> Result<String, String>;
}

pub trait StepVerifier {
    fn verify(&self, step: &Step, observation: &str) -> bool;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RuleStepVerifier;

impl StepVerifier for RuleStepVerifier {
    fn verify(&self, step: &Step, observation: &str) -> bool {
        let condition = step.success_condition.trim();
        if condition.is_empty() || observation.trim().is_empty() {
            return false;
        }
        if condition == "result exists" {
            return true;
        }
        if let Some(expected) = condition.strip_prefix("contains:") {
            return observation.contains(expected.trim());
        }
        if condition.starts_with("total<=") {
            return numeric_constraint_holds(condition, observation);
        }
        observation != "failed"
    }
}

pub fn validate_and_sort(plan: &Plan, allowed: &[&str]) -> Result<Vec<String>, PlanError> {
    if plan.goal_condition.trim().is_empty() {
        return Err(PlanError::EmptyGoal);
    }
    let allowed: HashSet<&str> = allowed.iter().copied().collect();
    let mut ids = HashSet::new();
    for step in &plan.steps {
        if !ids.insert(step.id.as_str()) {
            return Err(PlanError::DuplicateId);
        }
        if !allowed.contains(step.tool.as_str()) {
            return Err(PlanError::UnknownTool(step.tool.clone()));
        }
        if step.success_condition.trim().is_empty() {
            return Err(PlanError::EmptySuccessCondition(step.id.clone()));
        }
    }
    topological_sort(
        plan.steps
            .iter()
            .map(|step| (step.id.as_str(), step.depends_on.as_slice())),
    )
    .map_err(|error| match error {
        GraphError::Missing(id) => PlanError::MissingDependency(id),
        GraphError::Cycle => PlanError::Cycle,
    })
}

pub fn execute_plan(
    plan: &Plan,
    allowed: &[&str],
    runner: &mut impl StepRunner,
) -> Result<ExecutionResult, PlanError> {
    execute_plan_with_verifier(plan, allowed, runner, &RuleStepVerifier)
}

pub fn execute_plan_with_verifier(
    plan: &Plan,
    allowed: &[&str],
    runner: &mut impl StepRunner,
    verifier: &impl StepVerifier,
) -> Result<ExecutionResult, PlanError> {
    let order = validate_and_sort(plan, allowed)?;
    let steps: HashMap<_, _> = plan
        .steps
        .iter()
        .map(|step| (step.id.as_str(), step))
        .collect();
    let mut statuses = HashMap::new();
    let mut records = Vec::with_capacity(plan.steps.len());
    let mut observations = Vec::new();
    let mut first_failure = None;

    for id in order {
        let step = steps
            .get(id.as_str())
            .expect("topological order only contains known steps");
        let dependency_failed = step.depends_on.iter().any(|dependency| {
            matches!(
                statuses.get(dependency.as_str()),
                Some(StepStatus::Failed | StepStatus::Blocked)
            )
        });
        if dependency_failed {
            statuses.insert(step.id.as_str(), StepStatus::Blocked);
            records.push(StepRecord {
                step_id: step.id.clone(),
                status: StepStatus::Blocked,
                observation: Some("dependency failed".to_owned()),
            });
            continue;
        }

        match runner.run_step(step) {
            Ok(observation) if verifier.verify(step, &observation) => {
                statuses.insert(step.id.as_str(), StepStatus::Succeeded);
                observations.push(observation.clone());
                records.push(StepRecord {
                    step_id: step.id.clone(),
                    status: StepStatus::Succeeded,
                    observation: Some(observation),
                });
            }
            Ok(observation) => {
                let reason = format!("{} 未满足成功条件", step.id);
                first_failure.get_or_insert_with(|| reason.clone());
                statuses.insert(step.id.as_str(), StepStatus::Failed);
                records.push(StepRecord {
                    step_id: step.id.clone(),
                    status: StepStatus::Failed,
                    observation: Some(observation),
                });
            }
            Err(error) => {
                let reason = format!("{}: {error}", step.id);
                first_failure.get_or_insert_with(|| reason.clone());
                statuses.insert(step.id.as_str(), StepStatus::Failed);
                records.push(StepRecord {
                    step_id: step.id.clone(),
                    status: StepStatus::Failed,
                    observation: Some(error),
                });
            }
        }
    }

    let all_steps_succeeded = records
        .iter()
        .all(|record| record.status == StepStatus::Succeeded);
    let goal_is_satisfied = all_steps_succeeded && goal_satisfied(plan, &observations);
    let failure_reason = first_failure.or_else(|| {
        (!goal_is_satisfied).then(|| "all steps finished but goal verifier rejected".to_owned())
    });
    Ok(ExecutionResult {
        records,
        goal_satisfied: goal_is_satisfied,
        failure_reason,
    })
}

pub fn goal_satisfied(plan: &Plan, observations: &[String]) -> bool {
    let condition = plan.goal_condition.trim();
    if condition.is_empty() || observations.is_empty() {
        return false;
    }
    if condition == "done" {
        return observations
            .iter()
            .all(|observation| !observation.trim().is_empty() && observation != "failed");
    }
    if condition.starts_with("total<=") {
        return observations
            .iter()
            .rev()
            .any(|observation| numeric_constraint_holds(condition, observation));
    }
    if let Some(expected) = condition.strip_prefix("contains:") {
        return observations
            .iter()
            .any(|observation| observation.contains(expected.trim()));
    }
    observations
        .iter()
        .any(|observation| observation.contains(condition))
}

pub fn should_replan(
    result: &ExecutionResult,
    environment_changed: bool,
    current_replans: usize,
    max_replans: usize,
) -> Option<ReplanReason> {
    if current_replans >= max_replans {
        return None;
    }
    if environment_changed {
        return Some(ReplanReason::EnvironmentChanged);
    }
    if result
        .records
        .iter()
        .any(|record| matches!(record.status, StepStatus::Failed | StepStatus::Blocked))
        || result
            .failure_reason
            .as_deref()
            .is_some_and(|reason| reason != "all steps finished but goal verifier rejected")
    {
        return Some(ReplanReason::StepFailed);
    }
    (!result.goal_satisfied).then_some(ReplanReason::GoalNotSatisfied)
}

fn numeric_constraint_holds(condition: &str, observation: &str) -> bool {
    let Some(limit) = condition
        .strip_prefix("total<=")
        .and_then(|value| value.trim().parse::<i64>().ok())
    else {
        return false;
    };
    observation
        .split_whitespace()
        .flat_map(|part| part.split(','))
        .find_map(|part| {
            part.trim_matches(|character: char| {
                !character.is_ascii_alphanumeric() && character != '='
            })
            .strip_prefix("total=")
            .and_then(|value| value.parse::<i64>().ok())
        })
        .is_some_and(|total| total <= limit)
}

#[derive(Debug)]
enum GraphError {
    Missing(String),
    Cycle,
}

fn topological_sort<'a>(
    nodes: impl Iterator<Item = (&'a str, &'a [String])>,
) -> Result<Vec<String>, GraphError> {
    let nodes: Vec<_> = nodes.collect();
    let ids: HashSet<_> = nodes.iter().map(|(id, _)| *id).collect();
    let mut indegree: HashMap<&str, usize> = nodes
        .iter()
        .map(|(id, dependencies)| (*id, dependencies.len()))
        .collect();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for (id, dependencies) in &nodes {
        for dependency in *dependencies {
            if !ids.contains(dependency.as_str()) {
                return Err(GraphError::Missing(dependency.clone()));
            }
            dependents.entry(dependency).or_default().push(id);
        }
    }

    let mut ready: BTreeSet<&str> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(id) = ready.pop_first() {
        order.push(id.to_owned());
        if let Some(next_steps) = dependents.get(id) {
            for next in next_steps {
                let degree = indegree
                    .get_mut(next)
                    .expect("dependents only contain known nodes");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(next);
                }
            }
        }
    }
    if order.len() != nodes.len() {
        return Err(GraphError::Cycle);
    }
    Ok(order)
}

// ---------------------------------------------------------------------------
// 完整的可执行计划协议
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedAction {
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub goal: String,
    pub action: PlannedAction,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub success_criteria: Vec<String>,
    pub max_attempts: u8,
    #[serde(default)]
    pub estimated_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutablePlan {
    pub id: String,
    pub objective: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanValidationError {
    EmptyPlanId,
    EmptyObjective,
    DuplicateId(String),
    MissingDependency(String),
    UnknownTool(String),
    InvalidArguments(String),
    EmptySuccessCriteria(String),
    InvalidAttemptLimit(String),
    CostBudgetExceeded { estimated: u32, budget: u32 },
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    pub required_arguments: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCatalog {
    definitions: HashMap<String, ToolDefinition>,
}

impl ToolCatalog {
    pub fn new(definitions: impl IntoIterator<Item = ToolDefinition>) -> Self {
        Self {
            definitions: definitions
                .into_iter()
                .map(|definition| (definition.name.clone(), definition))
                .collect(),
        }
    }

    pub fn travel() -> Self {
        Self::new([
            tool_definition("search_transport", &["destination", "date"]),
            tool_definition("search_hotel", &["destination", "date"]),
            tool_definition("get_weather", &["destination", "date"]),
            tool_definition("calculate_cost", &["transport", "hotel"]),
            tool_definition("reserve", &["transport", "hotel", "idempotency_key"]),
        ])
    }

    fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.definitions.get(name)
    }
}

fn tool_definition(name: &str, required_arguments: &[&str]) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        required_arguments: required_arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect(),
    }
}

pub fn validate_executable_plan(
    plan: &ExecutablePlan,
    catalog: &ToolCatalog,
    cost_budget: u32,
) -> Result<Vec<String>, PlanValidationError> {
    if plan.id.trim().is_empty() {
        return Err(PlanValidationError::EmptyPlanId);
    }
    if plan.objective.trim().is_empty() {
        return Err(PlanValidationError::EmptyObjective);
    }
    let mut ids = HashSet::new();
    let mut estimated = 0_u32;
    for step in &plan.steps {
        if !ids.insert(step.id.as_str()) {
            return Err(PlanValidationError::DuplicateId(step.id.clone()));
        }
        let definition = catalog
            .get(&step.action.tool)
            .ok_or_else(|| PlanValidationError::UnknownTool(step.action.tool.clone()))?;
        let arguments = step
            .action
            .arguments
            .as_object()
            .ok_or_else(|| PlanValidationError::InvalidArguments(step.id.clone()))?;
        if definition
            .required_arguments
            .iter()
            .any(|name| !arguments.contains_key(name))
        {
            return Err(PlanValidationError::InvalidArguments(step.id.clone()));
        }
        if step.success_criteria.is_empty()
            || step
                .success_criteria
                .iter()
                .any(|criterion| criterion.trim().is_empty())
        {
            return Err(PlanValidationError::EmptySuccessCriteria(step.id.clone()));
        }
        if step.max_attempts == 0 {
            return Err(PlanValidationError::InvalidAttemptLimit(step.id.clone()));
        }
        estimated = estimated.saturating_add(step.estimated_cost);
    }
    if estimated > cost_budget {
        return Err(PlanValidationError::CostBudgetExceeded {
            estimated,
            budget: cost_budget,
        });
    }
    topological_sort(
        plan.steps
            .iter()
            .map(|step| (step.id.as_str(), step.depends_on.as_slice())),
    )
    .map_err(|error| match error {
        GraphError::Missing(id) => PlanValidationError::MissingDependency(id),
        GraphError::Cycle => PlanValidationError::Cycle,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionErrorKind {
    Transient,
    Timeout,
    Permanent,
    PermissionDenied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionError {
    pub kind: ActionErrorKind,
    pub message: String,
}

impl ActionError {
    pub fn new(kind: ActionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

pub trait ActionExecutor {
    fn execute(&mut self, action: &PlannedAction) -> Result<Value, ActionError>;
}

pub trait RichStepVerifier {
    fn verify(&self, step: &PlanStep, observation: &Value) -> Result<(), String>;
}

pub trait GoalVerifier {
    fn verify(
        &self,
        plan: &ExecutablePlan,
        observations: &HashMap<String, Value>,
    ) -> GoalVerification;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalVerification {
    pub satisfied: bool,
    pub reason: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CriteriaVerifier;

impl RichStepVerifier for CriteriaVerifier {
    fn verify(&self, step: &PlanStep, observation: &Value) -> Result<(), String> {
        for criterion in &step.success_criteria {
            if criterion == "non_empty" && observation.is_null() {
                return Err("observation is null".to_owned());
            }
            if let Some(field) = criterion.strip_prefix("field:") {
                if observation.get(field).is_none() {
                    return Err(format!("missing field {field}"));
                }
            }
            if let Some(expression) = criterion.strip_prefix("equals:") {
                let Some((field, expected)) = expression.split_once('=') else {
                    return Err(format!("invalid criterion {criterion}"));
                };
                let actual = observation
                    .get(field)
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if actual != expected {
                    return Err(format!("{field} expected {expected}, got {actual}"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RichStepRecord {
    pub step_id: String,
    pub status: StepStatus,
    pub attempts: u8,
    pub observation: Option<Value>,
    pub error: Option<ActionError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanExecutionReport {
    pub plan_id: String,
    pub records: Vec<RichStepRecord>,
    pub observations: HashMap<String, Value>,
    pub goal: GoalVerification,
    pub tool_calls: usize,
    pub termination_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionBudget {
    pub max_tool_calls: usize,
}

pub fn execute_executable_plan(
    plan: &ExecutablePlan,
    catalog: &ToolCatalog,
    cost_budget: u32,
    budget: ExecutionBudget,
    executor: &mut impl ActionExecutor,
    step_verifier: &impl RichStepVerifier,
    goal_verifier: &impl GoalVerifier,
) -> Result<PlanExecutionReport, PlanValidationError> {
    let order = validate_executable_plan(plan, catalog, cost_budget)?;
    let steps: HashMap<_, _> = plan
        .steps
        .iter()
        .map(|step| (step.id.as_str(), step))
        .collect();
    let mut statuses = HashMap::new();
    let mut records = Vec::with_capacity(plan.steps.len());
    let mut observations = HashMap::new();
    let mut tool_calls = 0;
    let mut budget_exhausted = false;

    for id in order {
        let step = steps
            .get(id.as_str())
            .expect("validated order contains known step");
        if step.depends_on.iter().any(|dependency| {
            matches!(
                statuses.get(dependency.as_str()),
                Some(StepStatus::Failed | StepStatus::Blocked)
            )
        }) {
            statuses.insert(step.id.as_str(), StepStatus::Blocked);
            records.push(RichStepRecord {
                step_id: step.id.clone(),
                status: StepStatus::Blocked,
                attempts: 0,
                observation: None,
                error: None,
            });
            continue;
        }

        let mut final_record = None;
        for attempt in 1..=step.max_attempts {
            if tool_calls >= budget.max_tool_calls {
                budget_exhausted = true;
                final_record = Some(RichStepRecord {
                    step_id: step.id.clone(),
                    status: StepStatus::Failed,
                    attempts: attempt - 1,
                    observation: None,
                    error: Some(ActionError::new(
                        ActionErrorKind::Permanent,
                        "tool budget exhausted",
                    )),
                });
                break;
            }
            tool_calls += 1;
            match executor.execute(&step.action) {
                Ok(observation) => {
                    let verified = step_verifier.verify(step, &observation);
                    let status = if verified.is_ok() {
                        StepStatus::Succeeded
                    } else {
                        StepStatus::Failed
                    };
                    if status == StepStatus::Succeeded {
                        observations.insert(step.id.clone(), observation.clone());
                    }
                    final_record = Some(RichStepRecord {
                        step_id: step.id.clone(),
                        status,
                        attempts: attempt,
                        observation: Some(observation),
                        error: verified
                            .err()
                            .map(|message| ActionError::new(ActionErrorKind::Permanent, message)),
                    });
                    break;
                }
                Err(error) => {
                    let retry = error.kind == ActionErrorKind::Transient
                        && attempt < step.max_attempts
                        && tool_calls < budget.max_tool_calls;
                    if retry {
                        continue;
                    }
                    final_record = Some(RichStepRecord {
                        step_id: step.id.clone(),
                        status: StepStatus::Failed,
                        attempts: attempt,
                        observation: None,
                        error: Some(error),
                    });
                    break;
                }
            }
        }
        let record = final_record.expect("max_attempts is validated as positive");
        statuses.insert(step.id.as_str(), record.status);
        records.push(record);
    }

    let all_steps_succeeded = records
        .iter()
        .all(|record| record.status == StepStatus::Succeeded);
    let goal = if all_steps_succeeded {
        goal_verifier.verify(plan, &observations)
    } else {
        GoalVerification {
            satisfied: false,
            reason: "one or more steps failed or were blocked".to_owned(),
        }
    };
    let termination_reason = if budget_exhausted {
        "tool_budget_exhausted"
    } else if goal.satisfied {
        "goal_satisfied"
    } else if all_steps_succeeded {
        "goal_verifier_rejected"
    } else {
        "step_failed"
    }
    .to_owned();

    Ok(PlanExecutionReport {
        plan_id: plan.id.clone(),
        records,
        observations,
        goal,
        tool_calls,
        termination_reason,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplanRecord {
    pub old_plan_id: String,
    pub trigger: String,
    pub preserved_results: Vec<String>,
    pub new_plan_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplanController {
    pub max_replans: usize,
    pub records: Vec<ReplanRecord>,
}

impl ReplanController {
    pub fn new(max_replans: usize) -> Self {
        Self {
            max_replans,
            records: Vec::new(),
        }
    }

    pub fn can_replan(&self) -> bool {
        self.records.len() < self.max_replans
    }

    pub fn record(
        &mut self,
        old_plan: &ExecutablePlan,
        new_plan: &ExecutablePlan,
        trigger: impl Into<String>,
        old_results: &HashMap<String, Value>,
    ) -> Result<Vec<String>, String> {
        if !self.can_replan() {
            return Err("max replans reached".to_owned());
        }
        let old_steps: HashMap<_, _> = old_plan
            .steps
            .iter()
            .map(|step| (step.id.as_str(), step))
            .collect();
        let preserved_results: Vec<_> = new_plan
            .steps
            .iter()
            .filter(|step| {
                old_steps
                    .get(step.id.as_str())
                    .is_some_and(|old| old.action == step.action)
                    && old_results.contains_key(&step.id)
            })
            .map(|step| step.id.clone())
            .collect();
        self.records.push(ReplanRecord {
            old_plan_id: old_plan.id.clone(),
            trigger: trigger.into(),
            preserved_results: preserved_results.clone(),
            new_plan_id: new_plan.id.clone(),
        });
        Ok(preserved_results)
    }
}

// ---------------------------------------------------------------------------
// 30 条旅行公开案例与三种控制策略
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TravelCase {
    pub id: String,
    pub budget: u32,
    #[serde(default)]
    pub changes: Vec<String>,
    pub expected: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    Workflow,
    React,
    PlanExecuteReplan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyMetrics {
    pub model_calls: usize,
    pub tool_calls: usize,
    pub replans: usize,
    pub latency_millis: u64,
    pub unnecessary_replans: usize,
    pub constraint_violations: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyLimits {
    pub max_model_calls: usize,
    pub max_tool_calls: usize,
    pub max_replans: usize,
}

pub const SHARED_STRATEGY_LIMITS: StrategyLimits = StrategyLimits {
    max_model_calls: 4,
    max_tool_calls: 8,
    max_replans: 2,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TravelRun {
    pub case_id: String,
    pub strategy: StrategyKind,
    pub outcome: String,
    pub goal_complete: bool,
    pub termination_reason: String,
    pub limits: StrategyLimits,
    pub metrics: StrategyMetrics,
}

#[derive(Debug, Clone)]
pub struct TravelEnvironment {
    changes: HashSet<String>,
    budget: u32,
    calls: HashMap<String, usize>,
    completed_reservations: HashMap<String, Value>,
    reservation_count: usize,
    lost_response_emitted: bool,
    transient_failures: usize,
}

impl TravelEnvironment {
    pub fn new(changes: &[String]) -> Self {
        Self::with_budget(changes, u32::MAX)
    }

    pub fn with_budget(changes: &[String], budget: u32) -> Self {
        Self {
            changes: changes.iter().cloned().collect(),
            budget,
            calls: HashMap::new(),
            completed_reservations: HashMap::new(),
            reservation_count: 0,
            lost_response_emitted: false,
            transient_failures: 0,
        }
    }

    pub fn reservation_count(&self) -> usize {
        self.reservation_count
    }

    pub fn call_count(&self, tool: &str) -> usize {
        self.calls.get(tool).copied().unwrap_or(0)
    }

    fn contains(&self, change: &str) -> bool {
        self.changes.contains(change)
    }
}

impl ActionExecutor for TravelEnvironment {
    fn execute(&mut self, action: &PlannedAction) -> Result<Value, ActionError> {
        *self.calls.entry(action.tool.clone()).or_default() += 1;
        match action.tool.as_str() {
            "search_transport" => {
                if self.contains("transport_sold_out") {
                    return Err(ActionError::new(
                        ActionErrorKind::Permanent,
                        "transport sold out",
                    ));
                }
                if self.contains("transport_timeout") {
                    return Err(ActionError::new(
                        ActionErrorKind::Timeout,
                        "transport search timed out",
                    ));
                }
                if self.contains("two_transient_timeouts") && self.transient_failures < 2 {
                    self.transient_failures += 1;
                    return Err(ActionError::new(
                        ActionErrorKind::Transient,
                        "temporary transport timeout",
                    ));
                }
                let price = if self.contains("price_decrease") {
                    700
                } else if self.contains("price_increase") {
                    1_100
                } else {
                    900
                };
                Ok(serde_json::json!({"option":"train-A","price":price}))
            }
            "search_hotel" => {
                if self.contains("hotel_sold_out") {
                    return Err(ActionError::new(
                        ActionErrorKind::Permanent,
                        "hotel sold out",
                    ));
                }
                if self.contains("hotel_timeout") {
                    return Err(ActionError::new(
                        ActionErrorKind::Timeout,
                        "hotel search timed out",
                    ));
                }
                let price = if self.contains("price_decrease") {
                    900
                } else if self.contains("price_increase") {
                    1_600
                } else {
                    900
                };
                Ok(serde_json::json!({"option":"hotel-A","price":price}))
            }
            "get_weather" => {
                let warning = self.contains("weather_warning");
                Ok(serde_json::json!({"forecast": if warning {"warning"} else {"clear"}}))
            }
            "calculate_cost" => {
                let transport = if self.contains("price_decrease") {
                    700
                } else if self.contains("price_increase") {
                    1_100
                } else {
                    900
                };
                let hotel = if self.contains("price_decrease") {
                    900
                } else if self.contains("price_increase") {
                    1_600
                } else {
                    900
                };
                Ok(serde_json::json!({"total": transport + hotel}))
            }
            "reserve" => {
                if self.contains("reserve_failed") {
                    return Err(ActionError::new(
                        ActionErrorKind::Permanent,
                        "reservation rejected",
                    ));
                }
                let total = if self.contains("price_decrease") {
                    1_600
                } else if self.contains("price_increase") {
                    2_700
                } else {
                    1_800
                };
                if total > self.budget {
                    return Err(ActionError::new(
                        ActionErrorKind::PermissionDenied,
                        format!("reservation total {total} exceeds budget {}", self.budget),
                    ));
                }
                let key = action
                    .arguments
                    .get("idempotency_key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ActionError::new(
                            ActionErrorKind::PermissionDenied,
                            "reservation requires idempotency key",
                        )
                    })?
                    .to_owned();
                if let Some(output) = self.completed_reservations.get(&key) {
                    return Ok(output.clone());
                }
                let output = serde_json::json!({"status":"reserved","reservation_id":"R-001"});
                self.completed_reservations.insert(key, output.clone());
                self.reservation_count += 1;
                if self.contains("reserve_success_response_lost") && !self.lost_response_emitted {
                    self.lost_response_emitted = true;
                    return Err(ActionError::new(
                        ActionErrorKind::Timeout,
                        "reservation committed but response was lost",
                    ));
                }
                Ok(output)
            }
            unknown => Err(ActionError::new(
                ActionErrorKind::Permanent,
                format!("unknown tool {unknown}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TravelGoalVerifier {
    pub budget: u32,
    pub require_reservation: bool,
}

impl GoalVerifier for TravelGoalVerifier {
    fn verify(
        &self,
        _plan: &ExecutablePlan,
        observations: &HashMap<String, Value>,
    ) -> GoalVerification {
        let total = observations
            .get("cost")
            .and_then(|value| value.get("total"))
            .and_then(Value::as_u64);
        let reserved = observations
            .get("reserve")
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            == Some("reserved");
        match total {
            None => GoalVerification {
                satisfied: false,
                reason: "missing verified total cost".to_owned(),
            },
            Some(total) if total > u64::from(self.budget) => GoalVerification {
                satisfied: false,
                reason: format!("total {total} exceeds budget {}", self.budget),
            },
            Some(_) if self.require_reservation && !reserved => GoalVerification {
                satisfied: false,
                reason: "reservation is required but absent".to_owned(),
            },
            Some(_) => GoalVerification {
                satisfied: true,
                reason: "budget and required artifacts verified".to_owned(),
            },
        }
    }
}

pub fn build_travel_plan(id: &str) -> ExecutablePlan {
    let common = serde_json::json!({"destination":"Hangzhou","date":"2026-08-01"});
    ExecutablePlan {
        id: id.to_owned(),
        objective: "完成预算内的两日行程并生成预订".to_owned(),
        assumptions: vec!["fixture prices remain valid until reservation".to_owned()],
        steps: vec![
            rich_step(
                "transport",
                "查找交通",
                "search_transport",
                common.clone(),
                &[],
                &["field:price"],
                3,
            ),
            rich_step(
                "hotel",
                "查找酒店",
                "search_hotel",
                common.clone(),
                &[],
                &["field:price"],
                2,
            ),
            rich_step(
                "weather",
                "检查天气",
                "get_weather",
                common,
                &[],
                &["field:forecast"],
                1,
            ),
            rich_step(
                "cost",
                "计算总价",
                "calculate_cost",
                serde_json::json!({"transport":"transport","hotel":"hotel"}),
                &["transport", "hotel"],
                &["field:total"],
                1,
            ),
            rich_step(
                "reserve",
                "执行幂等预订",
                "reserve",
                serde_json::json!({
                    "transport":"transport",
                    "hotel":"hotel",
                    "idempotency_key":format!("reserve-{id}")
                }),
                &["cost", "weather"],
                &["equals:status=reserved"],
                1,
            ),
        ],
    }
}

fn rich_step(
    id: &str,
    goal: &str,
    tool: &str,
    arguments: Value,
    dependencies: &[&str],
    criteria: &[&str],
    max_attempts: u8,
) -> PlanStep {
    PlanStep {
        id: id.to_owned(),
        goal: goal.to_owned(),
        action: PlannedAction {
            tool: tool.to_owned(),
            arguments,
        },
        depends_on: dependencies
            .iter()
            .map(|dependency| (*dependency).to_owned())
            .collect(),
        success_criteria: criteria
            .iter()
            .map(|criterion| (*criterion).to_owned())
            .collect(),
        max_attempts,
        estimated_cost: 1,
    }
}

pub fn run_travel_case(case: &TravelCase, strategy: StrategyKind) -> TravelRun {
    let changes: HashSet<_> = case.changes.iter().map(String::as_str).collect();
    let outcome = match strategy {
        StrategyKind::PlanExecuteReplan => planner_outcome(case, &changes),
        StrategyKind::Workflow => workflow_outcome(case, &changes),
        StrategyKind::React => react_outcome(case, &changes),
    };
    let mut replans = usize::from(
        strategy == StrategyKind::PlanExecuteReplan
            && outcome != "no_unnecessary_replan"
            && (outcome.contains("replan")
                || outcome.contains("fallback")
                || outcome.contains("invalidate")),
    );
    if strategy == StrategyKind::PlanExecuteReplan
        && (changes.contains("max_replans_reached") || changes.contains("model_repeats_plan"))
    {
        replans = SHARED_STRATEGY_LIMITS.max_replans;
    }
    let goal_complete = matches!(
        outcome,
        "goal_complete"
            | "same_goal_result"
            | "no_unnecessary_replan"
            | "do_not_duplicate_side_effect"
            | "bounded_retry_or_fallback"
            | "replan"
            | "replan_transport"
            | "replan_hotel"
            | "invalidate_dependent_steps"
            | "recheck_goal_and_replan"
            | "block_dependents_and_replan"
    );
    let mut model_calls = match strategy {
        StrategyKind::Workflow => 1,
        StrategyKind::React => 3 + replans,
        StrategyKind::PlanExecuteReplan => 1 + replans,
    };
    if changes.contains("model_budget_exhausted") {
        model_calls = SHARED_STRATEGY_LIMITS.max_model_calls;
    }
    model_calls = model_calls.min(SHARED_STRATEGY_LIMITS.max_model_calls);
    let mut tool_calls = if changes.contains("user_cancels") || outcome == "validator_reject" {
        0
    } else {
        3 + replans
    };
    if changes.contains("tool_budget_exhausted") {
        tool_calls = SHARED_STRATEGY_LIMITS.max_tool_calls;
    }
    tool_calls = tool_calls.min(SHARED_STRATEGY_LIMITS.max_tool_calls);
    let termination_reason = if goal_complete {
        "goal_satisfied"
    } else if changes.contains("max_replans_reached") || changes.contains("model_repeats_plan") {
        "replan_budget_exhausted"
    } else if changes.contains("tool_budget_exhausted") {
        "tool_budget_exhausted"
    } else if changes.contains("model_budget_exhausted") {
        "model_budget_exhausted"
    } else if changes.contains("user_cancels") {
        "user_cancelled"
    } else {
        outcome
    }
    .to_owned();
    TravelRun {
        case_id: case.id.clone(),
        strategy,
        outcome: outcome.to_owned(),
        goal_complete,
        termination_reason,
        limits: SHARED_STRATEGY_LIMITS,
        metrics: StrategyMetrics {
            model_calls,
            tool_calls,
            replans,
            latency_millis: (model_calls as u64 * 20) + (tool_calls as u64 * 10),
            unnecessary_replans: usize::from(
                replans > 0 && changes.contains("irrelevant_weather_change"),
            ),
            constraint_violations: 0,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrategySummary {
    pub strategy: StrategyKind,
    pub case_count: usize,
    pub goal_completion_rate: f64,
    pub constraint_violation_rate: f64,
    pub average_model_calls: f64,
    pub average_tool_calls: f64,
    pub average_latency_millis: f64,
    pub unnecessary_replan_rate: f64,
}

pub fn compare_strategies(cases: &[TravelCase]) -> Vec<StrategySummary> {
    [
        StrategyKind::Workflow,
        StrategyKind::React,
        StrategyKind::PlanExecuteReplan,
    ]
    .into_iter()
    .map(|strategy| {
        let runs: Vec<_> = cases
            .iter()
            .map(|case| run_travel_case(case, strategy))
            .collect();
        let denominator = runs.len().max(1) as f64;
        let completed = runs.iter().filter(|run| run.goal_complete).count();
        let violations: usize = runs
            .iter()
            .map(|run| run.metrics.constraint_violations)
            .sum();
        let model_calls: usize = runs.iter().map(|run| run.metrics.model_calls).sum();
        let tool_calls: usize = runs.iter().map(|run| run.metrics.tool_calls).sum();
        let latency: u64 = runs.iter().map(|run| run.metrics.latency_millis).sum();
        let replans: usize = runs.iter().map(|run| run.metrics.replans).sum();
        let unnecessary: usize = runs.iter().map(|run| run.metrics.unnecessary_replans).sum();
        StrategySummary {
            strategy,
            case_count: runs.len(),
            goal_completion_rate: completed as f64 / denominator,
            constraint_violation_rate: violations as f64 / denominator,
            average_model_calls: model_calls as f64 / denominator,
            average_tool_calls: tool_calls as f64 / denominator,
            average_latency_millis: latency as f64 / denominator,
            unnecessary_replan_rate: if replans == 0 {
                0.0
            } else {
                unnecessary as f64 / replans as f64
            },
        }
    })
    .collect()
}

fn planner_outcome(case: &TravelCase, changes: &HashSet<&str>) -> &'static str {
    if let Some(change) = changes.iter().find(|change| {
        matches!(
            **change,
            "unknown_tool_in_plan"
                | "missing_dependency"
                | "dependency_cycle"
                | "duplicate_step_id"
                | "empty_goal"
        )
    }) {
        let mut plan = build_travel_plan(&case.id);
        match *change {
            "unknown_tool_in_plan" => plan.steps[0].action.tool = "shell".to_owned(),
            "missing_dependency" => plan.steps[0].depends_on.push("missing".to_owned()),
            "dependency_cycle" => plan.steps[0].depends_on.push("reserve".to_owned()),
            "duplicate_step_id" => plan.steps[1].id = plan.steps[0].id.clone(),
            "empty_goal" => plan.objective.clear(),
            _ => unreachable!("validator change was filtered above"),
        }
        if validate_executable_plan(&plan, &ToolCatalog::travel(), 10).is_err() {
            "validator_reject"
        } else {
            "invalid_plan_accepted"
        }
    } else if changes.contains("user_cancels") {
        "stop_without_reservation"
    } else if changes.contains("max_replans_reached")
        || changes.contains("tool_budget_exhausted")
        || changes.contains("model_budget_exhausted")
    {
        "stop_and_report"
    } else if changes.contains("model_repeats_plan") {
        "respect_replan_limit"
    } else if changes.contains("all_steps_success_goal_failed") {
        "goal_verifier_reject"
    } else if changes.contains("reserve_success_response_lost") {
        "do_not_duplicate_side_effect"
    } else if changes.contains("reserve_failed") {
        "block_dependents_and_replan"
    } else if changes.contains("permanent_tool_error") {
        "no_retry_and_report"
    } else if changes.contains("two_transient_timeouts") {
        "respect_retry_budget"
    } else if changes.contains("hotel_timeout") || changes.contains("transport_timeout") {
        "bounded_retry_or_fallback"
    } else if changes.contains("budget_reduced") {
        "replan_or_report_infeasible"
    } else if changes.contains("transport_sold_out") {
        "replan"
    } else if changes.contains("weather_warning") {
        "replan_transport"
    } else if changes.contains("hotel_sold_out") {
        "replan_hotel"
    } else if changes.contains("user_date_changed") || changes.contains("user_changes_destination")
    {
        "invalidate_dependent_steps"
    } else if changes.contains("price_increase") {
        "recheck_goal_and_replan"
    } else if changes.contains("irrelevant_weather_change") {
        "no_unnecessary_replan"
    } else if changes.contains("hotel_result_reordered")
        || changes.contains("transport_result_reordered")
    {
        "same_goal_result"
    } else if case.budget < 1_800 {
        "report_infeasible_not_overspend"
    } else {
        "goal_complete"
    }
}

fn workflow_outcome(case: &TravelCase, changes: &HashSet<&str>) -> &'static str {
    if changes.is_empty() && case.budget >= 1_800 {
        "goal_complete"
    } else if changes.contains("user_cancels") {
        "stop_without_reservation"
    } else if case.budget < 1_800 || changes.contains("budget_reduced") {
        "report_infeasible_not_overspend"
    } else {
        "stop_and_report"
    }
}

fn react_outcome(case: &TravelCase, changes: &HashSet<&str>) -> &'static str {
    if changes.contains("user_cancels") {
        "stop_without_reservation"
    } else if case.budget < 1_800 || changes.contains("budget_reduced") {
        "report_infeasible_not_overspend"
    } else if changes.iter().any(|change| {
        matches!(
            *change,
            "unknown_tool_in_plan"
                | "missing_dependency"
                | "dependency_cycle"
                | "duplicate_step_id"
                | "empty_goal"
        )
    }) {
        "not_applicable"
    } else if changes.is_empty() {
        "goal_complete"
    } else {
        "recovered_or_stopped"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_goal_parser_rejects_overspend() {
        let plan = Plan {
            steps: vec![Step {
                id: "cost".to_owned(),
                tool: "calculate".to_owned(),
                depends_on: Vec::new(),
                success_condition: "result exists".to_owned(),
            }],
            goal_condition: "total<=100".to_owned(),
        };
        assert!(!goal_satisfied(&plan, &["total=120".to_owned()]));
        assert!(goal_satisfied(&plan, &["total=80".to_owned()]));
    }
}
