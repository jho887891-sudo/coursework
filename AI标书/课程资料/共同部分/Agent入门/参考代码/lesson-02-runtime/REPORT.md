# Lesson 2 实验报告：可靠 Agent Runtime

## 1. 问题定义

本实验不评价模型“聪不聪明”，而是验证 Runtime 能否在模型输出非法、重复、失败或超时时维持以下性质：

1. 外部副作用只来自 ApprovedAction；
2. 所有运行都在有限预算内结束；
3. 每个停止都有类型化原因；
4. Trace 足以还原系统实际发生的事件；
5. GoalVerifier 而不是模型拥有完成判定权。

## 2. 协议与信任边界

模型只返回字符串。Runtime 先把它解析成 `ProposedAction`，再校验成 `ApprovedAction`。Environment 的接口只接收 `ToolCall`，因此原始模型文本没有直接触发工具的路径。

本实验采用 JSON-only 协议，并拒绝未知字段。这会牺牲对“差不多正确”输出的宽容，但换来协议可测试、错误可归类和行为可审计。

Parser 与 Validator 的职责：

| 层 | 负责 | 示例 |
|---|---|---|
| Parser | JSON、action 字段、动作结构 | 普通文本、缺 action、未知动作 |
| Validator | 已知动作的参数语义 | 空 echo.text、空 finish.answer |
| GoalVerifier | 目标是否真的满足 | 没有工具观察时拒绝 finish |

## 3. 预算定义

`steps` 在开始一轮模型处理时增加。`model_calls` 和 `tool_calls` 在对应的 Trace 预备事件写入成功后、真实调用前增加，因此它们表示实际发起的调用。模型失败仍然占用 step 与 model call；Parser/Validator 拒绝不消耗工具预算。若 Trace 在调用前写入失败，外部调用不会发生，对应调用计数也不会增加。

预算检查优先级：

```text
循环入口：Deadline → StepBudget → ModelBudget
工具入口：Deadline → ToolBudget
```

同时耗尽 step 与 model 预算时返回 `StepBudget`。优先级写成固定契约，避免相同状态因代码分支顺序变化而产生不同停止原因。

## 4. 错误语义

| 错误 | 是否允许恢复 | 停止原因 |
|---|---:|---|
| 非法 JSON/未知动作/非法参数 | 在协议错误预算内允许 | `ProtocolError` |
| GoalVerifier 拒绝 finish | 允许 | 继续运行，记录 `FinishRejected` |
| Model 返回错误 | 否 | `ModelError` |
| Tool 返回错误 | 否 | `ToolError` |
| Trace Writer 失败 | 否 | `TraceError` |

协议错误恢复的目的不是猜测模型意图，而是把明确错误反馈给下一次模型输入。达到上限后必须停止。

## 5. 重复动作

指纹基于 `ApprovedAction` 的稳定序列化结果，而不是模型原始字符串。因此 JSON 字段顺序变化不会绕过检测；参数不同则被视为不同动作。

`max_consecutive_identical_actions = 2` 表示最多允许连续执行两次相同语义动作，第三次在副作用前以 `RepeatedAction` 停止。

## 6. Trace 不变量

每个事件包含 `run_id`、step、事件类型、详情、耗时、累计用量和剩余预算。测试验证：

- step 单调不减；
- 一次运行的 run_id 一致；
- 资源使用不超过预算；
- 正常写入时恰好一个 `RunTerminated`；
- Tool 调用有开始事件和成功/失败结果；
- Trace Writer 失败进入内存 Trace，并以 `TraceError` 结束。

Trace 不记录模型隐藏推理，只记录 Runtime 实际获得和处理的文本、动作、观察与控制决定。

## 7. 实验结果

### 无可靠边界的循环型模型

循环型模型可以永久返回 continue，Prompt 中的“请不要循环”无法构成硬保证。

### 加入 Runtime

- model call 预算保证无限 continue 最终停止；
- tool 预算保证第二个越界调用不会执行；
- deadline 在动作和工具边界阻止新的副作用；
- 重复检测在相同工具动作再次越界前停止；
- 终止后的脚本化模型输出不会被读取。

测试覆盖 Parser、恢复、GoalVerifier、四种预算、重复检测、Model/Tool/Trace 故障和 JSONL 序列化。

## 8. 已知边界

本课接口是同步的。Runtime 能在调用前后检查时钟，但不能中断一个已经阻塞的真实网络调用。生产系统需要异步超时、取消令牌和工具幂等键。

本课只有 echo 工具，未实现权限、schema registry 和有限重试；这些属于 Lesson 3。GoalVerifier 也只是教学用规则，不代表真实任务的完成判定已经解决。

## 9. 结论

可靠 Agent 的关键不是让模型承诺“表现良好”，而是把模型放在可验证的控制结构中。Parser、Validator、预算、GoalVerifier、重复检测和 Trace 共同把开放式模型行为约束为有限、可审计的 Runtime 行为。
