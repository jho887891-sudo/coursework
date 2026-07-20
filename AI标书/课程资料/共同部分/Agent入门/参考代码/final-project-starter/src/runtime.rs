//! Agent Runtime — budget control, action loop, trace, repeat detection.
//!
//! Adapted from lesson-02-runtime patterns and extended with:
//! - Tool timeout + retry (delegated to ReviewToolRegistry)
//! - Repeated action detection via fingerprint
//! - JSONL trace writing
//! - Explicit termination reasons
use crate::actions::{parse_action, validate_action, ApprovedAction};
use crate::tools::ReviewToolRegistry;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::time::Instant;

// ── Budget ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_steps: usize,
    pub max_model_calls: usize,
    pub max_tool_calls: usize,
    pub max_millis: u64,
    pub max_consecutive_identical_actions: usize,
    pub max_protocol_errors: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_steps: 50,
            max_model_calls: 30,
            max_tool_calls: 40,
            max_millis: 300_000, // 5 minutes
            max_consecutive_identical_actions: 3,
            max_protocol_errors: 5,
        }
    }
}

// ── Resource usage ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub steps: usize,
    pub model_calls: usize,
    pub tool_calls: usize,
    pub protocol_errors: usize,
}

// ── Observation ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub tool_name: String,
    pub output: String,
}

// ── Runtime state ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct RuntimeState {
    pub observations: Vec<Observation>,
    pub last_error: Option<String>,
    pub findings_count: usize,
    pub escalations: Vec<String>,
}

// ── Stop reason ───────────────────────────────────────────────────────────

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

// ── Run result ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RunResult {
    pub reason: StopReason,
    pub state: RuntimeState,
    pub usage: ResourceUsage,
    pub elapsed_ms: u64,
}

// ── Model trait ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelInput {
    pub step: usize,
    pub observations: Vec<Observation>,
    pub last_error: Option<String>,
}

pub trait AgentModel: Send {
    fn complete(&mut self, input: &ModelInput) -> Result<String, String>;
}

// ── Trace ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub step: usize,
    pub event: String,
    pub detail: String,
    pub elapsed_ms: u64,
    pub model_calls: usize,
    pub tool_calls: usize,
}

pub struct JsonlTraceWriter<W: Write> {
    writer: W,
    pub events: Vec<TraceEvent>,
}

impl<W: Write> JsonlTraceWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            events: Vec::new(),
        }
    }

    pub fn emit(
        &mut self,
        step: usize,
        event: &str,
        detail: &str,
        elapsed_ms: u64,
        model_calls: usize,
        tool_calls: usize,
    ) -> Result<(), String> {
        let te = TraceEvent {
            step,
            event: event.to_string(),
            detail: detail.to_string(),
            elapsed_ms,
            model_calls,
            tool_calls,
        };
        self.events.push(te.clone());
        let line = serde_json::to_string(&te).map_err(|e| e.to_string())?;
        self.writer
            .write_all(line.as_bytes())
            .map_err(|e| e.to_string())?;
        self.writer.write_all(b"\n").map_err(|e| e.to_string())?;
        Ok(())
    }
}

// For tests: in-memory trace
#[derive(Default)]
pub struct MemoryTrace {
    pub events: Vec<TraceEvent>,
    pub lines: Vec<String>,
}

impl MemoryTrace {
    pub fn emit(
        &mut self,
        step: usize,
        event: &str,
        detail: &str,
        elapsed_ms: u64,
        model_calls: usize,
        tool_calls: usize,
    ) -> Result<(), String> {
        let te = TraceEvent {
            step,
            event: event.to_string(),
            detail: detail.to_string(),
            elapsed_ms,
            model_calls,
            tool_calls,
        };
        let line = serde_json::to_string(&te).map_err(|e| e.to_string())?;
        self.lines.push(line);
        self.events.push(te);
        Ok(())
    }
}

