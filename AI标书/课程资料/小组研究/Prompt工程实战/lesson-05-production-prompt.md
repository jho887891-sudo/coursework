# Day 5：Prompt 治理独立实验

> 前 4 天你理解了 LLM 怎么采样、System Prompt 怎么设计、Few-shot 怎么选、A/B 怎么测。今天用独立 Demo 验证版本、模板、评测和回滚的最小闭环；完整灰度发布与幻觉检测作为骨干选做。

---

## 目标

搭建一个最小 Prompt 治理 Demo：保存两个 Prompt 版本，使用同一批小样本运行，对比指标，并根据结果选择候选版本或回滚。

---

## 架构

```
prompts/                          ← Git 仓库
├── judge_agent/
│   ├── v1.0.0.md                 ← 当前线上版本
│   ├── v1.0.1.md                 ← 候选版本
│   └── examples.json             ← 该 Agent 的 Few-shot 示例库
├── legal_verify_agent/
│   ├── v2.1.0.md
│   └── examples.json
├── config.toml                   ← 灰度路由配置
└── CHANGELOG.md                  ← 变更记录 + A/B 结果

┌──────────────────────────────────────────────────────┐
│                  Prompt Manager (Rust)                 │
│                                                        │
│  ┌─ Template Engine ────────────────────────────────┐ │
│  │  prompt.md 中有 {{role}}, {{dimensions}}          │ │
│  │  render("prompt.md", ctx) → 最终 System Prompt    │ │
│  └───────────────────────────────────────────────────┘ │
│                                                        │
│  ┌─ Git Hook (pre-commit) ──────────────────────────┐ │
│  │  检测 prompts/ 变更 → 自动跑 A/B 测试              │ │
│  │  → 有回归则阻止提交                                │ │
│  └───────────────────────────────────────────────────┘ │
│                                                        │
│  ┌─ Canary Router ──────────────────────────────────┐ │
│  │  读 config.toml → 按比例路由到新版                  │ │
│  │  [judge_agent] active="v1.0.0" canary="v1.0.1"   │ │
│  │  canary_ratio=0.1  # 10% 流量走新版                │ │
│  └───────────────────────────────────────────────────┘ │
│                                                        │
│  ┌─ Hallucination Detector ─────────────────────────┐ │
│  │  输出中提取法规引用 → 验证真实性                     │ │
│  └───────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

---

## 模块 1：Prompt 模板引擎

### 为什么需要模板

System Prompt 中有些内容是动态的——不同项目类型（施工招标 vs 货物采购）、不同 Agent 实例——但不能每次手工改 `.md`：

```markdown
<!-- prompts/judge_agent/v1.0.0.md -->
# 角色
你是{{role}}，专门审查{{project_type}}中的{{risk_category}}。

