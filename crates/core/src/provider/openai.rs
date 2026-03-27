use async_openai::{
    config::OpenAIConfig,
    error::OpenAIError,
    types::chat::{
        ChatCompletionMessageToolCall,
        ChatCompletionMessageToolCalls,
        ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestMessage,
        ChatCompletionRequestMessageContentPartImage,
        ChatCompletionRequestMessageContentPartText,
        ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageArgs,
        ChatCompletionRequestUserMessageContent,
        ChatCompletionRequestUserMessageContentPart,
        ChatCompletionTool,
        ChatCompletionTools,
        CreateChatCompletionRequestArgs,
        FunctionCall,
        FunctionObject,
        ImageUrl,
    },
    Client,
};
use async_trait::async_trait;
use futures::StreamExt;

use crate::config::types::ProviderConfig;
use crate::provider::{LLMProvider, ProviderError, StreamResult};
use crate::provider::types::*;

fn map_openai_error(err: OpenAIError) -> ProviderError {
    match err {
        OpenAIError::Reqwest(e) => ProviderError::Http(e),
        other => ProviderError::OpenAI(other.to_string()),
    }
}

#[allow(dead_code)]
pub struct OpenAIProvider {
    client: Client<OpenAIConfig>,
    model: String,
}

impl OpenAIProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let api_key = config.openai_api_key.as_deref().unwrap_or_default();
        let mut oai_config = OpenAIConfig::new().with_api_key(api_key);
        if let Some(base) = &config.openai_api_base {
            oai_config = oai_config.with_api_base(base);
        }
        Ok(Self {
            client: Client::with_config(oai_config),
            model: config.model.clone(),
        })
    }
}

fn build_messages(request: &LLMRequest) -> Result<Vec<ChatCompletionRequestMessage>, ProviderError> {
    let mut messages = Vec::new();

    if let Some(sys) = &request.system_prompt {
        messages.push(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(sys.clone())
                .build()
                .map_err(|e| ProviderError::OpenAI(e.to_string()))?
                .into(),
        );
    }

    for msg in &request.messages {
        match msg.role {
            LLMRole::User => {
                let has_images = msg.content.iter().any(|c| matches!(c, LLMContent::Image { .. }));
                if has_images {
                    // Build a multi-part content array with text and image parts
                    let mut parts: Vec<ChatCompletionRequestUserMessageContentPart> = Vec::new();
                    for part in &msg.content {
                        match part {
                            LLMContent::Text(text) => {
                                parts.push(ChatCompletionRequestUserMessageContentPart::Text(
                                    ChatCompletionRequestMessageContentPartText { text: text.clone() },
                                ));
                            }
                            LLMContent::Image { media_type, data } => {
                                let data_url = format!("data:{};base64,{}", media_type, data);
                                parts.push(ChatCompletionRequestUserMessageContentPart::ImageUrl(
                                    ChatCompletionRequestMessageContentPartImage {
                                        image_url: ImageUrl { url: data_url, detail: None },
                                    },
                                ));
                            }
                            _ => {}
                        }
                    }
                    let user_msg = ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Array(parts),
                        name: None,
                    };
                    messages.push(ChatCompletionRequestMessage::User(user_msg));
                } else {
                    let text: String = msg.content.iter().filter_map(|c| c.text()).collect::<Vec<_>>().join("");
                    messages.push(
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(text)
                            .build()
                            .map_err(|e| ProviderError::OpenAI(e.to_string()))?
                            .into(),
                    );
                }
            }
            LLMRole::Assistant => {
                let text_parts: String = msg.content.iter().filter_map(|c| c.text()).collect::<Vec<_>>().join("");
                let tool_calls: Vec<ChatCompletionMessageToolCalls> = msg
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        LLMContent::ToolCall { id, name, arguments } => {
                            Some(ChatCompletionMessageToolCalls::Function(
                                ChatCompletionMessageToolCall {
                                    id: id.clone(),
                                    function: FunctionCall {
                                        name: name.clone(),
                                        arguments: arguments.to_string(),
                                    },
                                },
                            ))
                        }
                        _ => None,
                    })
                    .collect();

                let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
                if !text_parts.is_empty() {
                    builder.content(text_parts);
                }
                if !tool_calls.is_empty() {
                    builder.tool_calls(tool_calls);
                }
                messages.push(
                    builder.build().map_err(|e| ProviderError::OpenAI(e.to_string()))?.into(),
                );
            }
            LLMRole::Tool => {
                for part in &msg.content {
                    if let LLMContent::ToolResult { tool_call_id, content, .. } = part {
                        messages.push(
                            ChatCompletionRequestToolMessageArgs::default()
                                .tool_call_id(tool_call_id.clone())
                                .content(content.clone())
                                .build()
                                .map_err(|e| ProviderError::OpenAI(e.to_string()))?
                                .into(),
                        );
                    }
                }
            }
            LLMRole::System => {}
        }
    }

    Ok(messages)
}

