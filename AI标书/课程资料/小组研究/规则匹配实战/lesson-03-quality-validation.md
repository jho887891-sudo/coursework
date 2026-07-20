# Day 3：规则质量验证 — Golden Standard + 召回率 + 误报分析

> "我写的 50 条规则在 3 份标书上跑了——匹配了 200 条结果。怎么知道哪些是真问题、哪些是误报？"今天你构建 Golden Standard——人工标注每条 clause 应该被哪条规则命中——然后用数据而不是感觉来衡量规则质量。

---

## 学习目标

1. 构建 Golden Standard——人工标注 3 份招标文件
2. 计算召回率/精确率/F1——按规则类型分列
3. 分析每条误报和漏检的根因——改进规则
4. 检测规则冲突——同一 clause 上两条规则的匹配结果矛盾

---

## 核心概念

### 1. Golden Standard 构建

```json
// golden/expected_matches.json
[
  {
    "document_id": "doc_sha256_xxx",
    "clause_id": "cl_a3f2b1c8",
    "clause_text": "投标人须在本市注册成立三年以上",
    "expected_rules": [
      {"rule_id": "DISC-001", "should_match": true},
      {"rule_id": "QUAL-003", "should_match": false}
    ]
  }
]
```

**标注粒度**：`(clause_id, rule_id)` 对。每个 clause 对于每条相关规则都有"应该匹配"或"不应该匹配"的判定。

**标注量**：3 份招标文件 × 平均 30 个 clauses × 100 条规则 = 9000 对——但大部分是"不应该匹配"（无关）。实际工作量：每份文件标注 15-30 条"应该匹配"的 `(clause, rule)` 对 + 抽查 10 条"不应该匹配"的验证。约 2-3 小时/人。

### 2. 召回率 + 精确率计算

```rust
pub struct RuleEval {
    tp: usize,  // Agent 匹配了，人工标注也说应该匹配
    fp: usize,  // Agent 匹配了，但人工标注说不应该匹配 → 误报
    fn: usize,  // Agent 没匹配，但人工标注说应该匹配 → 漏检
}

impl RuleEval {
    pub fn compute(golden: &[ExpectedMatch], actual: &[RuleMatch]) -> Self {
        let mut eval = RuleEval { tp: 0, fp: 0, fn: 0 };

        for expected in golden {
            let actual_matched = actual.iter()
                .any(|m| m.rule_id == expected.rule_id && m.clause_id == expected.clause_id);

            match (expected.should_match, actual_matched) {
                (true, true) => eval.tp += 1,
                (false, true) => eval.fp += 1,   // 误报！
                (true, false) => eval.fn += 1,    // 漏检！
                (false, false) => {}              // 正确跳过
            }
        }

        eval
    }

    pub fn recall(&self) -> f64 {
        self.tp as f64 / (self.tp + self.fn) as f64
    }

    pub fn precision(&self) -> f64 {
        self.tp as f64 / (self.tp + self.fp) as f64
    }

    pub fn f1(&self) -> f64 {
        let p = self.precision();
        let r = self.recall();
        if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
    }
}
```

**G2 的铁律**：Recall > 95%（宁可误报，不可漏检）。漏检 = Agent 不会审查这个条款 → 废标风险逃逸。误报 = Agent 多审查一个条款 → 多花几秒 + 几个 token。成本完全不对等。

### 3. 误报根因分析

最高频的误报根因分类：

| 根因 | 占比 | 示例 | 修复 |
|------|------|------|------|
| 未加行业分类 | 40% | "施工总承包资质"规则在 IT 采购招标中触发 | 加 `industry: "建筑工程"` |
| 未加触发条件 | 25% | "排斥性条款"规则在"定义"章节触发（章节只是解释术语） | 加 `trigger: { chapter_keywords: ["资格要求"] }` |
| 正则过于宽泛 | 20% | `.*限制.*` 匹配到"不受限制"（否定表述） | 正则加否定断言 `(?<!不)限制` |
| 关键词单字命中 | 15% | "要求" 在非法律语境中命中 | 用分词替代裸 contains |

### 4. 规则库迭代方法

Golden Standard 不是一次性工作——每次加规则、改规则，都要重新跑一遍验证召回率有没有下降。这和 G4 评测框架课的逻辑一致：每次 Prompt 变更→全量跑 Benchmark→不退化才合并。G2 组维护的是规则的质量基准。

```rust
pub fn detect_conflicts(rules: &[Rule], matches: &[RuleMatch]) -> Vec<Conflict> {
    let mut conflicts = vec<>();

    // 按 clause_id 分组
    let by_clause: HashMap<String, Vec<&RuleMatch>> = matches.iter()
        .fold(HashMap::new(), |mut acc, m| {
            acc.entry(m.clause_id.clone()).or_default().push(m);
            acc
        });

    for (clause_id, clause_matches) in &by_clause {
        for i in 0..clause_matches.len() {
            for j in i+1..clause_matches.len() {
                let rule_a = find_rule(rules, &clause_matches[i].rule_id);
                let rule_b = find_rule(rules, &clause_matches[j].rule_id);

                // 两条规则在同一 clause 上一条匹配一条不匹配 → 冲突
                if clause_matches[i].matched != clause_matches[j].matched {
                    conflicts.push(Conflict {
                        clause_id: clause_id.clone(),
                        rule_a: rule_a.id.clone(),
                        rule_b: rule_b.id.clone(),
                    });
                }
            }
        }
    }

    conflicts
}
```

---

## 动手

### 任务 1：构建 Golden Standard

选 3 份招标文件。对每份文件标注 15-30 条 `(clause_id, rule_id)` 的期望匹配结果。用你 Day 2 的引擎跑匹配→对比→计算混淆矩阵。

### 任务 2：误报漏检分析

输出 Top-10 的 fp（误报）和 fn（漏检）。对每条分析根因→修改规则→重新跑→对比改进前后的 Recall/Precision。

### 任务 3：规则迭代

根据误报漏检分析→修改 3 条规则的 patterns 或 conditions→重新跑 Golden Standard→对比改进前后的 Recall/Precision。

---

## 验收标准

- [ ] Golden Standard：3 份文件 × 15+ 标注
- [ ] Recall > 90%（首次，改进前基线）
- [ ] 误报分析：至少定位并修复 3 个主要误报根因
- [ ] 规则冲突：0 条未解决的冲突

---

## 思考题

1. Recall > 95% 是 G2 的铁律——但如果你为了 Recall 把正则写得极其宽泛（`.*`），FP 会爆炸。在什么场景下你接受低 Precision 来保证高 Recall？
2. Golden Standard 的标注——你需要同时标注 `should_match: true` 和 `should_match: false`。后者比前者多得多，怎么高效处理"不应该匹配"的大量 negative 对？
3. 规则引擎的误报和 AI Agent 的误报哪个更严重？为什么？（提示：用户看到规则引擎报告"公告期不足"——他会去核实吗？看到 Agent 报告"该条款存在歧视嫌疑"——他会去核实吗？）

---

## 与标书审核项目的关系

你今天的 Golden Standard = G2 组的"回归测试集"。每次加新规则或改现有规则→跑一遍 Golden Standard→召回率不能下降。这和评测框架课 G4 的 Pipeline 是同一个逻辑——只是 G2 的评测不需要 LLM-as-Judge，是纯确定性匹配。
