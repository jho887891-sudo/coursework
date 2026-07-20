use ai_client::{LlmClient, ChatMessage, ToolDef};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 从 .env 加载配置初始化客户端
    let client = LlmClient::from_env()?;

    // 场景 1：普通对话测试
    println!("--- 正在测试：普通对话 ---");
    let messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];

    let response = client.chat(&messages, &[]).await?;
    if let Some(content) = response.content {
        println!("LLM: {}\n", content);
    }

    // 场景 2：工具调用测试
    println!("--- 正在测试：工具调用识别 ---");
    let tool_messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("请帮我查一下上海现在的时间。"),
    ];

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

    let response = client.chat(&tool_messages, &tools).await?;

    // 检查模型是否成功触发了工具调用
    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!("LLM 想调用: {} ({})", tc.name, tc.arguments);
        }
    } else if let Some(content) = response.content {
        println!("LLM 未触发工具，直接回复: {}", content);
    }

    Ok(())
}