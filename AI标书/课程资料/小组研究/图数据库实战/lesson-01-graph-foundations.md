# Day 1：图论基础与图数据模型

> 法律不是一堆独立的文档。法律是一个引用网络——法规引用法规，案例引用条款，风险体现规则。今天你学会用图的眼光看法律。

---

## 学习目标

1. 理解有向图/无向图/异构图/度分布/连通分量等核心概念
2. 理解 Property Graph Model 与 RDF、关系型数据库的本质差异
3. 用 Cypher 写出 2-hop 法律关联查询
4. 解释为什么"法律数据天然是图"

---

## 核心概念

### 1. 图论——你需要的所有基础概念

#### 从七桥问题到法律引用网

1736 年，欧拉研究柯尼斯堡七桥问题——开创了图论。今天，你面对的是"法规引用网"。结构完全一样：节点（法规/案例）+ 边（引用/相似/修订）。

#### 有向图 vs 无向图

```
无向边：两个节点互相连接，没有方向。
  例：(Law A) --[SIMILAR_TO]-- (Law B)
  含义：A 和 B 是相似法规，从 A 能找到 B，从 B 也能找到 A。

有向边：起点 → 终点，单向。
  例：(Case) --[CITED_IN]--> (Article)
  含义：案例引用了条款。反向不成立——条款不会引用案例（条款是静态文本）。
```

在 Neo4j 中：`()-[]-()` 匹配无向，`()-[]->()` 匹配有向，`()<-[]-()` 匹配反向。

#### 异构图（Heterogeneous Graph）

不是所有节点都是同一类型。法律知识图谱中至少有 5 种节点：

```
(Law)        — 法规实体：名称、层级、版本、生效日期
(Article)    — 条款实体：条款编号、文本内容、所属法规
(Case)       — 案例实体：案由、法院、判决日期、判决摘要
(Risk)       — 风险模式：名称、类别、严重程度、描述
(Prohibition) — 禁止规则：规则描述、来源法规、来源条款
```

异构图的挑战：不同类型的节点可能不应该在同一向量空间中比较。Law-Law 相似度有意义，Law-Case 相似度可能没有——或者需要不同的距离度量。

#### 度（Degree）与度分布

```
节点的度 = 与它相连的边的数量
在无向图中：deg(v) = 边的数量
在有向图中：out_deg(v) + in_deg(v) = 出边 + 入边

法律知识图谱的度分布：
  - 宪法/招标投标法等核心法规 → 极高入度（被大量案例和地方法规引用）
  - 地方规章/单一条款案例 → 低度
  - 整体呈幂律分布（Power Law）：P(deg > k) ∝ k^(-α)，α ≈ 2-3
  
这对查询性能的影响：
  - 从高入度节点出发做 2-hop 遍历 → 扇出极大（数千条边）→ 需要 LIMIT
  - 低度节点图遍历很快但可能找不到足够的相关结果 → 需要扩展 hop 数
```

#### 连通分量（Connected Components）

```
一个连通分量 = 图中相互可达的所有节点集合
如果法律知识图中有孤立的连通分量 → 那个分量里的法规没有被任何案例引用过 → 可能是新法规或生僻法规
```

---

### 2. Property Graph Model — 图数据库的标准模型

#### 核心要素

```
Node (节点):
  - 可以有 0 或多个 Label (标签)，如 :Law:Active
  - 可以有 1 个或多个 Property (属性)，如 {name: "招标投标法", level: "法律"}
  - 每个节点有唯一的内部 ID

Relationship (关系):
  - 必须有 1 个 Type (类型)，如 :CITED_IN
  - 可以有 Properties，如 {year: 2023, context: "法院引用作为裁判依据"}
  - 必须有方向（但有向关系可被无向查询匹配）
  - 每个关系连接恰好 2 个节点
```

#### 与关系型数据库的对比

同一个查询："找到所有引用了《招标投标法》第 22 条的案例，以及这些案例是否导致了废标"。

