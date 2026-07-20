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

fn request(name: &str, arguments: serde_json::Value) -> ToolRequest {
    ToolRequest {
        request_id: format!("test-{name}"),
        name: name.to_owned(),
        arguments,
        idempotency_key: None,
    }
}

fn failure_kind(observation: &ToolObservation) -> ToolErrorKind {
    observation.error_kind().expect("expected failure")
}

#[tokio::test]
async fn dispatches_only_by_exact_registered_name() {
    let mut registry = Registry::default();
    registry.register(Box::new(EchoTool)).unwrap();
    let result = registry
        .execute(
            &request("echo", json!({"text":"hello"})),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    assert_eq!(
        result,
        ToolObservation::Success {
            output: json!({"text":"hello"}),
            attempts: 1,
            replayed: false
        }
    );

    let unknown = registry
        .execute(
            &request("ech0", json!({"text":"hello"})),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    assert_eq!(failure_kind(&unknown), ToolErrorKind::UnknownTool);
    assert_eq!(unknown.attempts(), 0);
}

#[tokio::test]
async fn invalid_arguments_are_rejected_before_any_attempt() {
    let mut registry = Registry::default();
    registry.register(Box::new(EchoTool)).unwrap();
    for arguments in [
        json!({}),
        json!({"text":""}),
        json!({"text":7}),
        json!({"text":"ok","surprise":true}),
    ] {
        let result = registry
            .execute(&request("echo", arguments), &ExecutionPolicy::read_only(0))
            .await;
        assert_eq!(failure_kind(&result), ToolErrorKind::InvalidArguments);
        assert_eq!(result.attempts(), 0);
    }
    assert!(!registry
        .trace()
        .iter()
        .any(|event| event.event_type == ToolTraceEventType::AttemptStarted));
}

#[tokio::test]
async fn side_effect_requires_permission_and_idempotency_key() {
    let tool = CounterTool::new(100);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();

    let denied = registry
        .execute(
            &request("counter_add", json!({"key":"x","amount":1})),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    assert_eq!(failure_kind(&denied), ToolErrorKind::PermissionDenied);
    assert_eq!(handle.value("x"), 0);

    let missing_key = registry
        .execute(
            &request("counter_add", json!({"key":"x","amount":1})),
            &ExecutionPolicy::with_state_changes(0),
        )
        .await;
    assert_eq!(
        failure_kind(&missing_key),
        ToolErrorKind::MissingIdempotencyKey
    );
    assert_eq!(handle.value("x"), 0);
}

#[tokio::test]
async fn transient_failure_recovers_with_one_bounded_retry() {
    let tool = FaultyTool::new([FaultMode::Transient, FaultMode::Success], 100);
    let calls = tool.calls_handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let result = registry
        .execute(
            &request("faulty", json!({})),
            &ExecutionPolicy::read_only(1),
        )
        .await;
    assert!(matches!(
        result,
        ToolObservation::Success { attempts: 2, .. }
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        registry
            .trace()
            .iter()
            .filter(|event| event.event_type == ToolTraceEventType::RetryScheduled)
            .count(),
        1
    );
}

#[tokio::test]
async fn permanent_failure_is_never_retried() {
    let tool = FaultyTool::new([FaultMode::Permanent, FaultMode::Success], 100);
    let calls = tool.calls_handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let result = registry
        .execute(
            &request("faulty", json!({})),
            &ExecutionPolicy::read_only(5),
        )
        .await;
    assert_eq!(failure_kind(&result), ToolErrorKind::Permanent);
    assert_eq!(result.attempts(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn timeout_is_visible_and_not_automatically_retried() {
    let tool = FaultyTool::new([FaultMode::Timeout, FaultMode::Success], 10);
    let calls = tool.calls_handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let result = registry
        .execute(
            &request("faulty", json!({})),
            &ExecutionPolicy::read_only(5),
        )
        .await;
    assert_eq!(failure_kind(&result), ToolErrorKind::Timeout);
    assert_eq!(result.attempts(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn malformed_tool_output_is_rejected_by_the_registry() {
    let mut registry = Registry::default();
    registry
        .register(Box::new(FaultyTool::new([FaultMode::MalformedOutput], 100)))
        .unwrap();
    let result = registry
        .execute(
            &request("faulty", json!({})),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    assert_eq!(failure_kind(&result), ToolErrorKind::MalformedOutput);
}

#[tokio::test]
async fn workspace_reader_allows_fixture_and_rejects_traversal() {
    let root = fixture_root();
    let mut registry = Registry::default();
    registry
        .register(Box::new(ReadWorkspaceFile::new(&root, 4096).unwrap()))
        .unwrap();
    let policy = ExecutionPolicy::read_only(0);

    let allowed = registry
        .execute(
            &request("read_fixture", json!({"path":"allowed.txt"})),
            &policy,
        )
        .await;
    assert!(matches!(allowed, ToolObservation::Success { .. }));

    let traversal = registry
        .execute(
            &request("read_fixture", json!({"path":"../secret.txt"})),
            &policy,
        )
        .await;
    assert_eq!(
        failure_kind(&traversal),
        ToolErrorKind::PathOutsideWorkspace
    );
}

#[tokio::test]
async fn registry_replays_one_completed_side_effect() {
    let tool = CounterTool::new(100);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let mut call = request("counter_add", json!({"key":"x","amount":1}));
    call.idempotency_key = Some("run-1-step-1".to_owned());
    let policy = ExecutionPolicy::with_state_changes(0);

    let first = registry.execute(&call, &policy).await;
    let second = registry.execute(&call, &policy).await;
    assert!(matches!(
        first,
        ToolObservation::Success {
            attempts: 1,
            replayed: false,
            ..
        }
    ));
    assert!(matches!(
        second,
        ToolObservation::Success {
            attempts: 0,
            replayed: true,
            ..
        }
    ));
    assert_eq!(handle.value("x"), 1);
    assert_eq!(registry.completed_count(), 1);
}

#[tokio::test]
async fn same_idempotency_key_with_different_arguments_is_a_conflict() {
    let tool = CounterTool::new(100);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let policy = ExecutionPolicy::with_state_changes(0);
    let mut first = request("counter_add", json!({"key":"x","amount":1}));
    first.idempotency_key = Some("same-key".to_owned());
    let mut conflicting = request("counter_add", json!({"key":"x","amount":5}));
    conflicting.idempotency_key = Some("same-key".to_owned());

    registry.execute(&first, &policy).await;
    let result = registry.execute(&conflicting, &policy).await;
    assert_eq!(failure_kind(&result), ToolErrorKind::IdempotencyConflict);
    assert_eq!(handle.value("x"), 1);
}

#[test]
fn duplicate_registration_is_rejected_and_definitions_are_sorted() {
    let mut registry = Registry::default();
    registry
        .register(Box::new(FaultyTool::new([], 100)))
        .unwrap();
    registry.register(Box::new(EchoTool)).unwrap();
    let duplicate = registry.register(Box::new(EchoTool)).unwrap_err();
    assert_eq!(duplicate.kind, ToolErrorKind::DuplicateRegistration);
    assert_eq!(
        registry
            .definitions()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>(),
        vec!["echo", "faulty"]
    );
}
