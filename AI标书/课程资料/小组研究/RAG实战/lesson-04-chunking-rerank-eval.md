# Day 4：Chunking 策略 + Reranker + 评测体系

> 同样的 BGE-M3 + Qdrant + HNSW，为什么有的 RAG 召回率 90%、有的只有 60%？答案在 Chunking 和 Reranker。今天你把这两个变量拆开、量化、优化——用数据说话。

---

## 学习目标

1. 理解 Chunking 的数学本质——在语义空间中寻找最优分割面
2. 实现 5 种 Chunking 策略并对比标书场景的 Recall 差异
3. 理解 Bi-Encoder 与 Cross-Encoder 的架构差异和适用场景
4. 搭建检索评测 Pipeline：4 指标 + Bootstrap 显著性检验
5. 找出标书场景的"最优 Chunking + Reranker"组合

---

## 核心概念

### 1. Chunking — 在语义空间中切分

#### Chunking 的本质问题

一段 5000 字的法规文本，BGE-M3 只能接受最多 8192 个 token。你要把它切成若干个 Chunk。切在哪？

关键约束：**检索的最小粒度 = 1 个 Chunk**。如果"第二十二条"被切到 3 个 Chunk 里，Agent 检索时只能看到碎片——它永远不知道完整的条款内容。

#### 五种策略的数学刻画

**① Fixed-Length with Overlap**

```
chunk_size = 512 chars, overlap = 128 chars
每个 chunk = 文本中连续的 512 字符
相邻 chunk 共享 128 字符区域

问题：切在句子中间 → "投标人须具备建筑工程施工总承包"
      被切成 "投标人须具备建" | "筑工程施工总承包"
→ Agent 搜索"建筑工程"时两个 Chunk 都可能命中，但都残缺
```

**② Paragraph-based**

```
分隔符：\n\n（空白行）
对标书适用（条款间天然有空白行分隔），但：
  - 跨段落长条款被切碎
  - 一份标书可能几个条款写在一段里（bad formatting）
```

**③ Recursive Splitting**

```
分隔符优先级：\n\n → \n → 。 → ， → （无分隔符则硬切）

递归逻辑：
  fn split(text):
    if len(text) <= max_size: return [text]
    for sep in ["\n\n", "\n", "。", "，", ""]:
      按 sep 切分 → 如果切出多个片段 → 对每个片段递归
      如果只用最高优先 sep 就能切得足够小 → 不往下试
```

这是 LangChain 的默认策略，工程上最稳健。

**④ Semantic Chunking**

```
核心思想：相邻句子的 embedding cosine similarity 陡然下降 = 语义边界

算法：
  sentences = split_by_sentence(text)  // 用 "。" 切句
  for i in 1..sentences.len():
    sim[i] = cosine(embed(sentences[i-1]), embed(sentences[i]))
  
  // 找局部极小值作为切分点
  for i in 1..sim.len()-1:
    if sim[i] < sim[i-1] * 0.8 && sim[i] < sim[i+1]:
      split_points.push(i)

问题：需要为每个句子做一次 embedding，1000 句的文档需要 1000 次推理 → 慢
```

**⑤ 结构感知分块（标书场景专用）**

```
核心思想：利用文档自身结构作为切分锚点

标书文档的结构信号：
  - 条款编号："第[一二三四五六七八九十百千\d]+条"  → 强制新 Chunk 起点
  - 章节标题："第[一二三四五六七八九十\d]+章"      → 强制新 Chunk 起点
  - 表格边界："┌─" / "<table>"                    → 表格整体保持完整
  - 金额模式："\d+万元" / "人民币\d+元"            → 不被切分

算法骨架：
  anchor_points = find_all_anchors(text)  // 所有条款编号/章节标题/表格边界
  chunks = []
  for each span between anchors:
    if span.len() <= max_size:
      chunks.push(span)  // 锚点间的文本整体保留
    else:
      // 太长的 span 用 recursive splitting 再细分，但保护金额和表格
      sub_chunks = recursive_split_with_protected_patterns(span)
      chunks.extend(sub_chunks)
```

这对标书场景的重要性：**一个条款 = 一个自包含的法律语义单元**。切断了条款，Agent 就会基于"不完整的法律要求"做判断——这是招标文件审核中最致命的错误。

#### Chunk Size 与 Overlap 对 Recall 的影响

这是一条 U 型曲线：

```
Chunk 太小（100 字）：
  → 信息碎片化 → 检索命中但内容不完整 → LLM 基于碎片回答 → 低 Faithfulness
Chunk 太大（2000 字）：
  → 一个 Chunk 包含太多不相关内容 → Embedding 被噪声稀释 → 检索不精准
最优（标书法规 ~ 300-600 字）：
  → 1-2 个完整条款 → 自包含的语义单元
```

