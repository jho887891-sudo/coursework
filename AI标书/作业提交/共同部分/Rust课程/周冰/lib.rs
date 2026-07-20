use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct LlmClient {
    api_key: String,
    model: String,
    endpoint: String,
    http_client: Client,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System { content: String },
    User { content: String },
    #[serde(rename_all = "camelCase")]
    Assistant {
        content: Option<String>,
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

impl ChatMessage {
    pub fn system(content: &str) -> Self {
        ChatMessage::System {
            content: content.to_string(),
        }
    }

    pub fn user(content: &str) -> Self {
        ChatMessage::User {
            content: content.to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Deserialize)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

// DashScope 请求体结构
#[derive(Debug, Serialize)]
struct DashScopeRequest {
    model: String,
    input: DashScopeInput,
    parameters: DashScopeParams,
}

#[derive(Debug, Serialize)]
struct DashScopeInput {
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
struct DashScopeParams {
    result_format: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDef>,
}

// DashScope 响应体结构
#[derive(Debug, Deserialize)]
struct DashScopeResponse {
    output: DashScopeOutput,
}

#[derive(Debug, Deserialize)]
struct DashScopeOutput {
    choices: Vec<DashScopeChoice>,
}

#[derive(Debug, Deserialize)]
struct DashScopeChoice {
    message: DashScopeMessage,
}

#[derive(Debug, Deserialize)]
struct DashScopeMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
}

impl LlmClient {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();
        let api_key = std::env::var("DASHSCOPE_API_KEY")
            .map_err(|_| anyhow!("DASHSCOPE_API_KEY 环境变量未设置"))?;
        let model = std::env::var("DASHSCOPE_MODEL").unwrap_or_else(|_| "qwen-plus".into());
        let endpoint = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation".into();
        let http_client = Client::new();

        Ok(Self {
            api_key,
            model,
            endpoint,
            http_client,
        })
    }

    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<LlmResponse> {
        let request = DashScopeRequest {
            model: self.model.clone(),
            input: DashScopeInput {
                messages: messages.to_vec(),
            },
            parameters: DashScopeParams {
                result_format: "message",
                tools: tools.to_vec(),
            },
        };

        let resp = self
            .http_client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("API 返回 {}: {}", status, body));
        }

        let dash_resp: DashScopeResponse = resp.json().await?;
        let first_choice = dash_resp
            .output
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("无返回消息"))?;
        let msg = first_choice.message;

        Ok(LlmResponse {
            content: msg.content,
            tool_calls: msg.tool_calls,
        })
    }
}