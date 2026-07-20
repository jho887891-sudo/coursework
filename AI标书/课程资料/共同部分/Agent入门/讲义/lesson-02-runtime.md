# 第2课：Agent Runtime — 状态、预算、终止与 Trace

> 模型负责提出动作，Runtime 负责决定这个动作能不能执行、执行后如何记录、什么时候必须停。

---

## 学习目标

1. 把 Agent Loop 写成可测试的 Runtime，而不是一个巨大 while；
2. 区分模型输出、合法动作和环境结果；
3. 实现步骤、时间和调用次数预算；
4. 记录可重放的结构化 Trace；
5. 使用 `FakeModel` 做确定性测试。

---

## 本 Lesson 必须实现

工程：`配套代码/lesson-02-runtime`。Lesson 1 的 Loop 已经帮助你理解控制结构，本 Lesson 独立重写一个可靠 Runtime：

1. `parse_action`：只接受定义过的动作，非法输出不得猜测；
2. `run_with`：通过 Model / Environment / Clock 实现 step/model/tool/time 四类预算；
3. 所有成功、失败、拒绝和停止都进入 `TraceEvent`；
4. 实现重复动作检测和确定性终止；
5. 使用自建 `FakeModel` 覆盖无限继续、非法格式、timeout 和恰好耗尽预算。

```powershell
cargo test -p lesson-02-runtime --test acceptance -- --ignored
```

AI 可以补 match 和 Trace 序列化，但预算扣减时机、停止优先级和错误语义必须由你定义并写入报告。

---

## 1. 模型不是最高权限

模型可以建议：

```json
{"action":"read_document","args":{"path":"rules.txt"}}
```

Runtime 必须检查：

- 动作是否存在；
- 参数能否解析；
- 是否有权限；
- 预算是否足够；
- 当前状态下是否允许；
- 动作结果应该怎样写回状态。

因此推荐分层：

```text
ModelOutput
   ↓ parse
ProposedAction
   ↓ validate + authorize + budget
ApprovedAction
   ↓ execute
Observation
   ↓ reduce
AgentState
```

---

## 2. Runtime 数据结构

```rust
struct Budget {
    max_steps: u32,
    max_model_calls: u32,
    max_duration: std::time::Duration,
}

enum TerminationReason {
    GoalCompleted,
    UserCancelled,
    BudgetExhausted,
    RepeatedAction,
    UnrecoverableError,
}

struct TraceEvent {
    run_id: String,
    step: u32,
    event_type: String,
    state_summary: String,
    action: Option<String>,
    observation: Option<String>,
    error: Option<String>,
    remaining_steps: u32,
}
```

Trace 不记录模型隐藏的思维链。它记录系统实际拥有的状态、选择的动作、观察到的结果和终止原因。

---

## 3. 用接口隔离模型

```rust
#[async_trait::async_trait]
trait Model {
    async fn propose(&self, input: &ModelInput) -> anyhow::Result<String>;
}
```

真实模型用于行为实验，测试使用脚本化模型：

```rust
struct FakeModel {
    outputs: std::sync::Mutex<VecDeque<String>>,
}
```

它可以稳定地产生：

- 合法动作；
- 非法 JSON；
- 未知动作；
- 重复动作；
- 过早结束。

如果 Runtime 只能靠真实 API 测试，就很难复现错误，也无法区分模型失败和程序失败。

---

## 4. 状态更新器

不要让模型直接修改内部状态。使用明确的 reducer：

```rust
fn reduce(state: &mut AgentState, event: RuntimeEvent) {
    match event {
        RuntimeEvent::ActionStarted(action) => { /* 更新计数 */ }
        RuntimeEvent::ObservationReceived(obs) => { /* 写入观察 */ }
        RuntimeEvent::ActionRejected(reason) => { /* 记录拒绝 */ }
        RuntimeEvent::Terminated(reason) => { /* 固化结束状态 */ }
    }
}
```

这样才能测试不变量：

- `steps <= max_steps`；
- 终止后不再执行动作；
- 被拒绝的动作不会改变外部环境；
- 每次执行都有对应 Trace；
- `run_id` 在一次运行内保持一致。

