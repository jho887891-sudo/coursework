# Day 1：ReAct Loop 内核 — 消息历史、Token Budget、流式解析

> Agent 通用课你写了 ReAct Loop。今天你读项目 600 行 `react_loop.rs` 源码——理解每一条消息如何影响 LLM 的下一个 token、Token Budget 怎么在 32K 窗口内分配、SSE 流怎么解析增量 tool call。

---

## 学习目标

1. 走读项目 `react_loop.rs` 源码——Thought→Action→Observation 循环的每个细节
2. 实现 Token Budget 管理器——上下文窗口裁剪策略
3. 实现 SSE 流式事件解析器——`data:` 行 + tool call 分片拼接
4. 对比三种停止条件的适用场景

---

## 核心概念

### 1. 项目 react_loop.rs 走读

```rust
// 简化版核心循环——对照项目源码看
pub async fn execute_react_loop(
    client: &LlmClient,
    agent_def: &AgentDefinition,
    user_query: &str,
    tools: &ToolRegistry,
    config: &ReActConfig,
) -> Result<Vec<AgentMessage>> {
    let mut messages = vec![];
    let mut token_budget = TokenBudget::new(config.max_tokens);

    // Step 1: 构建初始消息
    messages.push(Message::system(&agent_def.system_prompt));
    messages.push(Message::user(user_query));
    token_budget.consume_system(&agent_def.system_prompt)?;
    token_budget.consume_user(user_query)?;

    for turn in 0..config.max_turns {
        // Step 2: 调用 LLM
        let response = client.chat(
            &messages,
            tools.definitions(),     // ← G5 提供的工具列表
            config.tool_choice,     // "auto" / "required" / "none"
        ).await?;

        // Step 3: 解析响应
        if let Some(tool_calls) = response.tool_calls {
            for tc in tool_calls {
                // 终端工具 → 退出循环
                if tc.function.name == "output_finding" {
                    return Ok(messages);  // Agent 完成审查
                }

                // 执行工具 → 注入结果
                let result = tools.execute(&tc.function.name, tc.function.arguments).await?;
                messages.push(Message::tool(tc.id, &tc.function.name, &result));
                token_budget.consume_tool_result(&result)?;
            }
        } else {
            messages.push(Message::assistant(&response.content));
            token_budget.consume_assistant(&response.content)?;
        }

        // Step 4: 检查 Token Budget
        if token_budget.remaining() < config.min_tokens_for_next_turn {
            break;  // 预算不足→强制结束
        }
    }

    Ok(messages)
}
```

### 2. Token Budget — 32K 窗口的分配策略

```rust
pub struct TokenBudget {
    max_tokens: usize,      // 32K (qwen-plus)
    used: usize,
    reserved: usize,        // 保留给 System Prompt
}

impl TokenBudget {
    pub fn consume_system(&mut self, text: &str) -> Result<()> {
        let tokens = count_tokens(text);
        self.reserved = tokens;
        self.used += tokens;
        Ok(())
    }

    pub fn can_fit(&self, text: &str) -> bool {
        self.used + count_tokens(text) <= self.max_tokens
    }

    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.used)
    }
}
```

**裁剪策略（当预算不足时）**：

```
消息历史太长 → 需要裁剪 → 保留什么？

保留（按优先级）：
  1. System Prompt（Agent 的角色定义不能丢）
  2. 最近 3 轮对话 + 工具调用结果
  3. 早期的工具调用结果（Agent 需要上下文来理解之前的检索）

丢弃：
  4. 最早的对话轮次
  5. 纯文本思考（"Thought: 我需要检查..."——这些中间推理可以丢弃）
```

**为什么保留工具结果而不是思考文本**：Agent 的中间思考（"我需要搜索..."）在上下文溢出时可以安全丢弃——思考已经转化为行动（工具调用），而工具结果才是 Agent 下一个决策的基础。

### 3. 流式事件解析 — SSE + tool call 增量拼接

