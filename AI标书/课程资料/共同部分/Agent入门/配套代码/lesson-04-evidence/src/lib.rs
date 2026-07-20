#[derive(Debug, Clone)]
pub struct Passage {
    pub source_id: String,
    pub locator: String,
    pub text: String,
    pub version: String,
    pub effective_date: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceStatus {
    Supported,
    Partial,
    Insufficient,
    Conflicting,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    pub source_id: String,
    pub locator: String,
    pub quote: String,
}
#[derive(Debug, Clone)]
pub struct EvidenceAnswer {
    pub claim: String,
    pub status: EvidenceStatus,
    pub citations: Vec<Citation>,
    pub limitations: Vec<String>,
}
pub fn tokenize(_text: &str) -> Vec<String> {
    vec![]
}
pub fn search<'a>(_query: &str, _passages: &'a [Passage], _k: usize) -> Vec<&'a Passage> {
    vec![]
}
pub fn verify(_claim: &str, _hits: &[&Passage]) -> EvidenceAnswer {
    EvidenceAnswer {
        claim: _claim.into(),
        status: EvidenceStatus::Insufficient,
        citations: vec![],
        limitations: vec![],
    }
}
pub fn quote_exact(_passage: &Passage, _start: usize, _end: usize) -> Option<Citation> {
    None
}
pub fn document_authorizes_tool(_document_text: &str, _tool_name: &str) -> bool {
    false
}
#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
