//! # ai-client
use serde::Serialize;
use serde::ser::SerializeStruct;

/// DashScope 文本生成接口的默认地址。
const DEFAULT_ENDPOINT: &str = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

/// LLM 客户端
pub struct LlmClient {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    http: reqwest::Client,
}

#[derive(Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System { content: String },
    User { content: String },
    #[serde(rename_all = "camelCase")]
    Assistant {
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

/// 工具定义
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// LLM 返回的工具调用
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 一次 LLM 回复
#[derive(Debug)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

// 构造辅助函数
impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        ChatMessage::System { content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        ChatMessage::User { content: content.into() }
    }
    pub fn assistant(content: Option<String>, tool_calls: Option<Vec<ToolCall>>) -> Self {
        ChatMessage::Assistant { content, tool_calls }
    }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        ChatMessage::Tool { tool_call_id: tool_call_id.into(), content: content.into() }
    }
}

// ToolCall 的序列化
#[derive(Serialize)]
struct FunctionCall<'a> {
    name: &'a str,
    arguments: &'a str,
}

impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("ToolCall", 3)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("type", "function")?;
        s.serialize_field(
            "function",
            &FunctionCall { name: &self.name, arguments: &self.arguments },
        )?;
        s.end()
    }
}

// LlmClient 实现
impl LlmClient {
    pub fn from_env() -> anyhow::Result<Self> {
        // 加载 .env
        dotenv::dotenv().ok();

        let api_key = std::env::var("DASHSCOPE_API_KEY")
            .map_err(|_| anyhow::anyhow!("缺少环境变量 DASHSCOPE_API_KEY"))?;
        let model = std::env::var("DASHSCOPE_MODEL").unwrap_or_else(|_| "qwen-plus".to_string());

        Ok(Self::new(api_key, model))
    }

    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// 调用 LLM。
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> anyhow::Result<LlmResponse> {
        let messages_json: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| serde_json::to_value(m))
            .collect::<Result<_, _>>()?;

        let mut parameters = serde_json::json!({ "result_format": "message" });
        if !tools.is_empty() {
            let tools_json: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            parameters["tools"] = serde_json::Value::Array(tools_json);
        }

        let body = serde_json::json!({
            "model": self.model,
            "input": { "messages": messages_json },
            "parameters": parameters,
        });

        let resp = self
            .http
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            // 例：API 返回 401 Unauthorized
            anyhow::bail!("API 返回 {}", status);
        }

        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("响应不是合法 JSON: {}; 原文: {}", e, text))?;
        if let Some(code) = value.get("code").and_then(|c| c.as_str()) {
            let msg = value.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误");
            anyhow::bail!("API 错误 {}: {}", code, msg);
        }

        let message = &value["output"]["choices"]
            .get(0)
            .and_then(|c| c.get("message"))
            .ok_or_else(|| anyhow::anyhow!("响应缺少 output.choices[0].message"))?;

        let content = message
            .get("content")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());

        // 7) 解析工具调用（optional）。arguments 是 JSON 字符串，原样保留。
        let mut tool_calls = Vec::new();
        if let Some(calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
            for call in calls {
                let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let func = call.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}")
                    .to_string();
                tool_calls.push(ToolCall { id, name, arguments });
            }
        }

        Ok(LlmResponse { content, tool_calls })
    }
}
