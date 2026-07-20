# Day 3：法律知识图谱建模

> Day 2 你理解了 Neo4j 怎么存。今天你决定存什么。一个好的 Schema 让 3-hop 查询返回 10 条精准结果；一个差的 Schema 让同样的查询返回 10000 条噪声。

---

## 学习目标

1. 设计法律领域的异构图 Schema（8+ 种节点类型、10+ 种关系类型）
2. 实现确定性 ID 的实体消歧和条款编号归一化
3. 掌握 MERGE 幂等写入和批量导入策略
4. 从 G2 规则 YAML 的 source 字段自动抽取实体关系

---

## 核心概念

### 1. 领域 Schema 设计方法论

#### 从查询反推 Schema

不要从"我有什么数据"出发。从"我要回答什么问题"出发：

```
查询 1："招标投标法第 22 条在过去 3 年被哪些案例引用？"
  → 需要 Law.name + Article.article_number + Case.date + CITED_IN 关系

查询 2："这个废标案例中的风险条款，在地方法规中是否有类似规定？"
  → 需要 Risk + ProhibitionRule + Law.level + SIMILAR_TO 关系

查询 3："某省的政府采购实施细则是否在事实上比国家法更严格？"
  → 需要 Law.level + OVERLAPS_WITH 关系 + Article.text 的对比

查询 4："这条排斥性条款属于哪种禁止规则？历史上这类禁止规则导致了多少废标？"
  → 需要 ProhibitionRule + EXEMPLIFIES + Case.verdict + FOUND_IN 关系
```

从这 4 条查询反推出需要的节点类型和关系类型——这就是 Schema 的来源。

#### 法律领域的 Schema

**节点类型**：

```
(Law)                   法规实体
  law_id, name, level, version, effective_date, issuing_body, url

(Article)               条款实体
  article_id, law_id, article_number, title, text_hash, text

(Clause)                子条款（条款内部的细分条目）
  clause_id, article_id, text, type (definition/requirement/prohibition/penalty)

(Case)                  案例实体
  case_id, title, court, date, verdict, case_type, summary, full_text_url

(Risk)                  风险模式
  risk_id, name, category, severity, description, detection_pattern

(ProhibitionRule)       禁止规则
  rule_id, name, source_law, source_article, description, category

(BidDocument)           标书文档
  doc_id, title, type (招标/投标), date, file_url, parsed_json_ref

(AuditFinding)          审核发现
  finding_id, doc_id, severity, clause_id (被审核的条款), description
```

**关系类型**：

```
(l:Law)-[:HAS_ARTICLE]->(a:Article)           法规包含条款
(a:Article)-[:HAS_CLAUSE]->(c:Clause)          条款包含子条款
(l1:Law)-[:AMENDED_BY]->(l2:Law)               法规被修订（l2 是 l1 的新版本）
(l1:Law)-[:IMPLEMENTS]->(l2:Law)              下位法实施上位法
(l1:Law)-[:CITES]->(l2:Law)                    法规引用法规
(l1:Law)-[:OVERLAPS_WITH]->(l2:Law)            法规适用范围有重叠
(c:Case)-[:CITED_IN]->(a:Article)              案例引用了条款
(c:Case)-[:CITED_IN {year, context}]->(a:Article)  同上（带引用属性）
(r:Risk)-[:EXEMPLIFIES]->(p:ProhibitionRule)   风险体现禁止规则
(p:ProhibitionRule)-[:DERIVED_FROM]->(a:Article) 禁止规则来源于条款
(r1:Risk)-[:SIMILAR_TO]->(r2:Risk)             风险相似
(f:AuditFinding)-[:FOUND_IN]->(r:Risk)          审核发现识别了某风险
(f:AuditFinding)-[:ABOUT]->(cl:Clause)          审核发现针对某条款
(f:AuditFinding)-[:BASED_ON]->(a:Article)       审核发现基于条款
```

#### 禁止的 Schema 设计

```
❌ 把 Article 作为 Law 的属性存：
   Law {articles: [{number: "22", text: "..."}]}
   → 无法在 Article 之间建关系，无法独立查 Article

❌ CITED_IN 不带属性：
   (c)-[:CITED_IN]->(a)
   → 不知道案例是哪年引用的、引用的上下文是什么

❌ 所有节点只有一个 Node 标签：
   MATCH (n) WHERE n.type = 'Law' ...
   → 放弃 Label Index 的性能优势，变成全表扫描
```

