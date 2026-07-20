# Day 3：Agentic RAG + 推理链 — ReWOO、知识注入、可追溯性

> Agent 通用课你学了"Agent 可以调用搜索工具"。今天你理解为什么 ReWOO 比 ReAct 省 40% token、检索结果用什么格式注入上下文最能防幻觉、推理链的四步怎么逐段验证。

---

## 学习目标

1. 对比 ReWOO 与标准 ReAct 的 token 消耗和任务完成率
2. 掌握三种知识注入格式（纯文本/结构化 JSON/引用标注）及各自适用场景
3. 实现推理链四步验证器（Observation→Evidence→Rule→Conclusion）

---

## 核心概念

### 1. ReWOO — 规划与执行分离

ReAct 的问题：每次工具调用后 LLM 都要做一次推理（Thought），如果审查需要 5 次工具调用→5 次 LLM 推理→5 次 token 消耗。

ReWOO 的解法：把规划和执行分开。

```
Phase 1: Plan（LLM 推理 1 次）
  输入: "审查这份招标文件的合规性"
  输出: [
    { step: 1, tool: "search_knowledge", args: { query: "建筑工程资质要求" } },
    { step: 2, tool: "read_section", args: { clause_id: "cl_042" } },
    { step: 3, depends_on: [1, 2], tool: "output_finding", ... }
  ]

Phase 2: Execute（无 LLM 推理，直接执行工具）
  Step 1: search_knowledge → 返回法规列表
  Step 2: read_section → 返回条款原文
  Step 3: output_finding → 基于前两步结果输出结论

Phase 3: Solve（LLM 推理 1 次——可选）
  汇总所有工具结果 → 生成完整审核报告
```

对比 ReAct（3 次工具调用 = 3 次 LLM 推理），ReWOO 只需 1-2 次 LLM 推理。但代价是灵活性降低——如果 Step 1 返回的结果表明需要搜另一个关键词，ReWOO 无法动态调整（Plan 已经写死了）。ReAct 可以。

**适用判断**：
- 审查任务明确、步骤可预测（如"审查资质条款"）→ ReWOO
- 审查任务开放、需要探索（如"找出所有可能的合规问题"）→ ReAct

### 2. 推理链可追溯性

```rust
pub struct ReasoningChain {
    pub observation: Option<ObservationStep>,  // "观察到什么"
    pub evidence: Option<EvidenceStep>,        // "有什么证据"
    pub rule: Option<RuleStep>,                // "依据什么法规"
    pub conclusion: Option<ConclusionStep>,    // "得出什么结论"
}

pub struct ChainValidator;

impl ChainValidator {
    pub fn validate(&self, chain: &ReasoningChain, doc: &Document, neo4j: &Neo4jClient) -> ChainReport {
        // Step 1: Observation — clause_id 对应的原文中是否包含这段文本？
        let obs_valid = chain.observation.as_ref().map_or(false, |obs| {
            let original = doc.get_clause_text(&obs.clause_id);
            fuzzy_match(&obs.source_quote, &original) > 0.85
        });

        // Step 2: Evidence — evidence_id 是否在工具调用历史中存在？
        let ev_valid = chain.evidence.as_ref().map_or(false, |ev| {
            tool_call_history.contains_key(&ev.evidence_id)
        });

        // Step 3: Rule — 引用的法规条款是否真实存在？
        let rule_valid = chain.rule.as_ref().map_or(false, |rule| {
            neo4j.law_article_exists(&rule.law_name, &rule.article_number)
        });

        // Step 4: Conclusion — 前三步都有效 + 结论与证据一致
        let conc_valid = obs_valid && ev_valid && rule_valid;

        ChainReport { obs_valid, ev_valid, rule_valid, conc_valid }
    }
}
```

推理链验证的价值：不是给 Agent 打分——是给 Agent 的**每一条结论**标注"可追溯"或"不可追溯"。不可追溯的结论→高风险幻觉→不建议直接写入审核报告。

### 3. 知识注入格式

检索到的法规如何注入 Agent 上下文？三种格式对比：

```rust
// 格式 A：纯文本（最省 token）
// "以下是相关法规：\n建筑业企业资质管理规定第三条：从事建筑活动的企业应当..."
// → 130 tokens。但 Agent 需要自己提取法规名/条款号

// 格式 B：结构化 JSON（适中）
// {"law": "建筑业企业资质管理规定", "article": "3", "text": "...", "relevance": 0.94}
// → 180 tokens。Agent 可以直接引用 `law_name` 和 `article_number`——减少幻觉

// 格式 C：引用标注（最强防幻觉）
// "[来源: law_042, 建筑业企业资质管理规定, 第三条] 从事建筑活动的企业应当..."
// → 160 tokens。Agent 只需复制 `law_042` 到 output_finding 的 law_ref 字段
```

项目实际用 **格式 C**——检索结果中直接附带 `law_id` + `chunk_id`。Agent 的 System Prompt 强制要求：引用法规时使用结果中的 `law_id`，不要自己编造。

---

## 动手

### 任务 1：ReWOO vs ReAct 对比实验

同一个审查任务（3 份招标文件，各 5 个已知问题）。分别用 ReAct 和 ReWOO 模式审查。对比：token 消耗、审查时间、发现的问题数量、漏检率。

### 任务 2：推理链验证器

解析 Agent 的输出（JSON 格式），提取 Observation/Evidence/Rule/Conclusion 四步。对每步做验证（原文匹配、法规存在性、证据 ID 有效性）。输出验证报告——哪些结论可追溯，哪些不可追溯。

### 任务 3：知识注入格式对比

同一个 Agent，分别注入纯文本、结构化 JSON、引用标注三种格式的检索结果。对比：Agent 输出中的 `law_ref` 准确率（引用格式 C 预期最高）。

---

## 验收标准

- [ ] ReWOO vs ReAct 对比报告：token 消耗 + F1 对比
- [ ] 推理链验证器：至少正确识别 1 条"不可追溯"结论
- [ ] 知识注入格式对比：引用标注格式的 law_ref 准确率 > 90%

---

## 思考题

1. ReWOO 的 Plan 阶段输出了步骤列表——如果 LLM 生成了一个语法错误的步骤（如工具名拼错），Execute 阶段怎么处理？
2. 推理链验证的 `fuzzy_match` 阈值设 0.85——太高会标记真实引用为"不可追溯"（假阳性），太低会放过幻觉。你怎么确定最优阈值？
3. 引用标注格式要求每条检索结果都有 `law_id`。如果 G3 检索返回的结果缺少 `law_id`——Agent 还能引用吗？
