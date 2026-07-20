# Lesson 3 实验报告：可靠 Tool Registry

## 1. 实验问题

本实验验证：当模型提供未知工具名、非法参数、越界路径、错误权限或危险重试建议时，程序能否在外部副作用发生前拒绝，并把结果作为结构化 Observation 返回。

核心不变量：

1. 未注册工具不能通过近似名称执行；
2. 输入校验或授权失败时，Tool 调用次数为 0；
3. 只有 `Transient` 可以按策略有限重试；
4. timeout 不能被解释为“动作没有发生”；
5. 相同幂等请求最多产生一次副作用；
6. Tool 成功输出仍必须通过 schema 和大小检查；
7. 每次 attempt 和最终结果都可通过 Trace 审计。

## 2. Tool Contract

`ToolSpec` 同时描述输入、输出、副作用、权限、timeout 和大小上限。Registry 只按精确 name 查找，不进行拼写纠正。

执行边界：

```text
Lookup
→ Input bytes
→ Input schema
→ Permission
→ Idempotency
→ Timeout + Call
→ Output schema
→ Output bytes
→ Cache
→ Observation + Trace
```

这个顺序保证被拒绝请求不会触碰 Tool，也保证“函数正常返回”不自动等于“输出可信”。

## 3. 错误与重试

| ToolErrorKind | 自动重试 |
|---|---:|
| Transient | 策略允许且预算未耗尽时 |
| Timeout | 否 |
| Permanent | 否 |
| PermissionDenied | 否 |
| InvalidArguments | 否 |
| MalformedOutput | 否 |

`max_retries = 1` 表示总 attempt 上限为 2。测试同时覆盖恢复成功、预算耗尽和关闭重试三种情况。

Timeout 默认不重试是有意选择：Future 超时只说明响应未按时返回，远端或工具可能已经改变环境。生产系统如果要重试 timeout，必须先证明操作幂等，或使用稳定 idempotency key。

## 4. 权限与路径

`read_fixture` 需要 `ReadWorkspace` 权限。路径处理先转为 workspace 下的候选绝对路径，再做词法规范化；存在的文件继续 canonicalize，以发现符号链接越界。

允许：

```text
allowed.txt
nested/../allowed.txt
```

拒绝：

```text
../secret.txt
workspace 外绝对路径
指向 workspace 外部的符号链接
非 txt/md/json/jsonl 文件
超出大小上限的文件
```

## 5. 幂等与“成功但响应丢失”

CounterTool 在产生副作用时先使用 `(counter_key, idempotency_key)` 保存完成结果，再返回响应。实验让第一次请求在计数从 0 变为 1 后超时：

```text
第一次：环境已变成 1，但 Registry 收到 Timeout
第二次：相同 key，Tool 返回第一次结果
最终计数：仍为 1
```

Registry 还会缓存已经成功返回的 `(tool_name, idempotency_key)`。同一 key 配不同参数返回 `IdempotencyConflict`，避免错误复用结果。

这两层处理解决不同窗口：

- Registry cache：成功响应后的重复请求；
- Tool 内部记录：副作用已提交但响应丢失。

## 6. Trace

Trace 记录 request_id、tool name、attempt、事件类型、详情和耗时。事件包括：

```text
request_rejected
attempt_started
retry_scheduled
tool_succeeded
tool_failed
idempotency_replayed
```

Trace 不保存隐藏思维链，只保存程序实际处理的请求边界和工具结果。

## 7. 实验结果

- 20 个 Rust 测试覆盖核心契约与安全不变量；
- 12 条公开 Tool case 全部符合 expected；
- 未授权 Counter 调用后计数保持 0；
- transient 一次失败后在第二次恢复；
- permanent 与 timeout 均只 attempt 一次；
- 响应丢失后重复请求不产生第二次副作用；
- JSON 字段、输出 schema、路径和大小均在边界处验证。

## 8. 已知边界

- Tokio timeout 会取消本地 Future，但不能撤销已经提交的远端副作用；
- 本课没有实现指数退避、抖动、熔断器或全局工具调用预算；
- `SearchText` 是教学用线性扫描，不是检索系统；
- 文件授权在支持符号链接的平台应继续覆盖竞争条件和 TOCTOU；
- CounterTool 的内存幂等记录在进程重启后会丢失，生产系统需要持久化原子写入。

## 9. 结论

Tool reliability 的核心不是函数能否调用，而是调用之前和之后的边界是否可证明。Schema、权限、timeout、错误分类、有限重试、幂等和 Trace 共同把模型建议限制为可控的环境动作。
