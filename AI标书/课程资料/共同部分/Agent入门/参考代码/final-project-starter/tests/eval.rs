/// Baseline evaluation: load eval.jsonl, run review() on every public case,
/// compute Precision / Recall / F1, Citation Correctness, and Abstention Accuracy.
use final_project_starter::*;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Instant;

const DATA_DIR: &str = "../../公开数据/final-project";

// ── Eval case schema (matching eval.jsonl) ───────────────────────────────

#[derive(Debug, Deserialize)]
struct EvalCase {
    case_id: String,
    document_path: String,
    clause_id: String,
    expected_risk_decision: String,
    expected_evidence_status: String,
    expected_next_action: String,
    #[allow(dead_code)]
    risk_type: String,
    acceptable_severities: Vec<String>,
    evidence_refs: Vec<String>,
    #[allow(dead_code)]
    rationale: String,
    #[allow(dead_code)]
    reviewed_by: Vec<String>,
    #[allow(dead_code)]
    tags: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    paired_case_id: Option<String>,
}

// ── Metrics accumulator ──────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Metrics {
    /// Risk detection
    tp: usize, // predicted risk, actually risk
    fp: usize, // predicted risk, actually not risk
    fn_: usize, // predicted not risk, actually risk
    tn: usize, // predicted not risk, actually not risk

    /// Citation correctness
    citations_correct: usize,
    citations_total: usize,

    /// Abstention accuracy (undetermined cases)
    abstain_correct: usize,
    abstain_total: usize,

    /// Counts
    total_cases: usize,
    passed: usize,
    failed: usize,

    /// Timing
    total_time_ms: u128,

    /// Individual case results
    results: Vec<CaseResult>,
}

#[derive(Debug)]
struct CaseResult {
    case_id: String,
    passed: bool,
    risk_ok: bool,
    evidence_status_ok: bool,
    next_action_ok: bool,
    citation_ok: bool,
    severity_ok: bool,
    details: String,
}

impl Metrics {
    fn precision(&self) -> f64 {
        if self.tp + self.fp == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fp) as f64
        }
    }

    fn recall(&self) -> f64 {
        if self.tp + self.fn_ == 0 {
            1.0
        } else {
            self.tp as f64 / (self.tp + self.fn_) as f64
        }
    }

    fn f1(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    fn citation_accuracy(&self) -> f64 {
        if self.citations_total == 0 {
            1.0
        } else {
            self.citations_correct as f64 / self.citations_total as f64
        }
    }

    fn abstention_accuracy(&self) -> f64 {
        if self.abstain_total == 0 {
            1.0
        } else {
            self.abstain_correct as f64 / self.abstain_total as f64
        }
    }

    fn accuracy(&self) -> f64 {
        if self.total_cases == 0 {
            0.0
        } else {
            (self.tp + self.tn) as f64 / self.total_cases as f64
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn load_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let content = fs::read_to_string(path).expect("cannot read JSONL file");
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<T>(l).expect("invalid JSONL line"))
        .collect()
}

fn load_rules_text() -> String {
    let rules_path = Path::new(DATA_DIR).join("rules.jsonl");
    let content = fs::read_to_string(&rules_path).expect("cannot read rules.jsonl");
    let mut rules_text = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let (Some(src), Some(loc), Some(text)) = (
                v["source_id"].as_str(),
                v["locator"].as_str(),
                v["verbatim_text"].as_str(),
            ) {
                rules_text.push_str(&format!("{}#{} {}\n", src, loc, text));
            }
        }
    }
    rules_text
}

