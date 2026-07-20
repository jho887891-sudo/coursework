use ai_client::{ChatMessage, LlmClient, ToolDef};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Err(e) = run().await {
        eprintln!("错误: {}", e);
    }
    Ok(())
}

async fn run() -> anyhow::Result<()> {
    // ── 场景 1 & 2 的公共准备：从 .env 加载配置 ──
    let client = LlmClient::from_env()?;

    // ── 场景 1：普通对话（不带工具）──
    println!("=== 场景 1：普通对话 ===");
    let messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];
    let response = client.chat(&messages, &[]).await?;
    println!("LLM: {}", response.content.unwrap_or_default());

    // ── 场景 2：工具调用 ──
    println!("\n=== 场景 2：工具调用 ===");
    let tools = vec![ToolDef {
        name: "get_time".into(),
        description: "获取当前时间".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "timezone": {"type": "string", "description": "时区，如 Asia/Shanghai"}
            },
            "required": ["timezone"]
        }),
    }];

    let time_messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手，需要使用工具获取准确时间。"),
        ChatMessage::user("现在 Asia/Shanghai 是几点？请使用工具获取准确时间。"),
    ];
    let response = client.chat(&time_messages, &tools).await?;

    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!("LLM 想调用工具: {}({})", tc.name, tc.arguments);
        }
    } else {
        println!("LLM（未调用工具）: {}", response.content.unwrap_or_default());
    }

    Ok(())
}