---

## 5. 重复动作检测

真实 Agent 常见失败：

```text
search("保证金比例")
search("保证金比例")
search("保证金比例")
...
```

Runtime 可以维护最近动作指纹：

```text
fingerprint = action_name + canonical_json(arguments)
```

连续重复超过阈值时，向模型返回错误观察，或以 `RepeatedAction` 结束。

---

## 6. Worked Example：一次 Runtime 怎样跑完

假设用户任务是“调用 echo 输出 hello，然后结束”。FakeModel 依次返回：

```jsonl
{"action":"echo","arguments":{"text":"hello"}}
{"action":"finish","arguments":{"answer":"done"}}
```

### Runtime 主循环骨架

```rust
pub async fn run(
    model: &dyn Model,
    env: &mut Environment,
    mut state: AgentState,
    budget: Budget,
) -> RunResult {
    let started = std::time::Instant::now();
    let mut model_calls = 0_u32;

    loop {
        // 1. 先由 Runtime 检查，不依赖模型自觉
        if state.steps >= budget.max_steps {
            return terminate(state, TerminationReason::BudgetExhausted);
        }
        if started.elapsed() >= budget.max_duration {
            return terminate(state, TerminationReason::BudgetExhausted);
        }
        if model_calls >= budget.max_model_calls {
            return terminate(state, TerminationReason::BudgetExhausted);
        }

        // 2. 只把需要的信息组织给模型
        let input = build_model_input(&state);
        model_calls += 1;
        let raw = match model.propose(&input).await {
            Ok(v) => v,
            Err(err) => {
                state.push_error(RuntimeErrorKind::Model, err.to_string());
                return terminate(state, TerminationReason::UnrecoverableError);
            }
        };

        // 3. 模型文本先解析为 ProposedAction
        let proposed = match parse_action(&raw) {
            Ok(v) => v,
            Err(err) => {
                state.observe(Observation::InvalidModelOutput(err.to_string()));
                state.steps += 1;
                continue;
            }
        };

        // 4. 校验动作；失败时绝不能触碰环境
        let approved = match validate_action(proposed, &state) {
            Ok(v) => v,
            Err(err) => {
                state.observe(Observation::ActionRejected(err.to_string()));
                state.steps += 1;
                continue;
            }
        };

        // 5. 执行并把结果写回状态
        let observation = env.execute(approved).await;
        state.apply(observation);
        state.steps += 1;

        // 6. 终止由状态和策略判断
        if state.goal_completed() {
            return terminate(state, TerminationReason::GoalCompleted);
        }
    }
}
```

这不是最终参考答案，而是责任顺序。你可以把各层拆成 struct 或 trait，但不能跳过边界。

### 运行轨迹

```jsonl
{"step":0,"event_type":"model_output","action":"echo","remaining_steps":3}
{"step":0,"event_type":"observation","observation":"hello","remaining_steps":3}
{"step":1,"event_type":"model_output","action":"finish","remaining_steps":2}
{"step":1,"event_type":"terminated","observation":"goal_completed","remaining_steps":2}
```

看到轨迹时，你应能回答：动作何时被允许、环境何时改变、为什么结束。

---

## 7. 跟做实验：从 Parser 开始

### Checkpoint A：解析

实现：

```rust
fn parse_action(raw: &str) -> Result<ProposedAction, ParseError>
```

至少测试：合法 JSON、普通文本、空字符串、缺字段。

### Checkpoint B：校验

实现：

```rust
fn validate_action(action: ProposedAction) -> Result<ApprovedAction, ValidationError>
```

让 `echo` 必须包含字符串 `text`，`finish` 必须包含 `answer`。

### Checkpoint C：预算

先使用永远输出 `echo` 的 FakeModel。没有预算时它不会结束；加入 `max_steps` 后必须得到 `BudgetExhausted`。

### Checkpoint D：重复检测

对 Action 名和规范化参数计算指纹。连续三次相同则停止。测试 JSON 字段顺序不同但语义相同的情况。

### Checkpoint E：Trace

每次模型输出、动作拒绝、工具结果和终止都写事件。测试事件 step 单调不减，且同一运行 `run_id` 一致。

