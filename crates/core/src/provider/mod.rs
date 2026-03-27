pub mod types;
pub mod openai;
pub mod openai_oauth;
pub mod codex;
pub mod anthropic;
pub mod bedrock;

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use thiserror::Error;

use types::{LLMRequest, LLMStreamEvent};

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("OpenAI error: {0}")]
    OpenAI(String),
    #[error("Anthropic error: {0}")]
    Anthropic(String),
    #[error("Bedrock error: {0}")]
    Bedrock(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("{0}")]
    Other(String),
}

pub type StreamResult =
    Pin<Box<dyn Stream<Item = Result<LLMStreamEvent, ProviderError>> + Send>>;

#[derive(Debug, Clone, Copy)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub initial_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(750),
        }
    }
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(err) => {
                err.is_timeout()
                    || err.is_connect()
                    || err.is_request()
                    || err.status().is_some_and(|status| {
                        status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
                    })
            }
            Self::OpenAI(msg) | Self::Anthropic(msg) | Self::Bedrock(msg) | Self::Other(msg) => {
                let msg = msg.to_ascii_lowercase();
                msg.contains("rate limit")
                    || msg.contains("temporar")
                    || msg.contains("timeout")
                    || msg.contains("timed out")
                    || msg.contains("try again")
                    || msg.contains("retry")
                    || msg.contains("overloaded")
                    || msg.contains("processing your request")
                    || msg.contains("server error")
                    || msg.contains("internal error")
            }
            Self::Json(_) | Self::Config(_) => false,
        }
    }
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn stream_message(&self, request: LLMRequest) -> Result<StreamResult, ProviderError>;
}

/// Build a provider from config.
pub fn build_provider(
    config: &crate::config::types::ProviderConfig,
) -> Result<Box<dyn LLMProvider>, ProviderError> {
    use crate::config::types::ProviderType;
    match config.provider_type {
        ProviderType::OpenAI => {
            if config.openai_oauth.is_some() {
                Ok(Box::new(codex::CodexProvider::new(config)?))
            } else {
                Ok(Box::new(openai::OpenAIProvider::new(config)?))
            }
        }
        ProviderType::Anthropic => Ok(Box::new(anthropic::AnthropicProvider::new(config)?)),
        ProviderType::Bedrock => Ok(Box::new(bedrock::BedrockProvider::new(config)?)),
    }
}

/// Fetch available model IDs from the provider's API.
pub async fn list_models(
    config: &crate::config::types::ProviderConfig,
) -> Result<Vec<String>, ProviderError> {
    use crate::config::types::ProviderType;
    match config.provider_type {
        ProviderType::OpenAI => {
            if config.openai_oauth.is_some() {
                // OAuth tokens can't access /v1/models; return known Codex models
                Ok(vec![
                    "gpt-5.4".into(),
                    "gpt-5.4-pro".into(),
                    "gpt-5.3-codex".into(),
                    "gpt-5.2-codex".into(),
                    "codex-mini-latest".into(),
                    "gpt-5-mini".into(),
                    "gpt-4.1".into(),
                    "gpt-4.1-mini".into(),
                    "o4-mini".into(),
                    "o3".into(),
                ])
            } else {
                openai::list_models(config).await
            }
        }
        ProviderType::Anthropic => anthropic::list_models(config).await,
        ProviderType::Bedrock => bedrock::list_models(config).await,
    }
}
