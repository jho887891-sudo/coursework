# Day 2：规则引擎 + AI Agent 协作

> Day 1 你写了 10 条 YAML 规则。今天你用 ~150 行 Rust 让它们跑起来——不是造一个复杂的匹配引擎，而是理解规则引擎和 AI Agent 怎么分工：引擎审"对/错"，Agent 审"好/坏"。

---

## 学习目标

1. 用 ~150 行 Rust 实现三合一匹配器（正则+关键词+字段比较）
2. 理解 RuleMatch[] 怎么转化为 Agent 的 System Prompt 上下文
3. 掌握分工边界——什么能规则化、什么只能交给 Agent

---

## 核心概念

### 1. 匹配器实现 —— 不是造引擎，是写规则

三个匹配器，每个 ~50 行。不要过度设计。

```rust
use regex::Regex;
use serde::Deserialize;

pub struct RuleEngine {
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    rule: Rule,                          // YAML 规则
    regex_patterns: Vec<Regex>,          // 预编译的正则
}

impl RuleEngine {
    /// 加载 YAML → 编译正则
    pub fn load(rules_dir: &str) -> Result<Self> {
        let rules = load_yaml_rules(rules_dir)?;
        let compiled = rules.into_iter().map(|rule| {
            let regex_patterns: Vec<Regex> = rule.patterns.iter()
                .filter(|p| p.pattern_type == "regex")
                .map(|p| Regex::new(&p.value).unwrap())
                .collect();
            CompiledRule { rule, regex_patterns }
        }).collect();
        Ok(RuleEngine { rules: compiled })
    }

    /// 对整份文档运行所有规则
    pub fn run(&self, doc: &ParsedDocument) -> Vec<RuleMatch> {
        let mut matches = vec![];

        for compiled in &self.rules {
            let rule = &compiled.rule;

            // ① 检查适用条件——不满足则跳过
            if !self.check_conditions(rule, doc) {
                continue;
            }

            // ② 对每个 clause 运行匹配
            for clause in &doc.clauses {
                let matched = self.match_clause(compiled, clause);
                if matched {
                    matches.push(RuleMatch {
                        rule_id: rule.id.clone(),
                        clause_id: clause.id.clone(),
                        severity: rule.severity.clone(),
                        category: rule.category.clone(),
                        suggestion: rule.suggestion.clone(),
                        law_ref: format!("《{}》{}", rule.source.law, rule.source.article),
                        matched_text: clause.text.clone(),
                    });
                }
            }
        }

        // ③ 去重 + 按 severity 排序
        matches.sort_by_key(|m| match m.severity.as_str() {
            "critical" => 0, "warning" => 1, _ => 2,
        });
        matches
    }

    /// 单个 clause 是否匹配某条规则
    fn match_clause(&self, compiled: &CompiledRule, clause: &Clause) -> bool {
        let rule = &compiled.rule;

        match rule.check.as_str() {
            "any_match" => {
                // 任一正则命中 OR 任一关键词命中
                let regex_hit = compiled.regex_patterns.iter()
                    .any(|re| re.is_match(&clause.text));
                let keyword_hit = rule.patterns.iter()
                    .filter(|p| p.pattern_type == "keyword")
                    .any(|p| clause.text.contains(&p.value));
                regex_hit || keyword_hit
            }
            "all_match" => {
                // 所有正则命中 AND 所有关键词命中
                let regex_all = compiled.regex_patterns.iter()
                    .all(|re| re.is_match(&clause.text));
                let keyword_all = rule.patterns.iter()
                    .filter(|p| p.pattern_type == "keyword")
                    .all(|p| clause.text.contains(&p.value));
                regex_all && keyword_all
            }
            _ => false,
        }
    }

    /// 检查规则的适用条件
    fn check_conditions(&self, rule: &Rule, doc: &ParsedDocument) -> bool {
        let cond = &rule.conditions;

        // ① 排除条件先检查——命中了直接跳过（优先级最高）
        if let Some(exclude) = &cond.exclude {
            if let Some(project_types) = &exclude.project_types {
                // 从文档中提取项目类型（通常在招标公告的第一段）
                let doc_type = extract_project_type(doc);
                if project_types.iter().any(|pt| doc_type.contains(pt)) {
                    return false;  // 命中排除条件→不触发规则
                }
            }
        }

        // ② 文档类型匹配
        if let Some(doc_type) = &cond.document_type {
            if !doc.file_name.contains(doc_type) {
                return false;
            }
        }

        // ③ 章节触发条件
        if let Some(trigger) = &cond.trigger {
            if let Some(keywords) = &trigger.chapter_keywords {
                let any_chapter_has = doc.chapters.iter()
                    .any(|ch| keywords.iter().any(|kw| ch.title.contains(kw)));
                if !any_chapter_has {
                    return false;  // 文档中没有触发的章节→不检查这条规则
                }
            }
        }

        true
    }
}

/// 从 ParsedDocument 中提取项目类型（招标公告第一段通常包含）
fn extract_project_type(doc: &ParsedDocument) -> String {
    doc.clauses.first()
        .map(|c| c.text.clone())
        .unwrap_or_default()
}
```

**总共约 100 行**。不需要 RegexSet、不需要 jieba 分词、不需要 Pipeline 抽象。`contains` 对中文关键词（通常 ≥ 4 字如"排斥性条款"）够用了——不会把"排斥"误匹配到"排斥性条款"以外的上下文，因为中文单字几乎没有独立的语义。

