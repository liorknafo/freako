use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Clear};
use freako_core::config::types::{AppConfig, ProviderType};

use super::{CHAT_BG, TOOL_STYLE};

const LABEL_STYLE: Style = Style::new().fg(Color::Indexed(252)).add_modifier(Modifier::BOLD);
const VALUE_STYLE: Style = Style::new().fg(Color::Indexed(75));
const HINT_STYLE: Style = Style::new().fg(Color::Indexed(243));
const LINK_STYLE: Style = Style::new().fg(Color::Indexed(75)).add_modifier(Modifier::UNDERLINED);
const SELECTED_BG: Color = Color::Rgb(50, 35, 70);

// ── Field definitions ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Cycle through fixed options with ←/→ or Tab
    Select,
    /// Toggle on/off with Tab or Enter
    Toggle,
    /// Free text input (Enter to edit)
    TextInput,
    /// Masked text input for secrets
    SecretInput,
    /// Clickable action (opens browser, triggers OAuth, etc.)
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Provider,
    Model,
    // OpenAI
    OpenAIApiBase,
    OpenAIApiKey,
    OpenAIGetKey,
    OpenAIOAuth,
    // Anthropic
    AnthropicApiKey,
    AnthropicGetKey,
    // Bedrock
    AwsRegion,
    AwsProfile,
    // Generation
    MaxTokens,
    Temperature,
    ThinkingEffort,
    // Context
    EnableCompaction,
    CompactAfterInputTokens,
    KeepRecentMessages,
    // Features
    EnableSkills,
    EnableMemory,
    // Instructions
    SystemPrompt,
}

impl SettingsField {
    pub fn kind(&self) -> FieldKind {
        match self {
            Self::Provider | Self::Model | Self::ThinkingEffort => FieldKind::Select,
            Self::EnableCompaction | Self::EnableSkills | Self::EnableMemory => FieldKind::Toggle,
            Self::OpenAIApiKey | Self::AnthropicApiKey => FieldKind::SecretInput,
            Self::OpenAIGetKey | Self::AnthropicGetKey | Self::OpenAIOAuth => FieldKind::Action,
            _ => FieldKind::TextInput,
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Provider => "Provider",
            Self::Model => "Model",
            Self::OpenAIApiBase => "API Base URL",
            Self::OpenAIApiKey => "API Key",
            Self::OpenAIGetKey => "Get API Key →",
            Self::OpenAIOAuth => "Login with ChatGPT →",
            Self::AnthropicApiKey => "API Key",
            Self::AnthropicGetKey => "Get API Key →",
            Self::AwsRegion => "AWS Region",
            Self::AwsProfile => "AWS Profile",
            Self::MaxTokens => "Max Tokens",
            Self::Temperature => "Temperature",
            Self::ThinkingEffort => "Thinking Effort",
            Self::EnableCompaction => "Auto-compact",
            Self::CompactAfterInputTokens => "Compact after input tokens",
            Self::KeepRecentMessages => "Keep recent messages",
            Self::EnableSkills => "Skills",
            Self::EnableMemory => "Memory",
            Self::SystemPrompt => "System Prompt",
        }
    }

    fn section(&self, config: &AppConfig) -> String {
        match self {
            Self::Provider | Self::Model => "Provider".to_string(),
            Self::OpenAIApiBase | Self::OpenAIApiKey | Self::OpenAIGetKey | Self::OpenAIOAuth => {
                if config.provider.openai_oauth.is_some() {
                    "OpenAI Connection (using: ChatGPT subscription)".to_string()
                } else if config.provider.openai_api_key.is_some() {
                    "OpenAI Connection (using: API key)".to_string()
                } else {
                    "OpenAI Connection (not configured)".to_string()
                }
            }
            Self::AnthropicApiKey | Self::AnthropicGetKey => "Anthropic Connection".to_string(),
            Self::AwsRegion | Self::AwsProfile => "AWS Bedrock".to_string(),
            Self::MaxTokens | Self::Temperature | Self::ThinkingEffort => "Generation".to_string(),
            Self::EnableCompaction | Self::CompactAfterInputTokens | Self::KeepRecentMessages => "Context".to_string(),
            Self::EnableSkills | Self::EnableMemory => "Features".to_string(),
            Self::SystemPrompt => "Additional Instructions".to_string(),
        }
    }

    fn is_relevant(&self, provider: &ProviderType) -> bool {
        match self {
            Self::OpenAIApiBase | Self::OpenAIApiKey | Self::OpenAIGetKey | Self::OpenAIOAuth
                => *provider == ProviderType::OpenAI,
            Self::AnthropicApiKey | Self::AnthropicGetKey
                => *provider == ProviderType::Anthropic,
            Self::AwsRegion | Self::AwsProfile
                => *provider == ProviderType::Bedrock,
            _ => true,
        }
    }

