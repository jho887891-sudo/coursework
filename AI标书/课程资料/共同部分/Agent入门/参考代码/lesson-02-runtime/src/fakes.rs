use crate::GoalVerifier;
use crate::{Clock, Environment, Model, ModelInput, Observation, RuntimeState, ToolCall};
use std::collections::VecDeque;

#[derive(Debug)]
pub struct ScriptedModel {
    replies: VecDeque<Result<String, String>>,
    pub calls: usize,
    pub inputs: Vec<ModelInput>,
}

impl ScriptedModel {
    pub fn new(replies: impl IntoIterator<Item = Result<String, String>>) -> Self {
        Self {
            replies: replies.into_iter().collect(),
            calls: 0,
            inputs: Vec::new(),
        }
    }
}

impl Model for ScriptedModel {
    fn complete(&mut self, input: &ModelInput) -> Result<String, String> {
        self.calls += 1;
        self.inputs.push(input.clone());
        self.replies
            .pop_front()
            .unwrap_or_else(|| Err("scripted model exhausted".to_owned()))
    }
}

#[derive(Debug, Default)]
pub struct EchoEnvironment {
    pub calls: Vec<ToolCall>,
    pub fail_with: Option<String>,
}

impl Environment for EchoEnvironment {
    fn execute(&mut self, call: &ToolCall) -> Result<Observation, String> {
        self.calls.push(call.clone());
        if let Some(error) = self.fail_with.take() {
            return Err(error);
        }
        Ok(Observation {
            tool_name: call.name.clone(),
            output: call.text.clone(),
        })
    }
}

#[derive(Debug)]
pub struct SequenceClock {
    values: VecDeque<u64>,
    last: u64,
}

impl SequenceClock {
    pub fn new(values: impl IntoIterator<Item = u64>) -> Self {
        Self {
            values: values.into_iter().collect(),
            last: 0,
        }
    }
}

impl Clock for SequenceClock {
    fn now_millis(&mut self) -> u64 {
        if let Some(value) = self.values.pop_front() {
            self.last = value;
        }
        self.last
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RequireObservation;

impl GoalVerifier for RequireObservation {
    fn is_complete(&mut self, state: &RuntimeState, answer: &str) -> bool {
        !state.observations.is_empty() && !answer.trim().is_empty()
    }
}
