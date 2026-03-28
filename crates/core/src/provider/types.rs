use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingEffort {
    Low,
    Medium,
    High,
}

impl fmt::Display for ThinkingEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

impl ThinkingEffort {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LLMRequest {
    pub messages: Vec<LLMMessage>,
    pub tools: Vec<LLMTool>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
    pub thinking_effort: Option<ThinkingEffort>,
}

pub use crate::session::types::Role;

#[derive(Debug, Clone)]
pub struct LLMMessage {
    pub role: Role,
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
