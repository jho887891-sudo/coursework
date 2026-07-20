use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::env;

///HTTP客户端封装
#[derive(Debug, Clone)]
pub struct LlmClient {
    api_key: String,
    model: String,
    endpoint: String,
    client: reqwest::Client,
}

//四种消息角色
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

///工具定义：描述LLM可调用的函数，使用JSON Schema描述参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

///工具调用：LLM返回的函数调用指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

///LLM响应：包含生成内容和工具调用
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

///请求体中的工具包装
#[derive(Serialize)]
struct RequestTool<'a> {
    #[serde(rename = "type")]
    tool_type: &'static str,
    function: &'a ToolDef,
}

///DashScope  请球体结构
#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    input: Input<'a>,
    parameters: Parameters<'a>,
}

#[derive(Serialize)]
struct Input<'a> {
    messages: &'a [ChatMessage],
}

#[derive(Serialize)]
struct Parameters<'a> {
    result_format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<RequestTool<'a>>>,
}

///DashScope 响应体结构
#[derive(Deserialize)]
struct ApiResponse {
    output: Output,
}

#[derive(Deserialize)]
struct Output {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ResponseToolCall>,
}

#[derive(Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseToolCallFunction,
}

#[derive(Deserialize)]
struct ResponseToolCallFunction {
    name: String,
    arguments: String,
}


impl LlmClient {
    ///从环境变量加载配置并创建客户端
    pub fn from_env() -> Result<Self> {
        // 加载 .env 文件，失败不报错
        dotenv::dotenv().ok();

        //读取API Key,读取失败返回错去
        let api_key = env::var("DASHSCOPE_API_KEY")
            .map_err(|_| anyhow!("未找到 DASHSCOPE_API_KEY 环境变量，请在 .env 中配置"))?;
        let model = env::var("DASHSCOPE_MODEL")
            .map_err(|_| anyhow!("未找到 DASHSCOPE_MODEL 环境变量，请在 .env 中配置"))?;
        //断电地址提供默认值，支持用户通过环境变量自定义
        let endpoint = env::var("DASHSCOPE_ENDPOINT").unwrap_or_else(|_| {
            "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation"
                .to_string()
        });

        let client = reqwest::Client::new();

        Ok(Self {
            api_key,
            model,
            endpoint,
            client,
        })
    }

    ///发送聊天请求，支持普通对话与工具调用
    /// - messages: 对话上下文，使用切片借用，不转移所有权
    /// - tools: 可用工具列表，为空时不启用工具调用能力
    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<LlmResponse> {
        let tools = if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|tool| RequestTool {
                        tool_type: "function",
                        function: tool,
                    })
                    .collect(),//转换为API要求格式并收集为向量
            )
        };

        //组装符合DashScope API规范的请求体
        let request_body = ChatRequest {
            model: &self.model,
            input: Input { messages },
            parameters: Parameters {
                result_format: "message",
                tools,
            },
        };

        //异步HTTP请求发送
        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        //主动校验HTTP状态码，异常时返回错误
        if !response.status().is_success() {
            let status = response.status();
            //尝试读取错误详情，读取失败则忽略，不影响主错误返回
            let _error_body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "{} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            ));
        }

        //将响应JSON解析为预定义的结构体
        let api_response: ApiResponse = response.json().await?;

        //取出第一条错误，列表为空时返回错误
        let choice = api_response
            .output
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("API 响应中无有效回复"))?;

        //将工具调用从API响应格式转换为LlmResponse所需格式
        let tool_calls = choice
            .message
            .tool_calls
            .into_iter()
            .map(|tc| ToolCall {
                id: tc.id,
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect();

        Ok(LlmResponse {
            content: choice.message.content,
            tool_calls,
        })
    }
}
