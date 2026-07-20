use ai_client::{ChatMessage, LlmClient, ToolDef};
use anyhow::Result;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let client = LlmClient::from_env()?;

    println!("=== 场景1：普通对话 ===");
    let messages = vec![
        ChatMessage::system("你是一个有帮助的AI助手。"),
        ChatMessage::user("用一句话介绍 Rust。"),
    ];
    let response = client.chat(&messages, &[]).await?;
    if let Some(content) = response.content {
        println!("LLM: {}", content);
    }

    println!("\n=== 场景2：工具调用 ===");
    let mut messages = vec![
        ChatMessage::system("你是一个AI助手，必须使用工具来回答问题。调用get_time工具时，必须传入timezone参数，值为Asia/Shanghai。"),
        ChatMessage::user("获取当前时间"),
    ];
    let tools = vec![ToolDef {
        name: "get_time".into(),
        description: "获取当前时间".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "timezone": {"type": "string", "description": "时区"}
            },
            "required": ["timezone"]
        }),
    }];
    
    let response = client.chat(&messages, &tools).await?;
    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!("LLM 想调用工具: {} ({})", tc.name, tc.arguments);
            
            let time_result = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let tool_response = json!({"time": time_result}).to_string();
            
            messages.push(ChatMessage::assistant("").with_tool_calls(vec![tc.clone()]));
            messages.push(ChatMessage::tool(&tc.id, &tool_response));
        }
        
        let final_response = client.chat(&messages, &[]).await?;
        if let Some(content) = final_response.content {
            println!("LLM: {}", content);
        }
    } else if let Some(content) = response.content {
        println!("LLM: {}", content);
    }

    println!("\n=== 场景3：错误处理 ===");
    let bad_client = ai_client::LlmClient {
        api_key: "invalid-key".to_string(),
        model: "qwen-plus".to_string(),
        endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions".to_string(),
        client: reqwest::Client::new(),
    };
    let messages = vec![ChatMessage::user("测试")];
    match bad_client.chat(&messages, &[]).await {
        Ok(_) => println!("错误：应该返回错误"),
        Err(e) => println!("错误：{}", e),
    }

    Ok(())
}
