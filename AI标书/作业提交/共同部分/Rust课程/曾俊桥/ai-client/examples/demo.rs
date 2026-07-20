use ai_client::{ChatMessage, LlmClient, ToolDef};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 场景1：普通对话
    println!("=== 场景1：普通对话 ===");
    //从.env中加载配置，初始化客户端
    let client = LlmClient::from_env()?;
    //构建对话上下文：系统提示词和用户提醒
    let messages = vec![
        ChatMessage::System {
            content: "你是一个有帮助的AI助手。".into(),
        },
        ChatMessage::User {
            content: "用一句话介绍 Rust。".into(),
        },
    ];

    //传入空工具列表，执行纯文本对话
    let response = client.chat(&messages, &[]).await?;
    println!(
        "LLM: {}",
        response.content.unwrap_or_else(|| "无回复内容".to_string())
    );

    //  场景2：工具调用
    println!("\n=== 场景2：工具调用 ===");
    //定义get_time工具，使用JSON Schema描述参数
    let tools = vec![ToolDef {
        name: "get_time".into(),
        description: "获取当前时间".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "timezone": {
                    "type": "string",
                    "description": "时区"
                }
            }
        }),
    }];

    //构造引导LLM调用工具的对话
    let messages = vec![
        ChatMessage::System {
            content: "用户询问时间时，必须调用 get_time 工具获取结果。".into(),
        },
        ChatMessage::User {
            content: "现在上海的时间是几点？".into(),
        },
    ];

    let response = client.chat(&messages, &tools).await?;
    //判断是否触发工具调用，分别处理
    if !response.tool_calls.is_empty() {
        for tc in &response.tool_calls {
            println!("LLM 想调用工具: {}({})", tc.name, tc.arguments);
        }
    } else {
        println!(
            "LLM: {}",
            response.content.unwrap_or_else(|| "无回复内容".to_string())
        );
    }

    //场景3：错误处理（无效 API Key）
    println!("\n=== 场景3：错误处理 ===");
    unsafe { env::set_var("DASHSCOPE_API_KEY", "sk-invalid-test-key"); }
    let bad_client = LlmClient::from_env()?;
    match bad_client.chat(&messages, &[]).await {
        Ok(_) => println!("意外：请求成功了"),
        Err(e) => println!("错误: API 返回 {}", e),
    }

    Ok(())
}