use lesson_02_runtime::*;
use std::collections::VecDeque;
struct FakeModel(VecDeque<Result<String, String>>);
impl Model for FakeModel {
    fn complete(&mut self) -> Result<String, String> {
        self.0
            .pop_front()
            .unwrap_or_else(|| Err("exhausted".into()))
    }
}
#[derive(Default)]
struct FakeEnv {
    calls: usize,
    fail: bool,
}
impl Environment for FakeEnv {
    fn execute(&mut self, _: &str) -> Result<String, String> {
        self.calls += 1;
        if self.fail {
            Err("timeout".into())
        } else {
            Ok("ok".into())
        }
    }
}
struct FakeClock(VecDeque<u64>);
impl Clock for FakeClock {
    fn now_millis(&mut self) -> u64 {
        self.0.pop_front().unwrap_or(u64::MAX)
    }
}
fn budget() -> Budget {
    Budget {
        max_steps: 4,
        max_model_calls: 4,
        max_tool_calls: 2,
        max_millis: 1_000,
        max_repeated_actions: 2,
    }
}
fn run_model(replies: impl IntoIterator<Item = Result<String, String>>, b: Budget) -> RunResult {
    let mut m = FakeModel(replies.into_iter().collect());
    let mut e = FakeEnv::default();
    let mut c = FakeClock([0, 1, 2, 3, 4, 5].into());
    run_with(&mut m, &mut e, &mut c, b)
}
#[test]
#[ignore = "implement parser"]
fn parses_known_actions() {
    assert_eq!(parse_action("FINISH").unwrap(), Action::Finish);
    assert_eq!(
        parse_action("TOOL:echo").unwrap(),
        Action::UseTool("echo".into())
    );
}
#[test]
#[ignore = "reject malformed output"]
fn malformed_action_never_executes() {
    assert_eq!(
        run_model([Ok("do whatever".into())], budget()).reason,
        StopReason::ParseError
    );
}
#[test]
#[ignore = "implement completion"]
fn finish_terminates() {
    assert_eq!(
        run_model([Ok("FINISH".into())], budget()).reason,
        StopReason::Completed
    );
}
#[test]
#[ignore = "implement model budget"]
fn infinite_continue_is_bounded() {
    let mut b = budget();
    b.max_model_calls = 2;
    assert_eq!(
        run_model((0..10).map(|_| Ok("CONTINUE".into())), b).reason,
        StopReason::ModelBudget
    );
}
#[test]
#[ignore = "implement tool budget"]
fn tool_calls_have_separate_budget() {
    let mut b = budget();
    b.max_tool_calls = 1;
    assert_eq!(
        run_model([Ok("TOOL:echo".into()), Ok("TOOL:echo".into())], b).reason,
        StopReason::ToolBudget
    );
}
#[test]
#[ignore = "implement repeated action detection"]
fn repeated_actions_stop() {
    let mut b = budget();
    b.max_repeated_actions = 1;
    assert_eq!(
        run_model([Ok("CONTINUE".into()), Ok("CONTINUE".into())], b).reason,
        StopReason::RepeatedAction
    );
}
#[test]
#[ignore = "implement deadline"]
fn elapsed_time_is_a_hard_limit() {
    let mut m = FakeModel([Ok("CONTINUE".into())].into());
    let mut e = FakeEnv::default();
    let mut c = FakeClock([0, 2_000].into());
    assert_eq!(
        run_with(&mut m, &mut e, &mut c, budget()).reason,
        StopReason::Deadline
    );
}
#[test]
#[ignore = "record failures"]
fn model_error_is_in_trace() {
    let r = run_model([Err("timeout".into())], budget());
    assert_eq!(r.reason, StopReason::ModelError);
    assert!(!r.trace.is_empty());
}
