#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    AskForGoal,
    Record(String),
    Finish(String),
    Stop(&'static str),
}

#[derive(Debug, Clone, Default)]
pub struct AgentState {
    pub turns: usize,
    pub goal: Option<String>,
    pub notes: Vec<String>,
    pub completed: bool,
}

#[derive(Debug, Clone)]
pub enum Observation {
    UserGoal(String),
    Fact(String),
    NoMoreInput,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LabError {
    NotImplemented(&'static str),
    InvalidTransition(&'static str),
}

pub fn choose_action(_state: &AgentState, _max_turns: usize) -> Result<Action, LabError> {
    Err(LabError::NotImplemented("choose_action"))
}

pub fn apply_observation(
    _state: &mut AgentState,
    _observation: Observation,
) -> Result<(), LabError> {
    Err(LabError::NotImplemented("apply_observation"))
}

pub fn run_scripted(
    _observations: &[Observation],
    _max_turns: usize,
) -> Result<Vec<Action>, LabError> {
    Err(LabError::NotImplemented("run_scripted"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