关系型（8 行 SQL）：
```sql
SELECT c.title, c.verdict, r.name as risk_name
FROM laws l
JOIN articles a ON l.id = a.law_id
JOIN case_citations cc ON a.id = cc.article_id
JOIN cases c ON cc.case_id = c.id
LEFT JOIN audit_findings af ON c.id = af.case_id
LEFT JOIN risks r ON af.risk_id = r.id
WHERE l.name = '招标投标法' AND a.article_number = '22';
-- 5 次 JOIN，每次 JOIN 是 B-Tree 索引查找 = 5 × O(log N)
```

图查询（5 行 Cypher）：
```cypher
MATCH (l:Law {name: '招标投标法'})-[:HAS_ARTICLE]->(a:Article {article_number: '22'})
      <-[:CITED_IN]-(c:Case)-[:FOUND_RISK]->(r:Risk)
RETURN c.title, c.verdict, r.name;
-- 1 次索引查找（找到 Law）+ N 次指针追踪 + M 次指针追踪
-- = O(1) per hop
```

差距不是语法层面的（Cypher 更简洁），而是**物理执行层面的**。

#### 与 RDF 的对比

```
RDF (三元组):
  <Law_042> <has_article> <Article_512> .
  <Article_512> <cited_in> <Case_089> .
  <Case_089> <citation_year> "2023"^^xsd:integer .
  
  问题："年份 2023"是 Article-Case 关系的属性，但在 RDF 中
        citation_year 和 cited_in 是两个独立的三元组
        → 需要 reification（把关系变成节点）才能绑定

Property Graph:
  (Article_512)-[:CITED_IN {year: 2023, court: "最高人民法院"}]->(Case_089)
  边上的属性天然绑定，不需要额外建模
```

标书法律场景选 PG 不选 RDF 的原因：法律的引用关系天然有属性（引用年份、引用方式、引用目的），PG 的边属性表达能力更强。

---

### 3. Cypher — 图的查询语言

#### 四种基本模式

```cypher
-- CREATE：创建数据
CREATE (l:Law {law_id: 'law_042', name: '建筑业企业资质管理规定', level: '部门规章'})
CREATE (l)-[:HAS_ARTICLE]->(a:Article {article_number: '3', text: '...'})

-- MATCH + RETURN：查询数据
MATCH (l:Law)-[:HAS_ARTICLE]->(a:Article)
WHERE l.level = '部门规章'
RETURN l.name, a.article_number, a.text
LIMIT 10

-- 变长路径：不固定跳数
MATCH (l:Law {law_id: 'law_042'})-[:HAS_ARTICLE|CITED_IN*1..3]->(related)
-- 从 law_042 出发，沿 HAS_ARTICLE 或 CITED_IN 边走 1 到 3 跳
RETURN related

-- WITH 管道：中间结果处理
MATCH (l:Law)-[:HAS_ARTICLE]->(a:Article)<-[:CITED_IN]-(c:Case)
WITH l, count(c) AS case_count
WHERE case_count > 10
RETURN l.name, case_count
ORDER BY case_count DESC
```

#### 为什么变长路径是图的核心武器

```cypher
-- 问题："招标投标法 → 所有被它引用的法规 → 所有引用了那些法规的案例 → 这些案例发现了什么风险"

-- 关系型：写不出来（需要动态次数的递归 CTE，且语义混乱）
-- 图：
MATCH (start:Law {law_id: 'law_bid'})-[:AMENDED_BY|OVERLAPS_WITH|CITED_IN*1..3]->(end:Case)-[:FOUND_RISK]->(r:Risk)
RETURN DISTINCT r.name, r.category, r.severity
```

---

## 动手

### 任务 1：Docker 启动 Neo4j + 手工建图