---

### 2. 实体消歧 — 让同一个实体只有一个节点

#### 确定性 ID 策略

```
Law:    SHA256(law_name + version + effective_date)[:8]
        例："建筑业企业资质管理规定"+"2015年修订版"+"2015-03-01"
        → SHA256 → "a3f2b1c84e7d..."

Article: SHA256(law_id + normalized_article_number)[:8]
        注意：article_id 中嵌入了 law_id → 不同法规的同号条款不会冲突

Case:   SHA256(case_title + court + date)[:8]

Risk:   SHA256(risk_name + category)[:8]
```

为什么用 SHA256 前 8 位（64-bit）：
- 确定性：相同输入总是产生相同 ID
- 去中心化：不同数据源、不同时间导入 → ID 一致（MERGE 自动去重）
- 碰撞概率：64-bit 空间，1B 实体下碰撞概率 ≈ 5×10⁻⁸

**绝对不要用自增 ID 或 UUID**——它们不具确定性，无法跨批次去重。

#### 条款编号归一化

同一个条款编号，"第22条"和"第二十二条"必须归一化为相同格式：

```rust
fn normalize_article_number(raw: &str) -> String {
    let s = raw
        .replace('　', "")                                    // 全角空格
        .chars()
        .map(|c| match c {
            '０'..='９' => (c as u8 - b'０' + b'0') as char, // 全角数字→半角
            _ => c,
        })
        .collect::<String>();
    
    // 识别条款编号模式
    let re = Regex::new(r"第\s*([\d.]+|[一二三四五六七八九十百千]+)\s*条").unwrap();
    if let Some(cap) = re.captures(&s) {
        let num_str = &cap[1];
        let num = chinese_number_to_arabic(num_str);  // "二十二" → 22
        format!("第{}条", num)
    } else {
        s
    }
}

fn chinese_number_to_arabic(s: &str) -> u32 {
    let chinese_digits = [
        ('一', 1), ('二', 2), ('三', 3), ('四', 4), ('五', 5),
        ('六', 6), ('七', 7), ('八', 8), ('九', 9), ('十', 10),
    ];
    // 处理 "三十二" = 3×10 + 2 = 32
    // 处理 "一百二十三" = 1×100 + 2×10 + 3 = 123
    // ...
}
```

---

### 3. MERGE — 图数据库的 UPSERT

#### MERGE 怎么工作

```cypher
MERGE (l:Law {law_id: 'law_042'})
ON CREATE SET l.name = '建筑业企业资质管理规定', l.level = '部门规章'
ON MATCH SET l.last_seen = timestamp()
```

MERGE 等价于：
1. 尝试 MATCH `(l:Law {law_id: 'law_042'})`
2. 如果找到 → ON MATCH
3. 如果没找到 → CREATE + ON CREATE

关键：MERGE 只应在**唯一标识属性**上做匹配。不要 MERGE 所有属性：

```cypher
-- ❌ 错误：如果 name 稍有差异（多了个空格），就创建重复节点
MERGE (l:Law {law_id: 'law_042', name: '建筑业企业资质管理规定', ...})

-- ✅ 正确：只在唯一 ID 上 MERGE
MERGE (l:Law {law_id: 'law_042'})
ON CREATE SET l.name = '建筑业企业资质管理规定'
```

**MERGE 的原子性陷阱**：在高并发下，两个事务同时 MERGE 同一个不存在的节点：
- T1 MATCH → 未找到 → 准备 CREATE
- T2 MATCH → 未找到 → 准备 CREATE
- T1 CREATE → 成功
- T2 CREATE → 违反唯一性约束 → 回滚

解决方案：在 `law_id` 上建唯一性约束 `CREATE CONSTRAINT FOR (l:Law) REQUIRE l.law_id IS UNIQUE`，Neo4j 在约束上自动建索引，MERGE 将利用约束保证原子性。

---

### 4. 关系抽取自动化

#### 从 G2 规则 YAML 的 source 字段提取

G2 每条规则有源信息：
```yaml
source:
  law: "建筑业企业资质管理规定"
  article: "第三条"
  version: "2015年修订版"
  effective_date: "2015-03-01"
```

