use crate::provider::types::{LLMContent, LLMMessage, LLMRequest, LLMTool, ThinkingEffort};
use crate::session::types::{ConversationMessage, MessagePart};
use crate::tools::ToolRegistry;

pub fn build_request(
    messages: &[ConversationMessage],
    registry: &ToolRegistry,
    model: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    thinking_effort: Option<ThinkingEffort>,
    system_prompt: Option<&str>,
) -> LLMRequest {
    let llm_messages = messages.iter().map(convert_message).collect();
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
        thinking_effort,
    }
}

fn convert_message(msg: &ConversationMessage) -> LLMMessage {
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

    LLMMessage { role: msg.role, content }
}
