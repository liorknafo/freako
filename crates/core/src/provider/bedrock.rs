use async_trait::async_trait;
use aws_sdk_bedrockruntime::{
    self as bedrock,
    types::{
        ContentBlock, ConversationRole, Message, SystemContentBlock, Tool as BedrockTool,
        ToolConfiguration, ToolInputSchema, ToolResultBlock, ToolResultContentBlock,
        ToolResultStatus, ToolSpecification, ToolUseBlock,
    },
};
use aws_smithy_types::Document;

use crate::config::types::ProviderConfig;
use crate::provider::types::*;
use crate::provider::{LLMProvider, ProviderError, StreamResult};

#[allow(dead_code)]
pub struct BedrockProvider {
    client: bedrock::Client,
    model: String,
}

impl BedrockProvider {
    pub fn new(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| ProviderError::Config("No tokio runtime available".into()))?;

        let client = std::thread::scope(|_| {
            rt.block_on(async {
                let mut aws_builder = aws_config::defaults(aws_config::BehaviorVersion::latest());
                if let Some(region) = &config.aws_region {
                    aws_builder =
                        aws_builder.region(aws_config::Region::new(region.clone()));
                }
                if let Some(profile) = &config.aws_profile {
                    aws_builder = aws_builder.profile_name(profile);
                }
                let aws_config = aws_builder.load().await;
                bedrock::Client::new(&aws_config)
            })
        });

        Ok(Self {
            client,
            model: config.model.clone(),
        })
    }
}

/// Convert a serde_json::Value into an aws_smithy_types::Document.
fn json_to_document(value: &serde_json::Value) -> Document {
    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(b) => Document::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Document::Number(aws_smithy_types::Number::NegInt(i))
            } else if let Some(f) = n.as_f64() {
                Document::Number(aws_smithy_types::Number::Float(f))
            } else {
                Document::Null
            }
        }
        serde_json::Value::String(s) => Document::String(s.clone()),
        serde_json::Value::Array(arr) => {
            Document::Array(arr.iter().map(json_to_document).collect())
        }
        serde_json::Value::Object(obj) => {
            Document::Object(obj.iter().map(|(k, v)| (k.clone(), json_to_document(v))).collect())
        }
    }
}

fn build_messages(request: &LLMRequest) -> Result<Vec<Message>, ProviderError> {
    let mut messages = Vec::new();
    for msg in &request.messages {
        match msg.role {
            Role::User => {
                // Note: LLMContent::Image is not yet supported for Bedrock.
                // Image content parts are skipped; only text is sent.
                let text: String = msg.content.iter().filter_map(|c| c.text()).collect::<Vec<_>>().join("");
                messages.push(
                    Message::builder()
                        .role(ConversationRole::User)
                        .content(ContentBlock::Text(text))
                        .build()
                        .map_err(|e| ProviderError::Bedrock(e.to_string()))?,
                );
            }
            Role::Assistant => {
                let mut builder = Message::builder().role(ConversationRole::Assistant);
                for part in &msg.content {
                    match part {
                        LLMContent::Text(text) => {
                            builder = builder.content(ContentBlock::Text(text.clone()));
                        }
                        LLMContent::ToolCall { id, name, arguments } => {
                            let doc = json_to_document(arguments);
                            builder = builder.content(ContentBlock::ToolUse(
                                ToolUseBlock::builder()
                                    .tool_use_id(id)
                                    .name(name)
                                    .input(doc)
                                    .build()
                                    .map_err(|e| ProviderError::Bedrock(e.to_string()))?,
                            ));
                        }
                        _ => {}
                    }
                }
                messages.push(builder.build().map_err(|e| ProviderError::Bedrock(e.to_string()))?);
            }
            Role::Tool => {
                let mut builder = Message::builder().role(ConversationRole::User);
                for part in &msg.content {
                    if let LLMContent::ToolResult { tool_call_id, content, is_error } = part {
                        let status = if *is_error { ToolResultStatus::Error } else { ToolResultStatus::Success };
                        builder = builder.content(ContentBlock::ToolResult(
                            ToolResultBlock::builder()
                                .tool_use_id(tool_call_id)
                                .content(ToolResultContentBlock::Text(content.clone()))
                                .status(status)
                                .build()
                                .map_err(|e| ProviderError::Bedrock(e.to_string()))?,
                        ));
                    }
                }
                messages.push(builder.build().map_err(|e| ProviderError::Bedrock(e.to_string()))?);
            }
            Role::System => {}
        }
    }
    Ok(messages)
}

