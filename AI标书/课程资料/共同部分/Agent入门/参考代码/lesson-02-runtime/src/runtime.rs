use crate::{
    parse_action, validate_action, ApprovedAction, RemainingBudget, ToolCall, TraceEvent,
    TraceEventType, TraceWriter,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_steps: usize,
    pub max_model_calls: usize,
    pub max_tool_calls: usize,
    pub max_millis: u64,
    pub max_consecutive_identical_actions: usize,
    pub max_protocol_errors: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub steps: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub protocol_errors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub tool_name: String,
    pub output: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeState {
    pub observations: Vec<Observation>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInput {
    pub step: usize,
    pub observations: Vec<Observation>,
    pub last_error: Option<String>,
}

pub trait Model {
    fn complete(&mut self, input: &ModelInput) -> Result<String, String>;
}

pub trait Environment {
    fn execute(&mut self, call: &ToolCall) -> Result<Observation, String>;
}

pub trait Clock {
    fn now_millis(&mut self) -> u64;
}

pub trait GoalVerifier {
    fn is_complete(&mut self, state: &RuntimeState, answer: &str) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Completed,
    StepBudget,
    ModelBudget,
    ToolBudget,
    Deadline,
    RepeatedAction,
    ModelError,
    ToolError,
    ProtocolError,
    TraceError,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub reason: StopReason,
    pub state: RuntimeState,
    pub trace: Vec<TraceEvent>,
    pub usage: ResourceUsage,
}

struct RunLog {
    run_id: String,
    budget: Budget,
    events: Vec<TraceEvent>,
}

impl RunLog {
    fn new(run_id: impl Into<String>, budget: Budget) -> Self {
        Self {
            run_id: run_id.into(),
            budget,
            events: Vec::new(),
        }
    }

    fn emit(
        &mut self,
        writer: &mut impl TraceWriter,
        usage: ResourceUsage,
        elapsed_millis: u64,
        event_type: TraceEventType,
        detail: impl Into<String>,
    ) -> Result<(), String> {
        let event = TraceEvent {
            run_id: self.run_id.clone(),
            step: usage.steps,
            event_type,
            detail: detail.into(),
            elapsed_millis,
            usage,
            remaining: RemainingBudget::from(self.budget, usage, elapsed_millis),
        };
        self.events.push(event.clone());
        if let Err(error) = writer.write(&event) {
            self.events.push(TraceEvent {
                run_id: self.run_id.clone(),
                step: usage.steps,
                event_type: TraceEventType::TraceWriteFailed,
                detail: error.clone(),
                elapsed_millis,
                usage,
                remaining: RemainingBudget::from(self.budget, usage, elapsed_millis),
            });
            return Err(error);
        }
        Ok(())
    }
}

fn elapsed(clock: &mut impl Clock, started_at: u64) -> u64 {
    clock.now_millis().saturating_sub(started_at)
}

fn result(reason: StopReason, state: RuntimeState, usage: ResourceUsage, log: RunLog) -> RunResult {
    RunResult {
        reason,
        state,
        trace: log.events,
        usage,
    }
}

fn trace_error_result(state: RuntimeState, usage: ResourceUsage, log: RunLog) -> RunResult {
    result(StopReason::TraceError, state, usage, log)
}

fn terminate(
    requested_reason: StopReason,
    state: RuntimeState,
    usage: ResourceUsage,
    elapsed_millis: u64,
    mut log: RunLog,
    writer: &mut impl TraceWriter,
) -> RunResult {
    let detail = serde_json::to_string(&requested_reason)
        .unwrap_or_else(|_| format!("{requested_reason:?}"));
    if log
        .emit(
            writer,
            usage,
            elapsed_millis,
            TraceEventType::RunTerminated,
            detail,
        )
        .is_err()
    {
        return trace_error_result(state, usage, log);
    }
    result(requested_reason, state, usage, log)
}

fn protocol_failure(
    message: String,
    state: &mut RuntimeState,
    usage: &mut ResourceUsage,
    elapsed_millis: u64,
    log: &mut RunLog,
    writer: &mut impl TraceWriter,
) -> Result<bool, ()> {
    usage.protocol_errors += 1;
    state.last_error = Some(message.clone());
    log.emit(
        writer,
        *usage,
        elapsed_millis,
        TraceEventType::ActionRejected,
        message,
    )
    .map_err(|_| ())?;
    Ok(usage.protocol_errors >= log.budget.max_protocol_errors.max(1))
}

pub fn run_with(
    model: &mut impl Model,
    environment: &mut impl Environment,
    clock: &mut impl Clock,
    verifier: &mut impl GoalVerifier,
    writer: &mut impl TraceWriter,
    budget: Budget,
    run_id: impl Into<String>,
) -> RunResult {
    let started_at = clock.now_millis();
    let mut state = RuntimeState::default();
    let mut usage = ResourceUsage::default();
    let mut log = RunLog::new(run_id, budget);
    let mut previous_fingerprint: Option<String> = None;
    let mut identical_streak = 0_usize;

    if log
        .emit(
            writer,
            usage,
            0,
            TraceEventType::RunStarted,
            "runtime started",
        )
        .is_err()
    {
        return trace_error_result(state, usage, log);
    }

    loop {
        let loop_elapsed = elapsed(clock, started_at);
        if loop_elapsed >= budget.max_millis {
            return terminate(
                StopReason::Deadline,
                state,
                usage,
                loop_elapsed,
                log,
                writer,
            );
        }
        if usage.steps >= budget.max_steps {
            return terminate(
                StopReason::StepBudget,
                state,
                usage,
                loop_elapsed,
                log,
                writer,
            );
        }
        if usage.model_calls >= budget.max_model_calls {
            return terminate(
                StopReason::ModelBudget,
                state,
                usage,
                loop_elapsed,
                log,
                writer,
            );
        }

        usage.steps += 1;
        let input = ModelInput {
            step: usage.steps,
            observations: state.observations.clone(),
            last_error: state.last_error.clone(),
        };
        if log
            .emit(
                writer,
                usage,
                loop_elapsed,
                TraceEventType::ModelCallStarted,
                "model call authorized",
            )
            .is_err()
        {
            return trace_error_result(state, usage, log);
        }

        usage.model_calls += 1;
        let raw = match model.complete(&input) {
            Ok(raw) => raw,
            Err(error) => {
                let after_model = elapsed(clock, started_at);
                if log
                    .emit(
                        writer,
                        usage,
                        after_model,
                        TraceEventType::ModelFailed,
                        error,
                    )
                    .is_err()
                {
                    return trace_error_result(state, usage, log);
                }
                return terminate(
                    StopReason::ModelError,
                    state,
                    usage,
                    after_model,
                    log,
                    writer,
                );
            }
        };

        let after_model = elapsed(clock, started_at);
        if log
            .emit(
                writer,
                usage,
                after_model,
                TraceEventType::ModelReturned,
                raw.clone(),
            )
            .is_err()
        {
            return trace_error_result(state, usage, log);
        }
        if after_model >= budget.max_millis {
            return terminate(StopReason::Deadline, state, usage, after_model, log, writer);
        }

        let proposed = match parse_action(&raw) {
            Ok(action) => action,
            Err(error) => {
                let should_stop = match protocol_failure(
                    error.message,
                    &mut state,
                    &mut usage,
                    after_model,
                    &mut log,
                    writer,
                ) {
                    Ok(value) => value,
                    Err(()) => return trace_error_result(state, usage, log),
                };
                if should_stop {
                    return terminate(
                        StopReason::ProtocolError,
                        state,
                        usage,
                        after_model,
                        log,
                        writer,
                    );
                }
                continue;
            }
        };
        if log
            .emit(
                writer,
                usage,
                after_model,
                TraceEventType::ActionParsed,
                format!("{proposed:?}"),
            )
            .is_err()
        {
            return trace_error_result(state, usage, log);
        }

        let approved = match validate_action(proposed) {
            Ok(action) => action,
            Err(error) => {
                let should_stop = match protocol_failure(
                    error.message,
                    &mut state,
                    &mut usage,
                    after_model,
                    &mut log,
                    writer,
                ) {
                    Ok(value) => value,
                    Err(()) => return trace_error_result(state, usage, log),
                };
                if should_stop {
                    return terminate(
                        StopReason::ProtocolError,
                        state,
                        usage,
                        after_model,
                        log,
                        writer,
                    );
                }
                continue;
            }
        };
        state.last_error = None;

        let fingerprint = serde_json::to_string(&approved)
            .expect("ApprovedAction derives Serialize and has no fallible custom serializer");
        if previous_fingerprint.as_deref() == Some(&fingerprint) {
            identical_streak += 1;
        } else {
            previous_fingerprint = Some(fingerprint);
            identical_streak = 1;
        }
        if identical_streak > budget.max_consecutive_identical_actions {
            if log
                .emit(
                    writer,
                    usage,
                    after_model,
                    TraceEventType::ActionRejected,
                    format!("连续相同动作达到 {identical_streak} 次"),
                )
                .is_err()
            {
                return trace_error_result(state, usage, log);
            }
            return terminate(
                StopReason::RepeatedAction,
                state,
                usage,
                after_model,
                log,
                writer,
            );
        }

        if log
            .emit(
                writer,
                usage,
                after_model,
                TraceEventType::ActionApproved,
                format!("{approved:?}"),
            )
            .is_err()
        {
            return trace_error_result(state, usage, log);
        }

        match approved {
            ApprovedAction::Continue => {}
            ApprovedAction::Finish { answer } => {
                if verifier.is_complete(&state, &answer) {
                    return terminate(
                        StopReason::Completed,
                        state,
                        usage,
                        after_model,
                        log,
                        writer,
                    );
                }
                state.last_error = Some("finish 被 GoalVerifier 拒绝".to_owned());
                if log
                    .emit(
                        writer,
                        usage,
                        after_model,
                        TraceEventType::FinishRejected,
                        "目标尚未满足，继续运行",
                    )
                    .is_err()
                {
                    return trace_error_result(state, usage, log);
                }
            }
            ApprovedAction::UseTool(call) => {
                if after_model >= budget.max_millis {
                    return terminate(StopReason::Deadline, state, usage, after_model, log, writer);
                }
                if usage.tool_calls >= budget.max_tool_calls {
                    return terminate(
                        StopReason::ToolBudget,
                        state,
                        usage,
                        after_model,
                        log,
                        writer,
                    );
                }

                if log
                    .emit(
                        writer,
                        usage,
                        after_model,
                        TraceEventType::ToolStarted,
                        format!("{}({:?})", call.name, call.text),
                    )
                    .is_err()
                {
                    return trace_error_result(state, usage, log);
                }
                usage.tool_calls += 1;
                match environment.execute(&call) {
                    Ok(observation) => {
                        let after_tool = elapsed(clock, started_at);
                        state.observations.push(observation.clone());
                        if log
                            .emit(
                                writer,
                                usage,
                                after_tool,
                                TraceEventType::ToolSucceeded,
                                observation.output,
                            )
                            .is_err()
                        {
                            return trace_error_result(state, usage, log);
                        }
                    }
                    Err(error) => {
                        let after_tool = elapsed(clock, started_at);
                        if log
                            .emit(writer, usage, after_tool, TraceEventType::ToolFailed, error)
                            .is_err()
                        {
                            return trace_error_result(state, usage, log);
                        }
                        return terminate(
                            StopReason::ToolError,
                            state,
                            usage,
                            after_tool,
                            log,
                            writer,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EchoEnvironment, MemoryTraceWriter, ScriptedModel, SequenceClock};

    struct AlwaysComplete;

    impl GoalVerifier for AlwaysComplete {
        fn is_complete(&mut self, _state: &RuntimeState, _answer: &str) -> bool {
            true
        }
    }

    #[test]
    fn successful_finish_is_owned_by_the_verifier() {
        let mut model = ScriptedModel::new([Ok(
            r#"{"action":"finish","arguments":{"answer":"done"}}"#.to_owned(),
        )]);
        let mut environment = EchoEnvironment::default();
        let mut clock = SequenceClock::new([0, 0, 0]);
        let mut verifier = AlwaysComplete;
        let mut writer = MemoryTraceWriter::default();
        let result = run_with(
            &mut model,
            &mut environment,
            &mut clock,
            &mut verifier,
            &mut writer,
            Budget {
                max_steps: 2,
                max_model_calls: 2,
                max_tool_calls: 1,
                max_millis: 100,
                max_consecutive_identical_actions: 2,
                max_protocol_errors: 1,
            },
            "unit-finish",
        );
        assert_eq!(result.reason, StopReason::Completed);
    }
}
