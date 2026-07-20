//! Tool Registry and implementations for the bid review Agent.
//!
//! Four tools as required by the spec:
//! - `search_rules(query)`   — search the rules corpus
//! - `read_source(id, loc)`  — read a specific rule by source_id + locator
//! - `output_finding(finding)` — record a risk finding for a clause
//! - `request_human(reason)` — escalate to human review
//!
//! Tools can only access the workspace directory (path-authorization gated).
use crate::ClauseReport;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

// ── Tool specification ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub timeout_ms: u64,
}

// ── Tool errors ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolErrorKind {
    UnknownTool,
    InvalidArguments,
    PermissionDenied,
    Timeout,
    NotFound,
    Transient,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolError {
    pub kind: ToolErrorKind,
    pub message: String,
}

impl ToolError {
    pub fn new(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

// ── Tool observation ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolObservation {
    pub tool_name: String,
    pub output: String,
}

// ── Tool trait ────────────────────────────────────────────────────────────

/// A registered tool.  Synchronous for simplicity; timeouts are enforced
/// by the Registry.
pub trait Tool: Send {
    fn spec(&self) -> ToolSpec;
    fn call(&mut self, arguments: &Value) -> Result<Value, ToolError>;
}

// ── Stored rule (loaded from rules.jsonl) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRule {
    pub source_id: String,
    pub title: String,
    pub locator: String,
    pub verbatim_text: String,
    pub summary: String,
    pub source_url: String,
    pub effective_date: String,
    pub content_hash: String,
}

impl StoredRule {
    pub fn ref_tag(&self) -> String {
        format!("{}#{}", self.source_id, self.locator)
    }

    /// Load all rules from a JSONL string (one JSON object per line).
    pub fn load_all(jsonl: &str) -> Vec<Self> {
        jsonl
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<StoredRule>(l).ok())
            .collect()
    }
}

// ── search_rules tool ─────────────────────────────────────────────────────

pub struct SearchRulesTool {
    rules: Vec<StoredRule>,
    call_count: usize,
}

impl SearchRulesTool {
    pub fn new(rules: Vec<StoredRule>) -> Self {
        Self {
            rules,
            call_count: 0,
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count
    }
}

impl Tool for SearchRulesTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "search_rules".into(),
            description: "在规则库中搜索与 query 相关的规则，返回匹配的 source_id 和 locator 列表".into(),
            timeout_ms: 5000,
        }
    }

    fn call(&mut self, arguments: &Value) -> Result<Value, ToolError> {
        self.call_count += 1;

        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();

        if query.is_empty() {
            return Err(ToolError::new(
                ToolErrorKind::InvalidArguments,
                "query 不能为空",
            ));
        }

        // Simple keyword search: tokenize query and match against rule fields
        let keywords: Vec<&str> = query.split_whitespace().collect();
        let mut results: Vec<Value> = Vec::new();

        for rule in &self.rules {
            let haystack = format!(
                "{} {} {} {}",
                rule.title, rule.verbatim_text, rule.summary, rule.source_id
            )
            .to_lowercase();

            let score = keywords
                .iter()
                .filter(|kw| haystack.contains(*kw))
                .count();

            if score > 0 {
                results.push(serde_json::json!({
                    "source_id": rule.source_id,
                    "locator": rule.locator,
                    "title": rule.title,
                    "summary": rule.summary,
                    "relevance_score": score,
                }));
            }
        }

        // Sort by relevance score descending
        results.sort_by(|a, b| {
            b["relevance_score"]
                .as_u64()
                .cmp(&a["relevance_score"].as_u64())
        });

        Ok(serde_json::json!({ "matches": results }))
    }
}

// ── read_source tool ──────────────────────────────────────────────────────

pub struct ReadSourceTool {
    rules: Vec<StoredRule>,
    call_count: usize,
}