# 审查维度
{{#each dimensions}}
- {{this.name}}: {{this.description}}
{{/each}}
```

### 模板语法设计

```
{{variable}}              → 简单替换
{{#each list}} ... {{/each}}  → 循环
{{#if condition}} ... {{/if}}  → 条件（可选）

// 禁止递归引用：变量值中不能再出现 {{ 模式
```

### 实现

```rust
pub struct TemplateEngine;

impl TemplateEngine {
    pub fn render(template: &str, ctx: &HashMap<String, Value>) -> Result<String> {
        let mut output = String::new();
        let mut chars = template.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '{' && chars.peek() == Some(&'{') {
                chars.next(); // consume second {
                let mut var_name = String::new();
                while let Some(ch) = chars.next() {
                    if ch == '}' && chars.peek() == Some(&'}') {
                        chars.next(); // consume second }
                        break;
                    }
                    var_name.push(ch);
                }
                
                let var_name = var_name.trim();
                
                if var_name.starts_with("#each ") {
                    // handle loop
                    let list_name = var_name[6..].trim();
                    let list = ctx.get(list_name).and_then(|v| v.as_array())
                        .ok_or_else(|| anyhow!("{{#each {list_name}}}: not an array"))?;
                    let item_content = extract_until_close(&mut chars, "#each")?;
                    for item in list {
                        let item_ctx = self.build_item_context(ctx, item);
                        output.push_str(&self.render(&item_content, &item_ctx)?);
                    }
                } else {
                    let value = ctx.get(var_name)
                        .ok_or_else(|| anyhow!("undefined variable: {var_name}"))?;
                    output.push_str(&self.value_to_string(value));
                }
            } else {
                output.push(ch);
            }
        }
        
        Ok(output)
    }
}
```

### 变量类型检查（编译时）

模板引擎可以做到：在 `render()` 之前就检测未定义的变量：

```rust
fn validate(template: &str, schema: &HashMap<String, ValueType>) -> Result<()> {
    let vars = extract_variables(template);
    for var in &vars {
        if !schema.contains_key(var) {
            bail!("undefined variable: {var}. Available: {:?}", schema.keys());
        }
    }
    Ok(())
}
```

---

## 模块 2：Git Hook 自动评测

```bash
#!/bin/bash
# .git/hooks/pre-commit

# 检测 prompts/ 目录是否有变更
CHANGED=$(git diff --cached --name-only -- 'prompts/*.md')
if [ -z "$CHANGED" ]; then
    exit 0  # 无 Prompt 变更，跳过
fi

echo "检测到 Prompt 变更：$CHANGED"
echo "自动运行 A/B 测试..."

# 对每个变更的 Agent 跑评测
for file in $CHANGED; do
    AGENT=$(basename $(dirname "$file"))
    NEW_VERSION=$(basename "$file" .md)
    OLD_VERSION=$(grep "active=" prompts/config.toml | grep "$AGENT" | cut -d'"' -f2)
    
    echo "Agent: $AGENT, $OLD_VERSION → $NEW_VERSION"
    cargo run --bin prompt-eval -- \
        --agent "$AGENT" \
        --old "prompts/$AGENT/$OLD_VERSION.md" \
        --new "$file" \
        --queries eval/queries.json \
        --repetitions 30
    
    RESULT=$?
    if [ $RESULT -eq 1 ]; then
        echo "❌ A/B 测试发现回归！提交被阻止。"
        echo "详见 eval/reports/${AGENT}_${NEW_VERSION}.md"
        exit 1
    fi
done

echo "✅ A/B 测试通过，允许提交。"
```

---

## 模块 3：幻觉检测 Pipeline

标书审核 Agent 的输出中最危险的错误是**幻觉**——引用不存在的法规、捏造条款内容、编造数字。

### 三种幻觉类型

```
法规幻觉: "依据《建设工程施工许可管理办法》第 15 条"
  → 这部法规根本不存在，或者第 15 条内容不对

原文幻觉: "招标文件第 5 页要求投标人须在东莞设立分支机构"
  → 原文第 5 页没有这句话，Agent 编的

数值幻觉: "该项目的预算上限为 5000 万元"
  → 原文的预算是 500 万，Agent 多加了一个零
```

### 检测 Pipeline

```rust
struct HallucinationDetector {
    neo4j: Neo4jClient,          // 用于验证法规引用
    document_cache: DocumentCache, // 用于验证原文引用
}

impl HallucinationDetector {
    async fn detect(&self, agent_output: &AuditFinding) -> Vec<Hallucination> {
        let mut hallucinations = vec![];
        
        // 1. 法规幻觉检测
        if let Some(law_ref) = &agent_output.law_ref {
            let (law_name, article_number) = parse_law_ref(law_ref);
            if !self.neo4j.law_exists(&law_name).await? {
                hallucinations.push(Hallucination::LawNotExist(law_name));
            } else if !self.neo4j.article_exists(&law_name, &article_number).await? {
                hallucinations.push(Hallucination::ArticleNotExist(law_name, article_number));
            }
        }
        
        // 2. 原文幻觉检测
        if let Some(source_quote) = &agent_output.source_quote {
            if let Some(clause_id) = &agent_output.clause_id {
                let original_text = self.document_cache.get_clause(clause_id).await?;
                if !fuzzy_match(source_quote, &original_text) {
                    hallucinations.push(Hallucination::SourceQuoteMismatch {
                        quoted: source_quote.clone(),
                        actual: original_text,
                    });
                }
            }
        }
        
        // 3. 数值幻觉检测（基础版）
        if let Some(numbers) = extract_numbers_from_finding(agent_output) {
            let doc_numbers = self.document_cache.get_all_numbers().await?;
            for n in &numbers {
                if !doc_numbers.contains(n) && !numbers_appear_in_law(n, agent_output) {
                    hallucinations.push(Hallucination::SuspiciousNumber(*n));
                }
            }
        }
        
        hallucinations
    }
}
```

### 检测策略

法规幻觉需要 Neo4j：
```
LLM 输出: "依据《建设工程施工许可管理办法》第 15 条"
  → 正则提取: law_name="建设工程施工许可管理办法", article="15"
  → Neo4j: MATCH (l:Law) WHERE l.name CONTAINS '建设工程施工许可管理办法'
  → 没找到 → 幻觉！标记
```

原文幻觉需要子串匹配：
```
LLM 输出: source_quote="投标人须在东莞市设立分支机构三年以上"
  → 在 clause_id 对应的原文中找子串
  → 没找到完全匹配 → 用编辑距离或 LLM 做语义相似度判断
  → 如果语义也不匹配 → 幻觉！
```

---

## 动手

### P0：Prompt 管理系统（及格线）

1. 搭建 `prompts/` Git 仓库（至少 3 个 Agent 的 System Prompt）
2. 实现模板引擎（`{{variable}}` 替换 + simple validation）
3. 实现 Git pre-commit hook → 触发 A/B 测试
4. A/B 测试报告自动写入 `eval/reports/`

### P1：灰度发布 + 幻觉检测（加分项）

1. `config.toml` 路由配置 → Canary router 按比例分发
2. 幻觉检测 Pipeline（法规幻觉 + 原文幻觉）→ 10 条测试输出中检出假法规
3. 幻觉报告 → 按 Agent 维度统计幻觉率

### P2：自动 Prompt 优化建议（挑战项）

当评测显示 Agent X 在"资质检测"维度 Precision 低：
- 自动收集 Agent X 在这个维度上的错误输出
- 提取共性错误模式
- LLM 生成改进版 Prompt draft → 人类审核

---

## 验收标准

| 验收项 | 权重 | 怎么测 |
|--------|------|--------|
| 模板引擎可用 | 15% | `render("{{role}}", {"role": "test"})` → "test" |
| Git hook 自动评测 | 20% | 修改 Prompt → commit → hook 触发 → 输出报告 |
| A/B 测试 CI 不退化 | 15% | Bootstrap CI 验证 |
| 幻觉检测 Pipeline | 15% | 10 条含假法条的输出 → 检测率 > 80% |
| 灰度路由 | 10% | config.toml 切换 → 生效 |
| Prompt 版本可追溯 | 10% | `git log prompts/` + CHANGELOG.md |
| 设计决策文档 | 15% | 模板引擎设计 / 幻觉检测策略 / A/B 框架设计 |

---

## 设计决策文档（必写）

1. **模板引擎为什么只支持 `{{variable}}` 和 `{{#each}}`，不支持 `{{#if}}`？** — 条件逻辑增加复杂度，且容易引入不一致。复杂逻辑在 Rust 层处理，模板只负责呈现
2. **幻觉检测的法规验证为什么需要 Neo4j 而不是向量检索？** — 法规是否存在是"精确匹配"问题，不是"相似匹配"问题。向量库不适合精确存在性检查
3. **A/B 测试为什么不用 t-test 而是 Bootstrap？** — LLM 输出的评分分布不是正态分布（有 floor/ceiling 效应），t-test 的正态假设不成立

---

## 与标书审核项目的关系

这个实验用于理解领域工具组未来 Prompt 管理基础设施的核心机制，不直接修改项目 Prompt 或发布配置。

独立 Demo 至少展示“两个 Prompt 版本→同一小批样本→指标比较→选择或回滚”的闭环。模板引擎、Git hook、灰度发布和幻觉检测可以分工选做；课程结果只提供设计依据。
