//! Agent Model implementations.
//!
//! - `RuleBasedAgentModel`: a deterministic model that uses the baseline
//!   matching logic but outputs structured Agent actions (search_rules →
//!   read_source → output_finding → finish).  This demonstrates the full
//!   Agent architecture without requiring a live LLM.
//!
//! - `ScriptedModel`: replays a fixed sequence of actions (for testing).
use crate::runtime::{AgentModel, ModelInput, Observation};
use crate::{baseline_match, ClauseReport, ReviewReport, RiskDecision};
use regex::Regex;

// ── RuleBasedAgentModel ───────────────────────────────────────────────────

/// A deterministic model that drives the Agent through the review workflow.
///
/// For each clause, it:
/// 1. `search_rules` to find relevant rules
/// 2. `read_source` for each matching rule to get exact quotes
/// 3. `output_finding` with the structured ClauseReport
/// 4. `finish` when all clauses are processed
///
/// This model *uses* the baseline matching logic internally but expresses
/// every decision through the typed Action protocol, producing a full
/// audit trail.
pub struct RuleBasedAgentModel {
    /// Clauses to process: (clause_id, clause_text)
    clauses: Vec<(String, String)>,
    /// Index of the current clause being processed
    clause_index: usize,
    /// Phase within the current clause: Search, Read, Output, Done
    phase: ClausePhase,
    /// Rules found by search_rules for the current clause
    search_results: Vec<serde_json::Value>,
    /// Index into search_results for read_source
    read_index: usize,
    /// Completed clause reports
    completed: Vec<ClauseReport>,
    /// Has the agent called finish?
    finished: bool,
    /// The original bid_text (for document_id)
    document_id: String,
    /// Number of observations already processed (avoids re-processing)
    processed_obs_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClausePhase {
    Search,
    Read,
    Output,
    Done,
}

impl RuleBasedAgentModel {
    pub fn new(bid_text: &str) -> Self {
        let (document_id, clauses) = parse_clauses_for_model(bid_text);
        Self {
            clauses,
            clause_index: 0,
            phase: ClausePhase::Search,
            search_results: Vec::new(),
            read_index: 0,
            completed: Vec::new(),
            finished: false,
            document_id,
            processed_obs_count: 0,
        }
    }

    /// Generate the next action based on current state.
    fn next_action(&mut self) -> String {
        if self.finished {
            return json_action("finish", &serde_json::json!({"summary": "审查完成"}));
        }

        if self.clause_index >= self.clauses.len() {
            self.finished = true;
            return json_action("finish", &serde_json::json!({"summary": "审查完成"}));
        }

        let (clause_id, clause_text) = &self.clauses[self.clause_index].clone();
        let (cid, ctext) = (clause_id.clone(), clause_text.clone());

        match self.phase {
            ClausePhase::Search => {
                // Build a search query from the clause text
                let query = build_search_query(&ctext);
                json_action("search_rules", &serde_json::json!({"query": query}))
            }
            ClausePhase::Read => {
                if self.read_index < self.search_results.len() {
                    let result = &self.search_results[self.read_index];
                    let source_id = result["source_id"].as_str().unwrap_or("");
                    let locator = result["locator"].as_str().unwrap_or("");
                    self.read_index += 1;
                    json_action(
                        "read_source",
                        &serde_json::json!({"source_id": source_id, "locator": locator}),
                    )
                } else {
                    // All rules read — now output the finding
                    self.phase = ClausePhase::Output;
                    self.next_action()
                }
            }
            ClausePhase::Output => {
                // Use baseline_match to determine the finding, then format as JSON
                let rules_for_baseline: Vec<crate::RuleDef> = vec![]; // baseline_match doesn't use this
                let m = baseline_match(&ctext, &rules_for_baseline);
                let report = ClauseReport {
                    clause_id: cid.clone(),
                    clause_text: ctext.clone(),
                    risk_decision: m.risk_decision,
                    evidence_status: m.evidence_status,
                    evidence_strength: m.evidence_strength,
                    risk_type: m.risk_type,
                    severity: m.severity,
                    claim: m.claim,
                    evidence: m.evidence,
                    reasoning_summary: m.reasoning_summary,
                    confidence_basis: m.confidence_basis,
                    limitations: m.limitations,
                    next_action: m.next_action,
                };

                let finding_json = serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
                self.completed.push(report);
                self.phase = ClausePhase::Done;

                json_action(
                    "output_finding",
                    &serde_json::json!({
                        "clause_id": cid,
                        "finding_json": finding_json,
                    }),
                )
            }
            ClausePhase::Done => {
                // Move to next clause
                self.clause_index += 1;
                self.phase = ClausePhase::Search;
                self.search_results.clear();
                self.read_index = 0;
                self.next_action()
            }
        }
    }

    /// Update internal state based on tool observations.
    fn process_observation(&mut self, obs: &Observation) {
        match obs.tool_name.as_str() {
            "search_rules" => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&obs.output) {
                    if let Some(matches) = v["matches"].as_array() {
                        self.search_results = matches.clone();
                    }
                }
                self.phase = ClausePhase::Read;
                self.read_index = 0;
            }
            "read_source" => {
                // Stay in Read phase; read_index was already incremented
            }
            "output_finding" => {
                // Finding recorded; phase set to Done in next_action
            }
            "request_human" => {
                // Escalation recorded
            }
            _ => {}
        }
    }

    /// Build the final ReviewReport from collected findings.
    pub fn build_report(&self) -> ReviewReport {
        ReviewReport {
            document_id: self.document_id.clone(),
            clauses: self.completed.clone(),
            trace_path: String::new(),
        }
    }
}

