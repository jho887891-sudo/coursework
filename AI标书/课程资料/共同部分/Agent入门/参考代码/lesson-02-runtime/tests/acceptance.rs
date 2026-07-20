use lesson_02_runtime::*;

fn budget() -> Budget {
    Budget {
        max_steps: 8,
        max_model_calls: 8,
        max_tool_calls: 3,
        max_millis: 1_000,
        max_consecutive_identical_actions: 3,
        max_protocol_errors: 2,
    }
}

fn run(
    replies: impl IntoIterator<Item = Result<String, String>>,
    budget: Budget,
) -> (RunResult, ScriptedModel, EchoEnvironment) {
    let mut model = ScriptedModel::new(replies);
    let mut environment = EchoEnvironment::default();
    let mut clock = SequenceClock::new(std::iter::repeat_n(0, 32));
    let mut verifier = RequireObservation;
    let mut writer = MemoryTraceWriter::default();
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        budget,
        "acceptance-run",
    );
    (result, model, environment)
}

fn ok(raw: &str) -> Result<String, String> {
    Ok(raw.to_owned())
}

#[test]
fn parses_the_single_json_protocol() {
    assert_eq!(
        parse_action(r#"{"action":"continue"}"#).unwrap(),
        ProposedAction::Continue
    );
    assert_eq!(
        parse_action(r#"{"action":"echo","arguments":{"text":"hello"}}"#).unwrap(),
        ProposedAction::Echo {
            text: "hello".to_owned()
        }
    );
}

#[test]
fn invalid_output_can_recover_within_the_protocol_error_budget() {
    let (result, _, environment) = run(
        [
            ok("not json"),
            ok(r#"{"action":"echo","arguments":{"text":"hello"}}"#),
            ok(r#"{"action":"finish","arguments":{"answer":"done"}}"#),
        ],
        budget(),
    );
    assert_eq!(result.reason, StopReason::Completed);
    assert_eq!(result.usage.protocol_errors, 1);
    assert_eq!(environment.calls.len(), 1);
}

#[test]
fn unknown_actions_never_reach_the_environment() {
    let mut configured = budget();
    configured.max_protocol_errors = 1;
    let (result, _, environment) = run([ok(r#"{"action":"delete_everything"}"#)], configured);
    assert_eq!(result.reason, StopReason::ProtocolError);
    assert!(environment.calls.is_empty());
}

#[test]
fn finish_is_only_a_proposal_until_the_goal_verifier_accepts_it() {
    let (result, model, environment) = run(
        [
            ok(r#"{"action":"finish","arguments":{"answer":"too early"}}"#),
            ok(r#"{"action":"echo","arguments":{"text":"evidence"}}"#),
            ok(r#"{"action":"finish","arguments":{"answer":"done"}}"#),
            ok(r#"{"action":"echo","arguments":{"text":"must not run"}}"#),
        ],
        budget(),
    );
    assert_eq!(result.reason, StopReason::Completed);
    assert_eq!(model.calls, 3);
    assert_eq!(environment.calls.len(), 1);
    assert!(result
        .trace
        .iter()
        .any(|event| event.event_type == TraceEventType::FinishRejected));
}

#[test]
fn model_calls_have_an_independent_hard_budget() {
    let mut configured = budget();
    configured.max_model_calls = 2;
    configured.max_consecutive_identical_actions = 10;
    let (result, model, _) = run(
        std::iter::repeat_n(ok(r#"{"action":"continue"}"#), 10),
        configured,
    );
    assert_eq!(result.reason, StopReason::ModelBudget);
    assert_eq!(model.calls, 2);
}

#[test]
fn tool_budget_is_checked_before_the_second_side_effect() {
    let mut configured = budget();
    configured.max_tool_calls = 1;
    let (result, _, environment) = run(
        [
            ok(r#"{"action":"echo","arguments":{"text":"first"}}"#),
            ok(r#"{"action":"echo","arguments":{"text":"second"}}"#),
        ],
        configured,
    );
    assert_eq!(result.reason, StopReason::ToolBudget);
    assert_eq!(environment.calls.len(), 1);
    assert_eq!(environment.calls[0].text, "first");
}

#[test]
fn step_budget_wins_when_step_and_model_budgets_exhaust_together() {
    let mut configured = budget();
    configured.max_steps = 2;
    configured.max_model_calls = 2;
    configured.max_consecutive_identical_actions = 10;
    let (result, _, _) = run(
        std::iter::repeat_n(ok(r#"{"action":"continue"}"#), 4),
        configured,
    );
    assert_eq!(result.reason, StopReason::StepBudget);
    assert_eq!(result.usage.steps, 2);
}

#[test]
fn repeated_action_is_rejected_before_an_extra_tool_call() {
    let mut configured = budget();
    configured.max_consecutive_identical_actions = 1;
    let same = r#"{"action":"echo","arguments":{"text":"same"}}"#;
    let (result, _, environment) = run([ok(same), ok(same)], configured);
    assert_eq!(result.reason, StopReason::RepeatedAction);
    assert_eq!(environment.calls.len(), 1);
}

#[test]
fn model_failure_has_a_trace_and_a_deterministic_reason() {
    let (result, _, environment) = run([Err("model timeout".to_owned())], budget());
    assert_eq!(result.reason, StopReason::ModelError);
    assert!(environment.calls.is_empty());
    assert!(result
        .trace
        .iter()
        .any(|event| event.event_type == TraceEventType::ModelFailed));
}

#[test]
fn every_run_ends_with_one_explicit_termination_event() {
    let (result, _, _) = run(
        [
            ok(r#"{"action":"echo","arguments":{"text":"hello"}}"#),
            ok(r#"{"action":"finish","arguments":{"answer":"done"}}"#),
        ],
        budget(),
    );
    assert_eq!(
        result
            .trace
            .iter()
            .filter(|event| event.event_type == TraceEventType::RunTerminated)
            .count(),
        1
    );
    assert!(result
        .trace
        .iter()
        .all(|event| event.run_id == "acceptance-run"));
}