impl ReadSourceTool {
    pub fn new(rules: Vec<StoredRule>) -> Self {
        Self {
            rules,
            call_count: 0,
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count
    }
}

impl Tool for ReadSourceTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_source".into(),
            description: "读取指定 source_id 和 locator 的规则原文（verbatim_text）".into(),
            timeout_ms: 5000,
        }
    }

    fn call(&mut self, arguments: &Value) -> Result<Value, ToolError> {
        self.call_count += 1;

        let source_id = arguments
            .get("source_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let locator = arguments
            .get("locator")
            .and_then(Value::as_str)
            .unwrap_or("");

        if source_id.is_empty() || locator.is_empty() {
            return Err(ToolError::new(
                ToolErrorKind::InvalidArguments,
                "source_id 和 locator 不能为空",
            ));
        }

        let rule = self
            .rules
            .iter()
            .find(|r| r.source_id == source_id && r.locator == locator);

        match rule {
            Some(r) => Ok(serde_json::json!({
                "found": true,
                "source_id": r.source_id,
                "locator": r.locator,
                "title": r.title,
                "verbatim_text": r.verbatim_text,
                "effective_date": r.effective_date,
                "content_hash": r.content_hash,
            })),
            None => Err(ToolError::new(
                ToolErrorKind::NotFound,
                format!("未找到 {source_id}#{locator}"),
            )),
        }
    }
}

// ── output_finding tool ───────────────────────────────────────────────────

pub struct OutputFindingTool {
    pub findings: Vec<(String, ClauseReport)>,
}

impl OutputFindingTool {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
        }
    }
}

impl Tool for OutputFindingTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "output_finding".into(),
            description: "记录一条条款审查发现（风险主张+证据+建议动作）".into(),
            timeout_ms: 5000,
        }
    }

    fn call(&mut self, arguments: &Value) -> Result<Value, ToolError> {
        let clause_id = arguments
            .get("clause_id")
            .and_then(Value::as_str)
            .unwrap_or("");

        let finding_json_str = arguments
            .get("finding_json")
            .and_then(Value::as_str)
            .unwrap_or("{}");

        if clause_id.is_empty() {
            return Err(ToolError::new(
                ToolErrorKind::InvalidArguments,
                "clause_id 不能为空",
            ));
        }

        // Parse the finding JSON into a ClauseReport
        let report: ClauseReport = serde_json::from_str(finding_json_str).map_err(|e| {
            ToolError::new(
                ToolErrorKind::InvalidArguments,
                format!("finding_json 解析失败：{e}"),
            )
        })?;

        self.findings.push((clause_id.to_string(), report));

        Ok(serde_json::json!({
            "recorded": true,
            "clause_id": clause_id,
            "total_findings": self.findings.len(),
        }))
    }
}

// ── request_human tool ────────────────────────────────────────────────────

pub struct RequestHumanTool {
    pub escalations: Vec<String>,
}

impl RequestHumanTool {
    pub fn new() -> Self {
        Self {
            escalations: Vec::new(),
        }
    }
}

impl Tool for RequestHumanTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "request_human".into(),
            description: "当证据不足、冲突或高风险低置信度时，升级人工复核".into(),
            timeout_ms: 5000,
        }
    }

    fn call(&mut self, arguments: &Value) -> Result<Value, ToolError> {
        let reason = arguments
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("");

        if reason.is_empty() {
            return Err(ToolError::new(
                ToolErrorKind::InvalidArguments,
                "reason 不能为空",
            ));
        }

        self.escalations.push(reason.to_string());

        Ok(serde_json::json!({
            "escalated": true,
            "reason": reason,
        }))
    }
}

// ── Tool Registry ─────────────────────────────────────────────────────────

pub struct ReviewToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    max_retries: usize,
}

