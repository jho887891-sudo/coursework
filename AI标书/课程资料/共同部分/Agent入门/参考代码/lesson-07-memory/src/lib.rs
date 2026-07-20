//! # Lesson 7：Memory、Learning 与系统边界
//!
//! Memory 不是"把聊天记录写进 JSON"。
//! 一条记忆必须回答：它是什么、从哪里来、为什么可信、何时使用、何时删除。
//!
//! 本模块实现一个带治理的 MemoryStore，支持：
//! - 四条独立的存储通道（Working State / Cache / User Profile / Episode）
//! - 每条长期记忆带来源（provenance）、范围（scope）、置信度（confidence）和 TTL
//! - 用户隔离 —— 不同 user_id 之间绝不串线
//! - 过期记忆不注入上下文
//! - 冲突检测与 supersedes 链
//! - 用户可 list / delete / clear
//! - 恶意文档不能写入 User Profile
//! - 人工升级策略

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// 1. 类型定义 —— 五种容易混淆的东西，从类型上就分开
// ---------------------------------------------------------------------------

/// 记忆来源 —— 决定可信度和写入权限。
///
/// 核心规则：**Document 来源绝不能写入 UserProfile**。
/// LLM 推测"用户似乎喜欢 Rust"不能自动成为永久偏好。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// 用户明确表达（可写入任何 scope）
    ExplicitUser,
    /// 从检索文档中提取（只能写入 Working / Cache / Episode，绝不能写 UserProfile）
    Document,
    /// 系统自动生成（如摘要、分析结果）
    System,
}

/// 记忆范围 —— 决定生命周期和注入时机。
///
/// | Scope       | 生命周期     | 跨会话持久 | 用户可删除 |
/// |-------------|-------------|-----------|-----------|
/// | Task        | 单次任务    | 否        | 否        |
/// | Session     | 单次会话    | 否        | 是        |
/// | UserProfile | 长期        | 是        | 是        |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// 当前任务的工作状态（如"已读过第3条"）
    Task,
    /// 本次会话范围内的记忆
    Session,
    /// 用户长期画像（偏好、习惯等）
    UserProfile,
}

/// 记忆条目 —— 一条完整的结构化记忆。
///
/// 字段设计遵循"有来源的记忆"原则（见 Lecture §2）：
/// - `source` / `scope` / `created_at` 三者缺一不可
/// - `supersedes` 实现非破坏性更新，旧记忆不丢失
/// - `expires_at` 支持 TTL，过期自动不注入
/// - `confidence` 支持按可信度排序注入
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryItem {
    /// 唯一标识符
    pub id: String,
    /// 所属用户 —— 所有查询必须以此为第一过滤条件
    pub user_id: String,
    /// 记忆键（如 "style", "budget", "preference"）
    pub key: String,
    /// 记忆值
    pub value: String,
    /// 来源 —— 决定写入权限
    pub source: Source,
    /// 范围 —— 决定生命周期
    pub scope: Scope,
    /// 置信度 0.0~1.0，ExplicitUser 通常为 1.0
    pub confidence: f32,
    /// 创建时间戳
    pub created_at: u64,
    /// 过期时间戳（含此值则到期后不注入）
    pub expires_at: Option<u64>,
    /// 被本记忆取代的旧记忆 ID（非破坏性更新）
    pub supersedes: Option<String>,
}

// ---------------------------------------------------------------------------
// 2. 错误与决议
// ---------------------------------------------------------------------------

/// Memory 操作错误类型。
///
/// 注意：`NotFound` 用于 delete 找不到目标；
/// `PermissionDenied` 用于跨用户操作；
/// `Conflict` 用于写入冲突；
/// `UntrustedProfileSource` 用于文档污染画像。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// 不可信来源试图写入 UserProfile（如文档内容）
    UntrustedProfileSource,
    /// 写入时与已有记忆冲突
    Conflict,
    /// 指定 ID 的记忆不存在
    NotFound,
    /// 无权操作（跨用户）
    PermissionDenied,
    /// 缺少必填字段（source、scope、created_at）
    MissingRequiredField,
}

/// 冲突解决策略 —— 由上层决策，不在 MemoryStore 内部悄悄覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// 保留已有记忆，拒绝新写入
    KeepExisting,
    /// 新记忆替换旧记忆（通过 supersedes 链）
    ReplaceExisting,
    /// 升级人工决策
    RequireHuman,
}