```cypher
// 创建 5 条法规
CREATE (l1:Law {law_id: 'law_bid', name: '中华人民共和国招标投标法', level: '法律'})
CREATE (l2:Law {law_id: 'law_proc', name: '中华人民共和国政府采购法', level: '法律'})
CREATE (l3:Law {law_id: 'law_bid_impl', name: '招标投标法实施条例', level: '行政法规'})
CREATE (l4:Law {law_id: 'law_constr', name: '建筑业企业资质管理规定', level: '部门规章'})
CREATE (l5:Law {law_id: 'law_gd_proc', name: '广东省实施<政府采购法>办法', level: '地方规章'})

// 创建关系
CREATE (l3)-[:IMPLEMENTS]->(l1)       // 实施条例→法
CREATE (l5)-[:IMPLEMENTS]->(l2)       // 地方规章→法
CREATE (l4)-[:OVERLAPS_WITH]->(l1)    // 资质管理规定与招标投标法有交叉
CREATE (l2)-[:OVERLAPS_WITH]->(l1)    // 政府采购法与招标投标法有交叉

// 创建条款
CREATE (a1:Article {article_number: '22', text: '招标人不得以不合理的条件限制或者排斥潜在投标人...'})
CREATE (l1)-[:HAS_ARTICLE]->(a1)
CREATE (a2:Article {article_number: '18', text: '采购人不得以不合理的条件对供应商实行差别待遇...'})
CREATE (l2)-[:HAS_ARTICLE]->(a2)

// 创建案例
CREATE (c1:Case {case_id: 'case_001', title: '某市道路工程招标排斥外地企业案',
                court: '财政部', date: '2023-06-15', verdict: '废标'})
CREATE (c1)-[:CITED_IN]->(a1)
CREATE (c1)-[:CITED_IN]->(a2)
```

### 任务 2：跑 20 条查询

必须覆盖的查询类型：
- 单节点查找（Index Seek）
- 1-hop 关联
- 2-hop 探索（扇出分析）
- 变长路径（2 到 4 跳）
- 条件过滤 + 聚合
- WITH 管道 + 排序
- 最短路径

### 任务 3：分析你的图

用 Neo4j Browser 的 `CALL db.schema.visualization()` 查看完整 Schema。用 `MATCH (n) RETURN labels(n), count(*)` 统计各类型节点数量。计算各节点类型的平均度。

---

## 验收标准

- [ ] Neo4j Docker 正常运行，从 Rust 代码连上
- [ ] 手工创建 ≥ 20 个节点 + 30 条关系，涵盖 4+ 种节点类型和 5+ 种关系类型
- [ ] 20 条 Cypher 查询全部可运行，含变长路径和聚合查询
- [ ] 能解释每条 `EXPLAIN` 输出的算子选择

---

## 思考题

1. 如果一份法规 A 引用了法规 B，B 引用了 C——A 是否"间接依赖"C？在图论中这叫什么？（提示：传递闭包）
2. 法律知识图谱中，什么类型的节点度最高？什么类型的节点度数接近 1？这个分布对图遍历策略有什么影响？
3. RDF 的三元组模型需要 3 个三元组才能表达 Property Graph 中一条带属性的边。这个冗余在什么场景下是优点（提示：SPARQL 的图模式匹配比 Cypher 更灵活）？

---

## 进阶挑战

- 用 `apoc.periodic.iterate()` 批量创建 1000 个随机法规节点，观察图规模增大时 Neo4j Browser 的渲染性能
- 对比 Cypher 和 SQL（SQLite 或 PostgreSQL）在执行 2-hop 关联查询时的延迟差异——用数据证明"图遍历更快"

---

## 与标书审核项目的关系

G6 组的 Neo4j 图就是这个结构的放大版——你今天定义的 `Law→Article→Case→Risk` 链路，就是 G6 Curator Pipeline 需要自动构建和维护的图。

G4 Agent 的 `graph_traverse` 工具会跑你这样写的 Cypher——从审核发现的 Risk 节点出发，沿 `EXEMPLIFIES` 边找到 ProhibitionRule，再沿 `SIMILAR_TO` 找到其他规则，最终定位到具体法规条款供 Agent 审查。
