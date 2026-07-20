# Lesson 1：实现最小 Agent Loop

依次实现 `choose_action`、`apply_observation`、`run_scripted`。先画出 State/Observation/Action/Termination，再写代码。

```powershell
cargo test -p lesson-01-agent-model
cargo test -p lesson-01-agent-model --test acceptance -- --ignored
```

AI 可以生成 match 样板和测试候选；你必须决定状态字段、动作语义、停止条件，并解释每次状态更新。

