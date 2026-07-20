# Lesson 1 示例：Agent 建模与边界

本示例分两层实现：

1. `lib.rs`：保持课程公开接口，完成最小 `State → Policy → Action` 与
   `Observation → State` 循环；
2. `agent.rs`：把会议准备任务建模为领域状态机，与 naive Chatbot 和固定
   Workflow 做对照。

## 运行

从 `示例代码` Workspace 根目录执行：

```powershell
cargo test -p lesson-01-agent-model -- --include-ignored
cargo fmt --all -- --check
cargo clippy -p lesson-01-agent-model --all-targets -- -D warnings
```

## 课堂交互 Demo

### 推荐：HTML 投影界面

双击打开：

```text
demo/index.html
```

HTML 版固定显示当前循环阶段、State 差异、预测选项、Action、Observation、
教师讲解和完整 Trace。它支持教师模式、学生预测模式、全屏以及键盘控制：

```text
← / →     上一步 / 下一步
Space     下一步
R         重置
1～4      切换场景
```

页面完全离线，不需要启动服务或连接网络。

### 命令行版

打开菜单，选择四个教学场景：

```powershell
cargo run -p lesson-01-agent-model --example lesson1_demo
```

直接运行某个场景：

```powershell
cargo run -p lesson-01-agent-model --example lesson1_demo -- complete
cargo run -p lesson-01-agent-model --example lesson1_demo -- missing-date
cargo run -p lesson-01-agent-model --example lesson1_demo -- read-failure
cargo run -p lesson-01-agent-model --example lesson1_demo -- user-declined
```

每一步会先显示 State 和预测问题，等待按 Enter 后再揭晓 Action 与 Observation。
备课时可以关闭暂停、一次查看全部输出：

```powershell
cargo run -p lesson-01-agent-model --example lesson1_demo -- all --no-pause
```

## 关键设计

- 初始请求先进入 State，再调用 Policy；完整输入不会先产生多余询问。
- Action 是意图，Observation 是环境事实；`ReadMaterial` 不会直接产生材料内容。
- 只有 `MaterialLoaded` 才能写入 `material_text`。
- `MaterialReadFailed` 会清除无效路径，下一轮选择 `AskForMaterialPath`。
- `NoMoreInput` 后拒绝任何新 Observation。
- Finish/Stop 会固化到运行状态；预算由程序强制。
- 脚本化环境会校验 Observation 是否与上一 Action 匹配。

## 文件

```text
src/lib.rs                     通用最小 Agent Loop
src/agent.rs                   会议准备 Agent
src/chatbot.rs                 naive Chatbot baseline
src/workflow.rs                固定 Workflow baseline
examples/lesson1_demo.rs       课堂逐步交互演示器
demo/index.html                离线 HTML 课堂界面
tests/state_tests.rs           状态转换与关闭语义
tests/policy_tests.rs          Policy、预算与终止
tests/architecture_comparison.rs  三架构对照与反例
traces/success.jsonl           成功轨迹
traces/failure-user-declined.jsonl  失败/合法停止轨迹
REPORT.md                      实验结论与局限
AI_CONTRIBUTION.md             AI 协作记录
```

## 重要边界

本示例没有接入真实 LLM，也没有实现真实 Tool Registry。会议 Agent 使用脚本化
Observation 模拟用户和文件工具反馈，目的是先证明状态、动作、观察和终止语义正确。
