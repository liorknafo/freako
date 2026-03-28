use futures::StreamExt;

use crate::config::types::{ContextConfig, MemoryConfig};
use crate::provider::types::{LLMContent, LLMMessage, LLMRequest, LLMRole, LLMStreamEvent, LLMTool};
use crate::provider::{LLMProvider, ProviderError};
use crate::session::types::{ConversationMessage, MessagePart, Role};
use crate::tools::ToolRegistry;

pub fn build_request(
    messages: &[ConversationMessage],
    registry: &ToolRegistry,
    model: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    thinking_effort: Option<&str>,
    system_prompt: Option<&str>,
    context_config: &ContextConfig,
    _memory_config: &MemoryConfig,
) -> LLMRequest {
    let request_messages = compact_messages(messages, context_config);
    let llm_messages = request_messages.iter().map(convert_message).collect();
    let tools = registry
        .all_tools()
        .into_iter()
        .map(|t| LLMTool {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters: t.parameters_schema(),
        })
        .collect();

    LLMRequest {
        messages: llm_messages,
        tools,
        model: model.to_string(),
        max_tokens,
        temperature,
        system_prompt: system_prompt.map(|s| s.to_string()),
        thinking_effort: thinking_effort.map(|s| s.to_string()),
    }
}

pub fn compact_messages(
    messages: &[ConversationMessage],
    context_config: &ContextConfig,
) -> Vec<ConversationMessage> {
    let interaction_count: usize = messages.iter().map(|m| m.parts.len().max(1)).sum();
    if !context_config.enable_compaction || interaction_count <= context_config.compact_after_messages {
        return messages.to_vec();
    }

    let keep_recent = context_config
        .keep_recent_messages
        .min(messages.len().saturating_sub(1));
    let split_at = messages.len().saturating_sub(keep_recent);
    if split_at == 0 {
        return messages.to_vec();
    }

    let older = &messages[..split_at];
    let recent = &messages[split_at..];
    let summary = summarize_messages(older);

    if summary.trim().is_empty() {
        return messages.to_vec();
    }

    let mut compacted = Vec::with_capacity(recent.len() + 1);
    compacted.push(ConversationMessage {
        role: Role::System,
        parts: vec![MessagePart::Text { text: summary }],
        timestamp: chrono::Utc::now(),
    });
    compacted.extend_from_slice(recent);
    compacted
}

const COMPACTION_SYSTEM_PROMPT: &str = "\
You are a conversation summarizer. Create a concise summary of the conversation history provided. Focus on:
1. Key decisions made and their rationale
2. Important context, constraints, and user preferences established
3. Files created, modified, or discussed (include paths)
4. Unresolved issues or ongoing work
5. Tool calls and their outcomes that are still relevant

Output a clear, structured summary using bullet points. Preserve all important context needed for continuing the conversation. Be concise — omit routine tool results and redundant details, but don't lose important decisions or state.
Start with 'Conversation summary:'.";

