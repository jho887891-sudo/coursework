# Day 4：AI 辅助规则生成 — 从 10 条到 100 条

> 你已经用少量规则理解了领域建模。今天研究 LLM 如何批量生成规则初稿，并用抽查、来源核验和边界 Case 识别它的错误。课程实验不追求凑满 100 条，生产规则扩充属于后续项目工作。

---

## 学习目标

1. 设计 LLM 规则提取的 Prompt（Few-shot + 结构化输出约束）
2. 从法规全文批量提取候选规则
3. 逐条人工校验——法条名称、条款编号、正则模式、适用条件
4. 分析 AI 提取 vs 人工手写的覆盖率差异

---

## 核心概念

### 1. Prompt 设计——让 LLM 理解六要素模型

```markdown
你是一个招标投标法规知识工程师。你的任务是从给定的法条原文中提取可执行的审核规则。

## 输出格式
每条规则输出为 YAML，包含以下字段：

- id: 规则唯一ID，格式为 {CATEGORY}-{序号}，如 DISC-001, QUAL-003
- category: 问题分类，从以下选择：
  [资质要求, 业绩要求, 报价要求, 工期要求, 安全要求, 排斥性条款, 程序合规]
- industry: 适用行业，从以下选择：
  [建筑工程, 政府采购, IT, 通用]
- severity: critical / warning / info
- source: 必须包含 { law, article, version, effective_date, excerpt }
- conditions: 适用条件 { document_type, project_types, trigger, exclude }
- patterns: 匹配模式列表 [{ type: regex|keyword, value, target }]
- check: any_match | all_match | compare_value
- suggestion: 修改建议（200字以内）

## 示例

输入法条: "招标投标法第十八条：招标人不得以不合理的条件限制或者排斥潜在投标人..."

输出规则:
```yaml
- id: "DISC-001"
  category: "排斥性条款"
  ...
```

## 重要提醒
1. 正则模式中的特殊字符需要转义
2. 适用条件一定要填——不要全局匹配
3. source.excerpt 必须是法条原文的逐字摘录
```

**Few-shot 示例的选择**：给 2 个正面示例（一条资质类规则 + 一条排斥性规则），1 个负面示例（"这条法条不适合转化为规则，因为..."）。

### 2. 校验清单——AI 出的规则不能直接用

```rust
pub struct RuleValidator;

impl RuleValidator {
    pub fn validate(rule: &Rule, neo4j: &Neo4jClient) -> Vec<ValidationError> {
        let mut errors = vec![];

        // ① 法规名称验证——在 Neo4j 中存在吗？
        if !neo4j.law_exists(&rule.source.law) {
            errors.push(ValidationError::LawNotFound(rule.source.law.clone()));
        }

        // ② 条款编号验证——该法规有此条款吗？
        if !neo4j.article_exists(&rule.source.law, &rule.source.article) {
            errors.push(ValidationError::ArticleNotFound(
                rule.source.law.clone(),
                rule.source.article.clone(),
            ));
        }

        // ③ 正则模式语法验证——能编译通过吗？
        for pattern in &rule.patterns {
            if pattern.pattern_type == "regex" {
                if regex::Regex::new(&pattern.value).is_err() {
                    errors.push(ValidationError::InvalidRegex(pattern.value.clone()));
                }
            }
        }

        // ④ 适用条件自洽——trigger 和 exclude 不矛盾
        if let Some(trigger) = &rule.conditions.trigger {
            if let Some(exclude) = &rule.conditions.exclude {
                // 检查是否有重叠
            }
        }

        // ⑤ industry 值在分类体系中
        let valid_industries = ["建筑工程", "政府采购", "IT", "通用"];
        if !valid_industries.contains(&rule.industry.as_str()) {
            errors.push(ValidationError::InvalidIndustry(rule.industry.clone()));
        }

        errors
    }
}
```

**AI 最高频的四种错误**：
1. 法规名称不准确（如"招标投标法实施条例"写成了"招投标法实施细则"）
2. 条款编号错误（AI 把"第十八条"写成"第18条"——这在本课是合法的但需要归一化）
3. 正则语法错误（未转义特殊字符、括号不匹配）
4. `conditions` 缺失——AI 倾向于不写适用条件（"不知道该不该加"）

### 3. 覆盖率分析——AI 盲区

用 AI 从《招标投标法》全文提取规则，同时人工标注完整法律中包含多少"可提取规则"。对比：

```
《招标投标法》全文 68 条，可提取的规则约 45 条

AI 提取：32 条 (71% 覆盖率)
AI 漏掉：13 条 (29% 盲区)
  - 需要综合多条法条推断的：6 条（AI 一次只看一条）
  - 需要行业知识的：4 条（AI 不知道"资质等级"有特级/一级/二级/三级）
  - 需要案例关联的：3 条（AI 不知道"串通投标"在实践中有什么模式）
```

**启示**：AI 辅助加速初稿，但不能替代人工。G2 组的 1 人应花 50% 时间研究法规本身——这部分 AI 帮不了。

---

## 动手

### 任务 1：LLM 批量提取

用 DashScope API + 设计的 Prompt → 输入《招标投标法》全文 → 输出 YAML 规则列表 → 解析为 Rust `Vec<Rule>`。

### 任务 2：逐条校验

对 AI 生成的每条规则跑 RuleValidator（Neo4j 验证法条存在性 + regex 编译验证 + industry 值校验）。标记不通过的规则→人工修正。

### 任务 3：覆盖率分析

对比 AI 提取的规则集 vs Day 1 你手写的规则集——哪些类型 AI 覆盖率低？为什么？

---

## 验收标准

- [ ] AI 提取 30+ 条规则
- [ ] 校验通过率 > 80%
- [ ] 覆盖率分析报告：AI 盲区 + 人工手写互补

---

## 思考题

1. LLM 提取的规则中，`severity` 字段准确率最低——AI 倾向于把所有排斥性条款标为 critical，但有些是 warning。怎么改进 Prompt？
2. 如果规则库有 500 条——每次 AI 提取新规则后都要逐条人工校验，不可持续。怎么用 Golden Standard 自动化验证？
3. AI 提取的规则中的 `suggestion` 字段经常过于泛化（"建议修改"）。怎么让 LLM 生成更具体的修改建议？
