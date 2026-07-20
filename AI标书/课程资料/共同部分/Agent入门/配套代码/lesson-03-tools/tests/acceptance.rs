use lesson_03_tools::*;
use std::{cell::Cell, path::Path, rc::Rc};
struct Echo;
impl Tool for Echo {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo",
            has_side_effect: false,
            max_input_len: 8,
        }
    }
    fn call(&mut self, input: &str) -> Result<String, ToolError> {
        if input.is_empty() {
            Err(ToolError::InvalidArgs)
        } else {
            Ok(input.into())
        }
    }
}
struct Writer;
impl Tool for Writer {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "write",
            has_side_effect: true,
            max_input_len: 8,
        }
    }
    fn call(&mut self, _: &str) -> Result<String, ToolError> {
        Ok("written".into())
    }
}
struct Flaky(usize);
impl Tool for Flaky {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "flaky",
            has_side_effect: false,
            max_input_len: 8,
        }
    }
    fn call(&mut self, _: &str) -> Result<String, ToolError> {
        self.0 += 1;
        if self.0 == 1 {
            Err(ToolError::Transient)
        } else {
            Ok("recovered".into())
        }
    }
}
struct AlwaysFails {
    name: &'static str,
    error: ToolError,
    calls: Rc<Cell<usize>>,
}
impl Tool for AlwaysFails {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name,
            has_side_effect: false,
            max_input_len: 8,
        }
    }
    fn call(&mut self, _: &str) -> Result<String, ToolError> {
        self.calls.set(self.calls.get() + 1);
        Err(self.error.clone())
    }
}
fn req(name: &str, input: &str) -> ToolRequest {
    ToolRequest {
        name: name.into(),
        input: input.into(),
        idempotency_key: None,
    }
}
fn read_policy() -> ExecutionPolicy {
    ExecutionPolicy {
        allow_side_effect: false,
        max_retries: 1,
    }
}
#[test]
#[ignore = "dispatch registered tools"]
fn dispatches_by_exact_name() {
    let mut r = Registry::default();
    r.register(Box::new(Echo));
    assert_eq!(r.execute(&req("echo", "hi"), &read_policy()).unwrap(), "hi");
}
#[test]
#[ignore = "fail closed"]
fn unknown_tool_is_not_guessed() {
    assert_eq!(
        Registry::default().execute(&req("ech0", "x"), &read_policy()),
        Err(ToolError::UnknownTool)
    );
}
#[test]
#[ignore = "validate before execution"]
fn invalid_args_are_rejected() {
    let mut r = Registry::default();
    r.register(Box::new(Echo));
    assert_eq!(
        r.execute(&req("echo", ""), &read_policy()),
        Err(ToolError::InvalidArgs)
    );
}
#[test]
#[ignore = "enforce permissions"]
fn side_effect_requires_permission() {
    let mut r = Registry::default();
    r.register(Box::new(Writer));
    assert_eq!(
        r.execute(&req("write", "x"), &read_policy()),
        Err(ToolError::PermissionDenied)
    );
}
#[test]
#[ignore = "bounded transient retry"]
fn transient_failure_can_recover_once() {
    let mut r = Registry::default();
    r.register(Box::new(Flaky(0)));
    assert_eq!(
        r.execute(&req("flaky", "x"), &read_policy()).unwrap(),
        "recovered"
    );
}
#[test]
#[ignore = "canonical workspace authorization"]
fn path_traversal_is_rejected() {
    assert_eq!(
        authorize_path(Path::new("workspace"), Path::new("../secret.txt")),
        Err(ToolError::PathOutsideWorkspace)
    );
}
#[test]
#[ignore = "idempotency for side effects"]
fn repeated_idempotency_key_reuses_result() {
    let mut r = Registry::default();
    r.register(Box::new(Writer));
    let p = ExecutionPolicy {
        allow_side_effect: true,
        max_retries: 0,
    };
    let q = ToolRequest {
        name: "write".into(),
        input: "x".into(),
        idempotency_key: Some("run-1-step-1".into()),
    };
    assert_eq!(r.execute(&q, &p).unwrap(), "written");
    assert_eq!(r.execute(&q, &p).unwrap(), "written");
    assert_eq!(r.completed_count(), 1);
}

#[test]
#[ignore = "enforce ToolSpec input limit"]
fn oversized_input_is_rejected_before_call() {
    let mut r = Registry::default();
    r.register(Box::new(Echo));
    assert_eq!(
        r.execute(&req("echo", "0123456789"), &read_policy()),
        Err(ToolError::InvalidArgs)
    );
}

#[test]
#[ignore = "timeout is visible and bounded"]
fn timeout_does_not_retry_without_explicit_policy() {
    let calls = Rc::new(Cell::new(0));
    let mut r = Registry::default();
    r.register(Box::new(AlwaysFails {
        name: "timeout",
        error: ToolError::Timeout,
        calls: calls.clone(),
    }));
    let p = ExecutionPolicy {
        allow_side_effect: false,
        max_retries: 0,
    };
    assert_eq!(r.execute(&req("timeout", "x"), &p), Err(ToolError::Timeout));
    assert_eq!(calls.get(), 1);
}

#[test]
#[ignore = "permanent errors never retry"]
fn permanent_error_is_attempted_once() {
    let calls = Rc::new(Cell::new(0));
    let mut r = Registry::default();
    r.register(Box::new(AlwaysFails {
        name: "permanent",
        error: ToolError::Permanent,
        calls: calls.clone(),
    }));
    let p = ExecutionPolicy {
        allow_side_effect: false,
        max_retries: 5,
    };
    assert_eq!(
        r.execute(&req("permanent", "x"), &p),
        Err(ToolError::Permanent)
    );
    assert_eq!(calls.get(), 1);
}
