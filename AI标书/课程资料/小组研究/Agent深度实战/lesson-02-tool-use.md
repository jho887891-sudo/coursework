# Day 2：Tool Use + ToolRegistry — 工具注入、SearchBuffer、错误隔离

> Agent 通用课你学了"工具是一个 trait"。今天你读项目 `tools/` 下 10 个工具文件的源码——理解 ToolRegistry 怎么注入到 Coordinator、SearchBuffer 怎么用 `Arc<RwLock<>>` 做并发去重、工具执行错误怎么隔离不传播到 Agent 循环。

---

## 学习目标

1. 走读 `AgentTool` trait + `ToolRegistry` 源码
2. 理解 SearchBuffer 的并发去重机制（`Arc<RwLock<HashSet>>` + Future 共享）
3. 掌握工具执行隔离——错误/超时/重试不传播到 Agent
4. 理解工具结果注入格式对 LLM 理解的影响

---

## 核心概念

### 1. AgentTool trait — G4↔G5 的解耦接口

```rust
/// G5 定义（工具实现侧）
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> serde_json::Value;  // OpenAI tool definition
    async fn execute(&self, args: serde_json::Value) -> Result<serde_json::Value>;
}

/// G4 使用（Agent 创建侧）
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn AgentTool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Box<dyn AgentTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn definitions(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|t| t.definition()).collect()
    }
}
```

`AgentTool` trait 是项目最关键的接口解耦——G5 维护工具实现，G4 通过 `registries.definitions()` 获取工具列表发给 LLM。两组完全并行开发：G5 新增一个工具→在 `ToolRegistry` 注册→G4 Agent 自动可用。

### 2. SearchBuffer — 并发去重的精妙设计

`search_knowledge.rs` 中的 SearchBuffer 是项目最具代表性的并发模式：

```rust
pub struct SearchBuffer {
    // key → 正在进行的搜索 Future
    pending: Arc<RwLock<HashMap<String, JoinHandle<SearchResult>>>>,
    // key → 已完成的结果缓存
    cache: Arc<RwLock<LruCache<String, SearchResult>>>,
}

impl SearchBuffer {
    pub async fn search(&self, query: &str, searcher: &dyn Searcher) -> SearchResult {
        let key = normalize_query(query);

        // 1. 先查缓存
        if let Some(cached) = self.cache.read().await.get(&key) {
            return cached.clone();
        }

        // 2. 检查是否有进行中的相同搜索
        let handle = {
            let pending = self.pending.read().await;
            pending.get(&key).cloned()
        };

        match handle {
            Some(handle) => {
                // 已有进行中的搜索 → 等待结果
                handle.await.unwrap()
            }
            None => {
                // 没有→启动新搜索
                let handle = tokio::spawn(searcher.search(query.clone()));
                self.pending.write().await.insert(key.clone(), handle);

                let result = handle.await.unwrap();
                // 写入缓存 + 从 pending 移除
                self.cache.write().await.put(key, result.clone());
                self.pending.write().await.remove(&key);

                result
            }
        }
    }
}
```

**关键模式**：`pending` 字典中存的是 JoinHandle——第一个请求插入 Future，后续请求 await 同一个 Future。避免了 3 个 Agent 同时搜索"建筑工程资质"时发出 3 次相同的 HTTP 请求到 G3。

### 3. 工具执行隔离

```rust
async fn execute_tool_safe(tool: &dyn AgentTool, args: &Value) -> ToolResult {
    match tokio::time::timeout(Duration::from_secs(5), tool.execute(args.clone())).await {
        Ok(Ok(result)) => ToolResult::Success(result),
        Ok(Err(e)) => ToolResult::Error(format!("{}: {}", tool.name(), e)),
        Err(_) => ToolResult::Timeout(tool.name().to_string()),
    }
}
```

三个关键隔离：
- **错误隔离**：工具 `execute()` 抛错→捕获为 `ToolResult::Error`，注入对话历史。Agent 看到错误结果后可以尝试换关键词重新搜索
- **超时隔离**：`tokio::time::timeout` 5 秒→未完成返回 `ToolResult::Timeout`
- **重试策略**：失败不重试工具本身（太慢），而是把错误信息注入 Agent 对话→Agent 自己决定是否重试

---

## 动手

### 任务 1：实现 ToolRegistry + AgentTool

实现完整的 `ToolRegistry`（register + definitions + execute）。用 3 个真实工具验证（search_knowledge / read_section / output_finding）。

### 任务 2：SearchBuffer 并发去重

实现 SearchBuffer。验证：3 个并发任务同时搜索相同关键词→只发 1 次 HTTP 请求→3 个任务都拿到相同结果。

### 任务 3：工具错误恢复

模拟 `search_knowledge` 返回错误（G3 超时）→验证 Agent 看到错误后能换关键词重试。模拟工具调用超时→验证 `output_finding` 不会被超时阻塞（它不需要重试）。

---

## 验收标准

- [ ] ToolRegistry 正确注入 3 个工具
- [ ] SearchBuffer：3 并发→1 次 HTTP 请求
- [ ] 工具错误不传播到 Agent 循环

---

## 思考题

1. SearchBuffer 的 `pending` 用 `RwLock<HashMap<..>>`。在高并发下（10 个 Agent 同时搜索），读写锁会成为瓶颈吗？怎么优化？
2. 工具结果注入对话历史的格式。如果把工具返回的 JSON 直接拼接为字符串 vs 包装为 `tool role` 消息——LLM 对这两种格式的理解有什么差异？
3. `output_finding` 工具不重试但其他工具重试——`execute_tool_safe` 怎么区分？