```rust
// DashScope / OpenAI 的 SSE 流格式
// data: {"choices":[{"delta":{"content":"建"}}]}
// data: {"choices":[{"delta":{"content":"筑"}}]}
// data: {"choices":[{"delta":{"tool_calls":[{"function":{"arguments":"{\"query"}}}]}}]}
// data: [DONE]

impl StreamParser {
    pub async fn parse_stream(
        mut stream: impl Stream<Item = Result<Bytes>>,
    ) -> Result<StreamOutput> {
        let mut text = String::new();
        let mut tool_calls: HashMap<usize, ToolCallBuilder> = HashMap::new();

        while let Some(chunk) = stream.next().await {
            let line = String::from_utf8(chunk?.to_vec())?;
            if line == "[DONE]" { break; }
            if !line.starts_with("data: ") { continue; }

            let data: StreamChunk = serde_json::from_str(&line[6..])?;
            for choice in data.choices {
                if let Some(content) = choice.delta.content {
                    text.push_str(&content);
                }
                if let Some(tc_deltas) = choice.delta.tool_calls {
                    for tc in tc_deltas {
                        let builder = tool_calls.entry(tc.index).or_default();
                        if let Some(name) = tc.function.name {
                            builder.name = Some(name);
                        }
                        if let Some(args) = tc.function.arguments {
                            builder.arguments.push_str(&args);
                        }
                    }
                }
            }
        }

        // 拼接完整的 tool calls
        let tool_calls = tool_calls.into_values()
            .map(|b| ToolCall {
                name: b.name.unwrap_or_default(),
                arguments: serde_json::from_str(&b.arguments)?,
            })
            .collect();

        Ok(StreamOutput { text, tool_calls })
    }
}
```

`tool_calls` 的 `function.arguments` 是 JSON 字符串的**增量片段**——需要在多帧之间累积拼接，最后才是一个完整的 JSON。这是流式解析最易出错的地方——`"{\"qu` 是不可解析的，需要等到完整的 `"{\"query\": \"建筑工程\"}"` 才能 `serde_json::from_str`。

### 4. 停止条件 — 三种机制

| 停止条件 | 触发方式 | 适用场景 |
|----------|---------|---------|
| `output_finding` 工具被调用 | Agent 主动输出结论 | 正常完成审查 |
| 达到 `max_turns` | 循环次数上限（默认 15） | Agent 陷入无限工具调用循环 |
| Token Budget < 阈值 | 上下文即将撑满 | 长文档审查，需要分段 |

---

## 动手

### 任务 1：Token Budget 管理器

实现 `TokenBudget` + 上下文裁剪策略。用 DashScope API 的 `return_token_usage=true` 验证 token 计数准确性。模拟长对话（50 轮）→验证裁剪后 System Prompt 仍在。

### 任务 2：流式事件解析器

实现 SSE 流的解析器。正确处理：正常文本流、tool call 增量拼接、`[DONE]` 结束符、错误帧（`data: {"error": ...}`）。

### 任务 3：项目源码分析

读 `backend-rust/src/agents/react_loop.rs`（或精简版）。画出消息流转的时序图（ASCII art）：System Prompt→User Query→Assistant(tool_calls)→Tool Result→Assistant→output_finding。

---

## 验收标准

- [ ] Token Budget 裁剪后 System Prompt 完整 + 最近 3 轮消息保留
- [ ] 流式解析正确拼接增量 tool call arguments 为完整 JSON
- [ ] 源码分析文档含时序图 + 至少 3 个关键决策点的注释

---

## 思考题

1. Token Budget 的裁剪策略在什么场景下会失败？（提示：System Prompt 自己就 2000 tokens——当用户文件本身就需要消耗 30K tokens 时）
2. 流式 tool call 的 `function.arguments` 是增量 JSON 字符串。如果网络丢帧，收到的是不连续的 JSON 片段——怎么检测和恢复？
3. `output_finding` 作为终端工具是一个设计选择。如果 LLM 在审查到一半就调用了 `output_finding`（没有审查完所有条款）——怎么防止？
