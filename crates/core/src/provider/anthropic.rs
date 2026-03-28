use async_trait::async_trait;
use serde_json::json;

use crate::config::types::ProviderConfig;
use crate::provider::types::*;
use crate::provider::{LLMProvider, ProviderError, StreamResult};

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
}

impl AnthropicProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config
            .anthropic_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| ProviderError::Config("Anthropic API key not configured".into()))?
            .clone();
        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            api_base: "https://api.anthropic.com".into(),
        })
    }
}

fn build_messages(request: &LLMRequest) -> Vec<serde_json::Value> {
    use std::collections::HashSet;

    let mut messages: Vec<serde_json::Value> = Vec::new();

    for msg in &request.messages {
        match msg.role {
            Role::User => {
                let mut content = Vec::new();
                for part in &msg.content {
                    match part {
                        LLMContent::Text(text) => {
                            content.push(json!({ "type": "text", "text": text }));
                        }
                        LLMContent::Image { media_type, data } => {
                            content.push(json!({
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": media_type,
                                    "data": data,
                                }
                            }));
                        }
                        _ => {}
                    }
                }
                messages.push(json!({ "role": "user", "content": content }));
            }
            Role::Assistant => {
                let mut content = Vec::new();
                for part in &msg.content {
                    match part {
                        LLMContent::Text(text) => content.push(json!({ "type": "text", "text": text })),
                        LLMContent::ToolCall { id, name, arguments } => {
                            // Anthropic requires "input" to be a JSON object, never null.
                            let input = if arguments.is_object() {
                                arguments.clone()
                            } else {
                                json!({})
                            };
                            content.push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
                        }
                        _ => {}
                    }
                }
                messages.push(json!({ "role": "assistant", "content": content }));
            }
            Role::Tool => {
                let mut content = Vec::new();
                for part in &msg.content {
                    if let LLMContent::ToolResult { tool_call_id, content: result_content, is_error } = part {
                        let mut result = json!({ "type": "tool_result", "tool_use_id": tool_call_id, "content": result_content });
                        if *is_error {
                            result["is_error"] = json!(true);
                        }
                        content.push(result);
                    }
                }
                // Merge into previous message if it is also a "user" message (consecutive tool results).
                let should_merge = messages
                    .last()
                    .map(|prev| {
                        prev.get("role").and_then(|r| r.as_str()) == Some("user")
                            && prev.get("content").and_then(|c| c.as_array()).is_some()
                    })
                    .unwrap_or(false);
                if should_merge {
                    if let Some(arr) = messages.last_mut().unwrap().get_mut("content").and_then(|c| c.as_array_mut()) {
                        arr.extend(content);
                    }
                } else {
                    messages.push(json!({ "role": "user", "content": serde_json::Value::Array(content) }));
                }
            }
            Role::System => {}
        }
    }

    // Ensure every tool_use id in an assistant message has a matching tool_result
    // in the immediately following user message. Add synthetic error results for
    // any that are missing (e.g. after cancellation).
    let len = messages.len();
    for i in 0..len {
        let is_assistant = messages[i].get("role").and_then(|r| r.as_str()) == Some("assistant");
        if !is_assistant {
            continue;
        }

        // Collect tool_use ids from this assistant message
        let tool_use_ids: Vec<String> = messages[i]
            .get("content")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                    .filter_map(|b| b.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if tool_use_ids.is_empty() {
            continue;
        }

        // Collect existing tool_result ids from the next user message (if any)
        let existing_ids: HashSet<String> = if i + 1 < len {
            messages[i + 1]
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                        .filter_map(|b| b.get("tool_use_id").and_then(|id| id.as_str()).map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            HashSet::new()
        };

        let missing: Vec<serde_json::Value> = tool_use_ids
            .iter()
            .filter(|id| !existing_ids.contains(id.as_str()))
            .map(|id| {
                json!({
                    "type": "tool_result",
                    "tool_use_id": id,
                    "content": "Cancelled by user",
                    "is_error": true
                })
            })
            .collect();

        if !missing.is_empty() {
            if i + 1 < len
                && messages[i + 1].get("role").and_then(|r| r.as_str()) == Some("user")
                && messages[i + 1].get("content").and_then(|c| c.as_array()).is_some()
            {
                // Append to existing user message
                if let Some(arr) = messages[i + 1].get_mut("content").and_then(|c| c.as_array_mut()) {
                    arr.extend(missing);
                }
            } else {
                // Insert a new user message with the missing tool_results
                messages.insert(i + 1, json!({ "role": "user", "content": missing }));
            }
        }
    }

    messages
}

fn build_tools(tools: &[LLMTool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.parameters }))
        .collect()
}

