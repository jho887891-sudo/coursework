use lesson_02_runtime::*;

fn budget() -> Budget {
    Budget {
        max_steps: 6,
        max_model_calls: 6,
        max_tool_calls: 3,
        max_millis: 100,
        max_consecutive_identical_actions: 2,
        max_protocol_errors: 2,
    }
}

struct AlwaysComplete;

impl GoalVerifier for AlwaysComplete {
    fn is_complete(&mut self, _state: &RuntimeState, _answer: &str) -> bool {
        true
    }
}

struct FailOnWrite {
    fail_at: usize,
    writes: usize,
}

impl TraceWriter for FailOnWrite {
    fn write(&mut self, _event: &TraceEvent) -> Result<(), String> {
        self.writes += 1;
        if self.writes == self.fail_at {
            Err("disk full".to_owned())
        } else {
            Ok(())
        }
    }
}

#[test]
fn deadline_before_tool_execution_prevents_the_side_effect() {
    let mut model = ScriptedModel::new([Ok(
        r#"{"action":"echo","arguments":{"text":"late"}}"#.to_owned()
    )]);
    let mut environment = EchoEnvironment::default();
    let mut clock = SequenceClock::new([0, 0, 101]);
    let mut verifier = AlwaysComplete;
    let mut writer = MemoryTraceWriter::default();
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        budget(),
        "deadline",
    );
    assert_eq!(result.reason, StopReason::Deadline);
    assert!(environment.calls.is_empty());
}

#[test]
fn tool_failure_is_distinct_from_model_and_protocol_failure() {
    let mut model = ScriptedModel::new([Ok(
        r#"{"action":"echo","arguments":{"text":"hello"}}"#.to_owned()
    )]);
    let mut environment = EchoEnvironment {
        fail_with: Some("tool timeout".to_owned()),
        ..EchoEnvironment::default()
    };
    let mut clock = SequenceClock::new([0, 0, 0, 1]);
    let mut verifier = AlwaysComplete;
    let mut writer = MemoryTraceWriter::default();
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        budget(),
        "tool-error",
    );
    assert_eq!(result.reason, StopReason::ToolError);
    assert_eq!(result.usage.tool_calls, 1);
    assert!(result
        .trace
        .iter()
        .any(|event| event.event_type == TraceEventType::ToolFailed));
}

#[test]
fn trace_failure_before_model_call_prevents_the_call() {
    let mut model = ScriptedModel::new([Ok(r#"{"action":"continue"}"#.to_owned())]);
    let mut environment = EchoEnvironment::default();
    let mut clock = SequenceClock::new([0]);
    let mut verifier = AlwaysComplete;
    let mut writer = FailOnWrite {
        fail_at: 1,
        writes: 0,
    };
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        budget(),
        "trace-failure",
    );
    assert_eq!(result.reason, StopReason::TraceError);
    assert_eq!(model.calls, 0);
    assert!(environment.calls.is_empty());
    assert_eq!(
        result.trace.last().unwrap().event_type,
        TraceEventType::TraceWriteFailed
    );
}

#[test]
fn semantically_identical_json_actions_share_a_fingerprint() {
    let mut configured = budget();
    configured.max_consecutive_identical_actions = 1;
    let mut model = ScriptedModel::new([
        Ok(r#"{"action":"echo","arguments":{"text":"same"}}"#.to_owned()),
        Ok(r#"{"arguments":{"text":"same"},"action":"echo"}"#.to_owned()),
    ]);
    let mut environment = EchoEnvironment::default();
    let mut clock = SequenceClock::new(std::iter::repeat_n(0, 16));
    let mut verifier = AlwaysComplete;
    let mut writer = MemoryTraceWriter::default();
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        configured,
        "canonical-repeat",
    );
    assert_eq!(result.reason, StopReason::RepeatedAction);
    assert_eq!(environment.calls.len(), 1);
}

#[test]
fn trace_steps_are_monotonic_and_usage_never_exceeds_budget() {
    let configured = budget();
    let mut model = ScriptedModel::new([
        Ok("bad".to_owned()),
        Ok(r#"{"action":"echo","arguments":{"text":"ok"}}"#.to_owned()),
        Ok(r#"{"action":"finish","arguments":{"answer":"done"}}"#.to_owned()),
    ]);
    let mut environment = EchoEnvironment::default();
    let mut clock = SequenceClock::new(std::iter::repeat_n(0, 24));
    let mut verifier = RequireObservation;
    let mut writer = MemoryTraceWriter::default();
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        configured,
        "invariants",
    );
    assert!(result
        .trace
        .windows(2)
        .all(|pair| pair[0].step <= pair[1].step));
    assert!(result.usage.steps <= configured.max_steps);
    assert!(result.usage.model_calls <= configured.max_model_calls);
    assert!(result.usage.tool_calls <= configured.max_tool_calls);
}
