use ai_client::{LlmClient, ChatMessage, ToolDef, ToolCall};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 从 .env 加载配置
    let client = LlmClient::from_env()?;

    // 2. 构建消息
    let messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];

    // 3. 调用 LLM（无工具）
    println!("===== 普通对话 =====");
    let response = client.chat(&messages, &[]).await?;
    println!("LLM: {}", response.content.unwrap_or_default());

    // 4. 调用 LLM（带工具）
    println!("\n===== 工具调用 =====");
    let tools = vec![
        ToolDef {
            name: "get_time".into(),
            description: "获取当前时间".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "timezone": {"type": "string", "description": "时区"}
                },
                "required": ["timezone"]
            }),
        },
    ];
    let response = client.chat(&messages, &tools).await?;

    // 5. 检查是否有工具调用
    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!(
                "LLM 想调用: {} ({})",
                tc.name,
                serde_json::to_string_pretty(&tc.arguments).unwrap_or_default()
            );
        }
    } else {
        println!("LLM: {}", response.content.unwrap_or_default());
    }

    Ok(())
}