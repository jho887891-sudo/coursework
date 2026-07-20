# Agent 深度实战：从 ReAct Loop 到 Multi-Agent 协作

> 5 天深度原理课。Agent 智能组主修。通过独立 Rust Demo 理解 ReAct Loop、Coordinator、AgentBus、SessionGraph 和 ToolRegistry；项目源码只读对照，不在课程中直接修改。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)：个人重在解释原理和分析失败，小组共同完成独立实验 Demo，完整框架实现作为骨干选做。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Rust 熟练 | struct/enum/trait/async/tokio::spawn/broadcast/Arc |
| Agent 基础 | 已完成 Agent 通用课程（7 周），理解 ReAct Loop 基本概念 |
| DashScope API Key | 团队统一下发 |
| 项目源码 | `backend-rust/src/agents/` 目录，只读走查 |
| 实验目录 | 新建独立 Cargo 工程，不放入项目业务代码目录 |

### 验证环境

```bash
cd backend-rust
cargo build --bin server    # 项目 Agent 框架能编译
cargo run --bin test_llm    # LLM 连接正常
```

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **ReAct Loop 内核** | Token Budget 管理器 + 流式事件解析器 + 消息历史管理器 |
| Day 2 | **Tool Use + ToolRegistry** | ToolRegistry 注入模式 + SearchBuffer 并发去重 + 工具错误恢复 |
| Day 3 | **Agentic RAG + 推理链** | ReWOO 规划器 + 推理链可追溯性验证器 |
| Day 4 | **Multi-Agent 协作** | Coordinator 调度器 + AgentBus 消费者 + 协作策略对比实验 |
| Day 5 | **综合实验** | 在独立 Demo 中验证一种协作策略、工具机制或幻觉检测方法 |

---

## 项目源码对照（只读）

```
backend-rust/src/agents/
├── coordinator.rs          ← Day 4 精读：7 阶段 Pipeline + spawn_agent
├── react_loop.rs           ← Day 1 精读：execute_react_loop + tool_call 解析
├── bus.rs                  ← Day 4 精读：broadcast channel + BusMessage
├── session_graph.rs        ← Day 4 精读：SessionNode + 动态生命周期
├── agent_definition.rs     ← Day 2 精读：AgentDefinition + AgentComplexity
├── tools/
│   ├── registry.rs         ← Day 2 精读：ToolRegistry + definitions()
│   ├── search_knowledge.rs ← Day 2 精读：SearchBuffer 实现
│   ├── read_section.rs     ← Day 2 精读：精读工具
│   ├── output_finding.rs   ← Day 1 精读：终端工具
│   └── validate_calculation.rs ← Day 2 精读：数值验证工具
├── prompts/
│   ├── judge_agent.md      ← Day 3 精读：审查 Agent 的 System Prompt
│   └── legal_verify.md     ← Day 3 精读：法规验证 Agent
└── coordinator/
    └── pipeline.rs         ← Day 4 精读：7 阶段调度
```

---

## 与通用 Agent 课程的区别

| | Agent 通用课程（全员） | Agent 深度实战（G4 专用） |
|---|---|---|
| 定位 | 从零写出第一个 Agent | 深入理解项目 Agent 框架的设计 |
| 深度 | ReAct Loop 基本概念 | Coordinator 7 阶段 Pipeline 源码走读 |
| 产出 | 独立的小型 Agent 项目 | 独立机制 Demo + 实验报告 |
| 前置 | Rust 基础 | Agent 通用课程 + Rust 熟练 |
| 对标 | 概念对应项目文件 | 只读源码走查，不在课程中修改业务代码 |

---

## 参考资源

- [ReAct 论文](https://arxiv.org/abs/2210.03629) — Yao et al., 2022
- [ReWOO 论文](https://arxiv.org/abs/2305.18323) — 规划+推理分离
- [AutoGen 论文](https://arxiv.org/abs/2308.08155) — 多 Agent 对话框架
- [Anthropic Tool Use](https://docs.anthropic.com/en/docs/build-with-claude/tool-use) — 工具调用规范
- 项目 `backend-rust/docs/设计.md` — Coordinator + AgentBus 完整设计文档
