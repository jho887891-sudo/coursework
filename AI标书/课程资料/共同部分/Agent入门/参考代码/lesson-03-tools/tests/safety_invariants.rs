use lesson_03_tools::*;
use serde_json::json;
use std::{path::PathBuf, sync::atomic::Ordering};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("公开数据")
        .join("lesson-03-fixtures")
}

fn request(name: &str, arguments: serde_json::Value, id: &str) -> ToolRequest {
    ToolRequest {
        request_id: id.to_owned(),
        name: name.to_owned(),
        arguments,
        idempotency_key: None,
    }
}

#[tokio::test]
async fn response_lost_after_commit_does_not_duplicate_the_side_effect() {
    let tool = CounterTool::with_lost_response_once(10);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let mut call = request(
        "counter_add",
        json!({"key":"danger","amount":1}),
        "lost-response",
    );
    call.idempotency_key = Some("stable-key".to_owned());
    let policy = ExecutionPolicy::with_state_changes(0);

    let first = registry.execute(&call, &policy).await;
    assert_eq!(first.error_kind(), Some(ToolErrorKind::Timeout));
    assert_eq!(handle.value("danger"), 1, "timeout 不代表动作没有发生");

    let second = registry.execute(&call, &policy).await;
    assert!(matches!(second, ToolObservation::Success { .. }));
    assert_eq!(handle.value("danger"), 1, "稳定 key 必须阻止重复副作用");
}

#[tokio::test]
async fn exhausted_transient_retry_is_bounded() {
    let tool = FaultyTool::new(
        [
            FaultMode::Transient,
            FaultMode::Transient,
            FaultMode::Success,
        ],
        100,
    );
    let calls = tool.calls_handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let result = registry
        .execute(
            &request("faulty", json!({}), "bounded"),
            &ExecutionPolicy::read_only(1),
        )
        .await;
    assert_eq!(result.error_kind(), Some(ToolErrorKind::Transient));
    assert_eq!(result.attempts(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn disabling_transient_retry_attempts_once() {
    let tool = FaultyTool::new([FaultMode::Transient, FaultMode::Success], 100);
    let calls = tool.calls_handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let policy = ExecutionPolicy {
        permissions: vec![Permission::ReadWorkspace],
        max_retries: 5,
        retry_transient: false,
    };
    let result = registry
        .execute(&request("faulty", json!({}), "no-retry"), &policy)
        .await;
    assert_eq!(result.error_kind(), Some(ToolErrorKind::Transient));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn two_different_keys_produce_two_side_effects() {
    let tool = CounterTool::new(100);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let policy = ExecutionPolicy::with_state_changes(0);
    for key in ["a", "b"] {
        let mut call = request(
            "counter_add",
            json!({"key":"x","amount":1}),
            &format!("request-{key}"),
        );
        call.idempotency_key = Some(key.to_owned());
        assert!(matches!(
            registry.execute(&call, &policy).await,
            ToolObservation::Success { .. }
        ));
    }
    assert_eq!(handle.value("x"), 2);
}

#[tokio::test]
async fn search_returns_stable_locator_and_exact_excerpt() {
    let root = fixture_root();
    let mut registry = Registry::default();
    registry
        .register(Box::new(SearchText::new(&root, 10).unwrap()))
        .unwrap();
    let result = registry
        .execute(
            &request("search_text", json!({"query":"允许读取"}), "search"),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    match result {
        ToolObservation::Success { output, .. } => {
            assert_eq!(output["matches"][0]["path"], "allowed.txt");
            assert_eq!(output["matches"][0]["line"], 1);
            assert!(output["matches"][0]["excerpt"]
                .as_str()
                .unwrap()
                .contains("允许读取"));
        }
        failure => panic!("expected success, got {failure:?}"),
    }
}

#[tokio::test]
async fn every_attempt_has_request_and_tool_identity_in_trace() {
    let mut registry = Registry::default();
    registry.register(Box::new(EchoTool)).unwrap();
    registry
        .execute(
            &request("echo", json!({"text":"trace"}), "trace-42"),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    assert!(registry
        .trace()
        .iter()
        .all(|event| { event.request_id == "trace-42" && event.tool_name == "echo" }));
    assert_eq!(
        registry
            .trace()
            .iter()
            .filter(|event| event.event_type == ToolTraceEventType::AttemptStarted)
            .count(),
        1
    );
    assert_eq!(
        registry.trace().last().unwrap().event_type,
        ToolTraceEventType::ToolSucceeded
    );
}

#[test]
fn lexical_normalization_allows_safe_parent_segments_but_not_escape() {
    let root = fixture_root();
    assert!(authorize_path(&root, std::path::Path::new("nested/../allowed.txt")).is_ok());
    assert_eq!(
        authorize_path(&root, std::path::Path::new("../secret.txt"))
            .unwrap_err()
            .kind,
        ToolErrorKind::PathOutsideWorkspace
    );
}
