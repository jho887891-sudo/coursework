# 第3课：Tool Use — 契约、失败恢复与权限

> Tool Use 的难点不是让模型输出函数名，而是把不可信建议变成受约束、可恢复、可审计的外部操作。

---

## 学习目标

1. 设计类型化 Tool 契约；
2. 实现参数校验、超时、有限重试和错误观察；
3. 理解幂等性、副作用和权限边界；
4. 使用故障注入验证 Agent 的恢复能力；
5. 衡量无效调用率和恢复率。

---

## 本 Lesson 必须实现

工程：`配套代码/lesson-03-tools`，数据：`公开数据/lesson-03-fixtures/`。

1. 完成 `Tool` 契约和 `Registry::execute` 精确分发；
2. 实现参数校验、side effect 权限检查和错误分类；
3. 只对明确的 `Transient` 错误进行有限重试；
4. 实现受 workspace 限制的读取工具、可注入失败的工具和非幂等 CounterTool；
5. 测试未知工具、路径穿越、timeout、成功但响应丢失和重复副作用。

```powershell
cargo test -p lesson-03-tools --test acceptance -- --ignored
```

实现重点不是调用函数，而是证明不可信动作建议不能越过程序权限边界。

---

## 1. Tool 是环境动作，不是普通函数

一个 Agent Tool 至少要说明：

| 字段 | 作用 |
|---|---|
| name | 稳定的动作标识 |
| description | 什么时候使用、什么时候不要使用 |
| input_schema | 参数类型、必填项和约束 |
| output_schema | 成功结果结构 |
| error_schema | 可恢复与不可恢复错误 |
| side_effect | 是否修改外部状态 |
| permission | 执行需要什么权限 |
| timeout | 最长执行时间 |

推荐接口：

```rust
#[async_trait::async_trait]
trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    async fn execute(&self, input: serde_json::Value)
        -> Result<serde_json::Value, ToolError>;
}
```

Runtime 负责注册、查找、校验和授权，Tool 负责执行自己的动作。

---

## 2. 错误也是 Observation

工具失败时不要 panic，也不要伪造成功结果。把错误转换成结构化观察：

```json
{
  "status": "error",
  "kind": "timeout",
  "retryable": true,
  "message": "search exceeded 800ms",
  "attempt": 1
}
```

模型只有看到真实错误，才可能改参数、换工具、向用户求助或结束。

但是否重试不能完全交给模型。Runtime 必须限制：

- 最大尝试次数；
- 总时间；
- 哪些错误允许重试；
- 是否需要退避；
- 重试是否安全。

---

## 3. 幂等性与副作用

`search_document` 重复调用通常只浪费成本；`send_email`、`pay`、`write_file` 重复调用可能造成真实损害。

因此每个有副作用的动作都应考虑：

- idempotency key；
- preview / dry-run；
- user confirmation；
- 去重记录；
- compensation action；
- 最小权限。

本课不实现付款等高风险动作，但要通过模拟工具理解语义。

---

## 4. 权限不写在 Prompt 里

模型说“我需要读 `C:\Users\...\.env`”不代表 Runtime 应该执行。

文件工具至少要：

1. 把用户输入路径规范化；
2. 解析为绝对路径；
3. 检查最终路径仍在允许根目录内；
4. 限制文件类型和大小；
5. 拒绝符号链接或路径穿越造成的越界；
6. 在 Trace 中记录授权结果。

权限由宿主程序执行，不能依赖“请模型不要访问敏感文件”。

---

## 5. 故障注入工具

为测试准备一个可配置 Tool：

```rust
enum FaultMode {
    None,
    TimeoutOnce,
    AlwaysTimeout,
    MalformedOutput,
    PermissionDenied,
    SucceedAfter(u8),
}
```

确定性故障比等待真实网络偶然失败更适合自动化测试。

---

## 6. Worked Example：一个可重试的搜索工具

### ToolSpec

```rust
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    input_schema: serde_json::Value,
    side_effect: SideEffect,
    timeout_ms: u64,
}

enum SideEffect {
    None,
    Reversible,
    Irreversible,
}
```

`search_text` 没有外部副作用，可以在临时超时时有限重试：

```rust
fn spec(&self) -> ToolSpec {
    ToolSpec {
        name: "search_text",
        description: "在允许目录的文本文件中搜索关键词；不要用于读取目录外文件",
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type":"string", "minLength":1}
            },
            "required": ["query"]
        }),
        side_effect: SideEffect::None,
        timeout_ms: 800,
    }
}
```

### 执行包装器

```rust
async fn execute_with_policy(
    tool: &dyn Tool,
    args: Value,
    policy: RetryPolicy,
) -> ToolObservation {
    for attempt in 1..=policy.max_attempts {
        let result = tokio::time::timeout(
            Duration::from_millis(tool.spec().timeout_ms),
            tool.execute(args.clone()),
        ).await;

        match result {
            Ok(Ok(value)) => return ToolObservation::Success { value, attempt },
            Ok(Err(err)) if !err.retryable => {
                return ToolObservation::Failure { error: err, attempt };
            }
            Ok(Err(err)) if attempt == policy.max_attempts => {
                return ToolObservation::Failure { error: err, attempt };
            }
            Err(_) if attempt == policy.max_attempts => {
                return ToolObservation::Timeout { attempt };
            }
            _ => continue,
        }
    }
    unreachable!()
}
```

这里的重要点是：重试由 Runtime policy 控制，而不是 Tool 自己无限重试。

