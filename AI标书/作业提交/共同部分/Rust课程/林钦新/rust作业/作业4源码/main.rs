use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

const API_URL: &str = "https://api.deepseek.com/chat/completions";
const MODEL: &str = "deepseek-chat";

/// 命令行 AI 助手
#[derive(Parser)]
#[command(name = "ai-assistant", version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 向 AI 助手提问
    Ask {
        /// 要提问的问题
        question: String,
    },
    /// 翻译文本到目标语言
    Translate {
        /// 要翻译的文本
        text: String,
        /// 目标语言（如 zh, ja, en 等）
        #[arg(short, long)]
        to: String,
    },
}

// ---------- DashScope API 类型 ----------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize, Debug)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Deserialize, Debug)]
struct AssistantMessage {
    content: String,
}

// ---------- API 调用 ----------

async fn call_dashscope(system_prompt: &str, user_input: &str) -> Result<String> {
    let api_key =
        std::env::var("DEEPSEEK_API_KEY").context("未设置 DEEPSEEK_API_KEY 环境变量，请在 .env 文件中配置")?;

    let client = reqwest::Client::new();
    let body = ChatRequest {
        model: MODEL.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_input.to_string(),
            },
        ],
    };

    let resp = client
        .post(API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("发送 API 请求失败，请检查网络连接")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("API 返回错误 ({}): {}", status, text);
    }

    let data: ChatResponse = resp.json().await.context("解析 API 响应失败")?;
    let content = data
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .context("API 返回了空的响应")?;

    Ok(content)
}

// ---------- 入口 ----------

#[tokio::main]
async fn main() -> Result<()> {
    // 从 .env 文件加载环境变量（文件不存在时不报错）
    dotenv::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Command::Ask { question } => {
            let answer = call_dashscope("你是一个有帮助的AI助手。", &question).await?;
            println!("{}", answer);
        }
        Command::Translate { text, to } => {
            let system_prompt = format!(
                "你是一个翻译助手，将用户输入翻译为{}，只输出译文。",
                to
            );
            let translation = call_dashscope(&system_prompt, &text).await?;
            println!("{}", translation);
        }
    }

    Ok(())
}