/// Parse a single SSE data line into an LLMStreamEvent.
fn parse_sse_event(event_type: &str, data: &str) -> Option<Result<LLMStreamEvent, ProviderError>> {
    match event_type {
        "content_block_start" => {
            let v: serde_json::Value = serde_json::from_str(data).ok()?;
            let cb = v.get("content_block")?;
            match cb.get("type")?.as_str()? {
                "tool_use" => {
                    let id = cb.get("id")?.as_str()?.to_string();
                    let name = cb.get("name")?.as_str()?.to_string();
                    Some(Ok(LLMStreamEvent::ToolCallStart { id, name }))
                }
                // Thinking blocks are silently ignored
                "thinking" => None,
                _ => None,
            }
        }
        "content_block_delta" => {
            let v: serde_json::Value = serde_json::from_str(data).ok()?;
            let delta = v.get("delta")?;
            match delta.get("type")?.as_str()? {
                "text_delta" => {
                    let text = delta.get("text")?.as_str()?.to_string();
                    Some(Ok(LLMStreamEvent::TextDelta(text)))
                }
                "input_json_delta" => {
                    let json_str = delta.get("partial_json")?.as_str()?.to_string();
                    Some(Ok(LLMStreamEvent::ToolCallDelta(json_str)))
                }
                // Thinking deltas are silently ignored
                "thinking_delta" => None,
                _ => None,
            }
        }
        "content_block_stop" => Some(Ok(LLMStreamEvent::ToolCallEnd)),
        "message_delta" => {
            let v: serde_json::Value = serde_json::from_str(data).ok()?;
            if let Some(usage) = v.get("usage") {
                let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                Some(Ok(LLMStreamEvent::Usage(TokenUsage { input_tokens: 0, output_tokens: output })))
            } else {
                None
            }
        }
        "message_start" => {
            let v: serde_json::Value = serde_json::from_str(data).ok()?;
            let usage = v.get("message")?.get("usage")?;
            let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Some(Ok(LLMStreamEvent::Usage(TokenUsage { input_tokens: input, output_tokens: 0 })))
        }
        "message_stop" => Some(Ok(LLMStreamEvent::Done)),
        "error" => {
            let v: serde_json::Value = serde_json::from_str(data).ok()?;
            let msg = v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            Some(Err(ProviderError::Anthropic(msg.to_string())))
        }
        _ => None,
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn stream_message(&self, request: LLMRequest) -> Result<StreamResult, ProviderError> {
        let messages = build_messages(&request);
        let tools = build_tools(&request.tools);

        let mut body = json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "messages": messages,
            "stream": true,
        });
        if let Some(system) = &request.system_prompt {
            body["system"] = json!(system);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        // Thinking/reasoning effort support
        if let Some(effort) = &request.thinking_effort {
            let budget: u32 = match effort {
                ThinkingEffort::Low => 5000,
                ThinkingEffort::Medium => 10000,
                ThinkingEffort::High => 30000,
            };
            body["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
            // Anthropic requires temperature=1 when thinking is enabled
            body["temperature"] = json!(1);
            // Ensure max_tokens is at least budget + 4096
            let min_max_tokens = budget + 4096;
            if request.max_tokens < min_max_tokens {
                body["max_tokens"] = json!(min_max_tokens);
            }
        }

        let url = format!("{}/v1/messages", self.api_base);
        let body_str = body.to_string();

        let response = self
            .client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .header("x-api-key", &self.api_key)
            .body(body_str)
            .send()
            .await
            .map_err(ProviderError::Http)?;

        // Check for error status before trying to stream
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Anthropic(format!("{}: {}", status, error_body)));
        }

        // Stream SSE from the successful response body
        use futures::StreamExt;
        let byte_stream = response.bytes_stream();

        let stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut current_event = String::new();
            let mut current_data = String::new();

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

                // Process complete lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        // Empty line = end of SSE event
                        if !current_data.is_empty() {
                            if let Some(event) = parse_sse_event(&current_event, &current_data) {
                                let is_done = matches!(&event, Ok(LLMStreamEvent::Done));
                                yield event;
                                if is_done { return; }
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
        };

        Ok(Box::pin(stream))
    }
}

/// Fetch available models from the Anthropic /v1/models endpoint.
pub async fn list_models(config: &ProviderConfig) -> Result<Vec<String>, ProviderError> {
    let api_key = config
        .anthropic_api_key
        .as_ref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| ProviderError::Config("Anthropic API key not configured".into()))?;

    let url = "https://api.anthropic.com/v1/models".to_string();
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(ProviderError::Http)?;

    if !resp.status().is_success() {
        let error_body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Anthropic(format!("Failed to list models: {}", error_body)));
    }

    let body: serde_json::Value = resp.json().await.map_err(ProviderError::Http)?;

    let mut models: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    models.sort();
    Ok(models)
}
