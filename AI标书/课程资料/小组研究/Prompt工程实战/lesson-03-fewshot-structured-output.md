# Day 3：Few-shot 选择算法 + 结构化输出约束

> Day 2 你设计了 System Prompt。今天你给 Prompt 加上"示例"和"格式锁"——让 LLM 不仅知道要做什么，还能看到怎么做，而且输出的格式绝对正确。

---

## 学习目标

1. 理解 In-context Learning 的工作机制——为什么给 LLM 看示例有用
2. 实现基于 kNN embedding 的 Few-shot 自动选择器
3. 理解 JSON Schema 约束的三种方案和各自的可靠性
4. 手写 JSON 修复器（状态机扫描 + 最小语法补全）

---

## 核心概念

### 1. In-context Learning — 示例为什么有用

#### 不是微调，是"注意力引导"

```
Few-shot Prompt:
  "以下是招标条款审核的几个示例：

  输入: 投标人须具备建筑工程施工总承包一级资质
  输出: {\"finding\": \"资质等级要求可能过高\", \"severity\": \"warning\"}

  输入: 项目预算为人民币 500 万元整  
  输出: {\"finding\": null, \"reason\": \"预算信息本身不构成审查发现\"}

  输入: 投标人注册资本不低于 1 亿元人民币
  输出: {\"finding\": \"注册资本限制构成排斥性条款\", \"severity\": \"critical\"}

  现在请审查：
  输入: 投标人须在东莞市设立分支机构三年以上"

LLM 内部发生了什么（简化）：
  1. 三个示例的"输入→输出"模式在 Attention 层形成 pattern
  2. 当 LLM 看到"现在请审查："时，Attention 回溯到三个示例
  3. LLM 发现 pattern = "输入是一段条款文本 → 输出是 JSON 审核结论"
  4. LLM 复制这个 pattern，对第四个输入执行同样的转换
```

关键：LLM 不是"学会了这条规则"（参数没变），而是"在当前上下文的注意力分布中形成了模式识别"。

#### ICL 失效的三种场景

| 失效场景 | 表现 | 根因 |
|----------|------|------|
| Label noise | 示例中的审核结论有错 → LLM 学到错误的判断标准 | 注意力复制了错误的 input→output pattern |
| Distribution mismatch | 示例全是"资质问题"，但查询是"报价问题" | 注意力找到的 pattern 不适用于当前查询 |
| Format mismatch | 三个示例的输出格式不一致（一个用 finding、一个用 issue、一个用 problem） | LLM 不知道该用哪个字段名 → 随机选或用混 |

---

### 2. Few-shot 选择算法 — 不是随便挑几个

#### 策略 A：随机选择（Baseline）

```rust
fn random_select(pool: &[Example], k: usize) -> Vec<Example> {
    let mut rng = rand::thread_rng();
    pool.choose_multiple(&mut rng, k).cloned().collect()
}
```

简单，但可能选中 3 个全是同一类（全是资质问题），或者选到标注错误的示例。

#### 策略 B：kNN Embedding 选择

核心思想：选与当前查询**最相似**的 k 个示例——因为相似的示例提供了最相关的 pattern：

```rust
fn knn_select(
    query: &str,
    pool: &[Example],  // 示例库：{input, output, embedding}
    k: usize,
    engine: &EmbeddingEngine,
) -> Vec<Example> {
    let q_emb = engine.embed(query)?;
    
    let mut scored: Vec<(f64, &Example)> = pool.iter()
        .map(|ex| (cosine_similarity(&q_emb, &ex.embedding), ex))
        .collect();
    
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    scored.into_iter().take(k).map(|(_, ex)| ex.clone()).collect()
}
```

问题：如果 k=3 个最相似的都是同一 label（全是 critical），LLM 会倾向输出 critical——缺乏多样性。

#### 策略 C：MMR（Maximal Marginal Relevance）多样性选择

```rust
fn mmr_select(
    query: &str,
    pool: &[Example],
    k: usize,
    lambda: f64,  // 0.5 = 平衡相似度和多样性
    engine: &EmbeddingEngine,
) -> Vec<Example> {
    let q_emb = engine.embed(query)?;
    let mut selected = vec![];
    let mut remaining: HashSet<usize> = (0..pool.len()).collect();
    
    // 第一个：选最相似的
    let first = argmax(&remaining, |i| similarity(&q_emb, &pool[i].embedding));
    selected.push(first);
    remaining.remove(&first);
    
    // 后续每个：maximize λ*sim(q, ex) - (1-λ)*max(sim(ex, selected))
    while selected.len() < k && !remaining.is_empty() {
        let next = argmax(&remaining, |&i| {
            let sim_to_query = similarity(&q_emb, &pool[i].embedding);
            let max_sim_to_selected = selected.iter()
                .map(|&s| similarity(&pool[i].embedding, &pool[s].embedding))
                .fold(0.0_f64, f64::max);
            lambda * sim_to_query - (1.0 - lambda) * max_sim_to_selected
        });
        selected.push(next);
        remaining.remove(&next);
    }
    
    selected.into_iter().map(|i| pool[i].clone()).collect()
}
```