    fn all() -> &'static [SettingsField] {
        &[
            Self::Provider, Self::Model,
            Self::OpenAIApiBase, Self::OpenAIApiKey, Self::OpenAIGetKey, Self::OpenAIOAuth,
            Self::AnthropicApiKey, Self::AnthropicGetKey,
            Self::AwsRegion, Self::AwsProfile,
            Self::MaxTokens, Self::Temperature, Self::ThinkingEffort,
            Self::EnableCompaction, Self::CompactAfterInputTokens, Self::KeepRecentMessages,
            Self::EnableSkills, Self::EnableMemory,
            Self::SystemPrompt,
        ]
    }
}

// ── State ───────────────────────────────────────────────────────

pub struct SettingsState {
    pub selected: usize,
    pub editing: bool,
    pub edit_buffer: String,
    /// For Select fields: index within the options list
    pub select_index: usize,
}

impl SettingsState {
    pub fn new() -> Self {
        Self { selected: 0, editing: false, edit_buffer: String::new(), select_index: 0 }
    }

    fn visible_fields(&self, config: &AppConfig) -> Vec<SettingsField> {
        SettingsField::all().iter()
            .filter(|f| f.is_relevant(&config.provider.provider_type))
            .copied()
            .collect()
    }

    pub fn selected_field(&self, config: &AppConfig) -> Option<SettingsField> {
        self.visible_fields(config).get(self.selected).copied()
    }

    pub fn move_up(&mut self, config: &AppConfig) {
        let count = self.visible_fields(config).len();
        if count > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
            self.sync_select_index(config);
        }
    }

    pub fn move_down(&mut self, config: &AppConfig) {
        let count = self.visible_fields(config).len();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
            self.sync_select_index(config);
        }
    }

    /// Sync select_index to current config value when navigating to a Select field
    fn sync_select_index(&mut self, config: &AppConfig) {
        if let Some(field) = self.selected_field(config) {
            if field.kind() == FieldKind::Select {
                let options = select_options(config, field);
                let current = get_field_display(config, field);
                self.select_index = options.iter().position(|o| *o == current).unwrap_or(0);
            }
        }
    }

    pub fn cycle_select(&mut self, config: &mut AppConfig, delta: isize) {
        if let Some(field) = self.selected_field(config) {
            let options = select_options(config, field);
            if options.is_empty() { return; }
            let len = options.len() as isize;
            self.select_index = ((self.select_index as isize + delta).rem_euclid(len)) as usize;
            apply_select(config, field, &options[self.select_index]);
        }
    }

    pub fn toggle(&mut self, config: &mut AppConfig) {
        if let Some(field) = self.selected_field(config) {
            let val = get_field_value(config, field);
            set_field_value(config, field, if val == "true" { "false" } else { "true" });
        }
    }

    pub fn start_editing(&mut self, config: &AppConfig) {
        if let Some(field) = self.selected_field(config) {
            self.editing = true;
            self.edit_buffer = get_field_value(config, field);
        }
    }

    pub fn cancel_editing(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
    }

    pub fn apply_edit(&mut self, config: &mut AppConfig) {
        if let Some(field) = self.selected_field(config) {
            set_field_value(config, field, &self.edit_buffer);
            self.editing = false;
            self.edit_buffer.clear();
        }
    }

    pub fn do_action(&self, config: &AppConfig) -> Option<SettingsAction> {
        match self.selected_field(config)? {
            SettingsField::OpenAIGetKey => Some(SettingsAction::OpenUrl("https://platform.openai.com/api-keys")),
            SettingsField::AnthropicGetKey => Some(SettingsAction::OpenUrl("https://console.anthropic.com/settings/keys")),
            SettingsField::OpenAIOAuth => {
                if config.provider.openai_oauth.is_some() {
                    Some(SettingsAction::OAuthLogout)
                } else {
                    Some(SettingsAction::StartOAuth)
                }
            }
            _ => None,
        }
    }
}