impl AgentModel for RuleBasedAgentModel {
    fn complete(&mut self, input: &ModelInput) -> Result<String, String> {
        // Process only NEW observations since last call
        let new_obs = &input.observations[self.processed_obs_count..];
        for obs in new_obs {
            self.process_observation(obs);
        }
        self.processed_obs_count = input.observations.len();

        // If there was an error, log it and continue
        if let Some(ref err) = input.last_error {
            eprintln!("Agent model received error: {err}");
        }

        // Generate next action
        Ok(self.next_action())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn json_action(action: &str, arguments: &serde_json::Value) -> String {
    serde_json::to_string(&serde_json::json!({
        "action": action,
        "arguments": arguments,
    }))
    .unwrap_or_else(|_| format!(r#"{{"action":"{action}","arguments":{{}}}}"#))
}

fn parse_clauses_for_model(bid_text: &str) -> (String, Vec<(String, String)>) {
    let re_clause = Regex::new(r"^(c-\d+)\s+(.+)$").unwrap();
    let mut doc_id = String::new();
    let mut clauses = Vec::new();

    for line in bid_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(caps) = re_clause.captures(line) {
            clauses.push((caps[1].to_string(), caps[2].to_string()));
        } else if doc_id.is_empty() {
            doc_id = line.to_string();
        }
    }

    if doc_id.is_empty() {
        doc_id = "unknown".to_string();
    }

    (doc_id, clauses)
}

/// Build a search query from clause text by extracting key terms.
fn build_search_query(clause_text: &str) -> String {
    // Extract meaningful keywords for searching the rules corpus
    let keywords: Vec<&str> = clause_text
        .split(|c: char| c.is_ascii_punctuation() || c == '，' || c == '。' || c == '、' || c == '：' || c == '：')
        .flat_map(|s| s.split_whitespace())
        .filter(|w| w.len() >= 2)
        .collect();

    if keywords.is_empty() {
        clause_text.to_string()
    } else {
        keywords.join(" ")
    }
}

// ── ScriptedModel (for tests) ─────────────────────────────────────────────

pub struct ScriptedModel {
    responses: Vec<Result<String, String>>,
    index: usize,
}

impl ScriptedModel {
    pub fn new(responses: Vec<Result<String, String>>) -> Self {
        Self {
            responses,
            index: 0,
        }
    }
}

impl AgentModel for ScriptedModel {
    fn complete(&mut self, _input: &ModelInput) -> Result<String, String> {
        if self.index >= self.responses.len() {
            return Err("no more scripted responses".into());
        }
        let resp = self.responses[self.index].clone();
        self.index += 1;
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ProposedAction;

    #[test]
    fn rule_based_model_emits_search_first() {
        let mut model = RuleBasedAgentModel::new("c-01 检查备注：缺少签字");
        let input = ModelInput {
            step: 1,
            observations: vec![],
            last_error: None,
        };
        let raw = model.complete(&input).unwrap();
        let parsed = crate::actions::parse_action(&raw).unwrap();
        assert!(matches!(parsed, ProposedAction::SearchRules { .. }));
    }

    #[test]
    fn rule_based_model_processes_all_clauses() {
        let mut model = RuleBasedAgentModel::new(
            "项目测试\nc-01 缺少签字\nc-02 报价：1200000 元",
        );

        // Step through the full workflow
        let mut step = 0;
        let max_steps = 20;
        let mut observations: Vec<Observation> = Vec::new();

        while step < max_steps {
            step += 1;
            let input = ModelInput {
                step,
                observations: observations.clone(),
                last_error: None,
            };
            let raw = model.complete(&input).unwrap();
            let parsed = crate::actions::parse_action(&raw).unwrap();

            match parsed {
                ProposedAction::Finish { .. } => break,
                ProposedAction::SearchRules { .. } => {
                    observations.push(Observation {
                        tool_name: "search_rules".into(),
                        output: r#"{"matches":[{"source_id":"R1","locator":"1.1","title":"签字","summary":"签字","relevance_score":1}]}"#.into(),
                    });
                }
                ProposedAction::ReadSource { .. } => {
                    observations.push(Observation {
                        tool_name: "read_source".into(),
                        output: r#"{"found":true,"verbatim_text":"出现缺少签字时转人工复核"}"#.into(),
                    });
                }
                ProposedAction::OutputFinding { .. } => {
                    observations.push(Observation {
                        tool_name: "output_finding".into(),
                        output: r#"{"recorded":true}"#.into(),
                    });
                }
                _ => {}
            }
        }

        let report = model.build_report();
        assert_eq!(report.clauses.len(), 2);
        assert_eq!(report.clauses[0].risk_decision, RiskDecision::Risk);
        assert_eq!(report.clauses[1].risk_decision, RiskDecision::Risk);
    }
}
