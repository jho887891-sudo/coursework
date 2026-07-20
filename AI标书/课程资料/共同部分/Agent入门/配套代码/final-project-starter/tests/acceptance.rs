use final_project_starter::*;
#[test]
#[ignore = "implement RuleBaseline before Agent integration"]
fn baseline_detects_explicit_missing_signature() {
    let r = review("c-01 检查备注：缺少签字", "出现缺少签字时转人工复核").unwrap();
    let c = &r.clauses[0];
    assert_eq!(c.clause_id, "c-01");
    assert_eq!(c.risk_decision, RiskDecision::Risk);
    assert_eq!(c.next_action, NextAction::HumanReview);
}
#[test]
#[ignore = "implement evidence-first refusal"]
fn insufficient_rules_produce_undetermined() {
    let r = review("c-01 保证金比例未知", "规则未说明保证金比例").unwrap();
    let c = &r.clauses[0];
    assert_eq!(c.risk_decision, RiskDecision::Undetermined);
    assert_eq!(c.evidence_status, EvidenceStatus::Insufficient);
    assert_eq!(c.next_action, NextAction::RetrieveMore);
}
#[test]
#[ignore = "exact evidence schema"]
fn evidence_has_stable_locator_and_exact_quote() {
    let r = review("c-01 缺少签字", "R1#1.1 出现缺少签字时转人工复核").unwrap();
    let e = &r.clauses[0].evidence[0];
    assert!(!e.source_id.is_empty());
    assert!(!e.locator.is_empty());
    assert!(!e.quote.is_empty());
}