/// Check if the output matches expectations for a single case.
fn evaluate_case(case: &EvalCase, report: &ReviewReport) -> CaseResult {
    let clause = report
        .clauses
        .iter()
        .find(|c| c.clause_id == case.clause_id);

    let Some(clause) = clause else {
        return CaseResult {
            case_id: case.case_id.clone(),
            passed: false,
            risk_ok: false,
            evidence_status_ok: false,
            next_action_ok: false,
            citation_ok: false,
            severity_ok: false,
            details: format!("clause {} not found in report", case.clause_id),
        };
    };

    // Map enums to expected strings
    let actual_risk = match clause.risk_decision {
        RiskDecision::Risk => "risk",
        RiskDecision::NoRisk => "no_risk",
        RiskDecision::Undetermined => "undetermined",
    };
    let actual_evidence = match clause.evidence_status {
        EvidenceStatus::Supported => "supported",
        EvidenceStatus::Partial => "partial",
        EvidenceStatus::Insufficient => "insufficient",
        EvidenceStatus::Conflicting => "conflicting",
    };
    let actual_next = match clause.next_action {
        NextAction::Complete => "complete",
        NextAction::RetrieveMore => "retrieve_more",
        NextAction::HumanReview => "human_review",
    };

    let risk_ok = actual_risk == case.expected_risk_decision;
    let evidence_status_ok = actual_evidence == case.expected_evidence_status;
    let next_action_ok = actual_next == case.expected_next_action;

    // Citation correctness: every evidence_ref should appear in clause.evidence
    let citation_ok = if case.evidence_refs.is_empty() {
        true // no citation expected
    } else {
        let expected_refs: HashSet<&str> = case.evidence_refs.iter().map(|s| s.as_str()).collect();
        let actual_refs: HashSet<String> = clause
            .evidence
            .iter()
            .map(|e| format!("{}#{}", e.source_id, e.locator))
            .collect();
        expected_refs.iter().all(|er| {
            actual_refs.iter().any(|ar| {
                ar == *er
            })
        })
    };

    // Severity check: actual severity should be in acceptable_severities
    let actual_severity = match clause.severity {
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
    };
    let severity_ok = case
        .acceptable_severities
        .iter()
        .any(|s| s == actual_severity);

    let all_ok = risk_ok && evidence_status_ok && next_action_ok && citation_ok && severity_ok;

    let mut details = Vec::new();
    if !risk_ok {
        details.push(format!(
            "risk: expected={} actual={}",
            case.expected_risk_decision, actual_risk
        ));
    }
    if !evidence_status_ok {
        details.push(format!(
            "evidence_status: expected={} actual={}",
            case.expected_evidence_status, actual_evidence
        ));
    }
    if !next_action_ok {
        details.push(format!(
            "next_action: expected={} actual={}",
            case.expected_next_action, actual_next
        ));
    }
    if !citation_ok {
        details.push(format!(
            "citation: expected refs {:?}, got {} evidence items",
            case.evidence_refs,
            clause.evidence.len()
        ));
    }
    if !severity_ok {
        details.push(format!(
            "severity: expected one of {:?} actual={}",
            case.acceptable_severities, actual_severity
        ));
    }

    CaseResult {
        case_id: case.case_id.clone(),
        passed: all_ok,
        risk_ok,
        evidence_status_ok,
        next_action_ok,
        citation_ok,
        severity_ok,
        details: if all_ok {
            "OK".into()
        } else {
            details.join("; ")
        },
    }
}

// ── Main eval test ───────────────────────────────────────────────────────

