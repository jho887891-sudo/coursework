use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::json;

#[derive(Parser)]
#[command(name = "ai-assistant")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Ask {
        question: String,
    },
    Translate {
        text: String,
        #[arg(short, long, default_value = "en")]
        to: String,
    },
}

async fn call_dashscope(api_key: &str, system_prompt: &str, user_content: &str) -> Result<String> {
    let client = Client::new();

    let body = json!({
        "model": "qwen-plus",
        "input": {
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_content}
            ]
        },
        "parameters": { "result_format": "message" }
    });

    let resp = client
        .post("https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("发送请求失败")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.context("读取错误响应失败")?;
        bail!("API 错误 {}: {}", status, text);
    }

    let result: serde_json::Value = resp.json().await.context("解析 JSON 失败")?;

    let reply = result
        .get("output")
        .and_then(|o| o.get("choices"))
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str());

    match reply {
        Some(content) => Ok(content.to_string()),
        None => bail!("API 返回格式异常: {:?}", result),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let api_key = std::env::var("DASHSCOPE_API_KEY")
        .context("请在 .env 文件中设置 DASHSCOPE_API_KEY")?;

    if api_key.is_empty() || api_key == "your-api-key-here" {
        bail!("请在 .env 文件中填写有效的 DASHSCOPE_API_KEY");
    }

    let cli = Cli::parse();

    match cli.command {
        Command::Ask { question } => {
            let reply = call_dashscope(&api_key, "你是一个有帮助的AI助手。", &question)
                .await
                .context("调用 LLM 失败")?;
            println!("{}", reply);
        }
        Command::Translate { text, to } => {
            let system_prompt = format!("你是一个翻译助手，将用户输入翻译为{}，只输出译文。", to);
            let reply = call_dashscope(&api_key, &system_prompt, &text)
                .await
                .context("翻译失败")?;
            println!("{}", reply);
        }
    }

    Ok(())
}
