//! Lesson 7 系统不变量测试
//!
//! 这些测试验证 MemoryStore 在故障注入和边界条件下的行为。
//! 与 acceptance.rs 的区别：这里测试系统不变量，不只验收功能。

use lesson_07_memory::*;

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

fn item(
    id: &str,
    user: &str,
    key: &str,
    value: &str,
    source: Source,
    scope: Scope,
) -> MemoryItem {
    MemoryItem {
        id: id.into(),
        user_id: user.into(),
        key: key.into(),
        value: value.into(),
        source,
        scope,
        confidence: if source == Source::ExplicitUser { 1.0 } else { 0.5 },
        created_at: 0,
        expires_at: None,
        supersedes: None,
    }
}

// ---------------------------------------------------------------------------
// 不变量 1：active_for 返回值永远不超过总条目数
// ---------------------------------------------------------------------------

#[test]
fn active_for_never_exceeds_total_items() {
    let mut store = MemoryStore::default();
    for i in 0..10 {
        store
            .write_memory(item(
                &format!("{i}"),
                "alice",
                &format!("key{i}"),
                &format!("value{i}"),
                Source::ExplicitUser,
                Scope::Session,
            ))
            .unwrap();
    }
    assert!(store.active_for("alice", 0).len() <= store.len());
}

// ---------------------------------------------------------------------------
// 不变量 2：删除后 active_for 和 list 结果一致（都不含已删除项）
// ---------------------------------------------------------------------------