---

## 8. 如何调试 Runtime

按层排查：

```text
看不到动作       → 检查 Model/FakeModel 输出
解析失败         → 检查 raw text 和 Parser
动作被拒绝       → 检查 schema、状态、权限
工具没执行       → 检查是否获得 ApprovedAction
状态没变化       → 检查 Observation 和 reducer
不结束           → 检查 goal verifier、预算、重复检测
Trace 不完整     → 检查每个 return/continue 前是否写事件
```

不要一看到失败就改 Prompt。

---

## 9. 本课自测

1. 为什么 `ModelOutput` 不能直接传给 Tool？
2. Parser 和 Validator 分别负责什么？
3. `finish` 是模型说了算，还是 Runtime 说了算？
4. deadline 和 max_steps 解决的是同一种问题吗？
5. Trace 为什么不应只是一串 `println!`？

参考方向：模型输出不可信；Parser 检查语法、Validator 检查语义；Runtime 验证目标；时间与步骤是不同预算；结构化 Trace 才能筛选、关联和重放。

---

## 10. 延伸学习

- Rust trait object 与依赖注入；
- serde 自定义反序列化错误；
- event sourcing 与 reducer 的基本思想；
- Tokio 的 timeout 和 `Instant`；
- property-based testing：随机输出也必须满足“最终终止”不变量。

---

## 11. 本课 AI 协作方式

### 本课起点

从课程根目录打开 `配套代码/lesson-02-runtime`。其中只提供 `Model`、预算/停止/Trace 类型和待实现入口；你要自己实现 `FakeModel`、动作解析、四类预算、重复检测和结构化 Trace。

### 可以交给 AI

- Parser 和 serde 数据结构初稿；
- JSONL Trace Writer；
- FakeModel 输出队列；
- 针对错误 enum 生成测试表格。

### 必须由你决定

- 每一种错误是否允许继续；
- max_steps、max_model_calls、deadline 怎样共同生效；
- `finish` 如何验证目标；
- 重复动作何时警告、何时终止；
- Trace 必须满足哪些不变量。

### 推荐 Prompt

```text
请为这个 Rust Runtime 写 JSONL TraceWriter。
不得修改 Runtime 状态，不得吞掉写入错误；每条事件必须包含 run_id、step、event_type。
先列出你认为的失败场景，再给代码和测试。
[粘贴 TraceEvent 接口]
```

### AI 代码审查任务

重点寻找：模型调用计数没有增加、`finish` 直接成功、非法 JSON 无限重试、rejected action 没写 Trace、deadline 只在循环结束后检查。

---

## 作业：实现 Mini Agent Runtime

### 基本要求

实现以下组件：

1. `Model` trait 与 `FakeModel`；
2. `ActionParser`：把模型文本解析为类型化动作；
3. `ActionValidator`：拒绝未知动作和非法参数；
4. `Budget`：限制步骤、模型调用次数和总时间；
5. `TerminationPolicy`；
6. JSONL `TraceWriter`；
7. `Runtime::run()`。

本 Lesson 的 Environment 只需支持：

- `echo(text)`；
- `finish(answer)`。

### 故障测试

至少覆盖：

- 第一次输出非法 JSON，第二次恢复；
- 模型请求未知动作；
- 连续重复同一动作；
- 达到最大步骤；
- `finish` 后模型仍试图执行动作；
- Trace 写入失败时主任务如何处理。

### 实验

对同一任务分别运行：

- 无预算 Runtime；
- 有预算和重复检测 Runtime。

使用预设的循环型 `FakeModel`，比较两者是否能确定性终止。

---

## 验收标准

- [ ] 离线 `cargo test` 不调用真实模型；
- [ ] 至少 8 个 Runtime 测试；
- [ ] 所有运行都有明确终止原因；
- [ ] 非法动作不会执行；
- [ ] Trace 可以按 step 顺序还原运行过程；
- [ ] `REPORT.md` 区分模型错误、协议错误、工具错误和 Runtime 错误。

---

## 思考题

> 为什么“让 System Prompt 告诉模型不要死循环”不能替代 Runtime 的最大步骤和重复动作检测？