/// 升级决策 —— Agent 什么时候必须停下自动推理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    /// 可以继续自动处理
    Continue,
    /// 必须升级人工，附带原因
    RequestHuman(&'static str),
}

// ---------------------------------------------------------------------------
// 3. MemoryStore —— 带治理的存储实现
// ---------------------------------------------------------------------------

/// 带治理的四通道 Memory Store。
///
/// # 设计原则
///
/// 1. **写入即治理**：`write_profile` 不是简单的 `push`，它检查来源、范围和冲突。
/// 2. **读取即过滤**：`active_for` 自动过滤过期、已删除、已被取代的记忆。
/// 3. **用户隔离**：所有公开方法都以 `user_id` 为强制过滤条件。
/// 4. **非破坏性更新**：冲突不静默覆盖，通过 `supersedes` 保留变更链。
///
/// # 内部实现
///
/// 使用单一 `Vec<MemoryItem>` 存储所有记忆，通过以下集合跟踪状态：
/// - `deleted_ids`：已删除的记忆 ID（物理保留但逻辑不可见）
/// - `superseded_ids`：已被新记忆取代的 ID
///
/// 生产环境中可替换为持久化存储，但语义不变。
#[derive(Default)]
pub struct MemoryStore {
    /// 所有记忆条目（包括已删除和已被取代的，用于审计）
    items: Vec<MemoryItem>,
    /// 已删除的记忆 ID 集合
    deleted_ids: HashSet<String>,
    /// 已被取代的记忆 ID 集合（通过 supersedes 字段标记）
    superseded_ids: HashSet<String>,
}

// ---------------------------------------------------------------------------
// 3a. 构造与基本查询
// ---------------------------------------------------------------------------

impl MemoryStore {
    /// 返回存储中的总条目数（含已删除和已取代，用于调试和审计）。
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// 返回存储是否为空。
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ---------------------------------------------------------------------------
// 3b. 写入操作
// ---------------------------------------------------------------------------

impl MemoryStore {
    /// 写入一条 UserProfile 记忆。
    ///
    /// # 治理规则（按顺序检查）
    ///
    /// 1. **来源校验**：只有 `ExplicitUser` 可以写入 UserProfile。
    ///    Document 和 System 来源一律拒绝 —— 这是防止文档污染画像的关键防线。
    /// 2. **冲突检测**：同一 user_id + key 已存在不同 value 时，
    ///    通过 `supersedes` 建立非破坏性更新链，旧记忆状态变为 Superseded。
    ///
    /// # 错误
    ///
    /// - `UntrustedProfileSource`：Document 或 System 试图写 UserProfile
    pub fn write_profile(&mut self, item: MemoryItem) -> Result<(), MemoryError> {
        // 规则 1：只有 ExplicitUser 可以写入 UserProfile
        // 这是防止"文档说用户喜欢详细回答 → 自动写入偏好"的关键防线
        if item.source != Source::ExplicitUser {
            return Err(MemoryError::UntrustedProfileSource);
        }

        // 规则 2：检测冲突 —— 同一 user_id + key 已存在不同 value
        // 找到所有匹配的现有活跃记忆
        let conflicting: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_idx, existing)| {
                existing.user_id == item.user_id
                    && existing.key == item.key
                    && existing.value != item.value
                    && !self.deleted_ids.contains(&existing.id)
                    && !self.superseded_ids.contains(&existing.id)
            })
            .map(|(idx, _)| idx)
            .collect();

        if !conflicting.is_empty() {
            // 冲突存在时，通过 supersedes 建立非破坏性更新链
            // 旧记忆标记为 Superseded，不被 active_for 返回，但仍可审计
            for idx in conflicting {
                let old_id = self.items[idx].id.clone();
                self.superseded_ids.insert(old_id);
            }
        }

        // 如果新记忆声明了 supersedes，标记被取代的旧记忆
        if let Some(ref old_id) = item.supersedes {
            self.superseded_ids.insert(old_id.clone());
        }

