# Lesson 2 优秀学生示例：可靠 Agent Runtime

这是 Lesson 2 的完整优秀学生示例。它不连接真实模型，而是通过 `ScriptedModel`、`EchoEnvironment` 和 `SequenceClock` 对 Runtime 做确定性故障注入。

## 它证明了什么

- 模型原始文本必须经过 Parser 和 Validator；
- `finish` 只是提议，`GoalVerifier` 拥有最终判断权；
- step/model/tool/time 四种预算分别生效；
- 非法输出可以有限恢复，但不会无限重试；
- 相同语义动作使用类型化指纹检测，不受 JSON 字段顺序影响；
- 被拒绝动作不会触碰 Environment；
- Model、Tool、Protocol、Trace 故障具有不同停止原因；
- 正常运行以结构化 JSONL Trace 完整还原。

## 运行

在课程的 `示例代码` 目录执行：

```powershell
cargo test --offline -p lesson-02-runtime --all-targets
cargo run --offline -p lesson-02-runtime --example lesson2_demo -- recovery
cargo run --offline -p lesson-02-runtime --example lesson2_demo -- repeated
cargo run --offline -p lesson-02-runtime --example lesson2_demo -- tool-error
```

演示会依次经历：

```text
非法模型输出
→ ActionRejected（仍在协议错误预算内）
→ echo("hello")
→ ToolSucceeded
→ finish("done")
→ GoalVerifier 接受
→ Completed
```

标准输出是 JSONL Trace，最后一行摘要输出停止原因与资源使用量。

## 阅读顺序

1. `src/action.rs`：不可信文本怎样成为 ApprovedAction；
2. `src/runtime.rs`：预算、计数、执行和终止的准确顺序；
3. `src/trace.rs`：内存 Trace 与 JSONL Writer；
4. `src/fakes.rs`：确定性模型、环境和时钟；
5. `tests/acceptance.rs`：课程统一验收规则；
6. `tests/runtime_invariants.rs`：故障注入与系统不变量；
7. `REPORT.md`：设计决策、边界与实验结论。

代码测试通过不等于掌握。现场应能解释：为什么预算在副作用前检查、为什么错误输出也消耗 step、为什么 finish 不能直接结束，以及 Trace 写入失败为何不能静默忽略。
