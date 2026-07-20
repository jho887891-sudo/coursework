use ai_client::{ChatMessage, LlmClient, ToolCall, ToolDef};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("===== 场景1：普通对话测试 =====");
    normal_chat().await?;

    println!("\n===== 场景2：工具调用测试 =====");
    tool_chat().await?;

    println!("\n===== 场景3：错误处理测试（错误 API Key） =====");
    error_handling().await?;

    Ok(())
}

/// 场景1：无工具普通对话
async fn normal_chat() -> Result<()> {
    let client = LlmClient::from_env()?;

    let messages = vec![
        ChatMessage::system("你是一个简洁的Rust技术助手"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];

    let resp = client.chat(&messages, &[]).await?;
    println!("LLM: {}", resp.content.unwrap_or_default());

    Ok(())
}

/// 场景2：带工具调用对话（get_time 工具）
async fn tool_chat() -> Result<()> {
    let client = LlmClient::from_env()?;

    let messages = vec![
        ChatMessage::system("你是一个助手，可以调用 get_time 工具获取时间。当用户询问时间时，你必须调用 get_time 工具来回答。"),
        ChatMessage::user("现在上海几点了？请调用 get_time 工具查一下。"),
    ];

    // 注册 get_time 工具
    let tools = vec![ToolDef {
        name: "get_time".into(),
        description: "获取指定时区的当前系统时间".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "timezone": {
                    "type": "string",
                    "description": "目标时区，例如：Asia/Shanghai、UTC"
                }
            },
            "required": ["timezone"]
        }),
    }];

    let resp = client.chat(&messages, &tools).await?;

    // 解析并打印工具调用信息
    if !resp.tool_calls.is_empty() {
        for ToolCall { name, arguments, .. } in resp.tool_calls {
            println!("LLM 想调用: {} ({})", name, arguments);
        }
    } else {
        let content = resp.content.unwrap_or_default();
        if content.trim().is_empty() {
            println!("LLM 返回了空响应（可能未触发工具调用）");
        } else {
            println!("LLM: {}", content);
        }
    }

    Ok(())
}

/// 场景3：错误处理——使用错误 API Key 测试 401 响应
async fn error_handling() -> Result<()> {
    // 使用错误的 API Key 构造客户端
    let client = LlmClient::new(
        "sk-错误密钥测试".into(),
        "qwen-plus".into(),
    );

    let messages = vec![
        ChatMessage::user("用一句话介绍 Rust。"),
    ];

    match client.chat(&messages, &[]).await {
        Ok(_) => println!("错误：本该失败但成功了"),
        Err(e) => println!("错误: {}", e),
    }

    Ok(())
}