#[test]
fn delete_removes_from_both_active_and_list() {
    let mut store = MemoryStore::default();
    store
        .write_profile(item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();

    store.delete("alice", "1").unwrap();

    assert!(store.active_for("alice", 0).is_empty());
    assert!(store.list("alice").is_empty());
}

// ---------------------------------------------------------------------------
// 不变量 3：clear_user 后该用户的 active_for 和 list 均为空
// ---------------------------------------------------------------------------

#[test]
fn clear_user_empties_both_active_and_list() {
    let mut store = MemoryStore::default();
    for i in 0..5 {
        store
            .write_profile(item(
                &format!("{i}"),
                "alice",
                "style",
                "value",
                Source::ExplicitUser,
                Scope::UserProfile,
            ))
            .unwrap();
    }

    store.clear_user("alice").unwrap();

    assert!(store.active_for("alice", 0).is_empty());
    assert!(store.list("alice").is_empty());
}

// ---------------------------------------------------------------------------
// 不变量 4：supersedes 链中只有最新项活跃
// ---------------------------------------------------------------------------

#[test]
fn supersedes_chain_only_latest_is_active() {
    let mut store = MemoryStore::default();

    // 写入 v1
    store
        .write_profile(item(
            "1", "alice", "style", "v1",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();

    // v2 取代 v1
    let mut v2 = item(
        "2", "alice", "style", "v2",
        Source::ExplicitUser, Scope::UserProfile,
    );
    v2.supersedes = Some("1".into());
    store.write_profile(v2).unwrap();

    // v3 取代 v2
    let mut v3 = item(
        "3", "alice", "style", "v3",
        Source::ExplicitUser, Scope::UserProfile,
    );
    v3.supersedes = Some("2".into());
    store.write_profile(v3).unwrap();

    // 只有 v3 活跃
    let active = store.active_for("alice", 0);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].value, "v3");

    // list 可以看到全部三个（审计）
    assert_eq!(store.list("alice").len(), 3);
}

// ---------------------------------------------------------------------------
// 不变量 5：重复删除同一记忆返回 NotFound（幂等）
// ---------------------------------------------------------------------------

#[test]
fn double_delete_is_idempotent() {
    let mut store = MemoryStore::default();
    store
        .write_profile(item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();

    // 第一次删除成功
    assert!(store.delete("alice", "1").is_ok());
    // 第二次删除返回 NotFound
    assert_eq!(store.delete("alice", "1"), Err(MemoryError::NotFound));
}

// ---------------------------------------------------------------------------
// 不变量 6：不同 scope 的同 key 记忆不冲突
// ---------------------------------------------------------------------------

#[test]
fn same_key_different_scope_no_conflict() {
    let mut store = MemoryStore::default();

    // Task scope 的记忆
    store
        .write_memory(item(
            "1", "alice", "status", "reading section 3",
            Source::System, Scope::Task,
        ))
        .unwrap();

    // UserProfile scope 的记忆 —— 不同的 scope，不冲突
    store
        .write_profile(item(
            "2", "alice", "status", "premium user",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();

    // 两个都应该活跃
    assert_eq!(store.active_for("alice", 0).len(), 2);
}

// ---------------------------------------------------------------------------
// 不变量 7：过期边界值测试
// ---------------------------------------------------------------------------

#[test]
fn expiry_boundary_exclusive() {
    let mut store = MemoryStore::default();
    let mut item = item(
        "1", "alice", "temp", "data",
        Source::ExplicitUser, Scope::Session,
    );
    item.expires_at = Some(100);

    store.write_memory(item).unwrap();

    // expires_at > now → 活跃
    assert_eq!(store.active_for("alice", 99).len(), 1);
    // expires_at == now → 不活跃（边界：> 而非 >= ）
    assert!(store.active_for("alice", 100).is_empty());
}

// ---------------------------------------------------------------------------
// 不变量 8：大量记忆场景下用户隔离依然有效
// ---------------------------------------------------------------------------

#[test]
fn user_isolation_under_load() {
    let mut store = MemoryStore::default();

    // 为 100 个用户各写入一条记忆
    for i in 0..100 {
        store
            .write_profile(item(
                &format!("{i}"),
                &format!("user{i}"),
                "data",
                &format!("value{i}"),
                Source::ExplicitUser,
                Scope::UserProfile,
            ))
            .unwrap();
    }

    // 每个用户只能看到自己的一条记忆
    for i in 0..100 {
        let active = store.active_for(&format!("user{i}"), 0);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].value, format!("value{i}"));
    }

    // 不存在的用户
    assert!(store.active_for("nonexistent", 0).is_empty());
}

// ---------------------------------------------------------------------------
// 不变量 9：System 来源可以写 Session 但不能写 UserProfile
// ---------------------------------------------------------------------------

#[test]
fn system_source_boundaries() {
    let mut store = MemoryStore::default();

    // System 可以写 Session
    let session_item = item(
        "1", "alice", "summary", "conversation about Rust",
        Source::System, Scope::Session,
    );
    assert!(store.write_memory(session_item).is_ok());

    // System 不能写 UserProfile
    let profile_item = item(
        "2", "alice", "preference", "likes Rust",
        Source::System, Scope::UserProfile,
    );
    assert_eq!(
        store.write_profile(profile_item),
        Err(MemoryError::UntrustedProfileSource)
    );
}

// ---------------------------------------------------------------------------
// 不变量 10：交叉删除不影响其他用户的记忆
// ---------------------------------------------------------------------------

#[test]
fn cross_user_operations_have_no_side_effects() {
    let mut store = MemoryStore::default();

    // Alice 有两条记忆
    store
        .write_profile(item(
            "a1", "alice", "k1", "v1",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();
    store
        .write_profile(item(
            "a2", "alice", "k2", "v2",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();

    // Bob 有一条记忆
    store
        .write_profile(item(
            "b1", "bob", "k1", "v1",
            Source::ExplicitUser, Scope::UserProfile,
        ))
        .unwrap();

    // Alice 删除自己的一条记忆
    store.delete("alice", "a1").unwrap();

    // Bob 的记忆完全不受影响
    assert_eq!(store.active_for("bob", 0).len(), 1);
    // Alice 还剩一条
    assert_eq!(store.active_for("alice", 0).len(), 1);
    assert_eq!(store.active_for("alice", 0)[0].id, "a2");
}