你需要自己跑实验验证这条曲线。

---

### 2. Reranker — 从粗筛到精排

#### Bi-Encoder vs Cross-Encoder

```
Bi-Encoder（BGE-M3）:
  [Query] → Encoder → q_vec (1024d)       独立编码！
  [Doc_1] → Encoder → d1_vec (1024d)      独立编码！
  [Doc_2] → Encoder → d2_vec (1024d)
  ...
  score(q, di) = cosine(q_vec, di_vec)    编码后简单乘法
  
  优势：Doc embedding 可以预计算！检索时只编码 query
  
Cross-Encoder（BGE-Reranker-v2-m3）:
  [CLS] Query [SEP] Doc [SEP] → Encoder (full attention) → sigmoid(score)
                                  ↑
               Query 和 Doc 在每一层 attention 都交互！
  
  优势：全交互 → 精度高很多
  劣势：每个 (q, d) pair 都要完整前向传播 → 不能预计算 → 慢
```

#### 为什么 Bi-Encoder 不够

```
Query: "投标人资质要求"
Doc_A: "投标人须具备建筑工程施工总承包二级及以上资质"     (相关)
Doc_B: "投标人应具备安全生产许可证，资质等级不作要求"      (表面相关但含义相反)

Bi-Encoder:
  - Query 编码时"看不到" Doc_A 和 Doc_B
  - "资质" "要求" 在 embedding 空间距离较近
  - Doc_A 和 Doc_B 的相似度可能差不多！
  
Cross-Encoder:
  - "投标人资质要求 [SEP] 资质等级不作要求"
  - Attention 在 "资质要求" 和 "不作要求" 之间建立连接
  - 直接判断："要求资质" 与 "不作要求" → 相关但含义相反 → 低分！
```

#### 为什么 Cross-Encoder 更快...

...如果你用好 batch：

```
❌ 20 次独立推理：
  for doc in top_20:
    score = reranker.score(query, doc)  // 每次 1 个 pair × 20 = 20 次前向
  latency = 20 × 30ms = 600ms

✅ 批量推理：
  pairs = [(query, doc_1), (query, doc_2), ..., (query, doc_20)]
  // 构造成一个 batch=20 的输入
  scores = reranker.score_batch(pairs)  // 一次前向
  latency = 1 × 50ms = 50ms  ← 12 倍加速！
```

Reranker 在你的 Pipeline 中的位置：

```
Query → Dense/Sparse 混合检索 → Top-20 候选 → Reranker batch=20 → Top-5 最终结果
        ↑ 快（< 10ms）                          ↑ 略慢（~50ms）但准很多
```

---

### 3. 评测体系 — "方案 A 显著优于方案 B" 的数学定义

#### 四个指标的形式化定义

给定查询 q 的 Top-K 返回结果 R，已知正确答案集 G（人工标注的相关文档 ID 集合）：

```
Recall@K = |R ∩ G| / |G|
  → "正确答案中有多少被检索到了？"
  → 标书审核：不能漏。高 Recall = 不遗漏法规。

Precision@K = |R ∩ G| / K
  → "返回的结果中有多少是对的？"
  → 标书审核：不能多。高 Precision = 不浪费 Agent 的注意力。

MRR = 1/|Q| × Σ 1/rank_i
  → "最好的那个答案排在第几位？"
  → rank_i = 第 i 个查询的第一个正确答案的排名（没有则 = ∞，贡献 0）
  → 标书审核：最好的法规应该排第一——用户只点第一个。

NDCG@K = DCG / IDCG
  → 考虑多级相关性（Perfect=3, Relevant=2, Partial=1, Irrelevant=0）
  → DCG = Σ_{i=1}^K rel_i / log2(i+1)
  → 第 1 位权重最高，排得越后贡献越小
  → 适用于"有些 Chunk 是完美匹配、有些是部分相关"的场景
```

#### Bootstrap 置信区间 — "A 真的比 B 好吗？"

```
问题：A 方案 Recall@10 = 0.85，B 方案 = 0.83。
      差了 2 个点——这个差异是真实存在的还是随机波动？

Bootstrap 方法：
  假设有 30 条测试查询的结果：
    scores_A = [0.9, 0.8, 0.7, ..., 0.95]  (30 个值)
    scores_B = [0.85, 0.78, 0.72, ..., 0.92] (30 个值)
  
  for _ in 1..10000:
    从 scores_A 中有放回地抽 30 个样本 → 算均值
    从 scores_B 中有放回地抽 30 个样本 → 算均值
    diff = mean_A - mean_B
    diffs.push(diff)
  
  CI_lower = percentile(diffs, 2.5%)   // 95% 置信区间下界
  CI_upper = percentile(diffs, 97.5%)  // 95% 置信区间上界
  
  如果 CI_lower > 0：
    → A 显著优于 B（95% 置信水平下，A 的 Recall 高于 B 的概率 > 97.5%）
  如果 CI_lower < 0 < CI_upper：
    → 差异不显著——你看到的 2% 差异可能是随机波动，再做 100 条 query 可能就反转了
```

