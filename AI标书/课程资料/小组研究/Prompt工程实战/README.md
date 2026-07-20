# Prompt 工程实战：从概率空间到生产级 Prompt 管理

> 5 天深度原理课。不讲提示词口诀，讲概率、结构化约束、实验设计和版本治理。所有实验使用独立 Prompt Demo，不修改项目 Prompt。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)。小组必做是一组小规模受控实验，完整管理平台作为骨干选做。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Rust 基础 | struct / enum / trait / async / Result / reqwest / serde |
| DashScope API Key | 团队统一下发 |
| 概率基础 | 知道什么是概率分布、条件概率。Day 1 会复习 |
| 好奇心 | 想知道"为什么 temperature=0 也会输出不一样" |

**不需要**：深度学习、Transformer 原理、Agent 课程、RAG 课程——全部概念在本课程内闭环。

### 验证环境

```bash
rustc --version   # ≥ 1.96
cargo --version

# 在 backend-rust/ 下执行，确认能连上 LLM
cargo run --bin test_llm
# 输出 "LLM 连接正常" 即就绪
```

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **LLM 推理本质** | Token 概率可视化工具 + 采样策略对比实验 |
| Day 2 | **System Prompt 工程** | 标书审核 Agent 的 System Prompt 完整设计 + Lost in the Middle 实验报告 |
| Day 3 | **Few-shot + 结构化输出** | Few-shot 自动选择算法 + JSON Schema 约束验证器 |
| Day 4 | **Prompt 评测 + A/B** | A/B 测试框架 + LLM-as-Judge 可靠性分析 |
| Day 5 | **Prompt 治理实验** | 版本、模板、A/B 与回滚的最小 Demo；完整平台为选做 |

---

## 怎么学

```
Day 1  Token 概率空间 → Temperature/Top-p/Top-k 采样 → 手写可视化
Day 2  System Prompt 信息架构 → Lost in the Middle → 标书审核拆解
Day 3  Few-shot 选择算法 → JSON Schema 约束 → 验证器
Day 4  A/B 测试 → LLM-as-Judge → Bootstrap CI
Day 5  版本化 + 灰度发布 + 幻觉检测 → 大作业
```

---

## 代码怎么写

**Demo 以 Rust 为主。** 每课亲手实现一个最小机制，例如采样模拟、Few-shot 选择、JSON 校验或 A/B 指标；不要求每位成员同时完成全部算法和生产平台。

| 任务 | 实现 |
|------|------|
| LLM 调用 | `reqwest` + DashScope API |
| 概率可视化 | 终端 ASCII heatmap |
| Few-shot 选择 | kNN embedding + 手写索引 |
| JSON 验证 | `serde_json` + 手写 Schema validator |
| A/B 测试 | 手写 Bootstrap CI |
| Prompt 模板 | `.md` 文件 + 手写模板引擎（`{{variable}}` 替换） |

---

## 与标书审核项目的关系

```
本课程 → G5 领域工具组
  ├─ Day 2 System Prompt → G5 每个 Agent 的 Prompt 模板
  ├─ Day 3 Few-shot + JSON → G5 工具调用格式约束
  ├─ Day 4 A/B 测试 → G4↔G5 反馈闭环（G4 跑评测 → G5 优化 Prompt）
  └─ Day 5 生产级管理 → G5 Prompt 版本化 + 灰度发布

G4 Agent 引擎直接消费：
  └─ AgentDefinition.system_prompt = G5 维护的 Prompt 模板文件
  └─ 每次 Prompt 变更 → Day 4 A/B pipeline 自动跑 → 通过才合并
```

---

## 参考资源

- 理解 Tokenization：用 DashScope API 调用时开启 `return_token_usage=true`，观察每条消息的实际 token 消耗——中文一个字可能对应 2-3 个 token
- [Lost in the Middle 论文](https://arxiv.org/abs/2307.03172) — Liu et al., 2023
- [Constitutional AI 论文](https://arxiv.org/abs/2212.08073) — Anthropic, 2022
- [LLM-as-Judge 论文](https://arxiv.org/abs/2306.05685) — Zheng et al., 2023
- [What Makes Good In-Context Examples?](https://arxiv.org/abs/2301.13661) — Few-shot 选择研究
