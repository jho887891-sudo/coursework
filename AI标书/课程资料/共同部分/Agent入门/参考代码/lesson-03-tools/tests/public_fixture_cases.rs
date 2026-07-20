use lesson_03_tools::*;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("公开数据")
        .join("lesson-03-fixtures")
}

fn request(name: &str, arguments: Value, id: &str, idempotency_key: Option<&str>) -> ToolRequest {
    ToolRequest {
        request_id: id.to_owned(),
        name: name.to_owned(),
        arguments,
        idempotency_key: idempotency_key.map(str::to_owned),
    }
}

fn expected_kind(name: &str) -> ToolErrorKind {
    match name {
        "permission_denied" => ToolErrorKind::PermissionDenied,
        "unknown_tool" => ToolErrorKind::UnknownTool,
        "invalid_arguments" => ToolErrorKind::InvalidArguments,
        "transient" => ToolErrorKind::Transient,
        "permanent" => ToolErrorKind::Permanent,
        "timeout" => ToolErrorKind::Timeout,
        other => panic!("fixture 使用了未知 expected：{other}"),
    }
}

fn fault_modes(case: &Value) -> Vec<FaultMode> {
    case["failures"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|value| match value.as_str().unwrap() {
            "transient" => FaultMode::Transient,
            "success" => FaultMode::Success,
            "permanent" => FaultMode::Permanent,
            "timeout" => FaultMode::Timeout,
            other => panic!("fixture 使用了未知 fault mode：{other}"),
        })
        .collect()
}

#[tokio::test]
async fn all_twelve_public_tool_cases_match_their_expected_outcomes() {
    let root = fixture_root();
    let text = fs::read_to_string(root.join("tool-cases.jsonl")).unwrap();
    let cases: Vec<Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 12, "Lesson 3 固定公开集必须保持 12 条");

    for case in cases {
        let id = case["id"].as_str().unwrap();
        let tool_name = case["tool"].as_str().unwrap();
        let expected = case["expected"].as_str().unwrap_or("success");
        let max_retries = case["max_retries"].as_u64().unwrap_or(0) as usize;
        let mut registry = Registry::default();

        match tool_name {
            "read_fixture" => registry
                .register(Box::new(ReadWorkspaceFile::new(&root, 4096).unwrap()))
                .unwrap(),
            "echo" => registry.register(Box::new(EchoTool)).unwrap(),
            "faulty" => registry
                .register(Box::new(FaultyTool::new(fault_modes(&case), 10)))
                .unwrap(),
            "counter_add" => registry.register(Box::new(CounterTool::new(100))).unwrap(),
            "shell" => {}
            other => panic!("fixture 使用了未知 tool：{other}"),
        }

        let arguments = match tool_name {
            "read_fixture" => json!({"path": case["path"].as_str().unwrap_or("")}),
            "echo" => json!({"text": case["input"].as_str().unwrap_or("")}),
            "faulty" => json!({}),
            "counter_add" => json!({
                "key":case["key"].as_str().unwrap(),
                "amount":1
            }),
            "shell" => json!({"path":case["path"]}),
            _ => unreachable!(),
        };
        let policy = if id == "read_only_write" {
            ExecutionPolicy::read_only(max_retries)
        } else {
            ExecutionPolicy::with_state_changes(max_retries)
        };

        if id == "idempotent_replay" {
            let call = request(tool_name, arguments, id, case["idempotency_key"].as_str());
            let first = registry.execute(&call, &policy).await;
            let second = registry.execute(&call, &policy).await;
            assert!(matches!(first, ToolObservation::Success { .. }));
            assert!(matches!(
                second,
                ToolObservation::Success { replayed: true, .. }
            ));
            continue;
        }
        if id == "different_keys_execute" {
            for key in case["idempotency_keys"].as_array().unwrap() {
                let call = request(tool_name, arguments.clone(), id, key.as_str());
                assert!(matches!(
                    registry.execute(&call, &policy).await,
                    ToolObservation::Success { .. }
                ));
            }
            continue;
        }

        let call = request(tool_name, arguments, id, case["idempotency_key"].as_str());
        let observation = registry.execute(&call, &policy).await;
        if expected == "success" {
            assert!(
                matches!(observation, ToolObservation::Success { .. }),
                "case {id} 应成功，实际为 {observation:?}"
            );
        } else {
            let actual = observation.error_kind();
            let matches_expected = if expected == "permission_denied" {
                matches!(
                    actual,
                    Some(ToolErrorKind::PermissionDenied | ToolErrorKind::PathOutsideWorkspace)
                )
            } else {
                actual == Some(expected_kind(expected))
            };
            assert!(
                matches_expected,
                "case {id} 的错误分类不符合公开集：expected={expected}, actual={actual:?}"
            );
        }
    }
}