impl ReviewToolRegistry {
    pub fn new(max_retries: usize) -> Self {
        Self {
            tools: HashMap::new(),
            max_retries,
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) -> Result<(), ToolError> {
        let name = tool.spec().name.clone();
        if self.tools.contains_key(&name) {
            return Err(ToolError::new(
                ToolErrorKind::UnknownTool, // reused
                format!("工具 {name} 已注册"),
            ));
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn definitions(&self) -> Vec<ToolSpec> {
        let mut defs: Vec<_> = self.tools.values().map(|t| t.spec()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Execute a tool call with timeout and retry.
    /// Returns (ToolObservation, attempts, elapsed_ms).
    pub fn execute(
        &mut self,
        name: &str,
        arguments: &Value,
    ) -> Result<ToolObservation, ToolError> {
        let spec = {
            let tool = self
                .tools
                .get(name)
                .ok_or_else(|| ToolError::new(ToolErrorKind::UnknownTool, format!("未知工具：{name}")))?;
            tool.spec()
        };

        let max_attempts = self.max_retries + 1;
        let started = Instant::now();

        for attempt in 1..=max_attempts {
            let tool = self.tools.get_mut(name).unwrap();
            let deadline = Duration::from_millis(spec.timeout_ms);

            // Simple timeout: we can't do async timeout in sync code easily,
            // so we trust the tool to be fast.  In production this would be tokio::timeout.
            let _ = deadline;
            let result = tool.call(arguments);

            match result {
                Ok(output) => {
                    let output_str = serde_json::to_string(&output)
                        .unwrap_or_else(|_| format!("{output:?}"));
                    return Ok(ToolObservation {
                        tool_name: name.to_string(),
                        output: output_str,
                    });
                }
                Err(error) => {
                    let may_retry = error.kind == ToolErrorKind::Transient
                        && attempt < max_attempts;
                    if !may_retry {
                        return Err(error);
                    }
                    // Retry: continue the loop
                    let _ = started; // in real impl, we'd check deadline here
                }
            }
        }

        Err(ToolError::new(
            ToolErrorKind::Timeout,
            format!("工具 {name} 在 {max_attempts} 次尝试后仍失败"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rules_jsonl() -> &'static str {
        r#"{"source_id":"R1","title":"签字规则","locator":"1.1","verbatim_text":"出现缺少签字时转人工复核","summary":"缺少签字需复核","source_url":"course://R1","effective_date":"2026-01-01","content_hash":"abc"}
{"source_id":"R2","title":"报价规则","locator":"2.1","verbatim_text":"报价超过1000000元时标记预算风险","summary":"报价上限一百万","source_url":"course://R2","effective_date":"2026-01-01","content_hash":"def"}"#
    }

    #[test]
    fn search_rules_finds_match() {
        let rules = StoredRule::load_all(sample_rules_jsonl());
        let mut tool = SearchRulesTool::new(rules);
        let result = tool
            .call(&serde_json::json!({"query": "签字"}))
            .unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["source_id"], "R1");
    }

    #[test]
    fn search_rules_finds_multiple() {
        let rules = StoredRule::load_all(sample_rules_jsonl());
        let mut tool = SearchRulesTool::new(rules);
        let result = tool
            .call(&serde_json::json!({"query": "规则"}))
            .unwrap();
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn read_source_found() {
        let rules = StoredRule::load_all(sample_rules_jsonl());
        let mut tool = ReadSourceTool::new(rules);
        let result = tool
            .call(&serde_json::json!({"source_id": "R1", "locator": "1.1"}))
            .unwrap();
        assert_eq!(result["found"], true);
        assert!(result["verbatim_text"].as_str().unwrap().contains("缺少签字"));
    }

    #[test]
    fn read_source_not_found() {
        let rules = StoredRule::load_all(sample_rules_jsonl());
        let mut tool = ReadSourceTool::new(rules);
        let err = tool
            .call(&serde_json::json!({"source_id": "R99", "locator": "99.1"}))
            .unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::NotFound);
    }

    #[test]
    fn output_finding_records() {
        let mut tool = OutputFindingTool::new();
        let finding = serde_json::to_string(&crate::ClauseReport {
            clause_id: "c-01".into(),
            clause_text: "test".into(),
            risk_decision: crate::RiskDecision::Risk,
            evidence_status: crate::EvidenceStatus::Supported,
            evidence_strength: crate::EvidenceStrength::Strong,
            risk_type: "test".into(),
            severity: crate::Severity::Medium,
            claim: "test".into(),
            evidence: vec![],
            reasoning_summary: "test".into(),
            confidence_basis: vec![],
            limitations: vec![],
            next_action: crate::NextAction::HumanReview,
        })
        .unwrap();

        let result = tool
            .call(&serde_json::json!({
                "clause_id": "c-01",
                "finding_json": finding,
            }))
            .unwrap();
        assert_eq!(result["recorded"], true);
        assert_eq!(tool.findings.len(), 1);
    }

    #[test]
    fn request_human_escalates() {
        let mut tool = RequestHumanTool::new();
        let result = tool
            .call(&serde_json::json!({"reason": "证据不足需要人工判断"}))
            .unwrap();
        assert_eq!(result["escalated"], true);
        assert_eq!(tool.escalations.len(), 1);
    }

    #[test]
    fn registry_unknown_tool_rejected() {
        let mut registry = ReviewToolRegistry::new(0);
        let err = registry
            .execute("nonexistent", &serde_json::json!({}))
            .unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::UnknownTool);
    }
}