---

## 7. Worked Example：为什么副作用工具更难

假设 `CounterTool` 收到请求后已经把计数从 0 改成 1，但响应在返回途中超时。

Runtime 看到的是：

```text
timeout
```

真实环境却已经改变。如果直接重试，计数会变成 2。

解决思路：

```rust
struct CounterArgs {
    amount: i32,
    idempotency_key: String,
}
```

Tool 保存已经处理过的 key：

```text
第一次 key=run-1-step-3：执行并保存结果
第二次 key=run-1-step-3：返回第一次结果，不重复执行
```

这说明“请求失败”和“动作没有发生”不是同一件事。

---

## 8. 跟做实验：实现 Tool Registry

### Checkpoint A：注册与发现

实现：

```rust
struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}
```

测试重复注册、未知工具和 `definitions()` 输出。

### Checkpoint B：参数校验

在调用 Tool 前校验 `query` 必填且非空。故意传入数字、空字符串和缺字段。

### Checkpoint C：超时

让 `FaultyTool::TimeoutOnce` 第一次睡眠超时、第二次立即成功。确认 Trace 有两次 attempt，最终只产生一个成功 Observation。

### Checkpoint D：权限

实现 `resolve_inside(root, requested)`。测试：

```text
rules/a.txt             允许
rules/../rules/a.txt    规范化后仍在 root，可按策略允许
../.env                 拒绝
绝对路径到 root 外      拒绝
```

### Checkpoint E：幂等

同一个 key 调用 CounterTool 两次，环境计数只增加一次。

---

## 9. 常见错误排查

| 现象 | 优先检查 |
|---|---|
| 工具一直重试 | retryable 与 max_attempts |
| timeout 后副作用重复 | idempotency key 是否稳定 |
| `../` 被放行 | 是否在规范化后检查最终绝对路径 |
| 模型换个工具名就绕过限制 | Registry 是否只允许 allowlist |
| Tool Error 变成模型幻觉 | 是否把结构化错误作为 Observation 回填 |

---

## 10. 本课自测

1. 参数合法和权限允许有什么区别？
2. timeout 能否证明工具没有执行？
3. 哪类错误不应重试？
4. 为什么 read 工具也需要输出大小限制？
5. Tool description 能否授予权限？

---

## 11. 延伸学习

- Tokio `timeout` 与 cancellation；
- HTTP 幂等方法和 idempotency key；
- capability-based security；
- 路径规范化、符号链接和 workspace sandbox；
- circuit breaker、backoff 与 retry storm。

---

## 12. 本课 AI 协作方式

### 本课起点

从课程根目录打开 `配套代码/lesson-03-tools`。其中只提供 Tool trait、空 Registry、错误类型和待解锁验收测试；Registry 分发、ReadWorkspaceFile、FaultyTool、CounterTool、权限、重试与幂等策略均由你实现。种子输入在 `公开数据/lesson-03-fixtures/`。

### 可以交给 AI

- ToolSpec / JSON Schema 样板；
- 参数反序列化；
- timeout 包装器；
- fixture 和 table-driven tests；
- 错误显示文本。

### 必须由你决定

- 错误是否 retryable；
- 有副作用工具如何幂等；
- workspace root 怎样授权；
- 输出大小和调用预算；
- 自动重试还是人工确认。

### 推荐 Prompt

```text
请审查这个 Rust execute_with_policy，不要直接重写。
按 InvalidInput、PermissionDenied、Timeout、RateLimited、MalformedOutput 分类，指出哪些分支不应重试。
特别检查“动作已经生效但响应超时”的情况，并给出最小回归测试。
[粘贴函数]
```

### AI 代码审查任务

检查 AI 是否把所有 timeout 自动重试、是否先执行后授权、是否只用字符串包含 `..` 判断路径、是否为每次重试生成不同幂等键。

---

## 作业：可靠的 Tool Registry

### 必做工具

1. `ReadWorkspaceFile`：只能读取 `fixtures/`；
2. `SearchText`：在多个文本文件中查找关键词；
3. `CounterTool`：模拟有副作用的计数操作；
4. `FaultyTool`：按配置产生故障。

### Runtime 能力

- 根据 JSON Schema 或等价规则校验参数；
- 单次调用超时；
- 仅对 retryable 错误重试，最多 2 次；
- `CounterTool` 使用 idempotency key 防重复；
- 文件路径越界时拒绝执行；
- 工具错误作为 Observation 写回；
- Trace 记录尝试次数、耗时和最终状态。

### 公开测试

- 缺少必填参数；
- 参数类型错误；
- 文件路径使用 `../` 越界；
- 第一次超时、第二次成功；
- 永久超时；
- 同一 idempotency key 调用两次；
- 工具输出不是约定结构。

### 实验指标

在 20 个脚本化任务上统计：

- task success rate；
- invalid action rate；
- tool recovery rate；
- duplicate side-effect count；
- average tool attempts；
- p95 latency。

---

## 验收标准

- [ ] Tool 定义与执行解耦；
- [ ] 未通过校验/授权的动作不会执行；
- [ ] 有副作用工具不会因重试重复生效；
- [ ] 永久故障能在预算内结束；
- [ ] 至少 10 个工具契约与故障测试；
- [ ] 报告解释一次“重试反而更危险”的场景。

---

## 思考题

> 一个工具的成功率从 95% 提高到 99%，为什么不一定让 Agent 的任务成功率提高？请考虑调用链长度、错误相关性和恢复策略。
