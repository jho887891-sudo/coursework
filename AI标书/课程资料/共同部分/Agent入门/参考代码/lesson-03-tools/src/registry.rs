use crate::{
    ExecutionPolicy, Permission, Tool, ToolContext, ToolError, ToolErrorKind, ToolObservation,
    ToolRequest, ToolSpec, ToolTraceEvent, ToolTraceEventType,
};
use serde_json::Value;
use std::{collections::HashMap, time::Instant};
use tokio::time::{timeout, Duration};

#[derive(Debug, Clone)]
struct CompletedCall {
    arguments_fingerprint: String,
    output: Value,
}

#[derive(Default)]
pub struct Registry {
    tools: HashMap<String, Box<dyn Tool>>,
    completed: HashMap<(String, String), CompletedCall>,
    trace: Vec<ToolTraceEvent>,
}

impl Registry {
    pub fn register(&mut self, tool: Box<dyn Tool>) -> Result<(), ToolError> {
        let spec = tool.spec();
        if self.tools.contains_key(&spec.name) {
            return Err(ToolError::new(
                ToolErrorKind::DuplicateRegistration,
                format!("工具 {} 已注册", spec.name),
            ));
        }
        self.tools.insert(spec.name, tool);
        Ok(())
    }

    pub fn definitions(&self) -> Vec<ToolSpec> {
        let mut definitions: Vec<_> = self.tools.values().map(|tool| tool.spec()).collect();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }

    pub fn trace(&self) -> &[ToolTraceEvent] {
        &self.trace
    }

    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }

    fn record(
        &mut self,
        request: &ToolRequest,
        attempt: usize,
        event_type: ToolTraceEventType,
        detail: impl Into<String>,
        started: Instant,
    ) {
        self.trace.push(ToolTraceEvent {
            request_id: request.request_id.clone(),
            tool_name: request.name.clone(),
            attempt,
            event_type,
            detail: detail.into(),
            elapsed_millis: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    fn reject(
        &mut self,
        request: &ToolRequest,
        error: ToolError,
        started: Instant,
    ) -> ToolObservation {
        self.record(
            request,
            0,
            ToolTraceEventType::RequestRejected,
            error.message.clone(),
            started,
        );
        ToolObservation::Failure { error, attempts: 0 }
    }

    pub async fn execute(
        &mut self,
        request: &ToolRequest,
        policy: &ExecutionPolicy,
    ) -> ToolObservation {
        let started = Instant::now();
        let spec = match self.tools.get(&request.name) {
            Some(tool) => tool.spec(),
            None => {
                return self.reject(
                    request,
                    ToolError::new(
                        ToolErrorKind::UnknownTool,
                        format!("未知工具：{}", request.name),
                    ),
                    started,
                );
            }
        };

        let encoded_input = match serde_json::to_vec(&request.arguments) {
            Ok(value) => value,
            Err(error) => {
                return self.reject(
                    request,
                    ToolError::new(
                        ToolErrorKind::InvalidArguments,
                        format!("参数无法序列化：{error}"),
                    ),
                    started,
                );
            }
        };
        if encoded_input.len() > spec.max_input_bytes {
            return self.reject(
                request,
                ToolError::new(
                    ToolErrorKind::InvalidArguments,
                    format!(
                        "输入大小 {} bytes 超过限制 {} bytes",
                        encoded_input.len(),
                        spec.max_input_bytes
                    ),
                ),
                started,
            );
        }
        if let Err(message) = spec.input_schema.validate(&request.arguments) {
            return self.reject(
                request,
                ToolError::new(ToolErrorKind::InvalidArguments, message),
                started,
            );
        }
        if let Some(missing) = spec
            .required_permissions
            .iter()
            .find(|permission| !policy.permissions.contains(permission))
        {
            return self.reject(
                request,
                ToolError::new(
                    ToolErrorKind::PermissionDenied,
                    format!("缺少权限：{}", permission_name(*missing)),
                ),
                started,
            );
        }
        if spec.requires_idempotency_key
            && request.idempotency_key.as_deref().is_none_or(str::is_empty)
        {
            return self.reject(
                request,
                ToolError::new(
                    ToolErrorKind::MissingIdempotencyKey,
                    "副作用工具必须提供非空 idempotency_key",
                ),
                started,
            );
        }

        let arguments_fingerprint =
            String::from_utf8(encoded_input).expect("JSON encoding is always valid UTF-8");
        if let Some(key) = request.idempotency_key.as_ref() {
            let cache_key = (request.name.clone(), key.clone());
            if let Some(completed) = self.completed.get(&cache_key).cloned() {
                if completed.arguments_fingerprint != arguments_fingerprint {
                    return self.reject(
                        request,
                        ToolError::new(
                            ToolErrorKind::IdempotencyConflict,
                            "同一 idempotency_key 不能用于不同参数",
                        ),
                        started,
                    );
                }
                self.record(
                    request,
                    0,
                    ToolTraceEventType::IdempotencyReplayed,
                    "复用 Registry 中已完成的结果",
                    started,
                );
                return ToolObservation::Success {
                    output: completed.output,
                    attempts: 0,
                    replayed: true,
                };
            }
        }

        let context = ToolContext {
            request_id: request.request_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
        };
        let max_attempts = policy.max_retries.saturating_add(1);

        for attempt in 1..=max_attempts {
            self.record(
                request,
                attempt,
                ToolTraceEventType::AttemptStarted,
                "调用已通过 schema 与权限检查",
                started,
            );
            let call_result = {
                let tool = self
                    .tools
                    .get_mut(&request.name)
                    .expect("tool existence checked before execution");
                timeout(
                    Duration::from_millis(spec.timeout_ms),
                    tool.call(request.arguments.clone(), &context),
                )
                .await
            };

            let result = match call_result {
                Ok(result) => result,
                Err(_) => Err(ToolError::new(
                    ToolErrorKind::Timeout,
                    format!("工具超过 {}ms deadline", spec.timeout_ms),
                )),
            };

            match result {
                Ok(output) => {
                    if let Err(message) = spec.output_schema.validate(&output) {
                        let error = ToolError::new(ToolErrorKind::MalformedOutput, message);
                        self.record(
                            request,
                            attempt,
                            ToolTraceEventType::ToolFailed,
                            error.message.clone(),
                            started,
                        );
                        return ToolObservation::Failure {
                            error,
                            attempts: attempt,
                        };
                    }
                    let output_size = serde_json::to_vec(&output)
                        .map(|bytes| bytes.len())
                        .unwrap_or(usize::MAX);
                    if output_size > spec.max_output_bytes {
                        let error = ToolError::new(
                            ToolErrorKind::OutputTooLarge,
                            format!(
                                "输出大小 {output_size} bytes 超过限制 {} bytes",
                                spec.max_output_bytes
                            ),
                        );
                        self.record(
                            request,
                            attempt,
                            ToolTraceEventType::ToolFailed,
                            error.message.clone(),
                            started,
                        );
                        return ToolObservation::Failure {
                            error,
                            attempts: attempt,
                        };
                    }

                    if spec.requires_idempotency_key {
                        let key = request
                            .idempotency_key
                            .as_ref()
                            .expect("validated idempotency key")
                            .clone();
                        self.completed.insert(
                            (request.name.clone(), key),
                            CompletedCall {
                                arguments_fingerprint,
                                output: output.clone(),
                            },
                        );
                    }
                    self.record(
                        request,
                        attempt,
                        ToolTraceEventType::ToolSucceeded,
                        output.to_string(),
                        started,
                    );
                    return ToolObservation::Success {
                        output,
                        attempts: attempt,
                        replayed: false,
                    };
                }
                Err(error) => {
                    let may_retry = error.kind == ToolErrorKind::Transient
                        && policy.retry_transient
                        && attempt < max_attempts;
                    if may_retry {
                        self.record(
                            request,
                            attempt,
                            ToolTraceEventType::RetryScheduled,
                            error.message,
                            started,
                        );
                        continue;
                    }
                    self.record(
                        request,
                        attempt,
                        ToolTraceEventType::ToolFailed,
                        error.message.clone(),
                        started,
                    );
                    return ToolObservation::Failure {
                        error,
                        attempts: attempt,
                    };
                }
            }
        }

        unreachable!("max_attempts is at least one")
    }
}

fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::ReadWorkspace => "read_workspace",
        Permission::ModifyState => "modify_state",
    }
}
