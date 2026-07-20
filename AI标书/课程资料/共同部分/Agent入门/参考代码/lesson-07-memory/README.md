# Lesson 7 优秀学生示例：带治理的 Memory Store

这是 Lesson 7 的完整优秀学生示例。它实现了一个带完整治理策略的 MemoryStore：每条长期记忆都必须携带来源（provenance）、范围（scope）、置信度（confidence）和 TTL；恶意文档不能污染用户画像；冲突不静默覆盖；用户可以随时查看和删除自己的数据。

## 它证明了什么

- **写入即治理**：`write_profile` 不是简单的 `push`，而是检查来源、范围和冲突的决策点
- **Document 不能污染 UserProfile**：只有 `ExplicitUser` 来源可以写入用户画像 —— 文档中说"用户喜欢详细回答"不能自动成为记忆
- **用户隔离是硬约束**：所有查询方法以 `user_id` 为第一强制过滤条件，不同用户之间绝不串线
- **过期记忆自动不注入**：`active_for` 过滤 `expires_at`，保证过期数据不影响上下文
- **冲突不静默覆盖**：`resolve_conflict` 对相反偏好返回 `RequireHuman`，通过 `supersedes` 建立非破坏性更新链
- **用户可以掌控自己的数据**：`list` / `delete` / `clear` 全部可用，删除后不可恢复
- **Agent 知道何时升级**：`escalation_for` 在证据冲突、不可逆操作和低置信度时触发人工升级
- **26 个单元测试覆盖所有治理路径**：污染防护、跨用户删除、幂等删除、过期边界、置信度排序

## 运行

在课程的 `示例代码` 目录执行：

```powershell
# 运行单元测试（26 个）
cargo test --offline -p lesson-07-memory --lib

# 运行验收测试（8 个）
cargo test --offline -p lesson-07-memory --test acceptance -- --ignored

# 运行所有测试
cargo test --offline -p lesson-07-memory --all-targets

# 运行系统不变量测试（故障注入）
cargo test --offline -p lesson-07-memory --test memory_invariants
```

## 阅读顺序

1. `src/lib.rs`：类型定义 → MemoryStore 实现 → 冲突解决 → 升级策略 → 单元测试
2. `tests/acceptance.rs`：课程统一验收规则（8 个 ignored 测试）
3. `tests/memory_invariants.rs`：故障注入与系统不变量测试
4. `REPORT.md`：设计决策、治理策略与实验结论
5. 讲义 `lesson-07-memory-boundaries.md`：理论、Worked Example 和消融实验

代码测试通过不等于掌握 Memory 治理。现场应能解释：为什么 Document 不能写 UserProfile、为什么冲突不应静默覆盖、什么条件下必须升级人工、以及怎样验证删除后数据不会从 cache/episode 副本中"复活"。

## 架构概览

### 四通道分离

| 通道 | Scope | 来源限制 | 生命周期 | 示例 |
|------|-------|---------|---------|------|
| Working State | Task | 无限制 | 单次任务 | "已读过第3条" |
| Cache | Session | 无限制 | 单次会话 | query 检索结果 |
| User Profile | UserProfile | 仅 ExplicitUser | 长期 | "回答尽量简洁" |
| Episodic | Session | 无限制 | 单次会话 | "上次审核发生了什么" |

### 写入治理流程

```text
MemoryItem
  → source 是否为 ExplicitUser？（UserProfile 必须）
  → 是否与已有记忆冲突？
    → 是：旧记忆 → superseded，新记忆 → active
    → 否：直接写入
  → 写入 items
```

### 读取过滤流程（active_for）

```text
items
  → user_id 匹配          （防线 1：用户隔离）
  → 未删除                （防线 2：逻辑删除）
  → 未被取代              （防线 3：superseded 不可见）
  → 未过期                （防线 4：TTL 检查）
  → 按 confidence 降序    （防线 5：高置信度优先）
  → 返回引用
```

## 关键设计决策

### 1. 为什么 Document 不能写 UserProfile？

检索文档可能包含"用户偏好详细回答"这类内容，但这是文档作者的观点，不是用户的明确表达。如果允许自动写入，恶意文档可以悄悄修改用户画像。**只有用户明确表达的偏好才写入画像** —— 这是防止 Memory Poisoning 的关键防线。

### 2. 为什么用 supersedes 而不是直接覆盖？

直接覆盖会丢失历史。用户说"以后请简短回答"时：
- 旧偏好"详细"进入 superseded 状态（不可见但可审计）
- 新偏好"简短"成为活跃版本
- `active_for` 只返回活跃版本
- `list` 仍能看到变更历史

### 3. 什么时候升级人工？

- **证据冲突**：两个可信来源给出矛盾结论 → 不能自动裁决
- **不可逆操作**：动作副作用无法撤销 → 必须人工确认
- **低置信度**：Agent 对决策把握不足 → 不应冒险

升级不是失败，而是正确的系统边界行为。

## 常见错误排查

| 现象 | 检查 |
|------|------|
| 删除后又出现 | 是否还有 cache/episode 副本 |
| 多用户记忆串线 | user_id 是否作为强制过滤条件 |
| 旧偏好覆盖新偏好 | supersedes 与活跃状态 |
| Prompt 越来越长 | 注入数量、scope、TTL 和长度预算 |
| 文档污染画像 | 写入策略是否校验 source 类型 |
