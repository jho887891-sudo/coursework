use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System { content: String },
    User { content: String },
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

    pub fn assistant(content: &str) -> Self {
        ChatMessage::Assistant {
            content: Some(content.to_string()),
            tool_calls: None,
        }
    }

    pub fn tool(tool_call_id: &str, content: &str) -> Self {
        ChatMessage::Tool {
            tool_call_id: tool_call_id.to_string(),
            content: content.to_string(),
        }
    }

    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        if let ChatMessage::Assistant { tool_calls: ref mut tc, .. } = self {
            *tc = Some(tool_calls);
        }
        self
    }
}

#[derive(Debug)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl Serialize for ToolDef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ToolDef", 2)?;
        state.serialize_field("type", "function")?;
        state.serialize_field(
            "function",
            &serde_json::json!({
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }),
        )?;
        state.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallInner {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl<'de> Deserialize<'de> for ToolCall {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawToolCall {
            id: String,
            function: ToolCallInner,
        }
        let raw = RawToolCall::deserialize(deserializer)?;
        Ok(ToolCall {
            id: raw.id,
            name: raw.function.name,
            arguments: raw.function.arguments,
        })
    }
}

impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ToolCall", 3)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("function", &ToolCallInner {
            name: self.name.clone(),
            arguments: self.arguments.clone(),
        })?;
        state.serialize_field("type", "function")?;
        state.end()
    }
}

#[derive(Debug)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug)]
pub struct LlmClient {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    pub client: Client,
}

impl LlmClient {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        let api_key = std::env::var("DASHSCOPE_API_KEY")
            .context("请在 .env 文件中设置 DASHSCOPE_API_KEY")?;

        let model = std::env::var("DASHSCOPE_MODEL").unwrap_or_else(|_| "qwen-plus".to_string());

        let endpoint = std::env::var("DASHSCOPE_ENDPOINT")
            .unwrap_or_else(|_| {
                "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
                    .to_string()
            });

        if api_key.is_empty() || api_key == "your-api-key-here" {
            bail!("请在 .env 文件中填写有效的 DASHSCOPE_API_KEY");
        }

        Ok(Self {
            api_key,
            model,
            endpoint,
            client: Client::new(),
        })
    }

    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<LlmResponse> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
            body["tool_choice"] = serde_json::json!("auto");
        }
        let resp = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
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

        let result: Value = resp.json().await.context("解析 JSON 失败")?;

        let choices = result.get("choices").context("缺少 choices 字段")?;
        let choice = choices.get(0).context("choices 为空")?;
        let message = choice.get("message").context("缺少 message 字段")?;

        let content = message.get("content").and_then(|c| c.as_str()).map(|s| s.to_string());

        let tool_calls: Vec<ToolCall> = message
            .get("tool_calls")
            .and_then(|tc| tc.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            content,
            tool_calls,
        })
    }
}
