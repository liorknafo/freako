use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub messages: Vec<LLMMessage>,
    pub tools: Vec<LLMTool>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
    /// Thinking/reasoning effort: "off", "low", "medium", "high"
    pub thinking_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LLMRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct LLMMessage {
    pub role: LLMRole,
    pub content: Vec<LLMContent>,
}

#[derive(Debug, Clone)]
pub enum LLMContent {
    Text(String),
    Image {
        media_type: String, // e.g. "image/png"
        data: String,       // base64-encoded
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        content: String,
        is_error: bool,
    },
}

impl LLMContent {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LLMTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub enum LLMStreamEvent {
    TextDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta(String),
    ToolCallEnd,
    Usage(TokenUsage),
    Done,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
