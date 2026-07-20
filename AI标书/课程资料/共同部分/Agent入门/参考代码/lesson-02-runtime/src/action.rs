use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposedAction {
    Continue,
    Echo { text: String },
    Finish { answer: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovedAction {
    Continue,
    UseTool(ToolCall),
    Finish { answer: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolErrorKind {
    InvalidJson,
    MissingAction,
    UnknownAction,
    InvalidArguments,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub kind: ProtocolErrorKind,
    pub message: String,
}

impl ProtocolError {
    fn new(kind: ProtocolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContinueEnvelope {
    action: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionEnvelope<T> {
    action: String,
    arguments: T,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EchoArguments {
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishArguments {
    answer: String,
}

pub fn parse_action(raw: &str) -> Result<ProposedAction, ProtocolError> {
    let value: Value = serde_json::from_str(raw).map_err(|error| {
        ProtocolError::new(
            ProtocolErrorKind::InvalidJson,
            format!("模型输出不是合法 JSON：{error}"),
        )
    })?;

    let action = value
        .as_object()
        .and_then(|object| object.get("action"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProtocolError::new(
                ProtocolErrorKind::MissingAction,
                "顶层必须包含字符串字段 action",
            )
        })?;

    match action {
        "continue" => {
            let envelope: ContinueEnvelope = serde_json::from_value(value).map_err(|error| {
                ProtocolError::new(
                    ProtocolErrorKind::InvalidArguments,
                    format!("continue 结构不合法：{error}"),
                )
            })?;
            debug_assert_eq!(envelope.action, "continue");
            Ok(ProposedAction::Continue)
        }
        "echo" => {
            let envelope: ActionEnvelope<EchoArguments> =
                serde_json::from_value(value).map_err(|error| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("echo.arguments 不合法：{error}"),
                    )
                })?;
            debug_assert_eq!(envelope.action, "echo");
            Ok(ProposedAction::Echo {
                text: envelope.arguments.text,
            })
        }
        "finish" => {
            let envelope: ActionEnvelope<FinishArguments> =
                serde_json::from_value(value).map_err(|error| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("finish.arguments 不合法：{error}"),
                    )
                })?;
            debug_assert_eq!(envelope.action, "finish");
            Ok(ProposedAction::Finish {
                answer: envelope.arguments.answer,
            })
        }
        unknown => Err(ProtocolError::new(
            ProtocolErrorKind::UnknownAction,
            format!("未知动作：{unknown}"),
        )),
    }
}

pub fn validate_action(action: ProposedAction) -> Result<ApprovedAction, ProtocolError> {
    match action {
        ProposedAction::Continue => Ok(ApprovedAction::Continue),
        ProposedAction::Echo { text } if text.trim().is_empty() => Err(ProtocolError::new(
            ProtocolErrorKind::InvalidArguments,
            "echo.arguments.text 不能为空",
        )),
        ProposedAction::Echo { text } => Ok(ApprovedAction::UseTool(ToolCall {
            name: "echo".to_owned(),
            text,
        })),
        ProposedAction::Finish { answer } if answer.trim().is_empty() => Err(ProtocolError::new(
            ProtocolErrorKind::InvalidArguments,
            "finish.arguments.answer 不能为空",
        )),
        ProposedAction::Finish { answer } => Ok(ApprovedAction::Finish { answer }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_field_order_does_not_change_the_action() {
        let first = parse_action(r#"{"action":"echo","arguments":{"text":"hello"}}"#).unwrap();
        let second = parse_action(r#"{"arguments":{"text":"hello"},"action":"echo"}"#).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn extra_fields_are_rejected() {
        let error = parse_action(r#"{"action":"continue","surprise":true}"#).unwrap_err();
        assert_eq!(error.kind, ProtocolErrorKind::InvalidArguments);
    }
}
