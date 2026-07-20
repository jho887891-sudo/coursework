# 逐 Lesson 实现指南：把 Agent 核心机制完整做一遍

本课程不是代码浏览课，也不是让你调用一个现成 Agent 框架。Lesson 1–7 各有一个独立 Rust 小工程，你要在每个工程中亲自完成一个核心机制，最终再把这些思想组合进 Final Project。

独立工程的目的是降低初学者的上下文负担，不是降低深度。完成课程后，你应当分别实现过 Agent Loop、Runtime、Tool、Evidence、Planning、Evaluation 和 Memory。

## 配套代码是什么

每个 Lesson crate 初始包含：

- 本 Lesson 所需的最小类型和函数签名；
- `NotImplemented` 或保守失败的占位行为；
- 一个确保工程能编译的 smoke test；
- 一组默认 `ignored` 的验收测试。

它不包含标准实现。验收测试初始失败是课程起点，不是代码损坏。

## 每个 Lesson 的实现循环

```text
读知识讲解，理解组件解决的问题
→ 读接口规约和不变量
→ 运行 smoke test，确认脚手架可编译
→ 运行 ignored 验收测试，观察真实失败
→ 实现一个接口，使一组测试通过
→ 增加讲义要求的边界和故障测试
→ 用公开 fixture 做实验
→ 解释实现、失败与局限
```

课程规定接口行为和验收标准，但不会规定你必须按哪一行、哪一种算法实现。

## Lesson 与独立工程

| Lesson | crate | 必须完整实现的核心 |
|---|---|---|
| Lesson 1 | `lesson-01-agent-model` | State、Observation、Action、Policy、Loop、Termination |
| Lesson 2 | `lesson-02-runtime` | Parser、Budget、StopReason、重复检测、Trace |
| Lesson 3 | `lesson-03-tools` | Tool、Registry、校验、权限、错误分类、有限重试 |
| Lesson 4 | `lesson-04-evidence` | tokenizer、检索、locator、支持判断、拒答、冲突 |
| Lesson 5 | `lesson-05-planning` | Plan、Validator、依赖图、GoalVerifier、RePlan 条件 |
| Lesson 6 | `lesson-06-evaluation` | Runner、混淆矩阵指标、失败分母、重复实验、消融 |
| Lesson 7 | `lesson-07-memory` | provenance、scope、TTL、冲突、删除、用户隔离 |
| Final Project | `final-project-starter` | 重新组合前面机制，完成端到端审查辅助系统 |

## 如何运行

从 `ai-bid/docs/课程/Agent入门课程/配套代码` 进入 Workspace：

```powershell
# 所有脚手架应能编译；ignored 验收测试不会自动运行
cargo test --workspace

# 查看 Lesson 1 尚未完成的验收要求；初始应失败
cargo test -p lesson-01-agent-model --test acceptance -- --ignored

# 也可以一次只攻克一个测试
cargo test -p lesson-01-agent-model --test acceptance `
  asks_when_goal_is_unknown -- --ignored --exact
```

实现 Lesson 1 后，再运行同一条验收命令。其他 Lesson 只需替换包名。不要通过删除测试、移除 `ignored` 后继续跳过、降低断言或硬编码样本来制造“通过”。

## AI 如何分担工作量

AI 可以完成：Cargo/模块样板、重复 match、JSONL loader、fixture 候选、测试样板和报告格式。你仍然必须对每个核心接口完成以下工作：

1. 先用自己的话写出不变量；
2. 审查 AI 是否改变接口或偷偷使用默认成功；
3. 至少增加一个 AI 没想到的反例；
4. 运行故障测试并定位根因；
5. 在答辩中解释并现场修改。

“实现一遍”不要求每个字符都手敲，但要求每个核心机制都由你集成、验证、修复并真正理解。

## 每个 Lesson 的完成证据

- 本 Lesson 的验收测试全部通过；
- 至少新增一个边界测试和一个故障测试；
- 一条成功 Trace 和一条失败 Trace（不适用 Trace 时提交等价运行记录）；
- `REPORT.md`：不变量、实现选择、实验结果、失败根因、局限；
- `AI_CONTRIBUTION.md`：AI 贡献、你发现的问题、你的修复。

现在进入[Lesson 1：Agent 到底是什么？](lesson-01-agent-model.md)。