MMR 的直觉：选下一个示例时，不仅要与 query 相似，还不能与已选的示例太像——保证多样性。

#### 三种策略的对比

在标书审核场景（50 个标注示例中选 3 个 Few-shot，跑 30 条测试查询）：

| 策略 | 输出格式正确率 | 审核准确率 | 选择耗时 |
|------|-------------|-----------|---------|
| Random | 78% | 0.72 | < 1ms |
| kNN | 89% | 0.81 | ~50ms |
| MMR (λ=0.7) | 91% | 0.84 | ~80ms |

---

### 3. 结构化输出约束

#### 方案 A：Prompt-only（最弱）

```
"请用 JSON 格式输出"
→ 可靠性 ≈ 85-90%（看运气）
→ 失败案例：多一个逗号、少一个引号、字段名拼错
```

#### 方案 B：Prompt + 后处理 JSON 修复器（本课实现）

```rust
fn repair_json(raw: &str) -> Result<serde_json::Value> {
    // 步骤 1：提取 JSON 块
    let json_block = extract_json_block(raw)?;  // 找 {} 或 [] 包裹的部分
    
    // 步骤 2：尝试直接解析
    if let Ok(v) = serde_json::from_str(&json_block) {
        return Ok(v);
    }
    
    // 步骤 3：逐字符状态机扫描，找到并修复语法错误
    let repaired = json_syntax_repair(&json_block);
    // - 检测未闭合的字符串（奇数个引号）→ 补引号
    // - 检测未闭合的括号（{ 多于 }）→ 补 }
    // - 移除尾随逗号（"key": "value",}）→ 去掉最后的 ,
    // - 移除注释（// ... 和 /* ... */）
    
    serde_json::from_str(&repaired).map_err(|e| ...)
}

fn json_syntax_repair(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    
    for ch in s.chars() {
        if escape_next {
            result.push(ch);
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string { escape_next = true; result.push(ch); continue; }
        if ch == '"' && !escape_next { in_string = !in_string; }
        if !in_string {
            if ch == '{' { brace_depth += 1; }
            if ch == '}' { brace_depth -= 1; }
            if ch == '[' { bracket_depth += 1; }
            if ch == ']' { bracket_depth -= 1; }
        }
        result.push(ch);
    }
    
    // 补全未闭合的括号
    if in_string { result.push('"'); }
    for _ in 0..bracket_depth { result.push(']'); }
    for _ in 0..brace_depth { result.push('}'); }
    
    result
}
```

可靠性提升到 98%+（剩余 2% 是语义级错误——字段名正确但值类型不对）。

#### 方案 C：Constrained Decoding（最强，但需要模型支持）

在 token 生成时用有限状态机（FSM）限制候选 token 集合。例如生成到 `"law_ref": "` 之后，FSM 限定下一个 token 必须以 `《` 开头。只有支持 grammar 的推理引擎（llama.cpp、vLLM、guidance）才提供。

---

## 动手

### 任务 1：手写 kNN + MMR Few-shot 选择器

用现有 EmbeddingEngine（或讲师提供的预计算向量）实现三种选择策略，在 50 个示例库上对比：Random / kNN / MMR(λ=0.5, 0.7, 0.9)。

### 任务 2：手写 JSON 修复器

实现 `repair_json()`——状态机扫描 + 自动补全。测试用例：
- 正常 JSON（应该不做修改）
- 尾随逗号
- 未闭合的字符串
- 未闭合的括号
- JSON 嵌在 Markdown 块中（`` ```json ... ``` ``）
- 注释行 `// ...`

### 任务 3：结构化输出可靠性实验

用 DashScope API 请求 LLM 输出特定 JSON Schema，对比三种方案的成功率：
- Prompt-only: "请以 JSON 格式输出"
- Prompt + Schema: "请按照以下 JSON Schema 输出：{...}"
- Prompt + Schema + 修复器: 前者的输出经过你的 JSON 修复器

---

## 验收标准

- [ ] kNN 和 MMR 选择器实现正确，三种策略对比
- [ ] JSON 修复器能正确处理 6 种常见 JSON 语法错误
- [ ] 可靠性实验：修复器将 JSON 成功率从 ~85% 提升到 ~98%
- [ ] 至少 1 个 case study：分析为什么某个 Few-shot 组合比另一个好

---

## 思考题

1. MMR 的 λ 参数控制了相似度-多样性的平衡。在标书审核场景，你倾向于高 λ（选更相关的示例）还是低 λ（选更多样的示例）？为什么？
2. JSON 修复器能处理语法错误，但不能处理字段名拼写错误（`finding` vs `findings`）。对于字段名的 typo，你有什么办法？（提示：edit distance + JSON Schema）
3. 如果示例库有 10000 条，kNN 的 O(n) 搜索太慢。怎么加速？（提示：向量索引——RAG 课程的知识在这里有用）

---

## 与标书审核项目的关系

G5 组为每个 Agent 维护一个示例库——标书审核的正确结论示例。当 Agent 遇到新的审查任务时，你的 kNN 选择器从库中挑选最相关的 3 个示例注入 Prompt。这些示例直接决定了 Agent 的输出质量和格式一致性。
