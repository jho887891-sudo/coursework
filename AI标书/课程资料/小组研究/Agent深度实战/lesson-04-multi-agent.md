# Day 4：Multi-Agent 协作 — Coordinator、AgentBus、SessionGraph

> Agent 通用课你学了"两个 Agent 可以协作"。今天你读项目 Coordinator 源码——7 阶段 Pipeline 每一步的输入输出、AgentBus 的 `broadcast::channel` 怎么路由消息、SessionGraph 怎么管理 11 个 Agent 的动态生命周期。最后跑四种协作策略的 Benchmark。

---

## 学习目标

1. 走读 Coordinator 7 阶段 Pipeline 源码
2. 理解 AgentBus 的发布/订阅模式和消息路由
3. 理解 SessionGraph 的节点生命周期
4. 实现四种协作策略的对比 Benchmark

---

## 核心概念

### 1. Coordinator — 7 阶段 Pipeline

项目 Coordinator 的核心设计：**阶段化、可观测、可裁剪**。

```rust
pub async fn run_coordinator(
    session: &mut Session,
    coord_definition: &CoordinatorDefinition,
    agent_registry: &AgentRegistry,
    tool_registry: &ToolRegistry,
) -> Result<AuditReport> {
    // Phase 1: Document Analysis
    // → 分析文档类型（招标/投标）、项目类型（施工/货物/服务）
    let doc_profile = analyze_document(&session.document).await?;

    // Phase 2: Rule Engine Pre-filter
    // → 用 G2 规则引擎做确定性检查（零 AI 调用）
    let rule_matches = rule_engine.check(&session.document)?;

    // Phase 3: Agent Dispatch
    // → 根据文档特征选择 Agent 组合
    let agents = coord_definition.select_agents(&doc_profile);
    // 例如：施工招标 → [JudgeAgent, LegalVerifyAgent, ComplianceAgent]

    // Phase 4: Parallel Review
    // → 多个 Agent 并发审查不同维度
    let findings = tokio::try_join_all(
        agents.iter().map(|agent| spawn_review_agent(agent, &session))
    ).await?;

    // Phase 5: Cross Validation
    // → Agent 之间的结论互审
    let validated = cross_validate(findings, &agent_bus).await?;

    // Phase 6: Aggregation
    // → 去重、合并严重度、排序
    let aggregated = aggregate_findings(validated);

    // Phase 7: Report Generation
    // → 结构化 JSON → 审核报告
    let report = generate_report(aggregated, &session);
    Ok(report)
}
```

### 2. AgentBus — tokio::sync::broadcast

```rust
pub struct AgentBus {
    tx: broadcast::Sender<AgentMessage>,
}

#[derive(Clone, Debug)]
pub enum AgentMessage {
    FindingReported { agent: String, finding: AuditFinding },
    ReviewProgress { agent: String, progress: f64, stage: String },
    AgentError { agent: String, error: String },
    ReviewComplete { agent: String },
}

impl AgentBus {
    pub fn subscribe(&self) -> broadcast::Receiver<AgentMessage> {
        self.tx.subscribe()
    }

    pub fn broadcast(&self, msg: AgentMessage) {
        let _ = self.tx.send(msg);  // 忽略"没有订阅者"的错误
    }
}
```

**为什么用 broadcast 而不是 mpsc**：多个消费者需要同时收到消息——前端 SSE 需要推送进度、Coordinator 需要监控错误、SessionGraph 需要记录生命周期事件。broadcast 是"一份消息、N 个订阅者"，mpsc 是"一份消息、一个消费者"。

**"忽略没有订阅者"的哲学**：`let _ = self.tx.send(msg)`——AgentBus 不关心谁在听。如果前端断开了 SSE，不影响 Agent 执行。这是"fire-and-forget"语义。

### 3. SessionGraph — 动态 Agent 生命周期

```rust
pub struct SessionGraph {
    nodes: HashMap<String, SessionNode>,
    edges: Vec<(String, String, RelationType)>,
}

pub struct SessionNode {
    pub agent_id: String,
    pub agent_type: String,
    pub status: AgentStatus,  // Idle → Running → Completed / Failed
    pub started_at: Instant,
    pub children: Vec<String>,
}
```

SessionGraph 追踪一次审核的全过程——哪些 Agent 被创建了、每个 Agent 的状态、Agent 之间的依赖关系。当 Coordinator 需要等 3 个子 Agent 都完成才能进入 Phase 5 时——查询 SessionGraph 找到 children→等待全部 `Completed`。

### 4. 协作策略对比实验

你必须在同一份 Benchmark 上跑四种策略：

```
Pipeline(串行):
  JudgeAgent → LegalVerifyAgent → ComplianceAgent
  延迟 = t1 + t2 + t3
  成本 = cost1 + cost2 + cost3
  F1: 中等（每步有上下文但串行无争议）

Parallel-Vote(并行投票):
  3 个 JudgeAgent 独立审查 → 多数投票
  延迟 = max(t1, t2, t3)
  成本 = 3 × cost
  F1: 较高（冗余审查→少数误判被多数纠正）

Debate(辩论):
  3 个 Agent 互相评论后给结论
  延迟 = N × (review + debate_rounds)
  成本 = N × cost + 辩论 token
  F1: 最高（全面审查+交叉对照）

Single-Agent(基线):
  1 个 Agent 独立审查
  延迟 = t
  成本 = cost
  F1: 最低（无冗余、无校验）
```

**实验要求**：
- 50 条 query × 30 次重复 = 1500 次推理/策略
- 指标：F1 + Bootstrap CI + token 成本 + P99 延迟
- 输出 Pareto 前沿——"花多少钱买多少 F1"

---

## 动手

### 任务 1：Coordinator 精简版

实现 3 阶段的 Coordinator（Dispatch→Parallel Review→Aggregate）。用 3 个 Agent 审查同一份招标文件。

### 任务 2：AgentBus 消息路由

实现 AgentBus。3 个 Agent 通过 AgentBus 报告进度→Coordinator 收集→打印到终端。验证 "fire-and-forget" 语义——kill 打印端后 Agent 不报错。

### 任务 3：协作策略 Benchmark

跑 Pipeline / Parallel-Vote / Debate / Single-Agent 四种策略。输出 F1+成本+延迟 三维对比 + Pareto 前沿分析。

---

## 验收标准

- [ ] Coordinator 3 阶段可用 + AgentBus 消息正确路由
- [ ] 四种协作策略 Benchmark 报告：含 Bootstrap CI + Pareto
- [ ] 至少 1 个策略优于 Single-Agent 基线（Bootstrap CI 验证）

---

## 思考题

1. Coordinator 的 Phase 4（Parallel Review）用 `tokio::try_join_all`。如果其中一个 Agent panic→`try_join_all` 会取消其他 Agent。这个行为是期望的吗？
2. AgentBus 用 `broadcast::channel`——如果订阅者处理速度慢，消息积压到 channel 满了。`send()` 会报错吗？怎么处理背压？
3. SessionGraph 的 `children` 字段追踪了子 Agent 依赖。如果子 Agent A 依赖子 Agent B 的结论——但 B 失败了（Failed）。A 应该怎么办？
