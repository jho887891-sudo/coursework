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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    let api_key = std::env::var("DASHSCOPE_API_KEY")
        .map_err(|_| anyhow::anyhow!("请在 .env 文件中设置 DASHSCOPE_API_KEY"))?;

    let cli = Cli::parse();
    let client = Client::new();

    match cli.command {
        Command::Ask { question } => {
            let reply = call_llm(&client, &api_key, "你是一个有帮助的AI助手。", &question).await?;
            println!("{}", reply);
        }
        Command::Translate { text, to } => {
            let system_prompt = format!("你是一个翻译助手，将用户输入翻译为{}，只输出译文。", to);
            let reply = call_llm(&client, &api_key, &system_prompt, &text).await?;
            println!("{}", reply);
        }
    }

    Ok(())
}

async fn call_llm(client: &Client, api_key: &str, system_prompt: &str, user_content: &str) -> anyhow::Result<String> {
    let body = json!({
        "model": "deepseek-chat",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_content}
            ]
    });

    let resp = client
        .post("https://api.deepseek.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await?;
        eprintln!("API 错误 {}: {}", status, text);
        std::process::exit(1);
    }

    let result: serde_json::Value = resp.json().await?;
    let reply = &result["choices"][0]["message"]["content"];
    Ok(reply.as_str().unwrap_or("").to_string())
}
