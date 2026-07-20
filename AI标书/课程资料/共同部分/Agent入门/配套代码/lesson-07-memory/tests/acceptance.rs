use lesson_07_memory::*;
fn item(id: &str, user: &str, value: &str, source: Source) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        user_id: user.into(),
        key: "style".into(),
        value: value.into(),
        source,
        scope: Scope::UserProfile,
        created_at: 0,
        expires_at: None,
        supersedes: None,
    }
}
#[test]
#[ignore = "write trusted profiles"]
fn explicit_user_memory_is_visible() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "concise", Source::ExplicitUser))
        .unwrap();
    assert_eq!(s.active_for("a", 0).len(), 1);
}
#[test]
#[ignore = "enforce provenance"]
fn document_cannot_write_profile() {
    assert_eq!(
        MemoryStore::default().write_profile(item("1", "a", "x", Source::Document)),
        Err(MemoryError::UntrustedProfileSource)
    );
}
#[test]
#[ignore = "isolate users"]
fn users_never_share_memory() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "x", Source::ExplicitUser))
        .unwrap();
    assert!(s.active_for("b", 0).is_empty());
}
#[test]
#[ignore = "filter expired memory"]
fn expired_memory_is_not_injected() {
    let mut x = item("1", "a", "x", Source::ExplicitUser);
    x.expires_at = Some(10);
    let mut s = MemoryStore::default();
    s.write_profile(x).unwrap();
    assert!(s.active_for("a", 10).is_empty());
}
#[test]
#[ignore = "implement conflict policy"]
fn opposite_preferences_are_not_silently_overwritten() {
    let a = item("1", "a", "concise", Source::ExplicitUser);
    let b = item("2", "a", "detailed", Source::ExplicitUser);
    assert_eq!(resolve_conflict(&a, &b), ConflictResolution::RequireHuman);
}
#[test]
#[ignore = "implement deletion"]
fn deleted_memory_is_not_returned() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "x", Source::ExplicitUser))
        .unwrap();
    s.delete("a", "1").unwrap();
    assert!(s.active_for("a", 0).is_empty());
}
#[test]
#[ignore = "implement clear"]
fn clear_only_removes_requested_user() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "x", Source::ExplicitUser))
        .unwrap();
    s.write_profile(item("2", "b", "y", Source::ExplicitUser))
        .unwrap();
    assert_eq!(s.clear_user("a").unwrap(), 1);
    assert_eq!(s.list("b").len(), 1);
}
#[test]
#[ignore = "human escalation policy"]
fn conflict_or_irreversible_action_escalates() {
    assert!(matches!(
        escalation_for(true, false, false),
        Escalation::RequestHuman(_)
    ));
    assert!(matches!(
        escalation_for(false, true, false),
        Escalation::RequestHuman(_)
    ));
}