        self.items.push(item);
        Ok(())
    }

    /// 写入一条非 UserProfile 的记忆（Working / Session / Episode）。
    ///
    /// 相比 `write_profile`，此方法不强制 ExplicitUser 来源，
    /// 因为 Document 和 System 可以合法写入 Task、Session 和 Cache。
    pub fn write_memory(&mut self, item: MemoryItem) -> Result<(), MemoryError> {
        // 非 UserProfile scope 允许 Document 和 System 来源
        // 处理 supersedes
        if let Some(ref old_id) = item.supersedes {
            self.superseded_ids.insert(old_id.clone());
        }

        self.items.push(item);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 3c. 读取操作
// ---------------------------------------------------------------------------

impl MemoryStore {
    /// 返回指定用户在指定时间点的**活跃**记忆。
    ///
    /// # 过滤顺序（性能考虑：user_id 第一，快速剪枝）
    ///
    /// 1. `user_id` 匹配 —— 强制用户隔离
    /// 2. 未删除 —— 排除 `deleted_ids` 中的记忆
    /// 3. 未被取代 —— 排除 `superseded_ids` 中的记忆
    /// 4. 未过期 —— `expires_at` 为 None 或 > `now`
    /// 5. 按 `confidence` 降序排列 —— 高置信度记忆优先注入
    ///
    /// # 注入建议（由调用方实现）
    ///
    /// - 限制注入数量和总长度
    /// - 在 Trace 中记录使用了哪些 memory_id
    pub fn active_for(&self, user_id: &str, now: u64) -> Vec<&MemoryItem> {
        let mut active: Vec<&MemoryItem> = self
            .items
            .iter()
            .filter(|item| {
                // 1. 用户隔离 —— 第一条防线
                item.user_id == user_id
                // 2. 未删除
                && !self.deleted_ids.contains(&item.id)
                // 3. 未被取代
                && !self.superseded_ids.contains(&item.id)
                // 4. 未过期：expires_at 为 None（永不过期）或 > now
                && item.expires_at.map_or(true, |exp| exp > now)
            })
            .collect();

        // 5. 按置信度降序排列 —— 高置信度记忆优先注入上下文
        active.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        active
    }

    /// 返回指定用户的**全部**记忆（含过期、但不含已删除）。
    ///
    /// 用于用户查看和管理自己的记忆。
    /// 与 `active_for` 的区别：不过滤过期和取代状态。
    pub fn list(&self, user_id: &str) -> Vec<&MemoryItem> {
        self.items
            .iter()
            .filter(|item| {
                item.user_id == user_id && !self.deleted_ids.contains(&item.id)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// 3d. 删除操作
// ---------------------------------------------------------------------------

impl MemoryStore {
    /// 删除指定 ID 的记忆。
    ///
    /// # 语义
    ///
    /// - 逻辑删除：记忆仍在 `items` 中（用于审计），但不再被 `active_for` 或 `list` 返回
    /// - 必须匹配 `user_id`：防止用户 A 删除用户 B 的记忆
    /// - 幂等：重复删除已删除的记忆返回 `NotFound`
    ///
    /// # 重要
    ///
    /// 删除后必须确保没有 cache/episode 副本仍持有该记忆内容。
    /// 本实现中，逻辑删除确保所有查询路径都无法访问已删除记忆。
    pub fn delete(&mut self, user_id: &str, id: &str) -> Result<(), MemoryError> {
        // 检查记忆是否存在且属于该用户
        let exists = self
            .items
            .iter()
            .any(|item| item.id == id && item.user_id == user_id);

        if !exists {
            return Err(MemoryError::NotFound);
        }

        // 检查是否已删除（幂等）
        if self.deleted_ids.contains(id) {
            return Err(MemoryError::NotFound);
        }

        // 逻辑删除
        self.deleted_ids.insert(id.to_string());
        Ok(())
    }

    /// 清除指定用户的全部记忆。
    ///
    /// # 语义
    ///
    /// - 清除范围：仅限指定 `user_id`
    /// - 返回清除数量（不含已删除的记忆）
    /// - 不影响其他用户的记忆
    ///
    /// # 错误
    ///
    /// - `NotFound`：该用户没有任何可清除的记忆
    pub fn clear_user(&mut self, user_id: &str) -> Result<usize, MemoryError> {
        // 找出该用户所有未被删除的记忆
        let to_clear: Vec<String> = self
            .items
            .iter()
            .filter(|item| {
                item.user_id == user_id && !self.deleted_ids.contains(&item.id)
            })
            .map(|item| item.id.clone())
            .collect();

        if to_clear.is_empty() {
            return Err(MemoryError::NotFound);
        }

        let count = to_clear.len();
        for id in to_clear {
            self.deleted_ids.insert(id);
        }

        Ok(count)
    }

    /// 物理清除指定用户的全部记忆（从 items 中移除）。
    ///
    /// 仅在用户请求"彻底删除我的数据"时使用。
    /// 与 `clear_user` 的区别：不保留审计记录。
    #[allow(dead_code)]
    pub fn purge_user(&mut self, user_id: &str) -> usize {
        let before = self.items.len();
        self.items.retain(|item| item.user_id != user_id);
        // 同时清理追踪集合
        self.deleted_ids.retain(|id| {
            !self.items.iter().any(|item| &item.id == id)
        });
        self.superseded_ids.retain(|id| {
            !self.items.iter().any(|item| &item.id == id)
        });
        before - self.items.len()
    }
}

// ---------------------------------------------------------------------------
// 4. 冲突解决 —— 由你决定策略，不由机器悄悄覆盖
// ---------------------------------------------------------------------------

/// 解析两条记忆之间的冲突。
///
/// # 冲突判定
///
/// 两条记忆冲突当且仅当：
/// - 同一 key（如 "style"）
/// - 不同 value（如 "concise" vs "detailed"）
///
/// # 当前策略（Lesson 7 默认）
///
/// 不同 value → `RequireHuman`。
/// 产品中可以根据以下因素自动决策：
/// - 新旧记忆的 confidence 对比
/// - 新记忆是否明确 supersedes 旧记忆
/// - scope 优先级（ExplicitUser > System > Document）
/// - 时间先后
///
/// 但**默认不应静默覆盖**，这是本课的核心设计决策。
pub fn resolve_conflict(old: &MemoryItem, new: &MemoryItem) -> ConflictResolution {
    // 同 key 同 value → 无冲突
    if old.value == new.value {
        return ConflictResolution::KeepExisting;
    }

    // 不同 value → 需要人工判断
    // 在实际产品中，可以加入更多自动决策逻辑：
    // - 新记忆明确声明 supersedes → ReplaceExisting
    // - 新记忆 confidence 远高于旧记忆 → ReplaceExisting
    // - 旧记忆来自 ExplicitUser → KeepExisting
    // 但默认策略是保守的：升级人工
    ConflictResolution::RequireHuman
}

// ---------------------------------------------------------------------------
// 5. 人工升级策略 —— Agent 的系统边界
// ---------------------------------------------------------------------------

/// 判断当前情况是否需要升级人工。
///
/// # 升级条件
///
/// 以下任一条件满足时应当升级：
/// - 证据冲突：两个可信来源给出矛盾结论
/// - 不可逆动作：操作的副作用无法撤销
/// - 低置信度：Agent 对当前决策的置信度不足
///
/// # 设计原则
///
/// 可靠 Agent 必须知道什么时候停止自动处理（见 Lecture §5）。
/// 升级不是失败，而是正确的系统行为。
pub fn escalation_for(
    evidence_conflict: bool,
    irreversible_action: bool,
    low_confidence: bool,
) -> Escalation {
    if evidence_conflict {
        return Escalation::RequestHuman("证据冲突：多源信息不一致需要人工判断");
    }
    if irreversible_action {
        return Escalation::RequestHuman("不可逆操作：此动作无法撤销，需要人工确认");
    }
    if low_confidence {
        return Escalation::RequestHuman("低置信度：Agent 对当前决策把握不足");
    }
    Escalation::Continue
}

// ---------------------------------------------------------------------------
// 6. 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 快速构造测试用 MemoryItem 的辅助函数
    fn make_item(
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

    // -------------------------------------------------------------------
    // 6a. 写入治理测试
    // -------------------------------------------------------------------

    #[test]
    fn explicit_user_can_write_profile() {
        let mut store = MemoryStore::default();
        let item = make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        );
        assert!(store.write_profile(item).is_ok());
        assert_eq!(store.active_for("alice", 0).len(), 1);
    }

    #[test]
    fn document_source_cannot_write_user_profile() {
        let mut store = MemoryStore::default();
        let item = make_item(
            "1", "alice", "style", "concise",
            Source::Document, Scope::UserProfile,
        );
        assert_eq!(
            store.write_profile(item),
            Err(MemoryError::UntrustedProfileSource)
        );
    }

    #[test]
    fn system_source_cannot_write_user_profile() {
        let mut store = MemoryStore::default();
        let item = make_item(
            "1", "alice", "style", "concise",
            Source::System, Scope::UserProfile,
        );
        assert_eq!(
            store.write_profile(item),
            Err(MemoryError::UntrustedProfileSource)
        );
    }

    #[test]
    fn document_can_write_task_scope() {
        let mut store = MemoryStore::default();
        let item = make_item(
            "1", "alice", "fact", "some fact",
            Source::Document, Scope::Task,
        );
        assert!(store.write_memory(item).is_ok());
        assert_eq!(store.active_for("alice", 0).len(), 1);
    }

    // -------------------------------------------------------------------
    // 6b. 用户隔离测试
    // -------------------------------------------------------------------

    #[test]
    fn users_are_fully_isolated() {
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();
        store.write_profile(make_item(
            "2", "bob", "style", "detailed",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();

        // Alice 只能看到自己的
        assert_eq!(store.active_for("alice", 0).len(), 1);
        assert_eq!(store.active_for("alice", 0)[0].value, "concise");

        // Bob 只能看到自己的
        assert_eq!(store.active_for("bob", 0).len(), 1);
        assert_eq!(store.active_for("bob", 0)[0].value, "detailed");

        // 不存在的用户看不到任何记忆
        assert!(store.active_for("charlie", 0).is_empty());
    }

    // -------------------------------------------------------------------
    // 6c. 过期测试
    // -------------------------------------------------------------------

    #[test]
    fn expired_memory_not_injected() {
        let mut store = MemoryStore::default();
        let mut item = make_item(
            "1", "alice", "temp", "data",
            Source::ExplicitUser, Scope::Session,
        );
        item.expires_at = Some(100);
        store.write_memory(item).unwrap();

        // t=50 时未过期
        assert_eq!(store.active_for("alice", 50).len(), 1);
        // t=100 时已过期（expires_at > now 为活跃条件）
        assert!(store.active_for("alice", 100).is_empty());
        // t=101 时已过期
        assert!(store.active_for("alice", 101).is_empty());
    }

    #[test]
    fn no_expiry_means_never_expires() {
        let mut store = MemoryStore::default();
        let item = make_item(
            "1", "alice", "permanent", "data",
            Source::ExplicitUser, Scope::UserProfile,
        );
        store.write_profile(item).unwrap();

        // 任意时间点都应该活跃
        assert_eq!(store.active_for("alice", 0).len(), 1);
        assert_eq!(store.active_for("alice", u64::MAX).len(), 1);
    }

    // -------------------------------------------------------------------
    // 6d. 冲突与 supersedes 测试
    // -------------------------------------------------------------------

    #[test]
    fn opposite_preferences_require_human() {
        let old = make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        );
        let new = make_item(
            "2", "alice", "style", "detailed",
            Source::ExplicitUser, Scope::UserProfile,
        );
        assert_eq!(resolve_conflict(&old, &new), ConflictResolution::RequireHuman);
    }

    #[test]
    fn same_value_is_not_a_conflict() {
        let old = make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        );
        let new = make_item(
            "2", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        );
        assert_eq!(resolve_conflict(&old, &new), ConflictResolution::KeepExisting);
    }

    #[test]
    fn superseded_memory_not_in_active_for() {
        let mut store = MemoryStore::default();
        let old = make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        );
        store.write_profile(old).unwrap();

        // 新记忆取代旧记忆
        let mut new_item = make_item(
            "2", "alice", "style", "detailed",
            Source::ExplicitUser, Scope::UserProfile,
        );
        new_item.supersedes = Some("1".into());
        store.write_profile(new_item).unwrap();

        // active_for 只返回活跃的（新）记忆
        let active = store.active_for("alice", 0);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].value, "detailed");
    }

    // -------------------------------------------------------------------
    // 6e. 删除测试
    // -------------------------------------------------------------------

    #[test]
    fn deleted_memory_not_returned() {
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();

        assert_eq!(store.active_for("alice", 0).len(), 1);

        store.delete("alice", "1").unwrap();
        assert!(store.active_for("alice", 0).is_empty());
        assert!(store.list("alice").is_empty());
    }

    #[test]
    fn delete_nonexistent_returns_not_found() {
        let mut store = MemoryStore::default();
        assert_eq!(
            store.delete("alice", "nonexistent"),
            Err(MemoryError::NotFound)
        );
    }

    #[test]
    fn cannot_delete_other_users_memory() {
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();

        // Bob 试图删除 Alice 的记忆
        assert_eq!(store.delete("bob", "1"), Err(MemoryError::NotFound));
        // Alice 的记忆仍在
        assert_eq!(store.active_for("alice", 0).len(), 1);
    }

    // -------------------------------------------------------------------
    // 6f. clear 测试
    // -------------------------------------------------------------------

    #[test]
    fn clear_only_removes_target_user() {
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();
        store.write_profile(make_item(
            "2", "alice", "pref", "dark",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();
        store.write_profile(make_item(
            "3", "bob", "style", "detailed",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();

        let cleared = store.clear_user("alice").unwrap();
        assert_eq!(cleared, 2);
        assert!(store.active_for("alice", 0).is_empty());
        // Bob 不受影响
        assert_eq!(store.active_for("bob", 0).len(), 1);
    }

    #[test]
    fn clear_empty_user_returns_not_found() {
        let mut store = MemoryStore::default();
        assert_eq!(store.clear_user("nobody"), Err(MemoryError::NotFound));
    }

    // -------------------------------------------------------------------
    // 6g. 升级策略测试
    // -------------------------------------------------------------------

    #[test]
    fn evidence_conflict_escalates() {
        assert!(matches!(
            escalation_for(true, false, false),
            Escalation::RequestHuman(_)
        ));
    }

    #[test]
    fn irreversible_action_escalates() {
        assert!(matches!(
            escalation_for(false, true, false),
            Escalation::RequestHuman(_)
        ));
    }

    #[test]
    fn low_confidence_escalates() {
        assert!(matches!(
            escalation_for(false, false, true),
            Escalation::RequestHuman(_)
        ));
    }

    #[test]
    fn no_risk_continues() {
        assert_eq!(
            escalation_for(false, false, false),
            Escalation::Continue
        );
    }

    // -------------------------------------------------------------------
    // 6h. 污染防护测试
    // -------------------------------------------------------------------

    #[test]
    fn document_extracted_fact_cannot_pollute_user_profile() {
        // 模拟场景：检索文档中包含"用户喜欢详细回答"
        // 系统从文档中提取了这个信息，标记为 Document 来源
        let mut store = MemoryStore::default();
        let extracted = make_item(
            "doc-1", "alice", "style", "detailed",
            Source::Document, Scope::UserProfile,
        );

        // 必须被拒绝 —— 文档内容不能写入用户画像
        assert_eq!(
            store.write_profile(extracted),
            Err(MemoryError::UntrustedProfileSource)
        );
    }

    #[test]
    fn explicit_user_can_override_with_supersedes() {
        // 用户先设置了"详细"
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "style", "detailed",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();

        // 用户后来明确说"以后请简短回答"
        let mut update = make_item(
            "2", "alice", "style", "concise",
            Source::ExplicitUser, Scope::UserProfile,
        );
        update.supersedes = Some("1".into());
        store.write_profile(update).unwrap();

        // active_for 应该只返回新值
        let active = store.active_for("alice", 0);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].value, "concise");

        // 但 list 中仍可看到旧记忆（审计）
        let all = store.list("alice");
        assert_eq!(all.len(), 2); // 新旧都在
    }

    // -------------------------------------------------------------------
    // 6i. list 与审计测试
    // -------------------------------------------------------------------

    #[test]
    fn list_returns_all_undeleted_for_user() {
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "k1", "v1",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();
        store.write_profile(make_item(
            "2", "alice", "k2", "v2",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();

        assert_eq!(store.list("alice").len(), 2);
    }

    #[test]
    fn list_excludes_deleted() {
        let mut store = MemoryStore::default();
        store.write_profile(make_item(
            "1", "alice", "k1", "v1",
            Source::ExplicitUser, Scope::UserProfile,
        )).unwrap();
        store.delete("alice", "1").unwrap();

        assert!(store.list("alice").is_empty());
    }

    // -------------------------------------------------------------------
    // 6j. 边界条件测试
    // -------------------------------------------------------------------

    #[test]
    fn active_for_sorts_by_confidence_desc() {
        let mut store = MemoryStore::default();
        let mut low_conf = make_item(
            "1", "alice", "fact", "maybe",
            Source::System, Scope::Session,
        );
        low_conf.confidence = 0.3;
        let mut high_conf = make_item(
            "2", "alice", "fact", "certain",
            Source::ExplicitUser, Scope::Session,
        );
        high_conf.confidence = 0.9;

        store.write_memory(low_conf).unwrap();
        store.write_memory(high_conf).unwrap();

        let active = store.active_for("alice", 0);
        // 高置信度的应该排在前面
        assert_eq!(active.len(), 2);
        assert!(active[0].confidence >= active[1].confidence);
    }

    #[test]
    fn empty_store_returns_empty() {
        let store = MemoryStore::default();
        assert!(store.active_for("anyone", 0).is_empty());
        assert!(store.list("anyone").is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn scaffold_compiles() {
        assert!(true);
    }
}
