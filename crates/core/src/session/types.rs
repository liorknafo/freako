use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::events::ToolOutputStream;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessagePart {
    Text { text: String },
    Image {
        media_type: String, // e.g. "image/png"
        data: String,       // base64-encoded
    },
    ToolCall { id: String, name: String, arguments: serde_json::Value },
    ToolResult {
        tool_call_id: String,
        name: String,
        content: String,
        is_error: bool,
        #[serde(default)]
        arguments: Option<serde_json::Value>,
    },
    ToolOutput {
        tool_call_id: String,
        stream: ToolOutputStream,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: Role,
    pub parts: Vec<MessagePart>,
    pub timestamp: DateTime<Utc>,
}

impl ConversationMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            parts: vec![MessagePart::Text { text: text.into() }],
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(parts: Vec<MessagePart>) -> Self {
        Self { role: Role::Assistant, parts, timestamp: Utc::now() }
    }

    pub fn tool_result(tool_call_id: String, name: String, content: String, is_error: bool, arguments: Option<serde_json::Value>) -> Self {
        Self {
            role: Role::Tool,
            parts: vec![MessagePart::ToolResult { tool_call_id, name, content, is_error, arguments }],
            timestamp: Utc::now(),
        }
    }

    pub fn full_text(&self) -> String {
        self.parts.iter().filter_map(|p| match p {
            MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join("")
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<ConversationMessage>,
    pub working_directory: String,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

impl Session {
    pub fn new(working_directory: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: "New Session".into(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            working_directory,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }
}