fn build_tools(tools: &[LLMTool]) -> Vec<ChatCompletionTools> {
    tools
        .iter()
        .map(|t| {
            ChatCompletionTools::Function(ChatCompletionTool {
                function: FunctionObject {
                    name: t.name.clone(),
                    description: Some(t.description.clone()),
                    parameters: Some(t.parameters.clone()),
                    strict: None,
                },
            })
        })
        .collect()
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn stream_message(&self, request: LLMRequest) -> Result<StreamResult, ProviderError> {
        let messages = build_messages(&request)?;
        let tools = build_tools(&request.tools);

        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder
            .model(&request.model)
            .messages(messages)
            .max_completion_tokens(request.max_tokens)
            .stream(true);

        if let Some(temp) = request.temperature {
            req_builder.temperature(temp);
        }
        if !tools.is_empty() {
            req_builder.tools(tools);
        }

        // Thinking/reasoning effort support
        // TODO: The async_openai crate's CreateChatCompletionRequestArgs may not expose
        // a `reasoning_effort` builder method yet. Once it does, set it here:
        //   if let Some(effort) = &request.thinking_effort {
        //       if effort != "off" {
        //           req_builder.reasoning_effort(effort);
        //       }
        //   }

        let oai_request = req_builder.build().map_err(|e| ProviderError::OpenAI(e.to_string()))?;

        let stream = self
            .client
            .chat()
            .create_stream(oai_request)
            .await
            .map_err(map_openai_error)?;

        let mapped = stream.filter_map(|result| async move {
            match result {
                Err(e) => Some(Err(map_openai_error(e))),
                Ok(response) => {
                    let choice = response.choices.first()?;
                    let delta = &choice.delta;

                    if let Some(content) = &delta.content {
                        if !content.is_empty() {
                            return Some(Ok(LLMStreamEvent::TextDelta(content.clone())));
                        }
                    }

                    if let Some(tool_calls) = &delta.tool_calls {
                        for tc in tool_calls {
                            if let Some(func) = &tc.function {
                                if let Some(name) = &func.name {
                                    let id = tc.id.clone().unwrap_or_default();
                                    return Some(Ok(LLMStreamEvent::ToolCallStart { id, name: name.clone() }));
                                }
                                if let Some(args) = &func.arguments {
                                    if !args.is_empty() {
                                        return Some(Ok(LLMStreamEvent::ToolCallDelta(args.clone())));
                                    }
                                }
                            }
                        }
                    }

                    if choice.finish_reason.is_some() {
                        if let Some(usage) = &response.usage {
                            return Some(Ok(LLMStreamEvent::Usage(TokenUsage {
                                input_tokens: usage.prompt_tokens as u32,
                                output_tokens: usage.completion_tokens as u32,
                            })));
                        }
                        return Some(Ok(LLMStreamEvent::Done));
                    }

                    None
                }
            }
        });

        Ok(Box::pin(mapped))
    }
}

/// Fetch available models from the OpenAI-compatible /v1/models endpoint.
pub async fn list_models(config: &ProviderConfig) -> Result<Vec<String>, ProviderError> {
    let api_key = if let Some(oauth) = &config.openai_oauth {
        oauth.access_token.as_str()
    } else {
        config.openai_api_key.as_deref().unwrap_or_default()
    };
    let base = config.openai_api_base.as_deref().unwrap_or("https://api.openai.com/v1");
    let url = format!("{}/models", base.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(ProviderError::Http)?;

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