你的抽取器读取这些字段 → 计算 `law_id` → MERGE Law 节点 → 计算 `article_id` → MERGE Article 节点 → MERGE HAS_ARTICLE 关系：

```rust
fn extract_from_rule(rule: &Rule) -> Vec<CypherStatement> {
    let law_id = deterministic_id(&rule.source.law, &rule.source.version, &rule.source.effective_date);
    let article_id = deterministic_id(&law_id, &normalize(&rule.source.article));
    
    vec![
        format!("MERGE (l:Law {{law_id: '{}'}}) ON CREATE SET l.name = '{}', l.level = '{}', l.effective_date = '{}'",
                law_id, rule.source.law, /* inferred_level */, rule.source.effective_date),
        format!("MERGE (a:Article {{article_id: '{}'}}) ON CREATE SET a.article_number = '{}'",
                article_id, rule.source.article),
        format!("MATCH (l:Law {{law_id: '{}'}}), (a:Article {{article_id: '{}'}}) MERGE (l)-[:HAS_ARTICLE]->(a)",
                law_id, article_id),
    ]
}
```

---

## 动手

### 任务 1：实现条款编号归一化

写一个 `normalize_article_number()`，覆盖至少以下输入：
- `"第22条"` → `"第22条"`
- `"第二十二条"` → `"第22条"`
- `"第３．２条"`（全角） → `"第3.2条"`
- `"第三条之二"` → `"第3条之2"`

用 50 个测试用例验证。

### 任务 2：批量导入 500 法规 + 2000 条款

从 JSON 数据文件生成 Cypher 批量导入脚本：

```json
[
  {
    "law_name": "中华人民共和国招标投标法",
    "level": "法律",
    "version": "1999年版",
    "effective_date": "2000-01-01",
    "articles": [
      {"number": "22", "text": "招标人不得以不合理的条件限制或者排斥潜在投标人..."},
      ...
    ]
  }
]
```

### 任务 3：从 G2 规则 YAML 抽取实体

读取 G2 组的 rules/ 目录 → 提取 source 字段 → 生成 MERGE Cypher → 执行 → 验证：所有规则的 source 都在图中有对应节点。

### 任务 4：图质量验证

```
约束验证：
  - 每个 Law 节点有唯一的 law_id
  - 每个 Article 节点有唯一的 article_id
  - 每个 Article 恰好属于一个 Law（HAS_ARTICLE 关系恰好 1 条）

手动抽样：
  - 随机抽 10 个 Law → 检查 name/level/effective_date 是否正确
  - 随机抽 10 条 HAS_ARTICLE → 检查起止节点的对应关系是否准确
```

---

## 验收标准

- [ ] Schema 设计文档：含所有节点类型、关系类型、属性清单
- [ ] 条款编号归一化器 50 个测试用例全部通过
- [ ] 500 法规 + 2000 条款成功导入 Neo4j，图约束验证通过
- [ ] 从 G2 规则 YAML 提取的实体在图中有对应节点

---

## 思考题

1. 法律修订链（AMENDED_BY）应该建模为有向关系还是无向关系？有向的话方向是什么？（提示：旧版→新版 还是 新版→旧版？）
2. 如果一部地方规章"事实上废止了"但没有正式发文——这个信息你是存为属性（`is_effectively_repealed: true`）还是关系（`(local)-[:EFFECTIVELY_REPEALED_BY]->(national)`）？
3. 同一个法规的不同版本共用同一个 law_id 还是不共用？（提示：案例引用的是 2015 版还是 2007 版——如果在图中不区分，查询结果会混淆）

---

## 进阶挑战

- 实现同步批量导入器：并行生成 Cypher + 事务批处理（每 1000 条 commit）+ 失败重试
- 用 LLM 从案例文本中自动抽取 CITED_IN 关系（用 few-shot prompt + JSON 输出格式约束）
- 实现 Schema 演进检测：新增数据源后自动检查是否引入了新的节点类型或关系类型，若发现则生成 Schema 变更日志

---

## 与标书审核项目的关系

G6 组 Curator Pipeline 的 Step 2（图构建）直接使用你今天写的实体抽取逻辑。G3 组的知识检索依赖于你建的 Schema——当 Agent 调用 `graph_traverse(law_id='law_042', depth=2)` 时，它期望找到的结构就是你今天设计的 Law→Article→Case 链路。