这是 G4 组评测框架的核心方法论——每次 Prompt 变更前后都要跑显著性检验。你今天学会的 Bootstrap 方法直接用在 G4 的自动化评测 Pipeline 中。

---

## 动手

### 任务 1：实现 5 种 Chunking 策略

```
chunkers/
├── fixed.rs          # Fixed-length (3 variants: 256/512/1024 + 128 overlap)
├── paragraph.rs      # Paragraph-based (\n\n split)
├── recursive.rs      # Recursive splitting (multi-separator fallback)
├── semantic.rs       # Semantic chunking (embedding similarity threshold)
└── structure_aware.rs # 结构感知（条款编号锚点 + 表格保护 + 金额保护）
```

### 任务 2：Reranker 集成

可选方案二选一：
- A）BGE-Reranker-v2-m3 ONNX 本地推理（推荐，零 API 成本）
- B）DashScope Rerank API（简单，但收费）

实现 batch 推理：`score_batch(query: &str, docs: &[&str]) -> Vec<f32>`

### 任务 3：构建标书评测集

30 条查询 + 每条查询标注 3-5 个相关 Chunk + 相关性等级：

```json
{
  "query": "投标人须具备建筑工程施工总承包二级及以上资质",
  "relevant": [
    {"chunk_id": "ch_042", "relevance": "Perfect"},    // 恰好包含该条款
    {"chunk_id": "ch_038", "relevance": "Relevant"},   // 资质要求概述
    {"chunk_id": "ch_105", "relevance": "Partial"}     // 施工企业资质分类
  ]
}
```

### 任务 4：评测矩阵

5 种 Chunking 策略 × 4 种检索策略（Dense/Sparse/RRF/Linear）× 有/无 Reranker × 4 个指标 = 160 个数据点

输出对比报告，含：
- 每种组合的指标值
- Bootstrap 95% CI
- 显著性判定矩阵（5×5，每种 pair-wise 比较是否显著）

### 任务 5：参数敏感性分析

固定最优策略组合，变化：
- Chunk size ∈ [128, 256, 512, 768, 1024]
- Chunk overlap ∈ [0, 32, 64, 128, 256]
- Reranker top_k ∈ [5, 10, 20, 50]

输出 Recall@10 随参数变化的曲线。

---

## 验收标准

- [ ] 至少实现 2 种有明显差异的 Chunking 策略并完成对照
- [ ] 其余策略能解释原理和适用边界；5 种全部实现为骨干选做
- [ ] Reranker 集成 + batch 推理（延迟 vs 准确率 trade-off）
- [ ] 30 条标书查询标注集（含多级相关性）


---

## 思考题

1. 结构感知分块依赖条款编号的正则匹配。如果一份标书使用了非标准格式（如"1.2.3"而非"第X条"），你的分块器会退化成什么行为？怎么提高鲁棒性？
2. Cross-Encoder Reranker 比 Bi-Encoder 慢 10-50 倍。什么场景下值得花这个延迟？（提示：思考标书审核场景——用户提交一份文件后等 30 秒看到审核报告。Reranker 多花 50ms 还是 5 秒？）
3. 你的评测集只有 30 条查询。当 Bootstrap CI 重叠时（差异不显著），可能的原因是什么？（提示：不是策略真的没差异，可能是 30 条样本不够大。CI 宽度 ∝ 1/√n）

---

## 进阶挑战

- 实现 Chunk 质量自动检测器：扫描所有 Chunk，标记被截断的条款编号、不完整的表格、孤立的金额
- LoRA 微调 BGE-Reranker：收集 50 对标书领域正负例 → 用 HuggingFace PEFT 微调 3 epoch → 对比微调前后的 Recall@10 变化（需要 Python，可以单独写脚本）
- 实现自适应检索策略：短查询 → RRF + 低 ef；长查询 → Dense + 高 ef；含数字 → 加权 Sparse

---

## 与标书审核项目的关系

G3 组的文档预处理模块直接使用你今天的最优 Chunking 策略。G4 组的评测 Pipeline 直接使用你今天的 Bootstrap 显著性检验方法。

**结构感知分块是标书场景的核心竞争力**——竞品用 LangChain 默认的 Recursive Splitting，条款被切得支离破碎，Agent 基于碎片做判断。你的分块器从源头保证每条条款完整的语义上下文。