#[test]
fn baseline_eval_full_public_set() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(DATA_DIR);

    // Load eval cases
    let eval_path = data_dir.join("eval.jsonl");
    assert!(eval_path.exists(), "eval.jsonl not found at {:?}", eval_path);
    let cases: Vec<EvalCase> = load_jsonl(&eval_path);
    assert!(!cases.is_empty(), "no eval cases found");

    // Load rules once
    let rules_text = load_rules_text();
    assert!(!rules_text.is_empty(), "rules_text is empty");

    let mut metrics = Metrics::default();
    let start = Instant::now();

    for case in &cases {
        // Load bid document
        let bid_path = data_dir.join(&case.document_path);
        assert!(
            bid_path.exists(),
            "bid file not found: {:?} (case {})",
            bid_path,
            case.case_id
        );
        let bid_text = fs::read_to_string(&bid_path).unwrap_or_else(|e| {
            panic!("cannot read bid file {:?}: {}", bid_path, e)
        });

        // Run review
        let case_start = Instant::now();
        let report = review(&bid_text, &rules_text).unwrap_or_else(|e| {
            panic!("review failed for case {}: {:?}", case.case_id, e)
        });
        let case_ms = case_start.elapsed().as_millis();
        metrics.total_time_ms += case_ms;

        // Evaluate
        let result = evaluate_case(case, &report);
        metrics.total_cases += 1;

        if result.passed {
            metrics.passed += 1;
        } else {
            metrics.failed += 1;
        }

        // Update risk confusion matrix
        let clause = report
            .clauses
            .iter()
            .find(|c| c.clause_id == case.clause_id)
            .unwrap();
        let actual_risk = matches!(clause.risk_decision, RiskDecision::Risk);
        let expected_risk = case.expected_risk_decision == "risk";

        if actual_risk && expected_risk {
            metrics.tp += 1;
        } else if actual_risk && !expected_risk {
            metrics.fp += 1;
        } else if !actual_risk && expected_risk {
            metrics.fn_ += 1;
        } else {
            metrics.tn += 1;
        }

        // Citation correctness
        if !case.evidence_refs.is_empty() {
            metrics.citations_total += 1;
            if result.citation_ok {
                metrics.citations_correct += 1;
            }
        }

        // Abstention accuracy (for undetermined cases)
        if case.expected_risk_decision == "undetermined" {
            metrics.abstain_total += 1;
            if result.risk_ok {
                metrics.abstain_correct += 1;
            }
        }

        metrics.results.push(result);
    }

    let total_ms = start.elapsed().as_millis();

    // ── Print report ─────────────────────────────────────────────────────
    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║          RuleBaseline — Public Eval Results             ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Total cases:  {:<43}║", metrics.total_cases);
    println!("║ Passed:       {:<43}║", metrics.passed);
    println!("║ Failed:       {:<43}║", metrics.failed);
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Risk Precision:  {:<6.4}                              ║", metrics.precision());
    println!("║ Risk Recall:     {:<6.4}                              ║", metrics.recall());
    println!("║ Risk F1:         {:<6.4}                              ║", metrics.f1());
    println!("║ Accuracy:        {:<6.4}                              ║", metrics.accuracy());
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Citation Correctness: {:<6.4}                          ║", metrics.citation_accuracy());
    println!("║ Abstention Accuracy:  {:<6.4}                          ║", metrics.abstention_accuracy());
    println!("╠══════════════════════════════════════════════════════════╣");
    println!(
        "║ Avg time/case:     {:>4} ms                        ║",
        if metrics.total_cases > 0 {
            metrics.total_time_ms / metrics.total_cases as u128
        } else {
            0
        }
    );
    println!(
        "║ Total eval time:   {:>4} ms                        ║",
        total_ms
    );
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Confusion Matrix:                                      ║");
    println!(
        "║   TP={:<3}  FP={:<3}  FN={:<3}  TN={:<3}                      ║",
        metrics.tp, metrics.fp, metrics.fn_, metrics.tn
    );
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // ── Per-case details ─────────────────────────────────────────────────
    println!("Per-case results:");
    println!(
        "{:<6} {:<8} {:<8} {:<10} {:<10} {:<10} {}",
        "Case", "Risk", "Evid.", "Next Act.", "Citation", "Severity", "Details"
    );
    println!("{}", "-".repeat(80));
    for r in &metrics.results {
        println!(
            "{:<6} {:<8} {:<8} {:<10} {:<10} {:<10} {}",
            r.case_id,
            if r.risk_ok { "✓" } else { "✗" },
            if r.evidence_status_ok { "✓" } else { "✗" },
            if r.next_action_ok { "✓" } else { "✗" },
            if r.citation_ok { "✓" } else { "✗" },
            if r.severity_ok { "✓" } else { "✗" },
            r.details,
        );
    }

    // ── Assertions ───────────────────────────────────────────────────────
    if metrics.failed > 0 {
        println!();
        println!(
            "WARNING: {} case(s) failed — review the details above.",
            metrics.failed
        );
    }

    // Baseline must beat random (F1 > 0.5) — lower-bound sanity check
    assert!(metrics.f1() > 0.5, "Baseline F1 too low: {:.4}", metrics.f1());
    assert!(
        metrics.citation_accuracy() >= 0.8,
        "Citation accuracy too low: {:.4}",
        metrics.citation_accuracy()
    );
}

