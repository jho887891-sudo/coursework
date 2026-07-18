use ai_client::{ChatMessage, LlmClient, ToolDef};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 初始化客户端
    let client = LlmClient::from_env()?;

    // --- 场景 1: 普通对话 ---
    println!("--- 场景 1: 普通对话 ---");
    let messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];
    
    let response = client.chat(&messages, &[]).await?;
    println!("LLM: {:?}", response.content);
    println!();

    // --- 场景 2: 工具调用 ---
    println!("--- 场景 2: 工具调用 ---");
    let tools = vec![
        ToolDef {
            name: "get_time".into(),
            description: "获取当前时间".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "timezone": {"type": "string", "description": "时区"}
                },
                "required": ["timezone"]
            }),
        },
    ];

    let messages = vec![
        ChatMessage::system("你是一个助手，如果用户问时间，请调用工具。"),
        ChatMessage::user("现在上海几点？"),
    ];

    let response = client.chat(&messages, &tools).await?;
    
    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!("LLM 想调用工具: {} ({})", tc.name, tc.arguments);
        }
    } else {
        println!("LLM: {:?}", response.content);
    }

    Ok(())
}