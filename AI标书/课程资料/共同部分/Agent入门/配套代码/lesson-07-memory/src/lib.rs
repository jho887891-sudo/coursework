#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    ExplicitUser,
    Document,
    System,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Task,
    Session,
    UserProfile,
}
#[derive(Debug, Clone)]
pub struct MemoryItem {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub value: String,
    pub source: Source,
    pub scope: Scope,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub supersedes: Option<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum MemoryError {
    UntrustedProfileSource,
    Conflict,
    NotFound,
    PermissionDenied,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    KeepExisting,
    ReplaceExisting,
    RequireHuman,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Escalation {
    Continue,
    RequestHuman(&'static str),
}
#[derive(Default)]
pub struct MemoryStore {
    items: Vec<MemoryItem>,
}
impl MemoryStore {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn write_profile(&mut self, _item: MemoryItem) -> Result<(), MemoryError> {
        Err(MemoryError::UntrustedProfileSource)
    }
    pub fn active_for(&self, _user_id: &str, _now: u64) -> Vec<&MemoryItem> {
        vec![]
    }
    pub fn list(&self, _user_id: &str) -> Vec<&MemoryItem> {
        vec![]
    }
    pub fn delete(&mut self, _user_id: &str, _id: &str) -> Result<(), MemoryError> {
        Err(MemoryError::NotFound)
    }
    pub fn clear_user(&mut self, _user_id: &str) -> Result<usize, MemoryError> {
        Err(MemoryError::NotFound)
    }
}
pub fn resolve_conflict(_old: &MemoryItem, _new: &MemoryItem) -> ConflictResolution {
    ConflictResolution::RequireHuman
}
pub fn escalation_for(
    _evidence_conflict: bool,
    _irreversible_action: bool,
    _low_confidence: bool,
) -> Escalation {
    Escalation::Continue
}
#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
