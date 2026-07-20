# 第1课：Agent 到底是什么？

> Lesson 1 不急着堆工具。先学会区分 Chatbot、Workflow 和 Agent，并把一个任务写成可检查的决策系统。

---

## 学习目标

完成本课后，你能够：

1. 用 Environment / State / Observation / Action / Policy / Termination 描述 Agent；
2. 区分 Chatbot、确定性 Workflow 和 Agent；
3. 为同一个任务实现规则版与 Agent 版；
4. 解释什么时候不应该使用 Agent。

---

## 本 Lesson 必须实现

工程：`配套代码/lesson-01-agent-model`。先读完第 1～5 节，再实现 `src/lib.rs` 中的：

1. `choose_action`：根据 State 和预算选择动作；
2. `apply_observation`：把环境观察写回 State；
3. `run_scripted`：形成 Observation → State → Action 循环；
4. 明确完成、预算耗尽和输入不足三种终止语义。

```powershell
cargo test -p lesson-01-agent-model --test acceptance -- --ignored
```

不得把所有逻辑塞进一个 `while`，也不得用真实 LLM 代替可确定测试的 Policy。完成后，你应能逐字段解释 State、逐分支解释 Action，并证明 Loop 一定终止。

---

## 1. 一个统一模型

Agent 不是某个框架，也不等于聊天窗口。它是一个在环境中循环决策的系统：

```text
Environment --Observation--> State --Policy--> Action
     ^                                      |
     |--------------- Result --------------|
```

六个基本元素：

| 元素 | 问题 | 文件审查示例 |
|---|---|---|
| Environment | Agent 在什么世界里行动？ | 文件、法规库、用户和工具 |
| State | 当前已知什么？ | 已读条款、检索结果、剩余预算 |
| Observation | 环境刚刚返回了什么？ | 某条法条原文、工具错误 |
| Action | Agent 可以做什么？ | 读取、检索、输出发现、请求人工 |
| Policy | 如何选择下一动作？ | 规则、LLM 或二者组合 |
| Termination | 什么时候结束？ | 目标完成、预算耗尽、无法继续 |

LLM 可以承担 Policy 的一部分，但 LLM 本身不是完整 Agent。

---

## 2. Chatbot、Workflow、Agent

### Chatbot

```text
用户消息 → 模型回复
```

它可以有对话历史，但未必能改变环境或自主选择行动。

### Workflow

```text
读取文件 → 切分 → 检索 → 分类 → 输出
```

步骤由程序员提前确定。对稳定、重复、可枚举的任务，Workflow 往往更便宜、更可控。

### Agent

```text
读取任务
  → 根据当前状态选择动作
  → 观察动作结果
  → 更新状态
  → 决定继续、换策略、求助或结束
```

Agent 的价值来自运行时决策，不来自文件名里有 `agent`。

---

## 3. 最小接口

本 Lesson 先不用真实模型，使用确定性的规则 Policy：

```rust
enum Action {
    AskUser { question: String },
    ReadDocument { path: String },
    ProduceAnswer { text: String },
    Stop { reason: String },
}

enum Observation {
    UserReply(String),
    Document(String),
    ActionFailed(String),
}

struct AgentState {
    goal: String,
    observations: Vec<Observation>,
    steps: usize,
}

trait Policy {
    fn decide(&self, state: &AgentState) -> Action;
}
```

最小循环：

```rust
while !terminated(&state) {
    let action = policy.decide(&state);
    let observation = environment.execute(action)?;
    state.observations.push(observation);
    state.steps += 1;
}
```

关键不在 while，而在循环中的语义：状态如何更新、动作是否允许、失败如何反馈、何时停止。

---

## 4. 部分可观察性

Agent 通常看不到真实世界的完整状态。例如用户说“帮我审核这个文件”，但没有给文件路径。

错误做法：猜一个路径继续执行。

合理动作：

```text
当前状态：缺少目标文件
可执行动作：AskUser
终止条件：用户取消，或获得有效路径后继续
```

