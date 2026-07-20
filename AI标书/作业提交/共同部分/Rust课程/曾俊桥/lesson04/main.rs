use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::json;
use std::env;

// 命令行参数定义
#[derive(Parser)]
#[command(name = "ai-cli")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 向AI提问
    Ask {
        /// 问题内容
        question: String,
    },
    /// 翻译文本
    Translate {
        /// 待翻译文本
        text: String,
        /// 目标语言
        #[arg(short, long, default_value = "en")]
        to: String,
    },
}

/// 调用 DashScope 大模型
async fn call_llm(
    client: &Client,
    api_key: &str,
    system_prompt: &str,
    user_input: &str,
) -> Result<String> {
    const API_URL: &str =
        "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

    let body = json!({
        "model": "qwen-plus",
        "input": {
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_input}
            ]
        },
        "parameters": {
            "result_format": "message"
        }
    });

    // 发送HTTP请求
    let resp = client
        .post(API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("网络请求失败")?;

    // 处理请求失败，不 panic
    if !resp.status().is_success() {
        let status = resp.status();
        let err_msg = resp.text().await.context("读取错误响应失败")?;
        bail!("API 调用失败，状态码：{}，错误信息：{}", status, err_msg);
    }

    // 解析返回结果
    let result: serde_json::Value = resp.json().await.context("解析响应 JSON 失败")?;
    let reply = result["output"]["choices"][0]["message"]["content"]
        .as_str()
        .context("响应格式异常，无法提取回复内容")?;

    Ok(reply.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    // 加载 .env 环境变量
    dotenv::dotenv().ok();

    // 解析命令行参数
    let cli = Cli::parse();

    // 读取 API Key
    let api_key = env::var("DASHSCOPE_API_KEY")
        .context("未找到 DASHSCOPE_API_KEY，请在 .env 文件中配置")?;

    let client = Client::new();

    // 分发子命令
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