pub enum SettingsAction {
    OpenUrl(&'static str),
    StartOAuth,
    OAuthLogout,
}

// ── Helpers ─────────────────────────────────────────────────────

fn select_options(config: &AppConfig, field: SettingsField) -> Vec<String> {
    match field {
        SettingsField::Provider => ProviderType::ALL.iter().map(|p| p.label().to_string()).collect(),
        SettingsField::Model => config.provider.provider_type.models().iter().map(|m| m.to_string()).collect(),
        SettingsField::ThinkingEffort => vec!["off".into(), "low".into(), "medium".into(), "high".into()],
        _ => vec![],
    }
}

fn apply_select(config: &mut AppConfig, field: SettingsField, value: &str) {
    match field {
        SettingsField::Provider => {
            for p in ProviderType::ALL {
                if p.label() == value {
                    config.provider.provider_type = p.clone();
                    if !config.provider.provider_type.models().contains(&config.provider.model.as_str()) {
                        config.provider.model = config.provider.provider_type.default_model().to_string();
                    }
                    break;
                }
            }
        }
        SettingsField::Model => config.provider.model = value.to_string(),
        SettingsField::ThinkingEffort => {
            config.provider.thinking_effort = freako_core::provider::types::ThinkingEffort::from_str_opt(value);
        }
        _ => {}
    }
}

fn get_field_value(config: &AppConfig, field: SettingsField) -> String {
    match field {
        SettingsField::Provider => config.provider.provider_type.label().to_string(),
        SettingsField::Model => config.provider.model.clone(),
        SettingsField::OpenAIApiBase => config.provider.openai_api_base.clone().unwrap_or_default(),
        SettingsField::OpenAIApiKey => config.provider.openai_api_key.clone().unwrap_or_default(),
        SettingsField::AnthropicApiKey => config.provider.anthropic_api_key.clone().unwrap_or_default(),
        SettingsField::AwsRegion => config.provider.aws_region.clone().unwrap_or_default(),
        SettingsField::AwsProfile => config.provider.aws_profile.clone().unwrap_or_default(),
        SettingsField::MaxTokens => config.provider.max_tokens.to_string(),
        SettingsField::Temperature => config.provider.temperature.map(|t| t.to_string()).unwrap_or_default(),
        SettingsField::ThinkingEffort => config.provider.thinking_effort.map(|e| e.to_string()).unwrap_or_else(|| "off".to_string()),
        SettingsField::EnableCompaction => config.context.enable_compaction.to_string(),
        SettingsField::CompactAfterInputTokens => config.context.compact_after_input_tokens.to_string(),
        SettingsField::KeepRecentMessages => config.context.keep_recent_messages.to_string(),
        SettingsField::SystemPrompt => config.system_prompt.clone().unwrap_or_default(),
        SettingsField::EnableSkills => config.skills.enabled.to_string(),
        SettingsField::EnableMemory => config.memory.enable_memory.to_string(),
        SettingsField::OpenAIGetKey | SettingsField::AnthropicGetKey | SettingsField::OpenAIOAuth => String::new(),
    }
}

fn get_field_display(config: &AppConfig, field: SettingsField) -> String {
    match field {
        SettingsField::OpenAIApiKey | SettingsField::AnthropicApiKey => mask_secret(&get_field_value(config, field)),
        _ => get_field_value(config, field),
    }
}

fn set_field_value(config: &mut AppConfig, field: SettingsField, value: &str) {
    match field {
        SettingsField::OpenAIApiBase => config.provider.openai_api_base = nonempty(value),
        SettingsField::OpenAIApiKey => config.provider.openai_api_key = nonempty(value),
        SettingsField::AnthropicApiKey => config.provider.anthropic_api_key = nonempty(value),
        SettingsField::AwsRegion => config.provider.aws_region = nonempty(value),
        SettingsField::AwsProfile => config.provider.aws_profile = nonempty(value),
        SettingsField::MaxTokens => { if let Ok(v) = value.parse() { config.provider.max_tokens = v; } }
        SettingsField::Temperature => {
            if value.is_empty() { config.provider.temperature = None; }
            else if let Ok(v) = value.parse() { config.provider.temperature = Some(v); }
        }
        SettingsField::EnableCompaction => config.context.enable_compaction = parse_bool(value),
        SettingsField::CompactAfterInputTokens => { if let Ok(v) = value.parse() { config.context.compact_after_input_tokens = v; } }
        SettingsField::KeepRecentMessages => { if let Ok(v) = value.parse() { config.context.keep_recent_messages = v; } }
        SettingsField::SystemPrompt => config.system_prompt = nonempty(value),
        SettingsField::EnableSkills => config.skills.enabled = parse_bool(value),
        SettingsField::EnableMemory => config.memory.enable_memory = parse_bool(value),
        _ => {}
    }
}

fn nonempty(s: &str) -> Option<String> {
    if s.is_empty() { None } else { Some(s.to_string()) }
}

fn parse_bool(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "true" | "1" | "yes" | "on")
}

fn mask_secret(s: &str) -> String {
    if s.is_empty() { return String::new(); }
    if s.len() <= 8 { return "*".repeat(s.len()); }
    format!("{}...{}", &s[..4], &s[s.len()-4..])
}

// ── Rendering ───────────────────────────────────────────────────

