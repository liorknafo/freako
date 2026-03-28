use std::sync::{Arc, Mutex};

use futures::StreamExt;

use crate::config::types::ContextConfig;
use crate::provider::types::{LLMContent, LLMMessage, LLMRequest, LLMStreamEvent, TokenUsage};
use crate::provider::{LLMProvider, ProviderError};
use crate::session::types::{ConversationMessage, MessagePart, Role};

const SYSTEM_PROMPT: &str = include_str!("prompts/compaction.txt");

pub struct Compaction {
    pub keep_recent_messages: usize,
    pub model: String,
    pub max_tokens: u32,
    pub usage: Arc<Mutex<TokenUsage>>,
}

impl Compaction {
    pub fn new(config: &ContextConfig, model: &str, max_tokens: u32, usage: Arc<Mutex<TokenUsage>>) -> Self {
        Self {
            keep_recent_messages: config.keep_recent_messages,
            model: model.to_string(),
            max_tokens,
            usage,
        }
    }

    /// Sends older messages to the LLM for summarization, keeping recent ones intact.
    /// Updates shared usage. The caller decides when to trigger this.
    pub async fn compact(
        &self,
        messages: &[ConversationMessage],
        provider: &dyn LLMProvider,
    ) -> Result<Vec<ConversationMessage>, ProviderError> {
        let keep_recent = self.keep_recent_messages
            .min(messages.len().saturating_sub(1));
        let split_at = messages.len().saturating_sub(keep_recent);
        if split_at == 0 {
            return Ok(messages.to_vec());
        }

        let older = &messages[..split_at];
        let recent = &messages[split_at..];

        let conversation_text = Self::format_messages(older);
        if conversation_text.trim().is_empty() {
            return Ok(messages.to_vec());
        }

        let request = LLMRequest {
            messages: vec![LLMMessage {
                role: Role::User,
                content: vec![LLMContent::Text(conversation_text)],
            }],
            tools: vec![],
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            temperature: Some(0.0),
            system_prompt: Some(SYSTEM_PROMPT.to_string()),
            thinking_effort: None,
        };

        let stream = provider.stream_message(request).await?;
        let mut summary = String::new();

        futures::pin_mut!(stream);
        while let Some(event) = stream.next().await {
            match event {
                Ok(LLMStreamEvent::TextDelta(text)) => summary.push_str(&text),
                Ok(LLMStreamEvent::Usage(u)) => {
                    let mut usage = self.usage.lock().unwrap();
                    usage.input_tokens += u.input_tokens;
                    usage.output_tokens += u.output_tokens;
                }
                Ok(LLMStreamEvent::Done) => break,
                Err(e) => return Err(e),
                _ => {}
            }
        }

        if summary.trim().is_empty() {
            return Ok(messages.to_vec());
        }

        let mut compacted = Vec::with_capacity(recent.len() + 1);
        compacted.push(ConversationMessage {
            role: Role::System,
            parts: vec![MessagePart::Text { text: summary }],
            timestamp: chrono::Utc::now(),
        });
        compacted.extend_from_slice(recent);
        Ok(compacted)
    }

    fn format_messages(messages: &[ConversationMessage]) -> String {
        let mut lines = Vec::new();

        for message in messages {
            let role = message.role;

            for part in &message.parts {
                match part {
                    MessagePart::Text { text } => {
                        let normalized = normalize_whitespace(text);
                        if !normalized.is_empty() {
                            lines.push(format!("[{role}]: {}", truncate_chars(&normalized, 2000)));
                        }
                    }
                    MessagePart::Image { media_type, .. } => {
                        lines.push(format!("[{role}]: [image: {media_type}]"));
                    }
                    MessagePart::ToolCall { name, arguments, .. } => {
                        lines.push(format!(
                            "[Assistant] Tool call `{name}`: {}",
                            truncate_chars(&arguments.to_string(), 1000)
                        ));
                    }
                    MessagePart::ToolResult { name, content, is_error, .. } => {
                        let status = if *is_error { "error" } else { "result" };
                        let normalized = normalize_whitespace(content);
                        if !normalized.is_empty() {
                            lines.push(format!(
                                "[Tool `{name}` {status}]: {}",
                                truncate_chars(&normalized, 1000)
                            ));
                        }
                    }
                    MessagePart::ToolOutput { stream, content, .. } => {
                        let normalized = normalize_whitespace(content);
                        if !normalized.is_empty() {
                            let stream_label = match stream {
                                crate::agent::events::ToolOutputStream::Stdout => "stdout",
                                crate::agent::events::ToolOutputStream::Stderr => "stderr",
                            };
                            lines.push(format!(
                                "[Tool output ({stream_label})]: {}",
                                truncate_chars(&normalized, 1000)
                            ));
                        }
                    }
                }
            }
        }

        truncate_chars(&lines.join("\n"), 50_000)
    }
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = input.chars().take(max_chars).collect::<String>();
    if input.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}
