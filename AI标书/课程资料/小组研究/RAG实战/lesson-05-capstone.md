# Day 5：RAG 检索链路独立实验

> 前 4 天你研究了影响检索质量的关键机制。今天用独立 Demo 串起“查询→召回→融合→重排→证据输出”，并用小样本解释效果和边界。100 QPS、完整监控和生产部署作为骨干选做。

---

## 目标

搭建一个最小 RAG 检索 Demo，输入少量标书条款查询，返回带来源定位的结构化证据列表。可以使用本地内存数据、Mock Embedding 或小型 Qdrant Collection。

---

## 架构

```
                    POST /api/v1/knowledge/search
                              │
                              ▼
┌─────────────────────────────────────────────────────┐
│                  API Layer (axum/actix-web)          │
│  - Request validation                                │
│  - Rate limiting (Semaphore 并发控制)                  │
│  - Error handling → JSON error response（不 panic！） │
└──────────────────────┬──────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│                  Query Pipeline                       │
│  1. Query → 预处理（trim/truncate/去重）              │
│  2. Cache lookup（LRU, TTL=5min）                    │
│  3. [miss] → EmbeddingEngine（Day 1）               │
│  4. → Qdrant 混合检索（Day 3）                       │
│  5. → Reranker 精排（Day 4）                         │
│  6. → 格式化为 EvidenceSet（对接 G4/G5 Schema）       │
│  7. → 写入 Cache                                     │
└──────────────────────┬──────────────────────────────┘
                       │
                       ▼
              HTTP 200 + EvidenceSet JSON
```

---

## 功能需求

### P0（必做 — 及格线）

| 功能 | 说明 |
|------|------|
| **HTTP API** | `POST /api/v1/knowledge/search` — 接收 query/category/top_k/strategy，返回 EvidenceSet |
| **数据入库** | 500+ 条法规向量 + 元数据在 Qdrant 中，启动时自动检测并创建 Collection（如已存在则跳过） |
| **混合检索** | Dense + Sparse → RRF 或 Linear 融合 → Top-K 候选 |
| **Reranker 精排** | 检索 Top-20 → Reranker batch → 返回 Top-10（Reranker 可配置开关） |
| **错误处理** | Qdrant 不可用 → 503；Embedding 超时 → 降级 BM25；空结果 → 200 + 空列表；参数校验 → 400 |
| **评测验证** | 启动后自动跑 Day 4 的 30 条评测集 → 输出 Recall@5（启动日志打印） |

### P1（进阶 — 加分项）

| 功能 | 说明 |
|------|------|
| **缓存** | 相同 query + category → 缓存命中返回（LRU, capacity=10000, TTL=5min） |
| **并发控制** | Semaphore 限制最大并发 embedding 推理数（防止 OOM） |
| **查询扩展** | 短查询（< 10 字）用 LLM 扩展为 2-3 个变体 → 并行检索 → 合并去重 |
| **A/B 测试框架** | `POST /api/v1/knowledge/eval/ab` — 同时跑两个策略 → 返回对比报告 JSON |
| **召回漂移监控** | 定时任务（每天）跑评测集 → 输出 Recall 变化量 → 降幅 > 5% 时告警日志 |

### P2（探索 — 挑战项）

| 功能 | 说明 |
|------|------|
| **多轮检索** | 支持 `conversation_id` → 结合对话历史中的上下文做检索 |
| **缓存预热** | 启动时加载 Top-100 高频查询的文件 → 预计算 embedding + 检索结果 |
| **Docker 一键部署** | `docker-compose up` 启动 Qdrant + RAG API + 前端 Swagger UI |

---

## 技术约束

| 组件 | 要求 |
|------|------|
| 后端语言 | Rust（核心检索链路必须 Rust） |
| HTTP 框架 | axum 或 actix-web |
| 向量嵌入 | BGE-M3 ONNX 本地推理（复用 Day 1 的 EmbeddingEngine） |
| 向量存储 | Qdrant（Docker 或 embedded mode） |
| Reranker | BGE-Reranker-v2-m3 ONNX 本地推理（Day 4 的 Reranker 模块） |
| 缓存 | 自实现 LRU（不允许用 moka 等现成缓存库——LRU 是基本功） |
| LLM（查询扩展） | DashScope qwen-plus（可选，进阶功能） |

---

## API 规范

### Request

```json
POST /api/v1/knowledge/search
Content-Type: application/json

{
  "query": "投标人须具备建筑工程施工总承包二级及以上资质",
  "category": "法规",
  "top_k": 10,
  "strategy": "hybrid",
  "rerank": true,
  "conversation_id": null
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| query | string | Ture | 查询文本 |
| category | string | False | 过滤类别：法规/案例/负面清单/范本。空 = 全部 |
| top_k | int | False | 返回数量，默认 10，最大 50 |
| strategy | string | False | dense/sparse/hybrid/bm25，默认 hybrid |
| rerank | bool | False | 是否启用 Reranker，默认 true |
| conversation_id | string | False | 多轮对话 ID（P2），默认 null |

### Response

```json
HTTP 200 OK
{
  "evidences": [
    {
      "id": "ev_sha256_a3f2b1c8",
      "title": "建筑业企业资质管理规定",
      "article": "第三条",
      "text": "从事建筑活动的企业应当依法取得建筑业企业资质证书。建筑业企业资质分为施工总承包资质、专业承包资质、施工劳务资质三个序列...",
      "relevance_score": 0.94,
      "source": "qdrant",
      "reranked": true,
      "chunk_id": "ch_00512",
      "law_id": "law_042",
      "category": "法规",
      "effective_date": "2015-03-01"
    }
  ],
  "retrieval_metadata": {
    "query_embedding_ms": 12,
    "qdrant_search_ms": 8,
    "rerank_ms": 45,
    "total_latency_ms": 68,
    "total_candidates": 20,
    "after_rerank": 10,
    "strategy_used": "hybrid_rrf_k60",
    "cache_hit": false
  }
}
```

### Error Responses

```json
HTTP 400  // 参数校验失败
{ "error": "invalid_request", "message": "top_k must be between 1 and 50" }

