use ai_client::{ChatMessage, LlmClient, ToolDef};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = LlmClient::from_env()?;

    let messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("你好，你是谁？"),
    ];

    let response = client.chat(&messages, &[]).await?;
    println!("LLM: {}", response.content.unwrap_or_default());

    let tools = vec![
        ToolDef {
            name: "get_time".into(),
            description: "获取当前时间".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "timezone": {"type": "string", "description": "时区"}
                }
            }),
        },
    ];
    let response = client.chat(&messages, &tools).await?;

    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!("LLM 想调用工具: {}({})", tc.name, tc.arguments);
        }
    }

    Ok(())
}