Agent 的能力不只体现在“回答”，还体现在识别缺失信息并采取合适动作。

---

## 5. Worked Example：缺少文件路径时怎么办

目标：用户说“帮我总结会议背景材料”，但没有给文件路径。

### 第一步：先写状态，不调用模型

```rust
#[derive(Debug, Clone)]
enum RunStatus {
    Running,
    Completed,
    Stopped,
}

#[derive(Debug, Clone)]
struct MeetingState {
    goal: String,
    date: Option<String>,
    participants: Vec<String>,
    material_path: Option<String>,
    material_text: Option<String>,
    status: RunStatus,
}
```

此时：

```text
material_path = None
material_text = None
```

### 第二步：列出允许动作

```rust
#[derive(Debug, Clone, PartialEq)]
enum MeetingAction {
    AskForDate,
    AskForMaterialPath,
    ReadMaterial { path: String },
    ProduceChecklist,
    Stop { reason: String },
}
```

动作集合越清楚，越容易测试权限和终止。不要一开始就让模型自由生成任意命令。

### 第三步：先实现规则 Policy

```rust
fn decide(state: &MeetingState) -> MeetingAction {
    if state.date.is_none() {
        return MeetingAction::AskForDate;
    }
    if state.material_path.is_none() {
        return MeetingAction::AskForMaterialPath;
    }
    if state.material_text.is_none() {
        return MeetingAction::ReadMaterial {
            path: state.material_path.clone().unwrap(),
        };
    }
    MeetingAction::ProduceChecklist
}
```

规则版的价值是形成可理解 baseline。以后换成 LLM Policy 时，你仍然知道合法动作和正确状态转换应该是什么。

### 第四步：执行动作，获得 Observation

```rust
enum MeetingObservation {
    UserProvidedDate(String),
    UserProvidedPath(String),
    MaterialLoaded(String),
    UserDeclined,
    FileNotFound(String),
}
```

注意：`ReadMaterial` 是 Action，`MaterialLoaded` 是 Observation。不要把“计划读取”和“已经读到”混为一谈。

### 第五步：更新状态

```rust
fn apply(state: &mut MeetingState, obs: MeetingObservation) {
    match obs {
        MeetingObservation::UserProvidedDate(v) => state.date = Some(v),
        MeetingObservation::UserProvidedPath(v) => state.material_path = Some(v),
        MeetingObservation::MaterialLoaded(v) => state.material_text = Some(v),
        MeetingObservation::UserDeclined => state.status = RunStatus::Stopped,
        MeetingObservation::FileNotFound(_) => state.material_path = None,
    }
}
```

文件不存在时把路径重新设为缺失，下一轮 Policy 会再次询问，而不是假装已读取成功。

---

## 6. 跟做实验：先不用 LLM

按顺序完成，每完成一步运行测试：

### Checkpoint A：状态与动作

- 建立 `MeetingState`；
- 建立 `MeetingAction` 和 `MeetingObservation`；
- 写测试确认初始状态缺少日期和路径。

### Checkpoint B：规则 Policy

- 缺日期时选择 `AskForDate`；
- 有日期但缺路径时选择 `AskForMaterialPath`；
- 有路径但没内容时选择 `ReadMaterial`；
- 信息齐全时选择 `ProduceChecklist`。

### Checkpoint C：环境反馈

- 模拟用户提供日期；
- 模拟不存在文件；
- 模拟用户拒绝继续；
- 检查状态是否正确改变。

### Checkpoint D：有限循环

加入 `max_steps = 8`。打印每一步的 State、Action、Observation。确认无论用户怎样回答都能结束。

通过四个 Checkpoint 后，再把 `Policy` 换成真实模型或 FakeModel。不要让 LLM 掩盖状态机错误。

---

## 7. 常见问题

### “规则这么清楚，为什么还需要 Agent？”

