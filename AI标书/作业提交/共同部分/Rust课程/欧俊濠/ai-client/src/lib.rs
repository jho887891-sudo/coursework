//! ai-client：封装阿里云 DashScope 通义千问 LLM API
//! 支持普通对话、工具调用、环境变量配置、完善错误处理

use anyhow::{anyhow, Result};
use dotenv::dotenv;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// LLM 客户端核心结构体，存储 API 配置与 HTTP 客户端
#[derive(Debug, Clone)]
pub struct LlmClient {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    http_client: Client,
}

/// 对话消息枚举，适配 DashScope 四种角色消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    /// 系统提示消息
    System { content: String },
    /// 用户提问消息
    User { content: String },
    /// AI 回复消息（支持文本+工具调用）
    #[serde(rename_all = "camelCase")]
    Assistant {
        content: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
    },
    /// 工具返回结果消息
    Tool {
        tool_call_id: String,
        content: String,
    },
}

impl ChatMessage {
    /// 快速创建系统消息
    pub fn system(content: &str) -> Self {
        Self::System {
            content: content.to_string(),
        }
    }

    /// 快速创建用户消息
    pub fn user(content: &str) -> Self {
        Self::User {
            content: content.to_string(),
        }
    }
}

/// 工具定义结构体，用于向 LLM 声明可用工具（JSON Schema）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// LLM 返回的工具调用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// 工具名称（来自 API 的 function.name）
    pub name: String,
    /// 工具参数（JSON 字符串或对象统一转为 Value）
    #[serde(deserialize_with = "deserialize_arguments")]
    pub arguments: Value,
}

/// 自定义反序列化：处理 arguments 可能是 JSON 字符串或对象的情况
fn deserialize_arguments<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    match v {
        Value::String(s) => serde_json::from_str(&s).map_err(serde::de::Error::custom),
        other => Ok(other),
    }
}

/// 适配 DashScope API 返回的工具调用格式（function 包裹层）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCallData {
    pub id: String,
    #[serde(rename = "function")]
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCallFunction {
    pub name: String,
    #[serde(deserialize_with = "deserialize_arguments")]
    pub arguments: Value,
}

/// LLM 统一响应结构体，封装文本内容与工具调用列表
#[derive(Debug, Clone, Default)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

impl LlmClient {
    /// 直接使用指定的 API Key 和模型名创建客户端
    pub fn new(api_key: String, model: String) -> Self {
        let endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions".to_string();
        let http_client = Client::new();
        Self {
            api_key,
            model,
            endpoint,
            http_client,
        }
    }

    /// 从 .env 文件加载配置，初始化客户端
    /// 读取变量：DASHSCOPE_API_KEY、DASHSCOPE_MODEL（可选，默认 qwen-plus）
    pub fn from_env() -> Result<Self> {
        // 加载环境变量文件
        dotenv().ok();

        // 读取必填 API 密钥
        let api_key = std::env::var("DASHSCOPE_API_KEY")
            .map_err(|_| anyhow!("缺失环境变量 DASHSCOPE_API_KEY，请检查 .env 文件"))?;

        // 读取模型名称，设置默认值 qwen-plus
        let model = std::env::var("DASHSCOPE_MODEL").unwrap_or_else(|_| "qwen-plus".to_string());

        // DashScope 统一兼容接口地址
        let endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions".to_string();

        // 初始化 HTTP 客户端
        let http_client = Client::new();

        Ok(Self {
            api_key,
            model,
            endpoint,
            http_client,
        })
    }

    /// 核心对话接口：支持普通对话 + 工具调用
    ///
    /// # 参数
    /// - `messages`: 对话上下文消息列表
    /// - `tools`: 工具定义列表，无工具则传空数组 `&[]`
    ///
    /// # 返回
    /// - `LlmResponse`：包含 LLM 回复文本（如有）和工具调用列表（如有）
    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<LlmResponse> {
        // 构建请求体基础参数
        let mut req_body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        // 存在工具则追加 tools 字段
        if !tools.is_empty() {
            req_body["tools"] = serde_json::to_value(tools)?;
        }

        // 发送 HTTP POST 请求
        let resp = self
            .http_client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| anyhow!("网络请求失败：{}", e))?;

        // 处理 HTTP 状态码错误（如 401 Unauthorized）
        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            // 提取错误详情（DashScope 可能在 body 中返回 code 和 message）
            let err_detail = if let Ok(val) = serde_json::from_str::<Value>(&err_body) {
                val["message"].as_str().unwrap_or(&err_body).to_string()
            } else {
                err_body
            };
            return Err(anyhow!(
                "API 返回 {} {}{}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown"),
                if err_detail.is_empty() {
                    String::new()
                } else {
                    format!(": {}", err_detail)
                }
            ));
        }

        // 解析响应 JSON
        let resp_json: Value = resp.json().await.map_err(|e| anyhow!("响应解析失败：{}", e))?;
        let choice = resp_json["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| anyhow!("API 响应无有效内容"))?;
        let msg = &choice["message"];

        // 提取文本内容
        let content = msg["content"].as_str().map(|s| s.to_string());

        // 解析工具调用（DashScope 返回的 tool_calls 含 function 包裹层）
        let tool_calls = if let Some(tc_array) = msg["tool_calls"].as_array() {
            tc_array
                .iter()
                .filter_map(|tc| {
                    let data: ToolCallData = serde_json::from_value(tc.clone()).ok()?;
                    Some(ToolCall {
                        id: data.id,
                        name: data.function.name,
                        arguments: data.function.arguments,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(LlmResponse { content, tool_calls })
    }
}
