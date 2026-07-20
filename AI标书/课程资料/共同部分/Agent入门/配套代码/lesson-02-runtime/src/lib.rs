#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub max_steps: usize,
    pub max_model_calls: usize,
    pub max_tool_calls: usize,
    pub max_millis: u64,
    pub max_repeated_actions: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Continue,
    UseTool(String),
    Finish,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    Completed,
    StepBudget,
    ModelBudget,
    ToolBudget,
    Deadline,
    RepeatedAction,
    ModelError,
    ToolError,
    ParseError,
}
#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub step: usize,
    pub event: String,
    pub detail: String,
    pub elapsed_millis: u64,
}
pub trait Model {
    fn complete(&mut self) -> Result<String, String>;
}
pub trait Environment {
    fn execute(&mut self, tool_name: &str) -> Result<String, String>;
}
pub trait Clock {
    fn now_millis(&mut self) -> u64;
}
pub struct RunResult {
    pub reason: StopReason,
    pub trace: Vec<TraceEvent>,
    pub model_calls: usize,
    pub tool_calls: usize,
}
pub fn parse_action(_raw: &str) -> Result<Action, StopReason> {
    Err(StopReason::ParseError)
}
pub fn run_with(
    _model: &mut impl Model,
    _environment: &mut impl Environment,
    _clock: &mut impl Clock,
    _budget: Budget,
) -> RunResult {
    RunResult {
        reason: StopReason::ParseError,
        trace: vec![],
        model_calls: 0,
        tool_calls: 0,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
