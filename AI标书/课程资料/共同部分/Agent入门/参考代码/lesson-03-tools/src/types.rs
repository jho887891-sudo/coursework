use crate::Schema;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadWorkspace,
    ModifyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffect {
    None,
    Reversible,
    Irreversible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Schema,
    pub output_schema: Schema,
    pub side_effect: SideEffect,
    pub required_permissions: Vec<Permission>,
    pub requires_idempotency_key: bool,
    pub timeout_ms: u64,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolRequest {
    pub request_id: String,
    pub name: String,
    pub arguments: Value,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPolicy {
    pub permissions: Vec<Permission>,
    pub max_retries: usize,
    pub retry_transient: bool,
}

impl ExecutionPolicy {
    pub fn read_only(max_retries: usize) -> Self {
        Self {
            permissions: vec![Permission::ReadWorkspace],
            max_retries,
            retry_transient: true,
        }
    }

    pub fn with_state_changes(max_retries: usize) -> Self {
        Self {
            permissions: vec![Permission::ReadWorkspace, Permission::ModifyState],
            max_retries,
            retry_transient: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorKind {
    UnknownTool,
    DuplicateRegistration,
    InvalidArguments,
    PermissionDenied,
    PathOutsideWorkspace,
    Timeout,
    Transient,
    Permanent,
    MalformedOutput,
    OutputTooLarge,
    MissingIdempotencyKey,
    IdempotencyConflict,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolError {
    pub kind: ToolErrorKind,
    pub message: String,
}

impl ToolError {
    pub fn new(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolObservation {
    Success {
        output: Value,
        attempts: usize,
        replayed: bool,
    },
    Failure {
        error: ToolError,
        attempts: usize,
    },
}

impl ToolObservation {
    pub fn error_kind(&self) -> Option<ToolErrorKind> {
        match self {
            ToolObservation::Success { .. } => None,
            ToolObservation::Failure { error, .. } => Some(error.kind),
        }
    }

    pub fn attempts(&self) -> usize {
        match self {
            ToolObservation::Success { attempts, .. }
            | ToolObservation::Failure { attempts, .. } => *attempts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolContext {
    pub request_id: String,
    pub idempotency_key: Option<String>,
}

#[async_trait]
pub trait Tool: Send {
    fn spec(&self) -> ToolSpec;

    async fn call(&mut self, arguments: Value, context: &ToolContext) -> Result<Value, ToolError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTraceEventType {
    RequestRejected,
    AttemptStarted,
    RetryScheduled,
    ToolSucceeded,
    ToolFailed,
    IdempotencyReplayed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolTraceEvent {
    pub request_id: String,
    pub tool_name: String,
    pub attempt: usize,
    pub event_type: ToolTraceEventType,
    pub detail: String,
    pub elapsed_millis: u64,
}
