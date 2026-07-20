# Lesson 3 优秀学生示例：可靠 Tool Registry

本示例把不可信工具建议变成类型化、可授权、可超时、可审计的外部操作。所有测试离线运行，不连接真实模型或网络服务。

## 已实现

- 严格 JSON Schema：必填、类型、长度、数组大小和未知字段；
- 精确 Registry 分发与重复注册拒绝；
- 输入/输出字节上限；
- `ReadWorkspaceFile` 的规范化路径与 workspace 授权；
- `SearchText` 的稳定 path/line/excerpt；
- `FaultyTool` 的确定性故障注入；
- 仅对 `Transient` 进行有限重试；
- Tokio 单次调用 timeout，timeout 默认不重试；
- `CounterTool` 的 Registry 级与 Tool 级双层幂等；
- 结构化 `ToolObservation` 与 `ToolTraceEvent`；
- 全部 12 条公开 Tool case。

## 运行

在课程的 `示例代码` 目录执行：

```powershell
cargo test --offline -p lesson-03-tools --all-targets

cargo run --offline -p lesson-03-tools --example lesson3_demo -- transient
cargo run --offline -p lesson-03-tools --example lesson3_demo -- permission
cargo run --offline -p lesson-03-tools --example lesson3_demo -- lost-response
```

三个演示分别证明：

1. `Transient → Success` 可以在一次有限重试后恢复；
2. 缺少 `ModifyState` 时 Counter 调用次数为 0；
3. Counter 已生效但响应超时后，相同 idempotency key 不会重复增加。

## 推荐阅读顺序

1. `src/types.rs`：Tool Contract、Observation 与 Trace；
2. `src/schema.rs`：输入/输出结构验证；
3. `src/registry.rs`：执行前边界与重试顺序；
4. `src/tools.rs`：文件、检索、故障和副作用工具；
5. `tests/acceptance.rs`：核心验收；
6. `tests/safety_invariants.rs`：副作用与故障不变量；
7. `tests/public_fixture_cases.rs`：12 条固定公开集；
8. `REPORT.md`：设计取舍与系统边界。