HTTP 503  // Qdrant 不可用
{ "error": "service_unavailable", "message": "Vector database connection failed. Retrying..." }

HTTP 504  // Embedding 超时
{ "error": "gateway_timeout", "message": "Embedding timed out after 2000ms. Try a shorter query." }
```

---

## 验收标准（权重制）

### 自动化验收（讲师脚本跑）

| 验收项 | 权重 | 怎么测 |
|--------|------|--------|
| 编译 + clippy 0 warning | 10% | `cargo clippy -- -D warnings` |
| 代码五层分离 | 10% | 代码审查：api / pipeline / embedding / search / rerank 各一个 module |
| 500 条法规入库 | 10% | `GET /api/v1/knowledge/stats` → `total_documents >= 500` |
| 端到端 P99 < 500ms | 15% | `wrk -t4 -c50 -d30s -s search.lua` 压测，P99 从 latency histogram |
| 30 条评测集 Recall@5 ≥ 0.85 | 20% | 跑 Day 4 的评测框架（以评测集的 ground truth 为准） |
| 缓存命中 < 10ms | 10% | 同一 query 连续请求 3 次，第 3 次 metadata.cache_hit=true, latency < 10ms |
| 错误处理完备 | 10% | 手动 kill Qdrant → 503；发 top_k=999 → 400；空 query → 400 |
| A/B 测试可运行 | 10% | `POST /api/v1/knowledge/eval/ab` body: `{strategies: ["rrf", "linear"], queries: [...]}` |
| 召回漂移监控 | 5% | 修改 Qdrant 数据 → 手动触发 `POST /api/v1/knowledge/eval/drift-check` → Recall 变化 |

### 代码审查

| 维度 | 说明 |
|------|------|
| 模块边界 | API / Pipeline / Embedding / Search / Rerank 五层各一个 Rust module，禁止跨层隐式依赖 |
| 错误传播 | 不用 unwrap——全部用 anyhow::Result + ? 传播。错误信息包含上下文 |
| 配置管理 | 所有可调参数（Qdrant URL、batch size、ef、α 权重）从 .env 或 CLI 读取，不硬编码 |
| README | 含架构图（ASCII art 即可）、启动方式、设计决策及原因 |

---

## 设计决策文档（README 中必写）

大作业的 README 不同于"怎么运行"，它需要回答你为什么这样设计：

1. **为什么选 RRF 而不是 Linear 融合？**
   - 在你的评测数据上两者的 Recall 差异是多少？Bootstrap CI 是否重叠？

2. **Chunk Size 为什么选这个值？**
   - Day 4 的参数敏感性分析 → 最优 chunk size 是多少？为什么不更大/更小？

3. **Qdrant 的 HNSW 参数为什么这样设？**
   - M / ef_construct / ef 的实际值 + 在你数据上的 Benchmark 数据支撑

4. **缓存策略的 trade-off**
   - LRU capacity 为什么是 10000？TTL 为什么是 5min？如果改为 1min 会怎样？

5. **一个你遇到的技术难点**
   - 讲一个具体的问题 + 你是如何定位 + 如何解决的（200 字即可）

---

## 时间建议

| 时间段 | 做什么 |
|--------|--------|
| Day 5 上午 | 架构设计 + 搭建骨架（API 框架 + 模块 stub + 错误定义） |
| Day 5 下午 | 核心 Pipeline：embedding → search → rerank → format |
| Day 5 傍晚 | 缓存 + 并发控制 + 错误处理 |
| Day 5 晚上 | A/B 测试 + 召回漂移监控 + 评测验证 |
| 提交 | 代码 + README + 评测报告 |

---

## 提示

- **先跑通 P0 全链路**，哪怕 Recall 只有 0.5。链跑通了再优化指标。
- **从 Day 1-4 的代码直接复用**——EmbeddingEngine、HnswIndex、Chunker、Reranker、EvalMetrics 都可以封装为 crate。
- **用 tracing 打桩定位瓶颈**——`tracing::info_span!("search", query = %query)` 包装每一步，P99 瓶颈一目了然。
- **Recall 不达标时二分排查**：embedding 质量（Day 1）→ chunking 策略（Day 4）→ 检索参数（Day 3）→ reranker（Day 4）。逐层排除。
- **引用是灵魂**——每条 Evidence 必须包含 `chunk_id` 和 `law_id`，让 Agent 能追溯到原文。没有溯源能力的检索结果 = 不可信的 AI 输出。

---

## 与标书审核项目的关系

这个实验用于理解知识检索 API、EvidenceSet 和检索 Pipeline 的核心职责。课程 Demo 不直接成为项目接口契约，也不直接迁移到现有服务；后续项目开发需重新确认 Schema、数据规模、性能和安全要求。

```
G3 组开发仓库
├── src/
│   ├── embedding/    ← 你的 EmbeddingEngine（Day 1）
│   ├── search/       ← 你的混合检索 Pipeline（Day 3+4）
│   ├── chunker/      ← 你的结构感知分块器（Day 4）
│   ├── rerank/       ← 你的 Reranker 模块（Day 4）
│   ├── eval/         ← 你的评测框架（Day 4）
│   ├── api/          ← 你的 HTTP API（Day 5）
│   └── ab/           ← 你的 A/B 测试框架（Day 5）
```

G4/G5 的 Agent 通过 `search_knowledge` 工具调用你的 HTTP API——你提供的是一个让 Agent"变聪明"的基础设施。
