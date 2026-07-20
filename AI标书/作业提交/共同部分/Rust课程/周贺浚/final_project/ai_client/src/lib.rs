use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};

pub struct LlmClient {
    api_key: String,
    model: String,
    endpoint: String,
    client: Client,
}

impl LlmClient {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();
        let api_key = std::env::var("DASHSCOPE_API_KEY")
            .context("请在 .env 文件中设置 DASHSCOPE_API_KEY")?;
        let model = std::env::var("DASHSCOPE_MODEL")
            .unwrap_or_else(|_| "deepseek-chat".to_string());
        let endpoint = std::env::var("DASHSCOPE_ENDPOINT")
            .unwrap_or_else(|_| "https://api.deepseek.com/v1/chat/completions".to_string());

        Ok(LlmClient { api_key, model, endpoint, client: Client::new() })
    }

    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<LlmResponse> {
        let mut body = json!({
            "model": self.model,
            "messages": messages,
        });

        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools.iter().map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            }).collect();
            body["tools"] = json!(tools_json);
        }

        let resp = self.client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("网络请求失败")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await?;
            anyhow::bail!("API 返回 {} {}", status, text);
        }

        let result: Value = resp.json().await?;
        let message = &result["choices"][0]["message"];

        let content = message["content"].as_str().map(|s| s.to_string());

        let tool_calls = message["tool_calls"]
            .as_array()
            .map(|calls| {
                calls.iter().map(|tc| ToolCall {
                    id: tc["id"].as_str().unwrap_or("").to_string(),
                    name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                    arguments: tc["function"]["arguments"].as_str().unwrap_or("").to_string(),
                }).collect()
            })
            .unwrap_or_default();

        Ok(LlmResponse { content, tool_calls })
    }
}

#[derive(Debug, Clone, Serialize)]
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
    pub fn system(content: impl Into<String>) -> Self {
        ChatMessage::System { content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        ChatMessage::User { content: content.into() }
    }
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ToolCall", 3)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("type", "function")?;
        let func = json!({
            "name": self.name,
            "arguments": self.arguments,
        });
        s.serialize_field("function", &func)?;
        s.end()
    }
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}
