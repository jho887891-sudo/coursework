# Agent Systems 入门：从“会调用模型”到“会构建可靠 Agent”

> 7 个基础 Lesson + 1 个 Final Project Lesson。不去导入现成 Agent 框架，而是用 Rust 亲手实现一个可测试、可观测、会使用证据、能在失败中恢复的 Agent Runtime。

## 第一次打开课程？不要先通读

请直接进入：[**逐 Lesson 实现指南：把 Agent 核心机制完整做一遍**](01-逐Lesson实现指南.md)。它说明独立工程、验收测试、实现深度和 AI 分工。

---

## 课程定位

这不是“七个 Lesson 认识七个 Agent 名词”的课程，也不是 Prompt 技巧合集。

课程围绕一个问题展开：

> 当模型需要在不完整信息、外部工具和有限预算下完成任务时，我们怎样设计、验证和约束它？

学完后，你应该既能写 Agent，也能判断一个问题是否根本不需要 Agent。

---

## 你需要准备什么

| 要求 | 说明 |
|---|---|
| Rust 基础 | 会使用 struct / enum / trait / Result / async |
| LLM Client | 能发送消息并获得模型回复；推荐复用前置 Rust 课程的 `ai-client` |
| 测试基础 | 会写 `#[test]` 和 `cargo test` |
| 实验意识 | 愿意记录失败，而不是只展示最好的一次结果 |

第一次接触 LLM/Agent 的同学，请先阅读：[预备读本：开始写 Agent 前，你需要先知道什么](00-预备知识.md)。

每个 Lesson 的必读、选读和阅读问题见：[学习资料索引](学习资料索引.md)。

课程允许 AI 辅助编码，使用前必须阅读：[AI Pair Programming 规范](AI协作规范.md)。

### 环境检查

以下命令从 `ai-bid` 仓库根目录开始执行：

```powershell
cargo --version

# 验证项目已有 LLM 连接
Set-Location backend-rust
cargo run --bin test_llm

# 验证课程配套代码
Set-Location '..\docs\课程\Agent入门课程\配套代码'
cargo test --workspace
```

真实模型只用于行为实验。大部分逻辑测试必须能使用 `FakeModel` 离线、确定性地运行。

### 统一依赖

随课脚手架只使用 Rust 标准库，克隆后可离线编译。你把它扩展成异步、JSONL 和真实模型版本时，建议在 Workspace 统一管理以下依赖：

| crate | 用途 |
|---|---|
| `serde` / `serde_json` | 类型化 JSON 和 JSONL |
| `async-trait` | 异步 Model / Tool trait |
| `tokio` | timeout、异步测试和任务调度 |
| `anyhow` / `thiserror` | 应用错误与类型化错误 |
| `sha2` | 稳定指纹和内容哈希示例 |

这些 crate **不是随课脚手架的必需依赖**。不要在各 Lesson 项目中选择互不兼容的版本；需要时由全班统一加入 Workspace，并记录版本和变更理由。

---

## 你会学到什么

| Lesson | 主题 | 核心产出 |
|---|---|---|
| Lesson 1 | [Agent 建模与边界](lesson-01-agent-model.md) | Chatbot / Workflow / Agent 三种实现与任务分类实验 |
| Lesson 2 | [Agent Runtime](lesson-02-runtime.md) | 状态、动作、观察、预算、终止条件和结构化 Trace |
| Lesson 3 | [Tool 与失败恢复](lesson-03-tool-reliability.md) | 类型化工具、超时、重试、幂等、权限与故障注入 |
| Lesson 4 | [Evidence 与 RAG](lesson-04-evidence-rag.md) | 可追溯检索、引用核验、拒答和检索策略对比 |
| Lesson 5 | [Planning 与 RePlan](lesson-05-planning.md) | Workflow / ReAct / Plan-Execute-RePlan 对比实验 |
| Lesson 6 | [Agent Evaluation](lesson-06-evaluation.md) | 隐藏测试、重复运行、Precision/Recall、成本和稳定性 |
| Lesson 7 | [Memory 与系统边界](lesson-07-memory-boundaries.md) | 有来源的记忆、冲突/遗忘、人工升级与系统卡 |
| Lesson 8–9 | [大作业](final-project.md) | 证据驱动的 Mini 标书风险审查 Agent |

Multi-Agent、MCP、向量数据库和项目生产框架放在后续“Agent 深度实战”中。本课先打牢它们依赖的 Runtime、证据和评测基础。

---

## 每个 Lesson 怎么学

```text
课前       阅读讲义，写下对实验结果的预测
课堂       概念讲解 + 现场实验 + 失败案例走读（90 分钟）
实验       运行脚手架与失败测试，先做 baseline，再让 AI 补机械实现（约 2 小时）
课后       跑公开测试、重复实验并解释结果（约 2～4 小时）
讲评       展示失败轨迹，不只展示成功 Demo
```

每个 Lesson 固定包含四份产物：

1. `src/`：实现代码；
2. `tests/`：自动化测试；
3. `traces/`：至少一条成功轨迹和一条失败轨迹；
4. `REPORT.md`：预测、结果、失败分析和设计结论。

配套代码、公开数据和 AI 协作模板都位于课程根目录。每课开头会注明具体起点。

公开数据只提供代表性种子样本。需要 20/30 条实验时，可以让 AI 扩写机械变体，但学生必须亲自定义覆盖维度、抽查 ground truth、去重，并在报告中标记生成方式；未经核验的 AI 标签不能直接充当标准答案。

---

## 统一实验流程

每个 Lesson 都按以下顺序工作：

1. **Prediction**：运行前写下你认为哪种方案更好；
2. **Baseline**：先实现最简单的非 Agent 方案；
3. **Agent**：加入本 Lesson 机制；
4. **Break it**：主动制造至少一种失败；
5. **Measure it**：在同一批测试上比较结果；
6. **Explain it**：说明收益、代价和适用边界。

“功能更多”不是结论，“在 30 个样本上完成率从 63% 提升到 80%，平均多花 1.7 次模型调用”才是结论。

---

## 统一评分方式

`cargo check` 和公开测试通过是提交门槛，不单独奖励大量分数。

| 维度 | 权重 |
|---|---:|
| 核心机制与任务效果 | 25% |
| 隐藏测试与故障恢复 | 20% |
| 自动化测试与不变量 | 15% |
| 实验设计与指标 | 20% |
| Trace、失败复盘与可解释性 | 10% |
| 代码质量与答辩 | 10% |

允许使用 AI 辅助写代码，但你必须能解释任意一段提交代码，并能在现场修改需求后继续完成任务。

---

## 三条课程纪律

1. **没有证据，不编结论。** 找不到依据时应明确说“不足以判断”。
2. **没有 baseline，不声称改进。** 加入 Agent、Planning 或 Memory 都要对比。
3. **不记录隐藏思维链。** Trace 记录状态摘要、动作、工具结果、错误、预算和终止原因。

---

## 与“Agent 深度实战”的衔接

完成本课后，你应当能够：

- 解释 `ReActLoop` 中状态、动作、观察和终止条件；
- 为 ToolRegistry 写契约测试和故障测试；
- 读懂一次 Agent Trace 并定位失败阶段；
- 用 Benchmark 判断某个策略是否真的更好；
- 进入项目源码级的 Coordinator、AgentBus、SessionGraph 和 Multi-Agent 学习。
