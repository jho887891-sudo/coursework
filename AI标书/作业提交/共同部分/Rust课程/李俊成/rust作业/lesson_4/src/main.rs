use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::json;
use std::env;

// 1. 定义命令行结构
#[derive(Parser)]
#[command(name = "ai-assistant", author = "Rust Learner", version = "1.0", about = "DashScope AI CLI 助手")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 调用 LLM 回答问题
    Ask {
        /// 你想问 AI 的具体问题
        question: String,
    },
    /// 翻译文本到指定语言
    Translate {
        /// 需要翻译的文本
        text: String,
        /// 目标语言，例如 zh, en, ja，默认为 zh
        #[arg(short, long, default_value = "zh")]
        to: String,
    },
}

// 2. 封装通用的 DashScope API 调用函数
async fn call_dashscope(system_prompt: &str, user_prompt: &str) -> anyhow::Result<String> {
    // 从环境变量中读取 API 密钥
    let api_key = env::var("DASHSCOPE_API_KEY")
        .map_err(|_| anyhow::anyhow!("未找到环境变量 DASHSCOPE_API_KEY，请检查 .env 文件"))?;

    let client = Client::new();
    let url = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

    // 构造请求体 JSON
    let body = json!({
        "model": "qwen-plus",
        "input": {
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_prompt}
            ]
        },
        "parameters": {
            "result_format": "message"
        }
    });

    // 发送异步 POST 请求
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    // 检查 HTTP 状态码是否成功，失败则非 panic 打印状态码与响应体
    if !resp.status().is_success() {
        let status = resp.status();
        let error_text = resp.text().await?;
        anyhow::bail!("API 响应错误 [状态码 {}]: {}", status, error_text);
    }

    // 解析返回的 JSON
    let result: serde_json::Value = resp.json().await?;
    
    // 安全地提取文本回复
    let reply = result["output"]["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("解析 API 返回内容失败，JSON 结构可能已改变"))?;

    Ok(reply.to_string())
}

// 3. 异步主函数入口
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 加载 .env 文件的环境变量
    dotenv::dotenv().ok();

    // 解析命令行参数
    let cli = Cli::parse();

    // 根据子命令执行不同的逻辑
    match cli.command {
        Command::Ask { question } => {
            let system_prompt = "你是一个有帮助的AI助手。";
            // 提示用户正在请求
            println!("正在思考中...");
            
            let reply = call_dashscope(system_prompt, &question).await?;
            println!("\nLLM:\n{}", reply);
        }
        Command::Translate { text, to } => {
            // 动态构造包含目标语言的 System Prompt
            let system_prompt = format!("你是一个翻译助手，将用户输入翻译为{}，只输出译文。", to);
            
            let reply = call_dashscope(&system_prompt, &text).await?;
            // 直接打印译文结果
            println!("{}", reply.trim());
        }
    }

    Ok(())
}