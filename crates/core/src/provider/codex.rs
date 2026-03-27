//! OpenAI Codex provider — uses the Responses API at chatgpt.com/backend-api
//! for ChatGPT subscription (OAuth) authentication.
//!
//! The official Codex CLI posts to `https://chatgpt.com/backend-api/codex/responses`
//! using the Responses API format, which works with the OAuth tokens that only
//! carry identity scopes (openid, profile, email, offline_access).

use async_trait::async_trait;
use serde_json::json;

use crate::config::types::ProviderConfig;
use crate::provider::types::*;
use crate::provider::{LLMProvider, ProviderError, StreamResult};

const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

pub struct CodexProvider {
    client: reqwest::Client,
    access_token: String,
}

impl CodexProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let access_token = config
            .openai_oauth
            .as_ref()
            .map(|o| o.access_token.clone())
            .ok_or_else(|| ProviderError::Config("OpenAI OAuth not configured".into()))?;

        Ok(Self {
            client: reqwest::Client::new(),
            access_token,
        })
    }
}

/// Ensure a tool call ID uses the Responses API `fc_` prefix.
fn normalize_call_id(id: &str) -> String {
    if id.starts_with("fc_") {
        id.to_string()
    } else if let Some(rest) = id.strip_prefix("call_") {
        format!("fc_{}", rest)
    } else {
        format!("fc_{}", id)
    }
}

/// Build the Responses API `input` array from our internal message format.
/// Ensures every function_call has a matching output and vice-versa.
fn build_input(request: &LLMRequest) -> Vec<serde_json::Value> {
    use std::collections::HashSet;

    // First pass: collect all tool_call IDs and all tool_result IDs
    let mut all_call_ids = HashSet::new();
    let mut all_result_ids = HashSet::new();
    for msg in &request.messages {
        match msg.role {
            LLMRole::Assistant => {
                for part in &msg.content {
                    if let LLMContent::ToolCall { id, .. } = part {
                        all_call_ids.insert(normalize_call_id(id));
                    }
                }
            }
            LLMRole::Tool => {
                for part in &msg.content {
                    if let LLMContent::ToolResult { tool_call_id, .. } = part {
                        all_result_ids.insert(normalize_call_id(tool_call_id));
                    }
                }
            }
            _ => {}
        }
    }

    let mut input = Vec::new();

    for msg in &request.messages {
        match msg.role {
            LLMRole::User => {
                let mut content = Vec::new();
                for part in &msg.content {
                    match part {
                        LLMContent::Text(text) => {
                            content.push(json!({ "type": "input_text", "text": text }));
                        }
                        LLMContent::Image { media_type, data } => {
                            let data_url = format!("data:{};base64,{}", media_type, data);
                            content.push(json!({ "type": "input_image", "image_url": data_url }));
                        }
                        _ => {}
                    }
                }
                if content.is_empty() {
                    content.push(json!({ "type": "input_text", "text": "" }));
                }
                input.push(json!({
                    "type": "message",
                    "role": "user",
                    "content": content
                }));
            }
            LLMRole::Assistant => {
                let mut text_content = Vec::new();
                for part in &msg.content {
                    if let LLMContent::Text(text) = part {
                        text_content.push(json!({ "type": "output_text", "text": text }));
                    }
                }
                if !text_content.is_empty() {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": text_content
                    }));
                }
                // Tool calls as top-level items
                for part in &msg.content {
                    if let LLMContent::ToolCall { id, name, arguments } = part {
                        let fc_id = normalize_call_id(id);
                        input.push(json!({
                            "type": "function_call",
                            "id": &fc_id,
                            "call_id": &fc_id,
                            "name": name,
                            "arguments": arguments.to_string()
                        }));
                        // If this call has no matching result, emit an error output
                        if !all_result_ids.contains(&fc_id) {
                            input.push(json!({
                                "type": "function_call_output",
                                "call_id": &fc_id,
                                "output": "Error: tool call was not executed (no result available)"
                            }));
                        }
                    }
                }
            }
            LLMRole::Tool => {
                for part in &msg.content {
                    if let LLMContent::ToolResult { tool_call_id, content, .. } = part {
                        let fc_id = normalize_call_id(tool_call_id);
                        // Skip orphaned results with no matching function_call
                        if !all_call_ids.contains(&fc_id) {
                            continue;
                        }
                        input.push(json!({
                            "type": "function_call_output",
                            "call_id": &fc_id,
                            "output": content
                        }));
                    }
                }
            }
            LLMRole::System => {}
        }
    }

    input
}

