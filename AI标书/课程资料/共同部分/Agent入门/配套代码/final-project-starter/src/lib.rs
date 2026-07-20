#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskDecision {
    Risk,
    NoRisk,
    Undetermined,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStatus {
    Supported,
    Partial,
    Insufficient,
    Conflicting,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStrength {
    Strong,
    Medium,
    Weak,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Low,
    Medium,
    High,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextAction {
    Complete,
    RetrieveMore,
    HumanReview,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceItem {
    pub source_id: String,
    pub locator: String,
    pub quote: String,
}
#[derive(Debug, Clone)]
pub struct ClauseReport {
    pub clause_id: String,
    pub clause_text: String,
    pub risk_decision: RiskDecision,
    pub evidence_status: EvidenceStatus,
    pub evidence_strength: EvidenceStrength,
    pub risk_type: String,
    pub severity: Severity,
    pub claim: String,
    pub evidence: Vec<EvidenceItem>,
    pub reasoning_summary: String,
    pub confidence_basis: Vec<String>,
    pub limitations: Vec<String>,
    pub next_action: NextAction,
}
#[derive(Debug, Clone)]
pub struct ReviewReport {
    pub document_id: String,
    pub clauses: Vec<ClauseReport>,
    pub trace_path: String,
}
#[derive(Debug, PartialEq, Eq)]
pub enum ReviewError {
    NotImplemented,
    InputError,
    RuntimeError,
}
pub fn review(_bid_text: &str, _rules_text: &str) -> Result<ReviewReport, ReviewError> {
    Err(ReviewError::NotImplemented)
}
#[cfg(test)]
mod tests {
    #[test]
    fn report_schema_compiles() {
        assert!(true);
    }
}
