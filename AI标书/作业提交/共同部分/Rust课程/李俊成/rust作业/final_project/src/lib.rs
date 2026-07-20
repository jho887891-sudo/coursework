use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;

// 1. 类型定义与 JSON 序列化/反序列化匹配

/// 工具调用的定义（发送给大模型）
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 大模型返回的工具调用指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String, // 百炼返回中包含 type: "function"
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// 匹配 DashScope 的聊天消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        // 百炼在有工具调用时，content 可能为 null 或 字符串，这里用 Option 兼容
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

// 为 ChatMessage 提供快捷构造方法，方便外部使用
impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        ChatMessage::System { content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        ChatMessage::User { content: content.into() }
    }
}

/// 统一对外导出的最终 LLM 响应结果
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ExposedToolCall>,
}

/// 对外暴露的简化版 ToolCall，方便外部通过 tc.name 和 tc.arguments 访问
#[derive(Debug, Clone)]
pub struct ExposedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

// 2. LlmClient 客户端封装与核心方法

pub struct LlmClient {
    api_key: String,
    model: String,
    endpoint: String,
    http_client: Client,
}

impl LlmClient {
    /// 从环境或 .env 文件加载配置初始化客户端
    pub fn from_env() -> anyhow::Result<Self> {
        // 尝试加载 .env 文件，如果不存在则忽略（允许直接读取系统环境变量）
        dotenv::dotenv().ok();

        let api_key = env::var("DASHSCOPE_API_KEY")
            .map_err(|_| anyhow::anyhow!("未找到环境变量 DASHSCOPE_API_KEY，请检查 .env 文件"))?;
            
        // 提供默认模型 qwen-plus
        let model = env::var("DASHSCOPE_MODEL").unwrap_or_else(|_| "qwen-plus".to_string());
        
        let endpoint = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation".to_string();

        Ok(Self {
            api_key,
            model,
            endpoint,
            http_client: Client::new(),
        })
    }

    /// 核心对话方法，支持传入工具列表
    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> anyhow::Result<LlmResponse> {
        // 1. 构造百炼标准的 Payload 请求体
        let mut input = json!({
            "messages": messages
        });

        // 2. 构造 parameters 配置，强制要求返回 message 格式
        let mut parameters = json!({
            "result_format": "message"
        });

        // 如果传入了工具，则放入 parameters 中（百炼的 tools 数组存放在 parameters 里）
        if !tools.is_empty() {
            if let Some(param_obj) = parameters.as_object_mut() {
                param_obj.insert("tools".to_string(), json!(tools));
            }
        }

        let body = json!({
            "model": self.model,
            "input": input,
            "parameters": parameters
        });

        // 3. 发送异步 HTTP 请求
        let resp = self.http_client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        // 4. 非 panic 错误处理：检查状态码并抓取错误信息
        if !resp.status().is_success() {
            let status = resp.status();
            let error_text = resp.text().await?;
            anyhow::bail!("API 响应失败 [状态码 {}]: {}", status, error_text);
        }

        // 5. 解析并提取返回结果
        let raw_json: serde_json::Value = resp.json().await?;
        
        // 定位到 choices 中的第一项 message
        let message_value = &raw_json["output"]["choices"][0]["message"];
        
        if message_value.is_null() {
            anyhow::bail!("API 响应的 JSON 结构异常，未能读取到 choices[0].message");
        }

        // 将其反序列化为我们定义的 Assistant 消息格式
        // 百炼结构示例: {"role": "assistant", "content": "...", "tool_calls": [...]}
        let content: Option<String> = message_value["content"].as_str().map(|s| s.to_string());
        
        let mut exposed_tool_calls = Vec::new();
        if let Some(tool_calls_array) = message_value["tool_calls"].as_array() {
            for tc_val in tool_calls_array {
                let tc: ToolCall = serde_json::from_value(tc_val.clone())?;
                exposed_tool_calls.push(ExposedToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments: tc.function.arguments,
                });
            }
        }

        Ok(LlmResponse {
            content,
            tool_calls: exposed_tool_calls,
        })
    }
}