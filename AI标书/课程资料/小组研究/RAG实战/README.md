# RAG 实战：从嵌入原理到生产级检索服务

> 5 天深度原理课。不讲 API 用法，讲检索机制、实验方法和工程边界。每个核心机制都用独立 Demo 验证，完整实现作为骨干选做。


---

## 你需要准备什么

| 要求 | 说明 |
|------|------|
| Rust 基础 | 完成 Rust 课程第 1-4 课：struct / enum / trait / async / Result / reqwest / serde |
| Agent 概念 | 完成 Agent 课程第 2-3 课（了解 Agent 怎么调用 Tool 即可） |
| Docker | 已安装，Day 3 需要跑 Qdrant |
| 硬件 | 内存 ≥ 8GB（BGE-M3 ONNX 模型 ~2GB） |
| DashScope API Key | 团队统一下发（Day 5 Reranker 可选用） |
| 好奇心 | 想知道"为什么搜得准"而不是"怎么调 API" |

### 验证环境

```powershell
# 确认 Rust 工具链
rustc --version  # ≥ 1.96
cargo --version

# 确认 Docker
docker --version

# 拉取 BGE-M3 模型（讲师会提供本地路径）
ls models/bge-m3-onnx/model.onnx

# 拉取 Qdrant 镜像（Day 3 用）
docker pull qdrant/qdrant
```

---

## 你会学到什么

每天围绕一个关键问题完成原理图、最小 Rust Demo 和简短实验记录。小组共同完成即可，不要求每位成员独立写完整系统。

| 天次 | 主题 | 你会理解或验证什么 |
|------|------|------------|
| Day 1 | **Embedding 原理深挖** | Tokenizer → BGE-M3 → ONNX 优化的完整 EmbeddingEngine |
| Day 2 | **ANN 近似最近邻搜索** | HNSW 搜索过程 Demo + 参数退化实验；完整索引为选做 |
| Day 3 | **Qdrant 内核实战** | 192 组参数实验矩阵 + Dense/Sparse 混合检索 |
| Day 4 | **Chunking + Reranker + 评测** | 4 种分块策略对比 + Reranker 精排 + Bootstrap 显著性检验 |
| Day 5 | **RAG 大作业** | 标书审核知识检索 HTTP API 服务 |

---

## 怎么学

```
Day 1  Embedding 原理 → 手写 ONNX 推理优化 → Benchmark 吞吐量/延迟
Day 2  ANN 算法 → 手工推演 + 简化 HNSW Demo → 对比召回率/延迟
Day 3  Qdrant 内核 → 192 组实验 → 混合检索最佳配置
Day 4  Chunking+Reranker → 评测框架 → 统计显著性检验
Day 5  全链路大作业 → 标书法规检索 HTTP API
```

---

## 代码怎么写

**Demo 自己写，生产组件不要求从零重造。** 必修部分亲手实现最小机制，例如一次分层搜索、一次 RRF 融合或一个指标计算；完整 Tokenizer、HNSW、IVF-PQ 和服务化实现属于骨干选做。

底层库可以调：
- `ort`（ONNX Runtime Rust binding）— Day 1
- `qdrant_client` — Day 3
- `reqwest` + `serde` — Day 5 HTTP 服务
- `tokenizers`（HuggingFace Tokenizers Rust binding）— Day 1

不允许调的：
- LangChain / LlamaIndex 的任何模块
- `faiss` / `annoy` 等现成 ANN 库（Day 2 你自己写 HNSW）
- RAGAS 等现成评估框架（Day 4 你自己写评测指标）

---

## 与项目的关系

```
本课程 → G3 知识检索组
  ├─ Day 1 EmbeddingEngine ──→ G3 的向量化模块
  ├─ Day 2 HNSW 原理 ──→ 理解 Qdrant 的调参（ef/m/M）
  ├─ Day 3 Qdrant 实战 ──→ G3 的向量存储层
  ├─ Day 4 Chunking + 评测 ──→ G3 的文档预处理 + 质量度量
  └─ Day 5 大作业 ──→ G3 的 HTTP API 服务原型

其他组也依赖本课程的产出：
  ├─ G4/G5 Agent ──→ 通过 search_knowledge 工具调用 G3 HTTP API
  └─ G6 知识沉淀 ──→ 向 Qdrant 写入审核后的精华知识
```

---

## 参考资源

- [HNSW 原论文](https://arxiv.org/abs/1603.09320) — Malkov & Yashunin, 2016
- [BGE-M3 论文](https://arxiv.org/abs/2402.03216) — BAAI, 2024
- [Qdrant 源码](https://github.com/qdrant/qdrant) — Rust 写的向量数据库
- [sentence-transformers 文档](https://www.sbert.net/) — Embedding 模型使用指南
- [ANN Benchmarks](https://ann-benchmarks.com/) — 各种 ANN 算法的性能对比
- [MTEB Leaderboard](https://huggingface.co/spaces/mteb/leaderboard) — Embedding 模型评测榜单
