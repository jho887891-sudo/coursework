use lesson_02_runtime::*;

#[test]
fn jsonl_writer_produces_one_parseable_object_per_event() {
    let mut model = ScriptedModel::new([Ok(
        r#"{"action":"finish","arguments":{"answer":"done"}}"#.to_owned(),
    )]);
    let mut environment = EchoEnvironment::default();
    let mut clock = SequenceClock::new([0, 0, 0]);
    struct Complete;
    impl GoalVerifier for Complete {
        fn is_complete(&mut self, _state: &RuntimeState, _answer: &str) -> bool {
            true
        }
    }
    let mut verifier = Complete;
    let mut writer = JsonlTraceWriter::new(Vec::new());
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        Budget {
            max_steps: 2,
            max_model_calls: 2,
            max_tool_calls: 0,
            max_millis: 100,
            max_consecutive_identical_actions: 1,
            max_protocol_errors: 1,
        },
        "jsonl-test",
    );
    assert_eq!(result.reason, StopReason::Completed);

    let bytes = writer.into_inner();
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), result.trace.len());
    for line in lines {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["run_id"], "jsonl-test");
        assert!(value["event_type"].is_string());
    }
}
