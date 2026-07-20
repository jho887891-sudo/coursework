mod action;
mod fakes;
mod runtime;
mod trace;

pub use action::{
    parse_action, validate_action, ApprovedAction, ProposedAction, ProtocolError,
    ProtocolErrorKind, ToolCall,
};
pub use fakes::{EchoEnvironment, RequireObservation, ScriptedModel, SequenceClock};
pub use runtime::{
    run_with, Budget, Clock, Environment, GoalVerifier, Model, ModelInput, Observation,
    ResourceUsage, RunResult, RuntimeState, StopReason,
};
pub use trace::{
    JsonlTraceWriter, MemoryTraceWriter, NoopTraceWriter, RemainingBudget, TraceEvent,
    TraceEventType, TraceWriter,
};
