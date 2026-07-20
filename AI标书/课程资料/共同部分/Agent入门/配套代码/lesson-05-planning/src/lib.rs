#[derive(Debug, Clone)]
pub struct Step {
    pub id: String,
    pub tool: String,
    pub depends_on: Vec<String>,
    pub success_condition: String,
}
#[derive(Debug, Clone)]
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
    ExecutionFailed(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Ready,
    Running,
    Succeeded,
    Failed,
    Blocked,
}
#[derive(Debug, Clone)]
pub struct StepRecord {
    pub step_id: String,
    pub status: StepStatus,
    pub observation: Option<String>,
}
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub records: Vec<StepRecord>,
    pub goal_satisfied: bool,
    pub failure_reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplanReason {
    EnvironmentChanged,
    StepFailed,
    GoalNotSatisfied,
}
pub trait StepRunner {
    fn run_step(&mut self, step: &Step) -> Result<String, String>;
}
pub fn validate_and_sort(_plan: &Plan, _allowed: &[&str]) -> Result<Vec<String>, PlanError> {
    Err(PlanError::EmptyGoal)
}
pub fn execute_plan(
    _plan: &Plan,
    _allowed: &[&str],
    _runner: &mut impl StepRunner,
) -> Result<ExecutionResult, PlanError> {
    Err(PlanError::ExecutionFailed("not implemented".into()))
}
pub fn goal_satisfied(_plan: &Plan, _observations: &[String]) -> bool {
    false
}
pub fn should_replan(
    _result: &ExecutionResult,
    _environment_changed: bool,
    _current_replans: usize,
    _max_replans: usize,
) -> Option<ReplanReason> {
    None
}
#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
