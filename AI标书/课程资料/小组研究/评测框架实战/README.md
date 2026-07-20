# 评测框架设计与 AI 系统测试

> 5 天深度原理课。不讲指标名词堆砌，讲评测的数学基础、Benchmark 构建、统计不确定性和幻觉审计。实验使用独立小数据集和评测 Demo。

本课遵循[专项课程统一学习规范](../../专项课程统一学习规范.md)。小组先用 10～20 条样本理解方法，50 份以上正式 Benchmark 属于后续项目工作。

---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Rust 基础 | struct / enum / trait / async / Result / reqwest / serde |
| 概率统计基础 | 知道均值/方差/正态分布。Day 1 会复习并扩展 |
| Agent 概念 | 知道"Agent 是一个带工具调用的 LLM 循环"即可 |
| 好奇心 | 想知道"你说 Agent 准确率 85%，怎么证明？" |

**不需要**：深度学习、RAG 课程、Prompt 课程——全部概念在本课程内闭环。

### 验证环境

```bash
rustc --version   # ≥ 1.96
cargo --version
```

---

## 你会学到什么

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **评测指标体系** | P/R/F1 计算器 + 多标签匹配器 + Cohen's Kappa 实现 |
| Day 2 | **Benchmark 构建** | 分层抽样器 + IAA 计算 + Gold Standard 仲裁 Pipeline |
| Day 3 | **自动化评测 Pipeline** | CI 集成评测框架 + 回归检测 + 多重比较校正 |
| Day 4 | **幻觉与推理质量** | 幻觉三分类检测器 + 推理链验证器 + 引用准确性检查 |
| Day 5 | **评测闭环 Demo** | 小样本指标 + 版本对比 + 不确定性 + 简洁报告 |

---

## 怎么学

```
Day 1  P/R/F1 → 多标签匹配 → Cohen's Kappa → 标书 5 维指标
Day 2  分层抽样 → 标注协议 → IAA → Gold Standard
Day 3  Bootstrap CI → 回归检测 → Bonferroni → 效应量
Day 4  幻觉三分类 → 推理链追溯 → 引用验证
Day 5  Dashboard + 竞品对标 + 大作业
```

---

## 代码怎么写

**Demo 以 Rust 为主。** 必修部分手写一个指标、一个重采样实验和一个失败处理机制；完整统计库、幻觉检测器和 Dashboard 可以小组分工或骨干选做。

| 任务 | 实现 |
|------|------|
| 评测指标 | 手写 P/R/F1/宏微平均/多标签匹配 |
| Bootstrap CI | 手写（Day 1 讲原理） |
| 标注工具 | CLI 交互式 + JSON 输出 |
| 幻觉检测 | 正则 + 状态机 + Neo4j 验证 |
| Dashboard | 终端 ASCII 或 Web（axum + 模板） |

---

## 与标书审核项目的关系

```
本课程 → G4 Agent 智能组
  ├─ Day 1-2 评测指标+Benchmark → G4 方向 A：50 份人工标注 + 自动化评测
  ├─ Day 3 自动化 Pipeline → G4↔G5 反馈闭环的引擎
  ├─ Day 4 幻觉检测 → G4 方向 B：推理质量与引用准确性
  └─ Day 5 Dashboard → G4 方向 A+C：可视化 + 协作策略对比

G5 Prompt 优化依赖本课程：
  └─ G5 改 Prompt → 本课程 Day 3 Pipeline 自动跑 → 回归报告 → G5 决策
```

---

## 参考资源

- [Confusion Matrix & Metrics](https://en.wikipedia.org/wiki/Confusion_matrix) — 所有指标从这里出发
- [Cohen's Kappa](https://en.wikipedia.org/wiki/Cohen%27s_kappa) — 标注一致性度量
- [Bonferroni Correction](https://en.wikipedia.org/wiki/Bonferroni_correction) — 多重比较校正
- [Bootstrap Methods](https://en.wikipedia.org/wiki/Bootstrapping_(statistics)) — Efron, 1979
- [RAGAS: RAG Assessment](https://arxiv.org/abs/2309.15217) — RAG 评测框架论文
