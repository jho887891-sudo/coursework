use anyhow::Result;
use dotenv::dotenv;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;

/// 工具定义，用于告诉 LLM 有哪些函数可以调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

/// LLM 返回的工具调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String, // 通常是 JSON 字符串
}

/// LLM 的响应结构
#[derive(Debug, Deserialize)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// 聊天消息角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    System { content: String },
    User { content: String },
    #[serde(rename_all = "camelCase")]
    Assistant {
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
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

/// LLM 客户端
pub struct LlmClient {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    client: Client,
}

impl LlmClient {
    /// 从环境变量加载配置
    pub fn from_env() -> Result<Self> {
        dotenv().ok();
        let api_key = env::var("DASHSCOPE_API_KEY")
            .map_err(|_| anyhow::anyhow!("缺少 DASHSCOPE_API_KEY 环境变量"))?;
        let model = env::var("DASHSCOPE_MODEL").unwrap_or("qwen-plus".to_string());
        let endpoint = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation"
            .to_string();

        Ok(LlmClient {
            api_key,
            model,
            endpoint,
            client: Client::new(),
        })
    }

    /// 调用 DashScope API
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> Result<LlmResponse> {
        // 构建请求体
        let mut payload = serde_json::json!({
            "model": self.model,
            "input": {
                "messages": messages
            },
            "parameters": {
                "result_format": "message"
            }
        });

        // 如果有工具定义，添加到请求体中
        if !tools.is_empty() {
            payload["parameters"]["tools"] = serde_json::to_value(tools)?;
        }

        // 发送请求
        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        // 错误处理
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            anyhow::bail!("API 请求失败 [{}]: {}", status, text);
        }

        // 解析响应
        let json: Value = response.json().await?;
        
        // 提取 output 部分
        let output = &json["output"];
        
        // 提取 content
        let content = output["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string());

        // 提取 tool_calls
        let tool_calls: Vec<ToolCall> = match &output["choices"][0]["message"]["tool_calls"] {
            Value::Array(arr) => arr
                .iter()
                .filter_map(|tc| {
                    // 注意：DashScope 的 tool_calls 结构中，function 参数在 "function" 字段下
                    let function = &tc["function"];
                    Some(ToolCall {
                        id: tc["id"].as_str().unwrap_or("").to_string(),
                        name: function["name"].as_str().unwrap_or("").to_string(),
                        arguments: function["arguments"].to_string(),
                    })
                })
                .collect(),
            _ => vec![],
        };

        Ok(LlmResponse {
            content,
            tool_calls,
        })
    }
}