// ── Goal verifier ─────────────────────────────────────────────────────────

pub trait GoalVerifier {
    /// Returns true when the agent has finished its review task.
    fn is_complete(&mut self, state: &RuntimeState, answer: &str) -> bool;
}

/// Default verifier: accept any non-empty finish.
pub struct DefaultVerifier;

impl GoalVerifier for DefaultVerifier {
    fn is_complete(&mut self, _state: &RuntimeState, answer: &str) -> bool {
        !answer.trim().is_empty()
    }
}

// ── Runtime loop ──────────────────────────────────────────────────────────

/// Run the agent loop.
///
/// For flexibility, the trace writer is abstracted behind a closure-style
/// interface rather than a trait.  Pass `|step, event, detail, elapsed, mc, tc|`
/// callbacks that return `Result<(), String>`.
pub fn run_agent(
    model: &mut impl AgentModel,
    tools: &mut ReviewToolRegistry,
    verifier: &mut impl GoalVerifier,
    budget: Budget,
    run_id: &str,
    mut trace_fn: impl FnMut(usize, &str, &str, u64, usize, usize) -> Result<(), String>,
) -> RunResult {
    let started = Instant::now();
    let mut state = RuntimeState::default();
    let mut usage = ResourceUsage::default();
    let mut previous_fingerprint: Option<String> = None;
    let mut identical_streak: usize = 0;

    let emit = |trace_fn: &mut dyn FnMut(_, &str, &str, _, _, _) -> Result<(), String>,
                step: usize,
                event: &str,
                detail: &str,
                usage: &ResourceUsage| {
        let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        trace_fn(step, event, detail, elapsed, usage.model_calls, usage.tool_calls)
    };

    if emit(&mut trace_fn, 0, "run_started", run_id, &usage).is_err() {
        return RunResult {
            reason: StopReason::TraceError,
            state,
            usage,
            elapsed_ms: started.elapsed().as_millis() as u64,
        };
    }

    loop {
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;

        // ── Budget checks ────────────────────────────────────────────────
        if elapsed_ms >= budget.max_millis {
            let _ = emit(&mut trace_fn, usage.steps, "run_terminated", "deadline", &usage);
            return RunResult {
                reason: StopReason::Deadline,
                state,
                usage,
                elapsed_ms,
            };
        }
        if usage.steps >= budget.max_steps {
            let _ = emit(&mut trace_fn, usage.steps, "run_terminated", "step_budget", &usage);
            return RunResult {
                reason: StopReason::StepBudget,
                state,
                usage,
                elapsed_ms,
            };
        }
        if usage.model_calls >= budget.max_model_calls {
            let _ = emit(&mut trace_fn, usage.steps, "run_terminated", "model_budget", &usage);
            return RunResult {
                reason: StopReason::ModelBudget,
                state,
                usage,
                elapsed_ms,
            };
        }

        usage.steps += 1;

        // ── Call model ───────────────────────────────────────────────────
        let input = ModelInput {
            step: usage.steps,
            observations: state.observations.clone(),
            last_error: state.last_error.clone(),
        };

        let _ = emit(&mut trace_fn, usage.steps, "model_call_started", "", &usage);

        usage.model_calls += 1;
        let raw = match model.complete(&input) {
            Ok(r) => r,
            Err(error) => {
                let _ = emit(
                    &mut trace_fn,
                    usage.steps,
                    "model_failed",
                    &error,
                    &usage,
                );
                return RunResult {
                    reason: StopReason::ModelError,
                    state,
                    usage,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                };
            }
        };

        let _ = emit(&mut trace_fn, usage.steps, "model_returned", &raw, &usage);

        // ── Parse action ─────────────────────────────────────────────────
        let proposed = match parse_action(&raw) {
            Ok(a) => a,
            Err(error) => {
                usage.protocol_errors += 1;
                state.last_error = Some(error.message.clone());
                let _ = emit(
                    &mut trace_fn,
                    usage.steps,
                    "action_rejected",
                    &error.message,
                    &usage,
                );
                if usage.protocol_errors >= budget.max_protocol_errors.max(1) {
                    return RunResult {
                        reason: StopReason::ProtocolError,
                        state,
                        usage,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }
                continue;
            }
        };

        let _ = emit(
            &mut trace_fn,
            usage.steps,
            "action_parsed",
            &format!("{proposed:?}"),
            &usage,
        );

        // ── Validate action ──────────────────────────────────────────────
        let approved = match validate_action(proposed) {
            Ok(a) => a,
            Err(error) => {
                usage.protocol_errors += 1;
                state.last_error = Some(error.message.clone());
                let _ = emit(
                    &mut trace_fn,
                    usage.steps,
                    "action_rejected",
                    &error.message,
                    &usage,
                );
                if usage.protocol_errors >= budget.max_protocol_errors.max(1) {
                    return RunResult {
                        reason: StopReason::ProtocolError,
                        state,
                        usage,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }
                continue;
            }
        };
        state.last_error = None;

        // ── Repeated action detection ────────────────────────────────────
        let fingerprint = format!("{approved:?}");
        if previous_fingerprint.as_deref() == Some(&fingerprint) {
            identical_streak += 1;
        } else {
            previous_fingerprint = Some(fingerprint);
            identical_streak = 1;
        }
        if identical_streak > budget.max_consecutive_identical_actions {
            let _ = emit(
                &mut trace_fn,
                usage.steps,
                "action_rejected",
                &format!("连续相同动作达到 {identical_streak} 次"),
                &usage,
            );
            return RunResult {
                reason: StopReason::RepeatedAction,
                state,
                usage,
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }

        let _ = emit(
            &mut trace_fn,
            usage.steps,
            "action_approved",
            &format!("{approved:?}"),
            &usage,
        );

        // ── Execute action ───────────────────────────────────────────────
        match approved {
            ApprovedAction::Continue => {
                let _ = emit(&mut trace_fn, usage.steps, "continue", "", &usage);
            }
            ApprovedAction::Finish { answer } => {
                if verifier.is_complete(&state, &answer) {
                    let _ = emit(
                        &mut trace_fn,
                        usage.steps,
                        "run_terminated",
                        "completed",
                        &usage,
                    );
                    return RunResult {
                        reason: StopReason::Completed,
                        state,
                        usage,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }
                state.last_error = Some("finish 被 GoalVerifier 拒绝，继续运行".into());
                let _ = emit(
                    &mut trace_fn,
                    usage.steps,
                    "finish_rejected",
                    "目标尚未满足",
                    &usage,
                );
            }
            ApprovedAction::UseTool(call) => {
                if usage.tool_calls >= budget.max_tool_calls {
                    return RunResult {
                        reason: StopReason::ToolBudget,
                        state,
                        usage,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }

                let _ = emit(
                    &mut trace_fn,
                    usage.steps,
                    "tool_started",
                    &format!("{}({})", call.name, call.arguments),
                    &usage,
                );

                usage.tool_calls += 1;
                match tools.execute(&call.name, &call.arguments) {
                    Ok(obs) => {
                        // Track findings and escalations
                        if call.name == "output_finding" {
                            state.findings_count += 1;
                        }
                        if call.name == "request_human" {
                            if let Ok(v) =
                                serde_json::from_str::<serde_json::Value>(&obs.output)
                            {
                                if let Some(reason) = v["reason"].as_str() {
                                    state.escalations.push(reason.to_string());
                                }
                            }
                        }
                        state.observations.push(Observation {
                            tool_name: obs.tool_name.clone(),
                            output: obs.output.clone(),
                        });
                        let _ = emit(
                            &mut trace_fn,
                            usage.steps,
                            "tool_succeeded",
                            &obs.output,
                            &usage,
                        );
                    }
                    Err(error) => {
                        state.last_error = Some(format!("{error}"));
                        let _ = emit(
                            &mut trace_fn,
                            usage.steps,
                            "tool_failed",
                            &format!("{error}"),
                            &usage,
                        );
                        // Don't terminate on tool error — let the model decide
                    }
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ProposedAction;
    use crate::tools::*;

    /// A scripted model that returns predefined responses in order.
    struct ScriptedModel {
        responses: Vec<Result<String, String>>,
        index: usize,
    }

    impl ScriptedModel {
        fn new(responses: Vec<Result<String, String>>) -> Self {
            Self {
                responses,
                index: 0,
            }
        }
    }

    impl AgentModel for ScriptedModel {
        fn complete(&mut self, _input: &ModelInput) -> Result<String, String> {
            if self.index >= self.responses.len() {
                return Err("no more responses".into());
            }
            let resp = self.responses[self.index].clone();
            self.index += 1;
            resp
        }
    }

    #[test]
    fn agent_finishes_on_completion() {
        let mut model = ScriptedModel::new(vec![Ok(
            r#"{"action":"finish","arguments":{"summary":"审查完成"}}"#.into(),
        )]);
        let rules = StoredRule::load_all(
            r#"{"source_id":"R1","title":"T","locator":"1.1","verbatim_text":"x","summary":"x","source_url":"u","effective_date":"2026-01-01","content_hash":"abc"}"#,
        );
        let mut registry = ReviewToolRegistry::new(0);
        registry
            .register(Box::new(SearchRulesTool::new(rules.clone())))
            .unwrap();
        registry
            .register(Box::new(ReadSourceTool::new(rules)))
            .unwrap();
        registry
            .register(Box::new(OutputFindingTool::new()))
            .unwrap();
        registry
            .register(Box::new(RequestHumanTool::new()))
            .unwrap();

        let mut verifier = DefaultVerifier;
        let mut trace = MemoryTrace::default();

        let result = run_agent(
            &mut model,
            &mut registry,
            &mut verifier,
            Budget::default(),
            "test-run",
            |step, event, detail, elapsed, mc, tc| {
                trace.emit(step, event, detail, elapsed, mc, tc)
            },
        );

        assert_eq!(result.reason, StopReason::Completed);
        assert!(trace.events.len() >= 4); // run_started, model_call*, run_terminated
    }

    #[test]
    fn agent_detects_repeated_actions() {
        // Return the same action 5 times — should hit repeat limit
        let mut responses = Vec::new();
        for _ in 0..10 {
            responses.push(Ok(
                r#"{"action":"search_rules","arguments":{"query":"test"}}"#.into(),
            ));
        }
        let mut model = ScriptedModel::new(responses);

        let rules = StoredRule::load_all(
            r#"{"source_id":"R1","title":"T","locator":"1.1","verbatim_text":"x","summary":"x","source_url":"u","effective_date":"2026-01-01","content_hash":"abc"}"#,
        );
        let mut registry = ReviewToolRegistry::new(0);
        registry
            .register(Box::new(SearchRulesTool::new(rules.clone())))
            .unwrap();
        registry
            .register(Box::new(ReadSourceTool::new(rules)))
            .unwrap();
        registry
            .register(Box::new(OutputFindingTool::new()))
            .unwrap();
        registry
            .register(Box::new(RequestHumanTool::new()))
            .unwrap();

        let mut verifier = DefaultVerifier;
        let mut trace = MemoryTrace::default();
        let budget = Budget {
            max_consecutive_identical_actions: 3,
            ..Budget::default()
        };

        let result = run_agent(
            &mut model,
            &mut registry,
            &mut verifier,
            budget,
            "test-repeat",
            |step, event, detail, elapsed, mc, tc| {
                trace.emit(step, event, detail, elapsed, mc, tc)
            },
        );

        assert_eq!(result.reason, StopReason::RepeatedAction);
    }
}
