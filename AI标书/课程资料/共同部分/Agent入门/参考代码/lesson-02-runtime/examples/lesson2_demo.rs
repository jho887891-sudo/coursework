use lesson_02_runtime::*;

fn main() {
    let scenario = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "recovery".to_owned());
    let (replies, fail_with, repeat_limit, run_id) = match scenario.as_str() {
        "recovery" => (
            vec![
                Ok("not json".to_owned()),
                Ok(r#"{"action":"echo","arguments":{"text":"hello"}}"#.to_owned()),
                Ok(r#"{"action":"finish","arguments":{"answer":"done"}}"#.to_owned()),
            ],
            None,
            2,
            "lesson2-recovery",
        ),
        "repeated" => (
            vec![
                Ok(r#"{"action":"echo","arguments":{"text":"same"}}"#.to_owned()),
                Ok(r#"{"arguments":{"text":"same"},"action":"echo"}"#.to_owned()),
            ],
            None,
            1,
            "lesson2-repeated",
        ),
        "tool-error" => (
            vec![Ok(
                r#"{"action":"echo","arguments":{"text":"hello"}}"#.to_owned()
            )],
            Some("simulated tool timeout".to_owned()),
            2,
            "lesson2-tool-error",
        ),
        other => {
            eprintln!("未知场景：{other}；可选 recovery、repeated、tool-error");
            std::process::exit(2);
        }
    };

    let mut model = ScriptedModel::new(replies);
    let mut environment = EchoEnvironment {
        fail_with,
        ..EchoEnvironment::default()
    };
    let mut clock = SequenceClock::new(std::iter::repeat_n(0, 32));
    let mut verifier = RequireObservation;
    let mut writer = JsonlTraceWriter::new(std::io::stdout());
    let result = run_with(
        &mut model,
        &mut environment,
        &mut clock,
        &mut verifier,
        &mut writer,
        Budget {
            max_steps: 6,
            max_model_calls: 6,
            max_tool_calls: 2,
            max_millis: 1_000,
            max_consecutive_identical_actions: repeat_limit,
            max_protocol_errors: 2,
        },
        run_id,
    );

    eprintln!(
        "STOP={:?} steps={} model_calls={} tool_calls={} protocol_errors={}",
        result.reason,
        result.usage.steps,
        result.usage.model_calls,
        result.usage.tool_calls,
        result.usage.protocol_errors
    );
}
