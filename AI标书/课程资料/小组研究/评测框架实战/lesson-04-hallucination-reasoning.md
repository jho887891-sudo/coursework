# Day 4：幻觉检测与推理质量

> Agent 说"依据《招标投标法》第 25 条，该条款构成排斥性限制"。第 25 条真的讲的是排斥性限制吗？还是 Agent 编的？今天你学会验证——不只是"对不对"，而是"每一步有没有据可查"。

---

## 学习目标

1. 实现幻觉三分类检测器（法规幻觉/原文幻觉/数值幻觉）
2. 验证推理链的四步可追溯性（Observation→Evidence→Rule→Conclusion）
3. 检查四种引用的准确性（clause_id/law_ref/source_quote/evidence_id）

---

## 核心概念

### 1. 幻觉三分类

#### 法规幻觉 — "这部法律不存在"

```
Agent 输出: "依据《建设工程施工许可管理办法》第 15 条"
                                    ↑
                    这部法规在 Neo4j 中存在吗？第 15 条的内容是什么？

检测 Pipeline:
  1. 正则提取法规引用: r"《(.+?)》第\s*([\d]+)\s*条"
  2. Neo4j: MATCH (l:Law) WHERE l.name CONTAINS '建设工程施工许可管理办法'
     → 没找到 → 法规不存在 幻觉！
  3. 找到了 → 检查第 15 条是否存在
     MATCH (l:Law {law_id: 'law_xxx'})-[:HAS_ARTICLE]->(a:Article {article_number: '15'})
     → 没找到 → 条款编号幻觉！
  4. 找到了 → 检查第 15 条的 text 是否与 Agent 引用的内容一致
     → 不一致 → 条款内容幻觉！
```

三段检测：法规存在性 → 条款存在性 → 条款内容一致性。

#### 原文幻觉 — "招标文件说..."

```
Agent 输出: "招标文件第 5 页要求投标人须在东莞设立分支机构三年以上"
           clause_id="b_5_2", source_quote="投标人须在东莞设立分支机构三年以上"

检测 Pipeline:
  1. 通过 clause_id 找到原文: document.blocks["b_5_2"].text
  2. 子串匹配: source_quote 是否出现在原文中？
     → 不出现 → 可能是原文幻觉
  3. 语义匹配（降级）: 计算 source_quote 和原文的 embedding cosine
     → similarity < 0.85 → 原文幻觉
     → similarity > 0.95 → 应该是同义改写（不算幻觉）
```

原文幻觉的微妙之处：Agent 可能"总结"了原文而非引用原文。如果 System Prompt 要求"逐字引用"→ 任何不是原文逐字摘录的都是幻觉。如果允许总结 → 只有完全错误的才是幻觉。

#### 数值幻觉 — "预算 5000 万元"

```
Agent 输出: "该项目的预算上限为 5000 万元" (原文是 500 万元)

检测 Pipeline:
  1. 提取 Agent 输出中的所有数字（含单位）
  2. 提取原文中的所有数字（含单位）
  3. 对 Agent 的每个数字: 检查是否在原文中出现过
     → 不在原文中 → 可能来自 LLM 的"推理"（如 3000 + 2000 = 5000）
     → 无法从输入数据中推导 → 数值幻觉
```

简单的数值比较不够——Agent 可以从输入数字计算出新数字（如全年费用 = 月费 × 12）。需要区分"合理的推导"和"凭空捏造"。

#### 两阶段检测策略

```
Phase 1: 正则粗筛 (recall-oriented)
  → 找出所有"看起来像"法规引用 / 原文引用 / 数值的文本
  → 目标: recall > 99% (几乎不漏)
  → 代价: 很多假阳性（正常的引用也可能被误标）

Phase 2: LLM 精判 (precision-oriented)
  → 对 Phase 1 的所有候选做逐一验证
  → Prompt: "以下引用是否真实？法规:《XX法》第Y条。请验证..."
  → 目标: precision > 90%
  → 代价: 每条验证需要 1 次 LLM 调用（~500ms）
```

---

### 2. 推理链可追溯性

标书审核的理想推理链应该有四步。每一步都有对应的验证方法：