**性能边界**：100 条规则 × 500 clauses 的简单循环 < 1ms。当规则数量到 1000+ 时，`regex::Regex` 预编译 + 按 `target` 字段（`all_clauses` vs `chapter_titles` vs `table_values`）对 clauses 做预分组——避免对表格数据跑"章节标题"的正则。但那是 Day 5 大作业的锦上添花，不是基础要求。

### 2. RuleMatch → Agent 上下文 —— 本课核心

引擎的输出不是终点。它是 Agent 的输入。

```rust
/// 将引擎的匹配结果转化为 Agent System Prompt 的上下文段落
pub fn build_agent_context(matches: &[RuleMatch], doc: &ParsedDocument) -> String {
    let criticals: Vec<_> = matches.iter().filter(|m| m.severity == "critical").collect();
    let warnings: Vec<_> = matches.iter().filter(|m| m.severity == "warning").collect();

    let mut ctx = String::new();
    ctx.push_str("## 初审结果（规则引擎自动检测）\n\n");
    ctx.push_str(&format!(
        "已标记 {} 个问题（critical: {}, warning: {}）。",
        matches.len(),
        criticals.len(),
        warnings.len()
    ));
    ctx.push_str("以下问题已通过确定性规则确认，请直接引用到审核报告中。\n\n");

    // 列出已确认的问题
    for m in matches {
        ctx.push_str(&format!(
            "- [{}] {}: {}\n  依据: {}\n  建议: {}\n\n",
            m.severity.to_uppercase(),
            m.category,
            m.matched_text.chars().take(100).collect::<String>(),
            m.law_ref,
            m.suggestion,
        ));
    }

    // 指出需要 Agent 深度审查的内容
    ctx.push_str("## 需要深度语义审查的条款\n\n");
    ctx.push_str("以下条款不在规则引擎的覆盖范围内，请进行语义分析：\n");

    // 找未被规则引擎标记的 clauses（可能包含 B 类问题）
    let matched_ids: HashSet<_> = matches.iter().map(|m| &m.clause_id).collect();
    let unreviewed: Vec<_> = doc.clauses.iter()
        .filter(|c| !matched_ids.contains(&c.id))
        .take(10)  // 只列前 10 条
        .collect();

    for clause in unreviewed {
        ctx.push_str(&format!("- 第 {} 条: {}...\n",
            clause.page,
            clause.text.chars().take(80).collect::<String>()
        ));
    }

    ctx
}
```

**为什么这样做**：Agent 拿到的 System Prompt 不再是一份"裸文档"——它知道哪些条款规则引擎已经确认了（直接写入报告），哪些需要它深度分析（它唯一需要思考的部分）。Agent 的 token 消耗集中在有价值的地方。

### 3. 分工边界 —— 什么能规则化、什么不能

**判断标准：这条法条的判定是否能用"是/否"回答？**

```
✅ 能规则化（客观可量化）：
  "公告期是否 ≥ 20 日？"               → 提取日期 → 比较 → 是/否
  "报价是否超过预算上限？"              → 提取金额 → 比较 → 是/否
  "安全生产许可证是否在有效期内？"       → 提取日期 → 比较 → 是/否
  "是否明确资质等级（一级/二级/三级）？" → 正则匹配 → 是/否
  "是否存在地域限制关键词？"            → 关键词 contains → 是/否

❌ 不能规则化（需要语义理解）：
  "是否构成歧视？"                      → 不只是"有没有"关键词，是"意图"问题
  "资质要求是否过高？"                  → 需要和项目规模对比 + 行业标准
  "'相应资质'是否为模糊表述？"          → 需要上下文判断
  "评分标准是否公平？"                  → 主观判断
  "是否存在围标串标的嫌疑？"            → 需要跨文档推理+行为模式
```

**现场讨论**：给出 10 条真实法条，学员用"是/否"标准判断适合规则引擎还是 Agent。讨论后再揭示标答。

---

## 动手

### 任务 1：实现匹配器（~80 行）

加载 Day 1 的 10 条规则→实现 `RuleEngine::load()` + `run()`→输入样本 ParsedDocument→验证匹配结果。

### 任务 2：build_agent_context（~30 行）

把匹配结果转化为一段 System Prompt 上下文文本。验证：拿给同学看——能否一眼知道"哪些已经确认了、哪些还需要审"。

### 任务 3：分工边界讨论

拿 10 条法条原文，逐条标注"适合规则引擎"还是"只能交给 Agent"。每标注一条写一句话理由。

---

## 验收标准

- [ ] 匹配器正确加载 YAML 规则并输出 RuleMatch[]
- [ ] `build_agent_context` 输出格式清晰——已确认 vs 待审查分列
- [ ] 分工边界：10 条法条中至少 7 条标注正确

---

## 思考题

1. "投标人须具备相应的施工资质"——这条适合规则引擎（检测"相应的"这个模糊词）还是 Agent（判断在上下文中是否合理）？为什么？
2. 如果规则引擎把"疑似问题"的条款也标记了（不只是确定性问题），Agent 能在 System Prompt 中区分"已确认"和"疑似"吗？你怎么设计 RuleMatch 的字段来支持这种区分？
3. 规则引擎的结果要不要直接展示给用户？还是只能给 Agent 看？为什么？

---

## 与标书审核项目的关系

你今天的 `build_agent_context` 函数就是 G2→G4 的接口。G4 Agent 的 System Prompt 中有这样一段文字——"初审结果：规则引擎已确认 12 个问题。请对以下 5 个条款进行语义审查。"——这段文字就是你的函数生成的。规则引擎的价值不在"自己能审"，在"让 Agent 审得更好"。