/// LLM-based compaction: sends older messages to the LLM for summarization.
pub async fn llm_compact_messages(
    messages: &[ConversationMessage],
    context_config: &ContextConfig,
    provider: &dyn LLMProvider,
    model: &str,
    max_tokens: u32,
) -> Result<Vec<ConversationMessage>, ProviderError> {
    let interaction_count: usize = messages.iter().map(|m| m.parts.len().max(1)).sum();
    if !context_config.enable_compaction || interaction_count <= context_config.compact_after_messages {
        return Ok(messages.to_vec());
    }

    let keep_recent = context_config
        .keep_recent_messages
        .min(messages.len().saturating_sub(1));
    let split_at = messages.len().saturating_sub(keep_recent);
    if split_at == 0 {
        return Ok(messages.to_vec());
    }

    let older = &messages[..split_at];
    let recent = &messages[split_at..];

    let conversation_text = format_messages_for_llm(older);
    if conversation_text.trim().is_empty() {
        return Ok(messages.to_vec());
    }

    let request = LLMRequest {
        messages: vec![LLMMessage {
            role: LLMRole::User,
            content: vec![LLMContent::Text(conversation_text)],
        }],
        tools: vec![],
        model: model.to_string(),
        max_tokens,
        temperature: Some(0.0),
        system_prompt: Some(COMPACTION_SYSTEM_PROMPT.to_string()),
        thinking_effort: None,
    };

    let stream = provider.stream_message(request).await?;
    let mut summary = String::new();

    futures::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        match event {
            Ok(LLMStreamEvent::TextDelta(text)) => summary.push_str(&text),
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

/// Format messages into a text block for the LLM to summarize.
fn format_messages_for_llm(messages: &[ConversationMessage]) -> String {
    let mut lines = vec!["Summarize the following conversation:\n".to_string()];

    for message in messages {
        let label = match message.role {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        };

        for part in &message.parts {
            match part {
                MessagePart::Text { text } => {
                    let normalized = normalize_whitespace(text);
                    if !normalized.is_empty() {
                        lines.push(format!("[{label}]: {}", truncate_chars(&normalized, 2000)));
                    }
                }
                MessagePart::Image { media_type, .. } => {
                    lines.push(format!("[{label}]: [image: {media_type}]"));
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

fn summarize_messages(messages: &[ConversationMessage]) -> String {
    let mut lines = vec![
        "Compacted conversation summary: earlier messages were condensed to save context window space. Preserve ongoing constraints, decisions, and unresolved work from this summary.".to_string(),
        String::new(),
    ];

    for message in messages {
        let label = match message.role {
            Role::System => "System",
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
        };

        for part in &message.parts {
            match part {
                MessagePart::Text { text } => {
                    let normalized = normalize_whitespace(text);
                    if !normalized.is_empty() {
                        lines.push(format!("- {label}: {normalized}"));
                    }
                }
                MessagePart::Image { media_type, .. } => {
                    lines.push(format!("- {label}: [image: {media_type}]"));
                }
                MessagePart::ToolCall { name, arguments, .. } => {
                    lines.push(format!(
                        "- Assistant tool call `{name}` with arguments: {}",
                        truncate_chars(&arguments.to_string(), 400)
                    ));
                }
                MessagePart::ToolResult { name, content, is_error, .. } => {
                    let status = if *is_error { "error" } else { "result" };
                    let normalized = normalize_whitespace(content);
                    if !normalized.is_empty() {
                        lines.push(format!(
                            "- Tool `{name}` {status}: {}",
                            truncate_chars(&normalized, 500)
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
                            "- Tool output ({stream_label}): {}",
                            truncate_chars(&normalized, 500)
                        ));
                    }
                }
            }
        }
    }

    truncate_chars(&lines.join("\n"), 12_000)
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

fn convert_message(msg: &ConversationMessage) -> LLMMessage {
    let role = match msg.role {
        Role::System => LLMRole::System,
        Role::User => LLMRole::User,
        Role::Assistant => LLMRole::Assistant,
        Role::Tool => LLMRole::Tool,
    };

    let content = msg
        .parts
        .iter()
        .map(|part| match part {
            MessagePart::Text { text } => LLMContent::Text(text.clone()),
            MessagePart::Image { media_type, data } => LLMContent::Image {
                media_type: media_type.clone(),
                data: data.clone(),
            },
            MessagePart::ToolCall { id, name, arguments } => LLMContent::ToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
            },
            MessagePart::ToolResult { tool_call_id, name, content, is_error, .. } => {
                // For sub_agent results, extract just the summary for the LLM
                let llm_content = if name == "sub_agent" {
                    serde_json::from_str::<crate::tools::sub_agent::SubAgentResult>(content)
                        .map(|r| r.summary)
                        .unwrap_or_else(|_| content.clone())
                } else {
                    content.clone()
                };
                LLMContent::ToolResult {
                    tool_call_id: tool_call_id.clone(),
                    content: llm_content,
                    is_error: *is_error,
                }
            }
            MessagePart::ToolOutput { .. } => LLMContent::Text(String::new()),
        })
        .collect();

    LLMMessage { role, content }
}