```
Observation（观察到的事实）
  "招标文件第 12 页要求投标人注册资本不低于 1 亿元"
  验证: clause_id 对应的原文中是否包含这个事实？
       子串匹配或语义匹配。

Evidence（证据）
  "行业标准中，同类项目的注册资本要求通常在 100-500 万元"
  验证: Evidence 是否来自 search_knowledge 工具的返回值？
       检查 evidence_id 是否在工具调用历史中存在。

Rule（法规依据）
  "《政府采购法》第 22 条规定，采购人不得以不合理的条件对供应商实行差别待遇"
  验证: 法规引用是否真实？→ 调用 Day 4 的法规幻觉检测。

Conclusion（结论）
  "该注册资本要求构成排斥性条款（severity=critical）"
  验证: Conclusion 是否从 Observation + Evidence + Rule 中逻辑推导而来？
       这一步最难自动化——需要 LLM-as-Judge 判断逻辑是否成立。
```

四步中缺失任何一步 → 推理链不完整，结论的可信度下降。

---

### 3. 引用准确性

Agent 输出中四种 ID 的正确性验证：

| 引用字段 | 验证方法 | 失败后果 |
|----------|----------|----------|
| `clause_id: "b_5_2"` | 在 ParsedDocument.blocks 中查找该 ID | 前端无法高亮原文位置 |
| `law_ref: "《政府采购法》第22条"` | Neo4j 存在性 + 内容一致性检查 | 法律依据不可信 |
| `source_quote: "投标人须在东莞..."` | 与 clause_id 对应原文做子串/语义匹配 | 引用的原文可能被篡改 |
| `evidence_id: "ev_sha256_a3f2"` | 在 G3 EvidenceSet 中查找该 ID | 证据来源不明确 |

---

## 动手

### 任务 1：实现幻觉三分类检测器

```rust
struct HallucinationDetector {
    neo4j: Neo4jClient,
    doc_cache: DocumentCache,
    llm_judge: LlmJudge,
}

impl HallucinationDetector {
    async fn detect_law_hallucination(&self, law_ref: &str) -> Option<Hallucination>;
    async fn detect_source_hallucination(&self, clause_id: &str, quote: &str) -> Option<Hallucination>;
    async fn detect_number_hallucination(&self, agent_output: &str, doc_id: &str) -> Vec<Hallucination>;
    async fn phase2_llm_verify(&self, candidate: &HallucinationCandidate) -> bool;
}
```

### 任务 2：推理链验证器

```rust
fn extract_reasoning_chain(agent_output: &str) -> ReasoningChain {
    ReasoningChain {
        observation: Option<String>,  // "观察到什么"
        evidence: Option<String>,     // "有什么证据"
        rule: Option<String>,         // "依据什么法规"
        conclusion: Option<String>,   // "结论是什么"
    }
}

fn validate_chain(chain: &ReasoningChain, doc: &Document, neo4j: &Neo4jClient) -> ChainReport;
```

### 任务 3：10 条 Agent 输出的幻觉审计

拿 3 个 Agent 各 10 条输出（或讲师提供的已有审核结论），跑你的幻觉检测器 + 推理链验证器。报告：幻觉率（按类型）、推理链完整率（按 Agent）。

---

## 验收标准

- [ ] 幻觉检测器能识别不存在的法规引用（≥ 80% 检出率）
- [ ] 推理链验证器能识别缺失的 Evidence 步骤
- [ ] 10 条审计报告：总幻觉率 + 分类幻觉率 + 推理链完整率
- [ ] 至少发现 1 个真实幻觉案例并分析根因

---

## 思考题

1. 如果一个 Agent 90% 的推理链缺了 Evidence 步骤——这说明什么？（提示：可能是 Evidence 被放在了 Observation 里，也可能是根本没查证据）
2. 数值幻觉检测中，怎么区分"合理的推导"（如总计=各项之和）和"凭空捏造"？
3. 如果你的幻觉检测器假阳性率 30%（Phase 1 之后）——30% 的正常引用被标记为"可能幻觉"。这对用户意味着什么？怎么改进？

---

## 与标书审核项目的关系

G4 方向 B 的核心交付物——你的幻觉检测器 + 推理链验证器 = Agent 输出的"质量门禁"。G4 方向 A 的自动化评测 Pipeline 在算 F1 之外，还要算幻觉率和推理链完整率——这两项是你今天实现的功能直接供给的。

没有幻觉检测，Agent 说"依据《XX 法》"就敢写进审核报告——客户一旦验证发现这条法不存在，整个产品的信任就破产了。