fn build_tool_config(tools: &[LLMTool]) -> Result<Option<ToolConfiguration>, ProviderError> {
    if tools.is_empty() {
        return Ok(None);
    }
    let mut config = ToolConfiguration::builder();
    for t in tools {
        let doc = json_to_document(&t.parameters);
        config = config.tools(BedrockTool::ToolSpec(
            ToolSpecification::builder()
                .name(&t.name)
                .description(&t.description)
                .input_schema(ToolInputSchema::Json(doc))
                .build()
                .map_err(|e| ProviderError::Bedrock(e.to_string()))?,
        ));
    }
    Ok(Some(config.build().map_err(|e| ProviderError::Bedrock(e.to_string()))?))
}

#[async_trait]
impl LLMProvider for BedrockProvider {
    async fn stream_message(&self, request: LLMRequest) -> Result<StreamResult, ProviderError> {
        let messages = build_messages(&request)?;
        let tool_config = build_tool_config(&request.tools)?;

        let mut converse_builder = self
            .client
            .converse_stream()
            .model_id(&request.model)
            .set_messages(Some(messages));

        if let Some(system) = &request.system_prompt {
            converse_builder =
                converse_builder.system(SystemContentBlock::Text(system.clone()));
        }
        if let Some(tc) = tool_config {
            converse_builder = converse_builder.tool_config(tc);
        }

        let output = converse_builder
            .send()
            .await
            .map_err(|e| ProviderError::Bedrock(e.to_string()))?;

        let mut event_receiver = output.stream;

        let stream = async_stream::stream! {
            loop {
                match event_receiver.recv().await {
                    Ok(Some(event)) => {
                        use aws_sdk_bedrockruntime::types::ConverseStreamOutput;
                        match event {
                            ConverseStreamOutput::ContentBlockStart(start) => {
                                if let Some(cb_start) = start.start {
                                    use aws_sdk_bedrockruntime::types::ContentBlockStart as CBS;
                                    if let CBS::ToolUse(tool_start) = cb_start {
                                        yield Ok(LLMStreamEvent::ToolCallStart {
                                            id: tool_start.tool_use_id().to_string(),
                                            name: tool_start.name().to_string(),
                                        });
                                    }
                                }
                            }
                            ConverseStreamOutput::ContentBlockDelta(delta_event) => {
                                if let Some(delta) = delta_event.delta {
                                    use aws_sdk_bedrockruntime::types::ContentBlockDelta as CBD;
                                    match delta {
                                        CBD::Text(text) => yield Ok(LLMStreamEvent::TextDelta(text)),
                                        CBD::ToolUse(tool_delta) => {
                                            yield Ok(LLMStreamEvent::ToolCallDelta(tool_delta.input().to_string()));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            ConverseStreamOutput::ContentBlockStop(_) => {
                                yield Ok(LLMStreamEvent::ToolCallEnd);
                            }
                            ConverseStreamOutput::Metadata(meta) => {
                                if let Some(usage) = meta.usage {
                                    yield Ok(LLMStreamEvent::Usage(TokenUsage {
                                        input_tokens: usage.input_tokens as u32,
                                        output_tokens: usage.output_tokens as u32,
                                    }));
                                }
                            }
                            ConverseStreamOutput::MessageStop(_) => {
                                yield Ok(LLMStreamEvent::Done);
                            }
                            _ => {}
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        yield Err(ProviderError::Bedrock(e.to_string()));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

/// Fetch available model IDs from AWS Bedrock.
pub async fn list_models(config: &ProviderConfig) -> Result<Vec<String>, ProviderError> {
    let mut aws_builder = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = &config.aws_region {
        aws_builder = aws_builder.region(aws_config::Region::new(region.clone()));
    }
    if let Some(profile) = &config.aws_profile {
        aws_builder = aws_builder.profile_name(profile);
    }
    let aws_config = aws_builder.load().await;
    let client = aws_sdk_bedrock::Client::new(&aws_config);

    let resp = client
        .list_foundation_models()
        .send()
        .await
        .map_err(|e| ProviderError::Bedrock(e.to_string()))?;

    let mut models: Vec<String> = resp
        .model_summaries()
        .iter()
        .filter(|m| {
            // Only include models that support text generation via Converse
            m.output_modalities().iter().any(|om| {
                *om == aws_sdk_bedrock::types::ModelModality::Text
            })
        })
        .map(|m| m.model_id().to_string())
        .collect();

    models.sort();
    Ok(models)
}
