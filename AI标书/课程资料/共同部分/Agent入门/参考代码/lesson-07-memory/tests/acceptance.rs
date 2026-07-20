//! Lesson 7 验收测试
//!
//! 8 个 ignored 测试覆盖 Memory 治理的 8 条硬需求：
//! 1. 可信写入    —— ExplicitUser 可写 UserProfile
//! 2. 来源防护    —— Document 不能写 UserProfile
//! 3. 用户隔离    —— 不同 user_id 绝不串线
//! 4. 过期过滤    —— 过期记忆不注入 active_for
//! 5. 冲突检测    —— 相反偏好不静默覆盖
//! 6. 删除        —— delete 后不可见
//! 7. 清除        —— clear 仅影响指定用户
//! 8. 人工升级    —— 冲突/不可逆操作触发升级

use lesson_07_memory::*;

/// 快速构造测试用 MemoryItem。
/// 默认 scope=UserProfile, created_at=0, confidence=1.0（ExplicitUser）。
fn item(id: &str, user: &str, value: &str, source: Source) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        user_id: user.into(),
        key: "style".into(),
        value: value.into(),
        source,
        scope: Scope::UserProfile,
        confidence: if source == Source::ExplicitUser { 1.0 } else { 0.5 },
        created_at: 0,
        expires_at: None,
        supersedes: None,
    }
}

// ---------------------------------------------------------------------------
// Test 1: 可信写入 —— ExplicitUser 记忆可见
// ---------------------------------------------------------------------------

#[test]
#[ignore = "write trusted profiles"]
fn explicit_user_memory_is_visible() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "concise", Source::ExplicitUser))
        .unwrap();
    assert_eq!(s.active_for("a", 0).len(), 1);
}

// ---------------------------------------------------------------------------
// Test 2: 来源防护 —— Document 不能写 UserProfile
// ---------------------------------------------------------------------------

#[test]
#[ignore = "enforce provenance"]
fn document_cannot_write_profile() {
    assert_eq!(
        MemoryStore::default().write_profile(item("1", "a", "x", Source::Document)),
        Err(MemoryError::UntrustedProfileSource)
    );
}

// ---------------------------------------------------------------------------
// Test 3: 用户隔离 —— 不同用户绝不共享记忆
// ---------------------------------------------------------------------------

#[test]
#[ignore = "isolate users"]
fn users_never_share_memory() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "x", Source::ExplicitUser))
        .unwrap();
    assert!(s.active_for("b", 0).is_empty());
}

// ---------------------------------------------------------------------------
// Test 4: 过期过滤 —— 过期记忆不注入
// ---------------------------------------------------------------------------

#[test]
#[ignore = "filter expired memory"]
fn expired_memory_is_not_injected() {
    let mut x = item("1", "a", "x", Source::ExplicitUser);
    x.expires_at = Some(10);
    let mut s = MemoryStore::default();
    s.write_profile(x).unwrap();
    // t=10 时 expires_at=10，条件为 expires_at > now，故不活跃
    assert!(s.active_for("a", 10).is_empty());
}

// ---------------------------------------------------------------------------
// Test 5: 冲突检测 —— 相反偏好不静默覆盖
// ---------------------------------------------------------------------------

#[test]
#[ignore = "implement conflict policy"]
fn opposite_preferences_are_not_silently_overwritten() {
    let a = item("1", "a", "concise", Source::ExplicitUser);
    let b = item("2", "a", "detailed", Source::ExplicitUser);
    assert_eq!(resolve_conflict(&a, &b), ConflictResolution::RequireHuman);
}

// ---------------------------------------------------------------------------
// Test 6: 删除 —— 删除后不可见
// ---------------------------------------------------------------------------

#[test]
#[ignore = "implement deletion"]
fn deleted_memory_is_not_returned() {
    let mut s = MemoryStore::default();
    s.write_profile(item("1", "a", "x", Source::ExplicitUser))
        .unwrap();
    s.delete("a", "1").unwrap();
    assert!(s.active_for("a", 0).is_empty());
}

// ---------------------------------------------------------------------------
// Test 7: 清除 —— clear 仅影响指定用户
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Test 8: 人工升级 —— 冲突或不可逆操作触发升级
// ---------------------------------------------------------------------------

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
