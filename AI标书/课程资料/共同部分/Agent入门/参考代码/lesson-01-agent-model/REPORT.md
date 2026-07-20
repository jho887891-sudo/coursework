# Lesson 1 实验报告：运行时决策权决定系统是不是 Agent

## 1. 实验问题与预测

问题：面对完整输入、缺失字段、读取失败和用户拒绝时，Chatbot、固定 Workflow
与 Agent 的行为有何不同？

实验前预测：

- 完整输入下 Workflow 最简单、最稳定；
- naive Chatbot 能始终返回文本，但可能进行未经验证的补全；
- Agent 在缺失信息和环境失败时能选择询问、恢复或停止，但需要更多状态与测试。

## 2. 六要素在哪里

| 要素 | 实现 |
|---|---|
| Environment | `MeetingAgent::run` 中的脚本化 `MeetingObservation` 序列 |
| State | `MeetingState`：主题、日期、参与人、路径、材料内容、预算和完成状态 |
| Observation | 用户补充、材料读取成功、材料读取失败、用户拒绝 |
| Action | 询问具体字段、读取材料、生成清单或停止 |
| Policy | `MeetingState::decide` |
| Termination | `ProduceChecklist`、用户拒绝、步骤预算耗尽 |

通用脚手架中的 `AgentState` 另外演示了输入关闭与 Finish 后状态锁定。

## 3. 行为契约

1. 初始请求属于已发生的环境输入，必须先写入 State，再调用 Policy。
2. Action 只表达意图；只有匹配的 Observation 才能改变 State。
3. 文件路径存在不等于读取成功；只有 `MaterialLoaded` 能写入材料内容。
4. `MaterialReadFailed` 清除无效路径，下一轮必须请求新路径。
5. 用户拒绝是合法终止，不得继续自动补全。
6. 完成或停止后不得再修改状态。
7. 预算由程序强制，不依赖 Policy 自觉。

## 4. 确定性对照结果

以下结果来自 `tests/architecture_comparison.rs` 的固定场景。它们是教学用的小型
行为实验，不代表生产数据上的统计结论。

| 场景 | naive Chatbot | Workflow | Agent |
|---|---|---|---|
| 信息完整 | 返回文本 | 成功 | `ReadMaterial → ProduceChecklist` |
| 缺少日期 | 自动推定日期，存在错误风险 | `MissingDate` | `AskForDate` |
| 材料读取失败 | 仍声称已根据路径整理 | `MaterialNotFound` | `ReadMaterial → AskForMaterialPath` |
| 用户拒绝补日期 | 仍生成文本 | `MissingDate` | `AskForDate → Stop(user_declined)` |
| 完整输入是否多问 | 不适用 | 不适用 | 0 次多余询问 |

测试结果：

```text
公开验收测试             5 / 5
架构与反例测试           9 / 9
Policy 与终止测试        6 / 6
State 与转换测试         6 / 6
总计                     27 / 27
```

## 5. 结论

- 当字段完整、执行路径固定时，Workflow 更合适：结构简单、行为稳定。
- Agent 的真实新增能力不是“生成更漂亮的文本”，而是在运行时根据 State
  选择 `AskForDate`、`ReadMaterial`、`AskForMaterialPath` 或停止。
- Agent 同时引入更多状态转换、预算和协议错误，因此必须配套反例测试。
- naive Chatbot 只是一个刻意简化的教学 baseline。不能把它的失败推广成
  “所有 Chatbot 都会失败”，也不能据此声称 Agent 在真实业务中更优。

## 6. Action 与 Observation 为什么不能合并

`ReadMaterial { path }` 只是读取意图。环境可能返回：

- `MaterialLoaded { path, content }`；
- `MaterialReadFailed { path }`；
- 没有返回，系统继续等待。

如果动作和观察合并，系统就会把“准备读取”误当成“已经读到”，无法区分模型声称、
文件存在和真实读取成功。

## 7. 删除 Agent 版的条件

需要真实任务数据后才能决定，而不能使用未经测量的“95% 完整输入”等假设。
若同一测试集显示：

- Workflow 完成率不低于 Agent；
- Agent 没有减少缺失信息错误；
- Agent 增加了明显延迟、多余询问或失败路径；

则应删除 Agent，保留 Workflow。

## 8. 当前局限

- Policy 仍是确定规则，没有测试自由文本理解；
- Environment 是脚本化的，没有真实用户界面和文件 Tool；
- 没有时间、模型调用或工具调用预算；
- Chatbot baseline 被明确设计为 naive 版本，只用于展示未经验证补全；
- 样本量很小，只能验证机制，不能证明真实业务效果。

这些能力分别属于后续 Runtime、Tool 与 Evaluation Lesson。
