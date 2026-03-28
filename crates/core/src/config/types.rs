use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub use crate::provider::openai_oauth::OAuthCredentials;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    #[default]
    OpenAI,
    Anthropic,
    Bedrock,
}

impl ProviderType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Bedrock => "Bedrock",
        }
    }

    pub const ALL: &[ProviderType] = &[Self::OpenAI, Self::Anthropic, Self::Bedrock];

    /// Known models for this provider.
    pub fn models(&self) -> &'static [&'static str] {
        match self {
            Self::OpenAI => &[
                "gpt-5.4",
                "gpt-5.4-pro",
                "gpt-5-mini",
                "gpt-5.3-codex",
                "gpt-5.2-codex",
                "codex-mini-latest",
                "gpt-4.1",
                "gpt-4.1-mini",
                "gpt-4.1-nano",
                "gpt-4o",
                "gpt-4o-mini",
                "o4-mini",
                "o3",
                "o3-mini",
            ],
            Self::Anthropic => &[
                "claude-opus-4-6",
                "claude-sonnet-4-6",
                "claude-sonnet-4-20250514",
                "claude-opus-4-20250514",
                "claude-haiku-4-20250414",
                "claude-3-7-sonnet-20250219",
                "claude-3-5-haiku-20241022",
            ],
            Self::Bedrock => &[
                "global.anthropic.claude-opus-4-6-v1",
                "global.anthropic.claude-sonnet-4-6-v1",
                "us.anthropic.claude-sonnet-4-20250514-v1:0",
                "us.anthropic.claude-opus-4-20250514-v1:0",
                "us.anthropic.claude-haiku-4-20250414-v1:0",
                "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
                "us.amazon.nova-pro-v1:0",
                "us.amazon.nova-lite-v1:0",
            ],
        }
    }

    /// Default model for this provider.
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::OpenAI => "gpt-5.4",
            Self::Anthropic => "claude-sonnet-4-6",
            Self::Bedrock => "global.anthropic.claude-sonnet-4-6-v1",
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    // OpenAI
    pub openai_api_base: Option<String>,
    pub openai_api_key: Option<String>,
    pub openai_oauth: Option<OAuthCredentials>,
    // Anthropic
    pub anthropic_api_key: Option<String>,
    // Bedrock
    pub aws_region: Option<String>,
    pub aws_profile: Option<String>,
    // Common
    pub model: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub thinking_effort: Option<crate::provider::types::ThinkingEffort>,
}

impl ProviderConfig {
    /// Get the API key for the currently selected provider.
    pub fn api_key(&self) -> Option<&str> {
        match self.provider_type {
            ProviderType::OpenAI => self.openai_api_key.as_deref(),
            ProviderType::Anthropic => self.anthropic_api_key.as_deref(),
            ProviderType::Bedrock => None,
        }
    }

    /// Get the API base URL for the currently selected provider.
    pub fn api_base(&self) -> Option<&str> {
        match self.provider_type {
            ProviderType::OpenAI => self.openai_api_base.as_deref(),
            _ => None,
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::OpenAI,
            openai_api_base: Some("https://api.openai.com/v1".into()),
            openai_api_key: None,
            openai_oauth: None,
            anthropic_api_key: None,
            aws_region: None,
            aws_profile: None,
            model: "gpt-5.4".into(),
            max_tokens: 4096,
            temperature: Some(0.7),
            thinking_effort: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub command: String,
    pub args: Vec<String>,
    pub timeout_secs: u64,
}

impl Default for ShellConfig {
    fn default() -> Self {
        if cfg!(windows) {
            Self {
                command: "powershell".into(),
                args: vec!["-NoProfile".into(), "-Command".into()],
                timeout_secs: 120,
            }
        } else {
            Self {
                command: "bash".into(),
                args: vec!["-l".into(), "-c".into()],
                timeout_secs: 120,
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: String,
    pub font_size: f32,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            font_size: 14.0,
            window_width: 1200,
            window_height: 800,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    #[serde(default = "default_enable_compaction")]
    pub enable_compaction: bool,
    /// Trigger compaction when the last LLM request consumed more than this many input tokens.
    #[serde(default = "default_compact_after_input_tokens")]
    pub compact_after_input_tokens: u32,
    #[serde(default = "default_keep_recent_messages")]
    pub keep_recent_messages: usize,
}

fn default_enable_compaction() -> bool { true }
fn default_compact_after_input_tokens() -> u32 { 300_000 }
fn default_keep_recent_messages() -> usize { 12 }

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            enable_compaction: default_enable_compaction(),
            compact_after_input_tokens: default_compact_after_input_tokens(),
            keep_recent_messages: default_keep_recent_messages(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    #[serde(default = "default_skill_sources")]
    pub sources: Vec<String>,
}

fn default_skills_enabled() -> bool { true }
fn default_skill_sources() -> Vec<String> { vec!["vercel-labs/agent-skills".to_string()] }

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled: default_skills_enabled(),
            paths: Vec::new(),
            sources: default_skill_sources(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_enable_memory")]
    pub enable_memory: bool,
    #[serde(default = "default_memory_max_chars")]
    pub max_chars: usize,
}

fn default_enable_memory() -> bool { true }
fn default_memory_max_chars() -> usize { 8_000 }

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enable_memory: default_enable_memory(),
            max_chars: default_memory_max_chars(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: ProviderConfig,
    pub shell: ShellConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub auto_approve: Vec<String>,
    /// When true, the agent must avoid mutating project state and should
    /// produce a plan before execution. Read-only local tools, web tools,
    /// and non-mutating shell inspection commands may still be available.
    #[serde(default)]
    pub plan_mode: bool,
}

fn default_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("freako")
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: ProviderConfig::default(),
            shell: ShellConfig::default(),
            ui: UiConfig::default(),
            context: ContextConfig::default(),
            memory: MemoryConfig::default(),
            skills: SkillsConfig::default(),
            data_dir: default_data_dir(),
            system_prompt: None,
            auto_approve: Vec::new(),
            plan_mode: false,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("freako")
            .join("config.toml")
    }

    pub fn is_provider_configured(&self) -> bool {
        match self.provider.provider_type {
            ProviderType::OpenAI => {
                self.provider.openai_api_key.as_ref().is_some_and(|k| !k.is_empty())
                    || self.provider.openai_oauth.is_some()
            }
            ProviderType::Anthropic => {
                self.provider.anthropic_api_key.as_ref().is_some_and(|k| !k.is_empty())
            }
            ProviderType::Bedrock => {
                self.provider.aws_region.as_ref().is_some_and(|r| !r.is_empty())
            }
        }
    }
}
