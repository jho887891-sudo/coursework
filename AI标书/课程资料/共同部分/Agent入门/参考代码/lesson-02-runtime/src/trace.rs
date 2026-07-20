use crate::{Budget, ResourceUsage};
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceEventType {
    RunStarted,
    ModelCallStarted,
    ModelReturned,
    ModelFailed,
    ActionParsed,
    ActionApproved,
    ActionRejected,
    ToolStarted,
    ToolSucceeded,
    ToolFailed,
    FinishRejected,
    TraceWriteFailed,
    RunTerminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemainingBudget {
    pub steps: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub millis: u64,
}

impl RemainingBudget {
    pub fn from(budget: Budget, usage: ResourceUsage, elapsed_millis: u64) -> Self {
        Self {
            steps: budget.max_steps.saturating_sub(usage.steps),
            model_calls: budget.max_model_calls.saturating_sub(usage.model_calls),
            tool_calls: budget.max_tool_calls.saturating_sub(usage.tool_calls),
            millis: budget.max_millis.saturating_sub(elapsed_millis),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub run_id: String,
    pub step: usize,
    pub event_type: TraceEventType,
    pub detail: String,
    pub elapsed_millis: u64,
    pub usage: ResourceUsage,
    pub remaining: RemainingBudget,
}

pub trait TraceWriter {
    fn write(&mut self, event: &TraceEvent) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct NoopTraceWriter;

impl TraceWriter for NoopTraceWriter {
    fn write(&mut self, _event: &TraceEvent) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct MemoryTraceWriter {
    pub events: Vec<TraceEvent>,
}

impl TraceWriter for MemoryTraceWriter {
    fn write(&mut self, event: &TraceEvent) -> Result<(), String> {
        self.events.push(event.clone());
        Ok(())
    }
}

pub struct JsonlTraceWriter<W: Write> {
    writer: W,
}

impl<W: Write> JsonlTraceWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write> TraceWriter for JsonlTraceWriter<W> {
    fn write(&mut self, event: &TraceEvent) -> Result<(), String> {
        serde_json::to_writer(&mut self.writer, event).map_err(|error| error.to_string())?;
        self.writer
            .write_all(b"\n")
            .map_err(|error| error.to_string())
    }
}
