use clap::{Parser, Subcommand};
use dotenv::dotenv;
use reqwest::Client;
use serde_json::json;
use std::env;

// 1. 定义命令行参数结构
#[derive(Parser)]
#[command(name = "ai-assistant")]
#[command(about = "一个简单的命令行 AI 助手", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 向 AI 提问
    Ask {
        question: String,
    },
    /// 翻译文本
    Translate {
        text: String,
        #[arg(short, long, default_value = "en")]
        to: String,
    },
}

// 2. 主函数入口
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 加载 .env 环境变量
    dotenv().ok();
    
    // 获取 API Key
    let api_key = env::var("DASHSCOPE_API_KEY")
        .map_err(|_| anyhow::anyhow!("缺少 DASHSCOPE_API_KEY"))?;

    let cli = Cli::parse();
    let client = Client::new();

    // 根据子命令执行不同逻辑
    match cli.command {
        Command::Ask { question } => {
            let system_prompt = "你是一个有帮助的AI助手。";
            let reply = call_dashscope(&client, &api_key, system_prompt, &question).await?;
            println!("{}", reply);
        }
        Command::Translate { text, to } => {
            let system_prompt = format!("你是一个翻译助手，将用户输入翻译为{}，只输出译文。", to);
            let reply = call_dashscope(&client, &api_key, &system_prompt, &text).await?;
            println!("{}", reply);
        }
    }

    Ok(())
}

// 3. 封装 API 调用逻辑
async fn call_dashscope(
    client: &Client,
    api_key: &str,
    system_content: &str,
    user_content: &str,
) -> anyhow::Result<String> {
    let url = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

    // 构造请求体
    let request_body = json!({
        "model": "qwen-plus",
        "input": {
            "messages": [
                {"role": "system", "content": system_content},
                {"role": "user", "content": user_content}
            ]
        },
        "parameters": {
            "result_format": "message"
        }
    });

    // 发送请求
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    // 4. 错误处理：检查 HTTP 状态码
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await?;
        // 使用 bail! 宏提前返回错误，不 panic
        anyhow::bail!("API 请求失败 [{}]: {}", status, text);
    }

    // 解析 JSON 响应
    let json_resp: serde_json::Value = response.json().await?;
    
    // 提取回复内容，使用 as_str() 安全获取字符串
    let content = json_resp["output"]["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("无法解析回复内容")
        .to_string();

    Ok(content)
}
