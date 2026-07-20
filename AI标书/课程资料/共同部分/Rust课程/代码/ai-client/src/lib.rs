// 课程问题：
//   把 lesson-04 的 call_llm 函数改造成一个别人可以 import 的库。
//   使用这个库的人，不应该知道 DashScope 的 endpoint、
//   Authorization Header、JSON 字段名——只应该知道"发消息、拿回复"。
//
// 这节课的核心：公开什么、隐藏什么。
//   公开的 → 使用者的概念模型（消息、工具、回复）
//   隐藏的 → 传输格式、序列化细节、HTTP 细节

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_ENDPOINT: &str =
    "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

// ════════════════════════════════════════════════════════════════
// 公开 API —— 使用者需要知道的四个类型
// ════════════════════════════════════════════════════════════════

pub struct LlmClient {
    api_key: String,          // 字段私有：使用者不需要直接碰密钥
    model: String,
    endpoint: String,
    http: reqwest::Client,    // reqwest::Client 内部自带连接池，复用 TLS 握手
}

// ChatMessage：使用者的概念模型。
// 四种角色对应一次完整对话中的四类参与者。
#[derive(Debug, Clone)]
pub enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,         // 纯文本回复（工具调用时可能为空）
        tool_calls: Vec<ToolCall>,       // LLM 想调用的工具列表
    },
    Tool {
        tool_call_id: String,            // 对应 ToolCall.id，把结果"贴回"对应的调用
        content: String,                 // 工具执行结果（通常是 JSON 字符串）
    },
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: content.into(),
        }
    }
}

// ToolDef：使用者在调用前定义"我有哪些工具"。
//   name        → LLM 通过这个名字决定调用哪个
//   description → LLM 通过这段描述判断何时调用
//   parameters  → JSON Schema，告诉 LLM 参数格式
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Value, // serde_json::Value —— 任意合法的 JSON Schema
}

// ToolCall：LLM 的调用请求，不是执行结果。
// LLM 说"我想调用 get_time，参数是 {"timezone":"Asia/Shanghai"}"。
// 真正执行工具的是你的程序。LLM 只负责提议。
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,       // DashScope 生成的调用 ID，回传 Tool 消息时需要对应
    pub name: String,     // 工具名，对应 ToolDef.name
    pub arguments: String,// JSON 字符串，需要自己解析成具体参数
}

// LlmResponse：一次 chat 调用的返回值。
//   content     → 纯文本回复（可能有，可能没有）
//   tool_calls  → LLM 想调用的工具（可能是空的）
// 两者可以同时为空（少见但合法），也可以同时有值（少见但合法）。
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

// ════════════════════════════════════════════════════════════════
// 私有实现 —— 使用者不需要知道以下任何类型
// ════════════════════════════════════════════════════════════════

// ── 请求序列化层 ──
// 这一层只做一件事：把 ChatMessage → DashScope 要求的 JSON 格式。
// #[serde(tag = "role")] 自动给 JSON 加 "role": "system"/"user"/"assistant"/"tool"
// 使用者不需要知道这个字段名，库替他们拼好了。

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    input: RequestInput<'a>,
    parameters: RequestParameters<'a>,
}

#[derive(Serialize)]
struct RequestInput<'a> {
    messages: Vec<RequestMessage<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "role", rename_all = "lowercase")]
enum RequestMessage<'a> {
    System {
        content: &'a str,
    },
    User {
        content: &'a str,
    },
    Assistant {
        content: &'a Option<String>,
        tool_calls: Vec<RequestToolCall<'a>>,
    },
    Tool {
        tool_call_id: &'a str,
        content: &'a str,
    },
}

#[derive(Serialize)]
struct RequestToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,        // 固定 "function"，DashScope 的 API 要求
    function: RequestFunction<'a>,
}

#[derive(Serialize)]
struct RequestFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Serialize)]
struct RequestParameters<'a> {
    result_format: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")] // 无工具时不传 tools 字段
    tools: Vec<RequestTool<'a>>,
}

#[derive(Serialize)]
struct RequestTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: &'a ToolDef,     // 直接借走使用者的 ToolDef，零拷贝序列化
}

// ── 响应反序列化层 ──
// 只定义我们需要的字段。DashScope 返回的 JSON 里还有 usage、request_id 等，
// 不定义 = serde 自动忽略。

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
    function: ResponseFunction,
}

#[derive(Deserialize)]
struct ResponseFunction {
    name: String,
    arguments: String,
}

// ════════════════════════════════════════════════════════════════
// LlmClient 实现
// ════════════════════════════════════════════════════════════════

impl LlmClient {
    // from_env()：从环境变量读取配置。
    // 不把 api_key 写进代码、不写进配置文件——密钥永远走环境变量。
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();
        let api_key =
            std::env::var("DASHSCOPE_API_KEY").context("请在 .env 中设置 DASHSCOPE_API_KEY")?;
        let model = std::env::var("DASHSCOPE_MODEL").context("请在 .env 中设置 DASHSCOPE_MODEL")?;
        let endpoint =
            std::env::var("DASHSCOPE_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.into());

        if api_key.trim().is_empty() {
            bail!("DASHSCOPE_API_KEY 不能为空");
        }

        Ok(Self {
            api_key,
            model,
            endpoint,
            http: reqwest::Client::new(),
        })
    }