这个例子故意简单。你可能最终得出 Workflow 足够好，这正是课程要训练的判断。后续任务中，动作选择会依赖无法枚举的文本和证据，LLM Policy 才更有价值。

### “System Prompt 属于 State 吗？”

它是构造 ModelInput 的系统配置。State 可以保存任务事实，Prompt Builder 再把相关 State 变成模型能读的输入。

### “模型说已经读完文件，算 Observation 吗？”

不算。只有实际文件工具返回的结果才是环境 Observation。

---

## 8. 本课自测

1. `ReadMaterial` 和 `MaterialLoaded` 为什么不能合并？
2. 用户拒绝提供路径时，合理终止原因是什么？
3. 有 LLM 的固定步骤程序为什么仍可能是 Workflow？
4. 一个没有 LLM 的避障机器人能否是 Agent？

参考方向：动作是意图、观察是环境事实；用户取消是合法终止；关键看运行时决策；规则 Policy 也可以构成 Agent。

---

## 9. 延伸学习

- 复习 Rust enum 和模式匹配；
- 阅读经典 AI 中 Rational Agent 的定义；
- 画出你每天使用的一个软件的 Environment / State / Action；
- 尝试找一个“名字叫 Agent、实际是固定 Workflow”的产品功能。

---

## 10. 本课 AI 协作方式

### 本课起点

从课程根目录打开 `配套代码/lesson-01-agent-model`。项目只提供可编译接口、保守失败占位和四个待解锁验收测试；你需要完成状态更新、Policy、Loop 和终止，再实现 Chatbot 与固定 Workflow 对照组。

### 可以交给 AI

- 补充 CLI 输入处理；
- 为 Chatbot/Workflow 生成更多测试；
- 生成 `Display` 或 Debug 输出；
- 根据你定义的 enum 生成重复 match 分支。

### 必须由你决定

- MeetingState 包含什么；
- Action 与 Observation 的边界；
- 缺失信息和用户拒绝时怎么办；
- 终止条件；
- 三种架构哪一种适合哪些任务。

### 推荐 Prompt

```text
请只为以下 Rust enum 生成状态更新函数和 5 个单元测试。
不要增加新动作，不要使用 unwrap，不要决定业务终止规则。
必须覆盖文件不存在和用户拒绝两个 Observation。
[粘贴你自己设计的 enum]
```

### AI 代码审查任务

让 AI 生成 `apply_observation`，检查它是否把“模型声称已读取”错误地当成 `MaterialLoaded`，以及文件不存在后是否会无限询问。

---

## 作业：三种实现，一次架构判断

### 任务

实现一个“会议准备助手”，输入包括会议主题、日期、参与者和背景材料路径。

分别完成：

1. **Chatbot 版**：根据一条用户消息生成准备建议；
2. **Workflow 版**：固定执行字段检查 → 读取材料 → 生成清单；
3. **Agent 版**：根据缺失信息自主选择询问、读取或结束。

### 必测场景

- 信息完整；
- 缺少会议日期；
- 背景材料路径不存在；
- 用户拒绝提供缺失信息；
- 已经获得足够信息，Agent 不得继续询问。

### REPORT.md

回答：

1. 哪些测试中 Workflow 更合适？
2. Agent 版增加了什么能力，又增加了什么风险？
3. 你的 Policy、State、Environment 和 Termination 分别在哪里？
4. 什么证据会让你决定删掉 Agent 版？

---

## 验收标准

- [ ] 三个版本能在相同输入上运行；
- [ ] Agent 版没有把“缺少信息”伪装成答案；
- [ ] 至少 5 个自动化测试；
- [ ] 明确的最大步骤数；
- [ ] 一条失败轨迹；
- [ ] 能口头解释为什么“有历史的聊天机器人”不一定是 Agent。

---

## 思考题

> 如果一个系统的所有步骤都由代码提前写死，但其中某一步调用了 LLM，它是 Agent 吗？请从“运行时决策权”而不是名称回答。
