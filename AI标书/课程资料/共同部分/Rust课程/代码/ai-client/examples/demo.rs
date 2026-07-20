// 示例：ai-client 库的两种使用方式。
//
// 对比 lesson-04 的 main.rs：这里没有 endpoint、没有 Bearer Auth、
// 没有 json!() 宏、没有 serde 反序列化——因为库替使用者做完了。
// 使用者只看到三个概念：Client、Message、Response。

use ai_client::{ChatMessage, LlmClient, ToolDef};
use anyhow::Result;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    // ── 模式一：简单对话 ──
    // 三行：创建 client、构造消息、调用 chat()。
    // 不需要知道 DashScope 的 URL、认证方式、JSON 结构。
    let client = LlmClient::from_env()?;
    let messages = vec![
        ChatMessage::system("你是一个有帮助的 AI 助手。"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];

    let response = client.chat(&messages, &[]).await?;
    //                               ↑ 无工具 → 传空切片
    if let Some(content) = response.content {
        println!("LLM: {content}");
    }

    // ── 模式二：工具调用 ──
    // 定义一个工具：LLM 可以请求调用它，但 LLM 自己不执行。
    let tools = vec![ToolDef {
        name: "get_time".into(),
        description: "获取指定时区的当前时间".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "timezone": { "type": "string", "description": "例如 Asia/Shanghai" }
            },
            "required": ["timezone"]
        }),
    }];

    let tool_messages = vec![
        ChatMessage::system("需要当前时间时必须调用工具。"),
        ChatMessage::user("上海现在几点？"),
    ];

    let response = client.chat(&tool_messages, &tools).await?;
    //                                ↑ 传入工具定义，LLM 看到 tools 后决定是否调用

    // response.tool_calls 是 LLM 的"调用请求"，不是执行结果。
    // LLM 只负责说"我想调 get_time，参数是 Asia/Shanghai"。
    // 真正执行工具、拿到时间、把结果发回 LLM——是程序的事。
    for call in response.tool_calls {
        println!("LLM 想调用工具: {}({})", call.name, call.arguments);
    }

    // 完整的工具调用循环应该是：
    //   1. chat() → LLM 返回 ToolCall
    //   2. 你的程序执行工具，拿到结果
    //   3. 把结果包装成 ChatMessage::Tool，push 到 messages 末尾
    //   4. 再次 chat() → LLM 看到工具结果，生成最终回复
    //
    // 这个 demo 只展示到步骤 1——收到 ToolCall。
    // 完整的循环留作大作业。

    Ok(())
}
