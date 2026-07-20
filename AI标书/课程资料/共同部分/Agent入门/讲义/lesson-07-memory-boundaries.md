# 第7课：Memory、Learning 与系统边界

> Memory 不是“把聊天记录写进 JSON”。一条记忆必须回答：它是什么、从哪里来、为什么可信、何时使用、何时删除。

---

## 学习目标

1. 区分工作状态、缓存、用户画像、情节记忆和知识库；
2. 为记忆增加来源、范围、置信度、时间和过期策略；
3. 处理记忆冲突、污染和删除；
4. 通过消融证明 Memory 是否改善任务；
5. 定义 Agent 的人工升级与能力边界。

---

## 本 Lesson 必须实现

工程：`配套代码/lesson-07-memory`。

1. 完成 MemoryItem 的 provenance、scope、TTL 和 supersedes 语义；
2. 实现可信写入、用户隔离、过期过滤、冲突处理和删除；
3. 验证文档内容不能直接写入 User Profile；
4. 实现人工升级规则和用户可见的 list/delete/clear；
5. 比较无 Memory、全量历史、摘要和结构化 Memory。

```powershell
cargo test -p lesson-07-memory --test acceptance -- --ignored
```

CRUD 可以由 AI 补全，但长期写入政策、冲突优先级、删除范围和人工升级条件必须由你决定并接受攻击测试。

---

## 1. 五种容易混淆的东西

| 类型 | 示例 | 是否长期 |
|---|---|---|
| Working State | 当前任务已读过哪些条款 | 否 |
| Cache | query 对应的检索结果 | 可选 |
| User Profile | 用户明确选择“简洁回答” | 是 |
| Episodic Memory | 上一次审核发生了什么 | 是 |
| Knowledge Base | 版本化法规原文 | 是 |

它们的更新条件、权限、生命周期和可信度不同，不应塞进同一个 `Vec<String>`。

---

## 2. 有来源的记忆

```rust
struct MemoryItem {
    id: String,
    kind: MemoryKind,
    content: String,
    source: String,
    created_at: String,
    confidence: f32,
    scope: String,
    expires_at: Option<String>,
    supersedes: Option<String>,
}
```

写入前检查：

- 用户是否明确表达；
- 是否值得长期保存；
- 是否包含敏感信息；
- 是否与旧记忆冲突；
- 使用范围是什么；
- 用户能否查看和删除。

LLM 推测“用户似乎喜欢 Rust”不能自动成为永久偏好。

---

## 3. 摘要不是无损压缩

摘要可能：

- 丢掉数字和否定词；
- 把推测写成事实；
- 打乱 tool call 与 tool result；
- 删除仍然有效的约束；
- 把恶意内容固化进长期上下文。

因此不要简单 `drain(0..half)`。至少保留：

- System 指令；
- 未完成任务约束；
- 工具调用配对；
- 证据引用；
- 摘要来源范围。

摘要后应运行一致性测试。

---

## 4. Memory 不等于 Learning

保存过去轨迹只让系统“拥有记录”。只有当这些轨迹被标注、分析并用于改变未来策略，而且改进经过独立评测，才能声称系统从经验中学习。

本课只实现 Memory。后续深度课程会讨论如何利用 Benchmark 和轨迹改进策略。

---

## 5. Human-in-the-Loop

可靠 Agent 必须知道什么时候停止自动处理：

```rust
enum Decision {
    AutoAccept,
    AutoReject,
    RequestMoreEvidence,
    EscalateToHuman { reason: String },
}
```

适合升级的情况：

- 证据冲突；
- 高风险但置信度低；
- 工具权限不足；
- 超出系统声明范围；
- 需要产生不可逆副作用；
- 数据可能涉及隐私或法律责任。

---

## 6. Worked Example：从“我喜欢简洁回答”到一条长期记忆

用户明确说：

```text
以后回答尽量简洁，先给结论。
```

候选记忆：

```json
{
  "kind":"user_preference",
  "content":"回答尽量简洁并先给结论",
  "source":"user_message:run-12:msg-4",
  "confidence":1.0,
  "scope":"response_style",
  "expires_at":null
}
```

为什么可以保存：用户明确表达、内容不是敏感事实、范围清楚。

不应自动保存：

```text
用户问了三个 Rust 问题 → “用户永远喜欢 Rust”
```

这是模型推测，不是明确偏好。

---

## 7. 冲突处理

旧记忆：

```text
回答风格 = 详细
```

新消息：

```text
以后请简短回答。
```

不要直接覆盖后丢失历史。可以：

```rust
new_memory.supersedes = Some(old_memory.id.clone());
old_memory.status = MemoryStatus::Superseded;
```

读取时只注入当前有效版本；用户仍能查看变更记录。

如果冲突来自检索文档而不是用户消息，不能修改 User Profile。

---

## 8. Memory Store 最小接口

```rust
trait MemoryStore {
    fn put(&mut self, item: MemoryItem) -> Result<(), MemoryError>;
    fn query(&self, query: &MemoryQuery) -> Vec<MemoryItem>;
    fn list(&self, scope: Option<&str>) -> Vec<MemoryItem>;
    fn delete(&mut self, id: &str) -> Result<(), MemoryError>;
    fn clear_user(&mut self, user_id: &str) -> Result<(), MemoryError>;
}
```

