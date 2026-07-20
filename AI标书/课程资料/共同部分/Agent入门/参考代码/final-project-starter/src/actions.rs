//! Typed Actions for the bid review Agent.
//!
//! Every model output is parsed into a `ProposedAction`, validated into an
//! `ApprovedAction`, and then executed by the Runtime against the Tool Registry.
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Domain actions ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposedAction {
    Continue,
    SearchRules { query: String },
    ReadSource { source_id: String, locator: String },
    OutputFinding { clause_id: String, finding_json: String },
    RequestHuman { reason: String },
    Finish { summary: String },
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
    pub arguments: Value,
}

// ── Protocol errors ──────────────────────────────────────────────────────

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
    pub fn new(kind: ProtocolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

// ── Parse model output → ProposedAction ──────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContinueEnvelope {
    action: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchRulesArgs {
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadSourceArgs {
    source_id: String,
    locator: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputFindingArgs {
    clause_id: String,
    finding_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestHumanArgs {
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishArgs {
    summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionEnvelope<T> {
    action: String,
    arguments: T,
}

pub fn parse_action(raw: &str) -> Result<ProposedAction, ProtocolError> {
    let value: Value = serde_json::from_str(raw).map_err(|e| {
        ProtocolError::new(
            ProtocolErrorKind::InvalidJson,
            format!("模型输出不是合法 JSON：{e}"),
        )
    })?;

    let action = value
        .as_object()
        .and_then(|o| o.get("action"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProtocolError::new(ProtocolErrorKind::MissingAction, "顶层必须包含字符串字段 action")
        })?;

    match action {
        "continue" => {
            let _envelope: ContinueEnvelope =
                serde_json::from_value(value).map_err(|e| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("continue 结构不合法：{e}"),
                    )
                })?;
            Ok(ProposedAction::Continue)
        }
        "search_rules" => {
            let envelope: ActionEnvelope<SearchRulesArgs> =
                serde_json::from_value(value).map_err(|e| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("search_rules.arguments 不合法：{e}"),
                    )
                })?;
            Ok(ProposedAction::SearchRules {
                query: envelope.arguments.query,
            })
        }
        "read_source" => {
            let envelope: ActionEnvelope<ReadSourceArgs> =
                serde_json::from_value(value).map_err(|e| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("read_source.arguments 不合法：{e}"),
                    )
                })?;
            Ok(ProposedAction::ReadSource {
                source_id: envelope.arguments.source_id,
                locator: envelope.arguments.locator,
            })
        }
        "output_finding" => {
            let envelope: ActionEnvelope<OutputFindingArgs> =
                serde_json::from_value(value).map_err(|e| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("output_finding.arguments 不合法：{e}"),
                    )
                })?;
            Ok(ProposedAction::OutputFinding {
                clause_id: envelope.arguments.clause_id,
                finding_json: envelope.arguments.finding_json,
            })
        }
        "request_human" => {
            let envelope: ActionEnvelope<RequestHumanArgs> =
                serde_json::from_value(value).map_err(|e| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("request_human.arguments 不合法：{e}"),
                    )
                })?;
            Ok(ProposedAction::RequestHuman {
                reason: envelope.arguments.reason,
            })
        }
        "finish" => {
            let envelope: ActionEnvelope<FinishArgs> =
                serde_json::from_value(value).map_err(|e| {
                    ProtocolError::new(
                        ProtocolErrorKind::InvalidArguments,
                        format!("finish.arguments 不合法：{e}"),
                    )
                })?;
            Ok(ProposedAction::Finish {
                summary: envelope.arguments.summary,
            })
        }
        unknown => Err(ProtocolError::new(
            ProtocolErrorKind::UnknownAction,
            format!("未知动作：{unknown}"),
        )),
    }
}

// ── Validate ProposedAction → ApprovedAction ─────────────────────────────