pub fn render_settings(frame: &mut ratatui::Frame, state: &SettingsState, config: &AppConfig, area: Rect) {
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::NONE)
        .title(" Settings ")
        .title_bottom(" Esc close │ ↑↓ navigate │ ←→/Tab cycle │ Enter edit ")
        .style(Style::new().bg(CHAT_BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let fields = SettingsField::all().iter()
        .filter(|f| f.is_relevant(&config.provider.provider_type))
        .copied()
        .collect::<Vec<_>>();

    let mut lines: Vec<Line> = Vec::new();
    let mut prev_section = String::new();

    for (i, field) in fields.iter().enumerate() {
        let section = field.section(config);
        if section != prev_section {
            if !prev_section.is_empty() { lines.push(Line::raw("")); }
            lines.push(Line::styled(
                format!("  ── {} ──", section),
                TOOL_STYLE.add_modifier(Modifier::BOLD),
            ));
            prev_section = section;
        }

        let is_selected = i == state.selected;
        let kind = field.kind();
        let cursor = if is_selected { "▸ " } else { "  " };

        match kind {
            FieldKind::Select => {
                let options = select_options(config, *field);
                let current = get_field_display(config, *field);
                let current_idx = options.iter().position(|o| *o == current).unwrap_or(0);

                let mut spans = vec![
                    Span::styled(format!("{}{}: ", cursor, field.label()), LABEL_STYLE),
                ];
                // Render options inline with selected one highlighted
                for (oi, opt) in options.iter().enumerate() {
                    let is_current = oi == current_idx;
                    if oi > 0 { spans.push(Span::styled(" │ ", HINT_STYLE)); }
                    let style = if is_current {
                        if is_selected {
                            Style::new().fg(Color::White).bg(SELECTED_BG).add_modifier(Modifier::BOLD)
                        } else {
                            VALUE_STYLE.add_modifier(Modifier::BOLD)
                        }
                    } else {
                        HINT_STYLE
                    };
                    spans.push(Span::styled(format!(" {} ", opt), style));
                }
                lines.push(Line::from(spans));
            }

            FieldKind::Toggle => {
                let val = get_field_value(config, *field) == "true";
                let indicator = if val { "[●] On " } else { "[○] Off" };
                let style = if val {
                    Style::new().fg(Color::Indexed(78))  // green
                } else {
                    HINT_STYLE
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{}{}: ", cursor, field.label()), LABEL_STYLE),
                    Span::styled(indicator, if is_selected { style.bg(SELECTED_BG) } else { style }),
                ]));
            }

            FieldKind::Action => {
                let style = if is_selected { LINK_STYLE.bg(SELECTED_BG) } else { LINK_STYLE };
                // Show dynamic label for OAuth based on connection state
                let label = if *field == SettingsField::OpenAIOAuth {
                    if config.provider.openai_oauth.is_some() {
                        "ChatGPT Login: ✓ Connected (Enter to logout)"
                    } else {
                        "Login with ChatGPT →"
                    }
                } else {
                    field.label()
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{}  ", cursor), LABEL_STYLE),
                    Span::styled(label.to_string(), style),
                ]));
            }

            FieldKind::TextInput | FieldKind::SecretInput => {
                let display = if state.editing && is_selected {
                    if kind == FieldKind::SecretInput && !state.edit_buffer.is_empty() {
                        // Show plaintext while editing secret
                        state.edit_buffer.clone()
                    } else {
                        state.edit_buffer.clone()
                    }
                } else {
                    get_field_display(config, *field)
                };

                let is_empty = display.is_empty();
                let value_text = if is_empty { "(not set)".to_string() } else { display };
                let value_style = if state.editing && is_selected {
                    Style::new().fg(Color::White).bg(SELECTED_BG)
                } else if is_selected {
                    VALUE_STYLE.bg(SELECTED_BG)
                } else if is_empty {
                    HINT_STYLE
                } else {
                    VALUE_STYLE
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("{}{}: ", cursor, field.label()), LABEL_STYLE),
                    Span::styled(value_text, value_style),
                    if state.editing && is_selected {
                        Span::styled("▏", Style::new().fg(Color::White).bg(SELECTED_BG))
                    } else {
                        Span::raw("")
                    },
                ]));
            }
        }
    }

    // Scroll to keep selected visible
    let visible_height = inner.height as usize;
    let mut selected_line_idx = 0;
    let mut field_count = 0;
    for (li, line) in lines.iter().enumerate() {
        let text = line.spans.first().map(|s| s.content.as_ref()).unwrap_or("");
        if text.starts_with("▸ ") || text.starts_with("  ") && !text.starts_with("  ──") {
            if field_count == state.selected {
                selected_line_idx = li;
                break;
            }
            field_count += 1;
        }
    }
    let scroll = if selected_line_idx >= visible_height {
        (selected_line_idx - visible_height / 3) as u16
    } else {
        0
    };

    let paragraph = Paragraph::new(lines).style(Style::new().bg(CHAT_BG)).scroll((scroll, 0));
    frame.render_widget(paragraph, inner);
}