/// Build tools array in Responses API format.
fn build_tools(tools: &[LLMTool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect()
}

/// Parse a single SSE event from the Responses API stream.
fn parse_sse_event(event_type: &str, data: &str) -> Option<Result<LLMStreamEvent, ProviderError>> {
    let v: serde_json::Value = serde_json::from_str(data).ok()?;

    match event_type {
        "response.output_text.delta" => {
            let delta = v.get("delta")?.as_str()?.to_string();
            Some(Ok(LLMStreamEvent::TextDelta(delta)))
        }
        "response.function_call_arguments.delta" => {
            let delta = v.get("delta")?.as_str()?.to_string();
            Some(Ok(LLMStreamEvent::ToolCallDelta(delta)))
        }
        "response.output_item.added" => {
            let item = v.get("item")?;
            let item_type = item.get("type")?.as_str()?;
            if item_type == "function_call" {
                let id = item.get("call_id").or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())?.to_string();
                let name = item.get("name")?.as_str()?.to_string();
                Some(Ok(LLMStreamEvent::ToolCallStart { id, name }))
            } else {
                None
            }
        }
        "response.output_item.done" => {
            let item = v.get("item")?;
            let item_type = item.get("type")?.as_str()?;
            if item_type == "function_call" {
                Some(Ok(LLMStreamEvent::ToolCallEnd))
            } else {
                None
            }
        }
        "response.completed" => {
            if let Some(response) = v.get("response") {
                if let Some(usage) = response.get("usage") {
                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    return Some(Ok(LLMStreamEvent::Usage(TokenUsage { input_tokens: input, output_tokens: output })));
                }
            }
            Some(Ok(LLMStreamEvent::Done))
        }
        "error" => {
            let msg = v.get("error")
                .or_else(|| v.get("message"))
                .and_then(|e| {
                    if e.is_string() { e.as_str().map(|s| s.to_string()) }
                    else { e.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()) }
                })
                .unwrap_or_else(|| data.to_string());
            Some(Err(ProviderError::OpenAI(msg)))
        }
        _ => None,
    }
}

#[async_trait]
impl LLMProvider for CodexProvider {
    async fn stream_message(&self, request: LLMRequest) -> Result<StreamResult, ProviderError> {
        let input = build_input(&request);
        let tools = build_tools(&request.tools);

        let mut body = json!({
            "model": request.model,
            "input": input,
            "stream": true,
            "store": false,
        });
        if let Some(system) = &request.system_prompt {
            body["instructions"] = json!(system);
        }
        // temperature not supported on the Codex backend
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        // Thinking/reasoning effort support
        if let Some(effort) = &request.thinking_effort {
            if effort != "off" {
                body["reasoning"] = json!({"effort": effort});
            }
        }

        let url = format!("{}/responses", CODEX_BASE_URL);
        let body_str = body.to_string();

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(ProviderError::Http)?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(ProviderError::OpenAI(format!("{}: {}", status, error_body)));
        }

        // Stream SSE
        use futures::StreamExt;
        let byte_stream = response.bytes_stream();

        let stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut current_event = String::new();
            let mut current_data = String::new();
            let mut done_emitted = false;

            futures::pin_mut!(byte_stream);

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        yield Err(ProviderError::Http(e));
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        if !current_data.is_empty() {
                            if let Some(event) = parse_sse_event(&current_event, &current_data) {
                                let is_done = matches!(&event, Ok(LLMStreamEvent::Done));
                                yield event;
                                if is_done {
                                    done_emitted = true;
                                }
                            }
                        }
                        current_event.clear();
                        current_data.clear();
                    } else if let Some(value) = line.strip_prefix("event: ") {
                        current_event = value.to_string();
                    } else if let Some(value) = line.strip_prefix("data: ") {
                        current_data = value.to_string();
                    }
                }
            }

            if !done_emitted {
                yield Ok(LLMStreamEvent::Done);
            }
        };

        Ok(Box::pin(stream))
    }
}