/// Stability test: run the full eval N times; results must be identical
/// for a deterministic RuleBaseline.
#[test]
fn baseline_stability_5_runs() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(DATA_DIR);
    let eval_path = data_dir.join("eval.jsonl");
    let cases: Vec<EvalCase> = load_jsonl(&eval_path);
    let rules_text = load_rules_text();

    const RUNS: usize = 5;
    let mut all_runs: Vec<Vec<String>> = Vec::new();

    for _run in 0..RUNS {
        let mut decisions: Vec<String> = Vec::new();
        for case in &cases {
            let bid_path = data_dir.join(&case.document_path);
            let bid_text = fs::read_to_string(&bid_path).unwrap();
            let report = review(&bid_text, &rules_text).unwrap();
            let clause = report
                .clauses
                .iter()
                .find(|c| c.clause_id == case.clause_id)
                .unwrap();
            let decision = match clause.risk_decision {
                RiskDecision::Risk => "risk",
                RiskDecision::NoRisk => "no_risk",
                RiskDecision::Undetermined => "undetermined",
            };
            decisions.push(format!("{}:{}", case.case_id, decision));
        }
        all_runs.push(decisions);
    }

    // All runs must produce identical decisions
    for run in 1..RUNS {
        assert_eq!(
            all_runs[0], all_runs[run],
            "Stability failure: run 0 != run {}",
            run
        );
    }

    println!(
        "Stability OK: {} runs, all {} decisions identical",
        RUNS,
        all_runs[0].len()
    );
}

/// Agent eval: run review_agent() on all public cases and verify results
/// match the baseline exactly (same rule-matching logic, different path).
#[test]
fn agent_eval_matches_baseline() {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(DATA_DIR);
    let eval_path = data_dir.join("eval.jsonl");
    let cases: Vec<EvalCase> = load_jsonl(&eval_path);

    // Load rules as JSONL (agent uses JSONL format)
    let rules_path = data_dir.join("rules.jsonl");
    let rules_jsonl = fs::read_to_string(&rules_path).expect("cannot read rules.jsonl");

    let mut passed = 0;
    let mut failed = 0;

    for case in &cases {
        let bid_path = data_dir.join(&case.document_path);
        let bid_text = fs::read_to_string(&bid_path).unwrap();

        // Run through the Agent pipeline
        let report = final_project_starter::review_agent(&bid_text, &rules_jsonl, None)
            .unwrap_or_else(|e| panic!("agent review failed for {}: {:?}", case.case_id, e));

        let clause = report
            .clauses
            .iter()
            .find(|c| c.clause_id == case.clause_id)
            .unwrap_or_else(|| {
                panic!("clause {} not found in agent report for {}", case.clause_id, case.case_id)
            });

        let actual_risk = match clause.risk_decision {
            RiskDecision::Risk => "risk",
            RiskDecision::NoRisk => "no_risk",
            RiskDecision::Undetermined => "undetermined",
        };

        if actual_risk == case.expected_risk_decision {
            passed += 1;
        } else {
            failed += 1;
            eprintln!(
                "AGENT MISMATCH {}: expected={} actual={}",
                case.case_id, case.expected_risk_decision, actual_risk
            );
        }
    }

    println!(
        "Agent eval: {}/{} cases match baseline ({:.1}%)",
        passed,
        cases.len(),
        (passed as f64 / cases.len() as f64) * 100.0
    );

    // The Agent must match or exceed baseline on the public set
    assert_eq!(failed, 0, "Agent diverged from baseline on {} cases", failed);
}
