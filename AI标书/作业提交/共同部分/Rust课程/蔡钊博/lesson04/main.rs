use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

// ============================================================
// CLI 定义
// ============================================================

/// 命令行 AI 助手（基于 DashScope）
#[derive(Parser)]
#[command(name = "ai")]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 向 AI 提问
    Ask {
        /// 你想问的问题
        question: String,
    },
    /// 翻译文本到目标语言
    Translate {
        /// 要翻译的文本
        text: String,
        /// 目标语言（如 en、zh、ja、fr）
        #[arg(long, default_value = "en")]
        to: String,
    },
}

// ============================================================
// DashScope API 数据结构
// ============================================================

const API_BASE: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const MODEL: &str = "qwen-plus";

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

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: String,
}

// ============================================================
// API 调用
// ============================================================

fn call_api(system_prompt: &str, user_message: &str) -> Result<String> {
    // 从环境变量读取 API key（.env 已在 main 中加载）
    let api_key = std::env::var("DASHSCOPE_API_KEY")
        .context("未设置 DASHSCOPE_API_KEY，请在 .env 文件中配置")?;

    let client = reqwest::blocking::Client::new();

    let request = ChatRequest {
        model: MODEL.to_string(),
        messages: vec![
            Message {
                role: "system".into(),
                content: system_prompt.into(),
            },
            Message {
                role: "user".into(),
                content: user_message.into(),
            },
        ],
    };

    let response = client
        .post(format!("{}/chat/completions", API_BASE))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .context("API 请求发送失败，请检查网络连接")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        anyhow::bail!("API 返回错误 ({}): {}", status, body);
    }

    let chat_response: ChatResponse = response
        .json()
        .context("解析 API 响应失败")?;

    chat_response
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .context("API 响应中没有内容")
}

// ============================================================
// 子命令处理
// ============================================================

fn handle_ask(question: &str) -> Result<()> {
    println!("🤖 正在思考…\n");
    let answer = call_api("你是一个有帮助的AI助手。", question)?;
    println!("{}", answer);
    Ok(())
}

fn handle_translate(text: &str, target_lang: &str) -> Result<()> {
    let system_prompt = format!(
        "你是一个翻译助手。将用户输入翻译为{target_lang}，只输出译文，不要添加任何解释。",
        target_lang = target_lang
    );

    println!("🌐 正在翻译为 {}…\n", target_lang);
    let translated = call_api(&system_prompt, text)?;
    println!("{}", translated);
    Ok(())
}

// ============================================================
// main
// ============================================================

fn main() -> Result<()> {
    // 加载 .env 文件（不存在也不报错，可能用系统环境变量）
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    match cli.command {
        Command::Ask { question } => handle_ask(&question)?,
        Command::Translate { text, to } => handle_translate(&text, &to)?,
    }

    Ok(())
}