    // chat()：库的唯一核心方法。
    //   传入：消息历史 + 工具定义（没有就传 &[]）
    //   返回：文本回复 + 工具调用请求（可能都有，可能只有一种）
    //
    // 内部流程：
    //   ChatMessage → From 转换 → RequestMessage（加 role 标签）
    //   → serde 序列化 → HTTP POST → DashScope
    //   → 反序列化 → ApiResponse → 提取 LlmResponse
    pub async fn chat(&self, messages: &[ChatMessage], tools: &[ToolDef]) -> Result<LlmResponse> {
        let request = ChatRequest {
            model: &self.model,
            input: RequestInput {
                messages: messages.iter().map(RequestMessage::from).collect(),
                //                      ↑ From<&ChatMessage> for RequestMessage
                //                        把公开类型转换成 DashScope 私有格式
            },
            parameters: RequestParameters {
                result_format: "message",
                tools: tools
                    .iter()
                    .map(|function| RequestTool {
                        kind: "function",
                        function,
                    })
                    .collect(),
            },
        };

        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("无法连接 DashScope")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("API 返回 {status}：{body}");
        }

        let response: ApiResponse = response.json().await.context("API 返回了无法解析的数据")?;
        let message = response
            .output
            .choices
            .into_iter()
            .next()
            .context("API 没有返回任何回复")?
            .message;

        // 把 DashScope 私有的 ResponseToolCall 转成公开的 ToolCall
        Ok(LlmResponse {
            content: message.content,
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(|call| ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: call.function.arguments,
                })
                .collect(),
        })
    }
}

// ════════════════════════════════════════════════════════════════
// 类型转换层 —— 公开类型 ↔ DashScope 私有格式
// ════════════════════════════════════════════════════════════════
//
// 这是库设计的核心技巧：
//   对外暴露泛化的 ChatMessage（四种角色，概念清晰）
//   对内转换成 DashScope 要求的 RequestMessage（带 role 标签、type: "function"）
//
// 使用者的代码里永远不会出现 "role" / "tool_calls[0].type" / "function" 这些字段名。
// 拼错了？编译不过。API 改字段名？只改这一处。

impl<'a> From<&'a ChatMessage> for RequestMessage<'a> {
    fn from(message: &'a ChatMessage) -> Self {
        match message {
            ChatMessage::System { content } => Self::System { content },
            ChatMessage::User { content } => Self::User { content },
            ChatMessage::Assistant {
                content,
                tool_calls,
            } => Self::Assistant {
                content,
                tool_calls: tool_calls
                    .iter()
                    .map(|call| RequestToolCall {
                        id: &call.id,
                        kind: "function",
                        function: RequestFunction {
                            name: &call.name,
                            arguments: &call.arguments,
                        },
                    })
                    .collect(),
            },
            ChatMessage::Tool {
                tool_call_id,
                content,
            } => Self::Tool {
                tool_call_id,
                content,
            },
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 测试
// ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // 验证 Assistant 消息的序列化格式和 DashScope API 一致。
    // 测试关心的不是"能跑"，而是"生成的对端能识别的正确格式"。
    #[test]
    fn assistant_tool_call_uses_the_api_wire_format() {
        let message = ChatMessage::Assistant {
            content: None,
            tool_calls: vec![ToolCall {
                id: "call-1".into(),
                name: "get_time".into(),
                arguments: "{}".into(),
            }],
        };

        let json = serde_json::to_value(RequestMessage::from(&message)).unwrap();
        assert_eq!(json["tool_calls"][0]["function"]["name"], "get_time");
    }
}

// ════════════════════════════════════════════════════════════════
// 本课总结
// ════════════════════════════════════════════════════════════════
//
// 好的封装不是把代码塞进 struct，而是给使用者一个稳定、简单、难以误用的入口。
//
// 公开（pub）vs 私有（无 pub）的边界：
//   pub LlmClient    ← 使用者需要创建
//   pub ChatMessage  ← 使用者需要构造
//   pub ToolDef      ← 使用者需要定义
//   pub ToolCall     ← 使用者需要读取 LLM 的调用请求
//   pub LlmResponse  ← 使用者需要拿到回复
//
//   (私有) ChatRequest / RequestMessage / ApiResponse  ← 传输格式，和用户无关
//
// 类型转换层（From<&ChatMessage> for RequestMessage）：
//   对外：泛化的 ChatMessage（System / User / Assistant / Tool）
//   对内：DashScope 要求的带 role 标签的 JSON 格式
//   使用者永远不需要拼 "role": "system"——库替他们拼好了。
//
// 工具调用的完整循环：
//   用户提问 → LLM 返回 ToolCall（只提建议，没执行）
//   → 你的程序执行工具 → 结果包成 Tool 消息 → 再次 chat()
//   → LLM 看到结果，生成最终回复
//
// 验收标准不是代码能跑，而是：
//   一个没看过你代码的人，能不能通过 Cargo.toml 引入你的库，
//   三行代码就完成一次对话？