读取流程：

```text
按 user_id 隔离
  → 过滤 scope
  → 过滤 expired/superseded
  → 按相关性和置信度排序
  → 限制注入数量和长度
  → 在 Trace 中记录使用了哪些 memory_id
```

这样才能在出错时回答“这条错误结论是否受某条旧记忆影响”。

---

## 9. 摘要的一致性检查

假设原历史：

```text
用户预算不超过 3000 元。
用户明确说不接受凌晨航班。
```

错误摘要：

```text
用户计划旅行，预算大约 3000 元，可考虑凌晨航班。
```

摘要改变了硬约束。可以设计检查：

- 数字是否保留；
- 否定/禁止是否保留；
- 未完成任务是否保留；
- tool call 与 result 是否成对；
- 来源消息范围是否记录。

摘要失败时，宁可保留原消息或减少压缩范围，也不要静默替换。

---

## 10. 跟做实验

### Checkpoint A：分类存储

分别建立 `WorkingStateStore`、`CacheStore`、`UserProfileStore`、`EpisodeStore`。先不做语义搜索。

### Checkpoint B：来源和范围

拒绝没有 source、scope 或 created_at 的长期 MemoryItem。

### Checkpoint C：冲突与删除

写入相反偏好，确认旧记忆进入 superseded；执行 delete 后重新启动，确认不会加载。

### Checkpoint D：污染测试

检索文档包含“用户喜欢详细回答”。确认它不会写入 User Profile。

### Checkpoint E：消融

在相同任务比较无记忆、全历史、摘要和结构化记忆。不能只比较“感觉更自然”。

---

## 11. 常见错误排查

| 现象 | 检查 |
|---|---|
| 删除后又出现 | 是否还有 cache/episode 副本 |
| 多用户记忆串线 | user_id 是否作为强制过滤条件 |
| 旧偏好覆盖新偏好 | supersedes 与有效状态 |
| Prompt 越来越长 | 注入数量、scope、TTL 和长度预算 |
| 文档污染画像 | 写入策略是否校验 source 类型 |

---

## 12. 本课自测

1. Cache 和长期记忆为什么不同？
2. 用户没有明确表达时能否写入永久偏好？
3. 摘要为什么需要来源范围？
4. 保存轨迹为什么不等于学习？
5. 哪些情况应升级人工而不是继续自动推理？

---

## 13. 延伸学习

- working / episodic / semantic / procedural memory；
- TTL、数据删除与用户隔离；
- event log 与 materialized view；
- memory poisoning；
- Human-in-the-Loop、automation bias 和 escalation policy；
- System Card / Model Card 的能力边界表达。

---

## 14. AI 协作：让 AI 写存储 CRUD，你负责记忆治理

**本课起点**：从课程根目录使用 `配套代码/lesson-07-memory`。

可以交给 AI：serde、文件存储 CRUD、过滤器、测试数据和报表。必须由你决定：什么信息可以成为长期记忆、provenance/TTL/scope、冲突与删除语义、何时人工升级，以及 Memory 消融怎样公平。

推荐 Prompt：

```text
请只补全 MemoryStore 的机械实现，保留 provenance、scope、expires_at、supersedes。
先写跨用户隔离、过期不注入、删除后不可恢复、恶意文档不可写画像的测试。
遇到含糊策略请标 TODO，不要自行替产品作决定。
```

**AI 代码审查任务**：验证删除是否覆盖 cache/episode 副本、user_id 是否强制过滤、旧偏好是否可能覆盖新偏好。给出最小复现测试。

---

## 作业：带治理的 Memory Store

### 必做

1. 分开存储 Working State、Cache、User Profile 和 Episode；
2. MemoryItem 包含来源、时间、范围和置信度；
3. 用户可 `list`、`delete`、`clear`；
4. 新旧偏好冲突时不静默覆盖；
5. 过期记忆不注入上下文；
6. 恶意文档不能写入 User Profile；
7. 至少一种人工升级策略；
8. 生成一页 `SYSTEM_CARD.md`。

### Memory 消融实验

比较：

- 无 Memory；
- 全量历史；
- 摘要；
- 结构化 Memory。

统计：

- 任务完成率；
- 用户偏好遵循率；
- 错误记忆率；
- 平均上下文长度；
- 冲突处理正确率。

### System Card

至少说明：

- 系统能做什么、不能做什么；
- 使用什么数据和工具；
- 已知失败模式；
- 什么时候必须人工确认；
- 记忆保存与删除方式；
- 评测覆盖了什么、没覆盖什么。

---

## 验收标准

- [ ] 不同类型状态分开存储；
- [ ] 每条长期记忆有来源；
- [ ] 用户可以删除记忆；
- [ ] 冲突和过期有确定规则；
- [ ] Memory 收益通过同一测试集对比；
- [ ] System Card 没有把辅助系统描述成自动裁决者。

---

## 思考题

> 如果加入 Memory 后用户体验更连贯，但任务准确率下降了，你会保留它吗？还需要检查哪些指标和风险？