pub fn validate_action(action: ProposedAction) -> Result<ApprovedAction, ProtocolError> {
    match action {
        ProposedAction::Continue => Ok(ApprovedAction::Continue),
        ProposedAction::SearchRules { query } if query.trim().is_empty() => {
            Err(ProtocolError::new(
                ProtocolErrorKind::InvalidArguments,
                "search_rules.query 不能为空",
            ))
        }
        ProposedAction::SearchRules { query } => {
            Ok(ApprovedAction::UseTool(ToolCall {
                name: "search_rules".into(),
                arguments: serde_json::json!({ "query": query }),
            }))
        }
        ProposedAction::ReadSource {
            source_id,
            locator,
        } if source_id.trim().is_empty() || locator.trim().is_empty() => Err(
            ProtocolError::new(
                ProtocolErrorKind::InvalidArguments,
                "read_source.source_id 和 locator 不能为空",
            ),
        ),
        ProposedAction::ReadSource {
            source_id,
            locator,
        } => Ok(ApprovedAction::UseTool(ToolCall {
            name: "read_source".into(),
            arguments: serde_json::json!({ "source_id": source_id, "locator": locator }),
        })),
        ProposedAction::OutputFinding {
            clause_id,
            finding_json,
        } if clause_id.trim().is_empty() || finding_json.trim().is_empty() => Err(
            ProtocolError::new(
                ProtocolErrorKind::InvalidArguments,
                "output_finding 参数不能为空",
            ),
        ),
        ProposedAction::OutputFinding {
            clause_id,
            finding_json,
        } => {
            // Validate that finding_json is valid JSON
            if serde_json::from_str::<Value>(&finding_json).is_err() {
                return Err(ProtocolError::new(
                    ProtocolErrorKind::InvalidArguments,
                    "output_finding.finding_json 不是合法 JSON",
                ));
            }
            Ok(ApprovedAction::UseTool(ToolCall {
                name: "output_finding".into(),
                arguments: serde_json::json!({
                    "clause_id": clause_id,
                    "finding_json": finding_json,
                }),
            }))
        }
        ProposedAction::RequestHuman { reason } if reason.trim().is_empty() => Err(
            ProtocolError::new(
                ProtocolErrorKind::InvalidArguments,
                "request_human.reason 不能为空",
            ),
        ),
        ProposedAction::RequestHuman { reason } => {
            Ok(ApprovedAction::UseTool(ToolCall {
                name: "request_human".into(),
                arguments: serde_json::json!({ "reason": reason }),
            }))
        }
        ProposedAction::Finish { summary } if summary.trim().is_empty() => Err(
            ProtocolError::new(
                ProtocolErrorKind::InvalidArguments,
                "finish.summary 不能为空",
            ),
        ),
        ProposedAction::Finish { summary } => Ok(ApprovedAction::Finish { answer: summary }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_search_rules() {
        let raw = r#"{"action":"search_rules","arguments":{"query":"保证金"}}"#;
        let action = parse_action(raw).unwrap();
        assert_eq!(
            action,
            ProposedAction::SearchRules {
                query: "保证金".into()
            }
        );
    }

    #[test]
    fn parse_read_source() {
        let raw = r#"{"action":"read_source","arguments":{"source_id":"R1","locator":"1.1"}}"#;
        let action = parse_action(raw).unwrap();
        assert_eq!(
            action,
            ProposedAction::ReadSource {
                source_id: "R1".into(),
                locator: "1.1".into()
            }
        );
    }

    #[test]
    fn parse_output_finding() {
        let raw = r#"{"action":"output_finding","arguments":{"clause_id":"c-01","finding_json":"{}"}}"#;
        let action = parse_action(raw).unwrap();
        assert!(matches!(action, ProposedAction::OutputFinding { .. }));
    }

    #[test]
    fn parse_request_human() {
        let raw = r#"{"action":"request_human","arguments":{"reason":"证据冲突"}}"#;
        let action = parse_action(raw).unwrap();
        assert_eq!(
            action,
            ProposedAction::RequestHuman {
                reason: "证据冲突".into()
            }
        );
    }

    #[test]
    fn parse_finish() {
        let raw = r#"{"action":"finish","arguments":{"summary":"审查完成"}}"#;
        let action = parse_action(raw).unwrap();
        assert_eq!(
            action,
            ProposedAction::Finish {
                summary: "审查完成".into()
            }
        );
    }

    #[test]
    fn parse_continue() {
        let raw = r#"{"action":"continue"}"#;
        let action = parse_action(raw).unwrap();
        assert_eq!(action, ProposedAction::Continue);
    }

    #[test]
    fn unknown_action_is_rejected() {
        let raw = r#"{"action":"delete_everything","arguments":{}}"#;
        let err = parse_action(raw).unwrap_err();
        assert_eq!(err.kind, ProtocolErrorKind::UnknownAction);
    }

    #[test]
    fn extra_fields_are_rejected() {
        let raw = r#"{"action":"continue","surprise":true}"#;
        let err = parse_action(raw).unwrap_err();
        assert_eq!(err.kind, ProtocolErrorKind::InvalidArguments);
    }

    #[test]
    fn validate_empty_query_rejected() {
        let action = ProposedAction::SearchRules {
            query: "  ".into(),
        };
        let err = validate_action(action).unwrap_err();
        assert_eq!(err.kind, ProtocolErrorKind::InvalidArguments);
    }

    #[test]
    fn validate_bad_finding_json_rejected() {
        let action = ProposedAction::OutputFinding {
            clause_id: "c-01".into(),
            finding_json: "not json".into(),
        };
        let err = validate_action(action).unwrap_err();
        assert_eq!(err.kind, ProtocolErrorKind::InvalidArguments);
    }
}
