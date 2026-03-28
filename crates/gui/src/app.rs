use std::collections::{HashMap, HashSet};

use iced::widget::{column, container, markdown, row, text_editor};
use iced::{Element, Length, Subscription, Task};
use tokio::sync::mpsc;
use uuid::Uuid;

use freako_core::agent::context::compact_messages;
use freako_core::agent::events::AgentEvent;
use freako_core::agent::loop_::{run_agent_loop, ApprovalResponse};
use freako_core::config;
use freako_core::config::types::{AppConfig, ContextConfig, OAuthCredentials};
use freako_core::provider::openai_oauth;
use freako_core::memory::store::{canonicalize_scope_key, MemoryStore};
use freako_core::memory::types::MemoryScope;
use freako_core::session::store::SessionStore;
use freako_core::session::title::maybe_generate_session_title;
use freako_core::session::types::Session;

use crate::ui::{approval_dialog, chat_view, input_area, plan_panel, settings_panel, sidebar, status_bar};
use crate::ui::approval_dialog::PendingApproval;
use crate::ui::chat_view::scroll_to_bottom;
use crate::ui::theme::AppTheme;

/// Summary of a stored session for the sidebar.
#[derive(Debug, Clone)]
pub struct SessionEntry {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct MemoryEntryView {
    pub title: String,
    pub content: String,
    pub scope: MemoryScope,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default)]
pub enum OAuthStatus {
    #[default]
    Idle,
    WaitingForCallback,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct GroupedMessage {
    pub id: Uuid,
    pub message: freako_core::session::types::ConversationMessage,
}

pub struct App {
    pub config: AppConfig,
    pub session: Session,
    pub input_text: String,
    pub input_content: text_editor::Content,
    pub streaming_text: String,
    pub streaming_content: markdown::Content,
    /// Tool calls accumulated during the current streaming response.
    pub streaming_tool_calls: Vec<(String, String, serde_json::Value)>,
    pub message_contents: Vec<markdown::Content>,
    /// Grouped messages (consecutive assistant+tool merged) for display
    pub grouped_messages: Vec<GroupedMessage>,
    /// Pre-parsed markdown for each grouped message
    pub grouped_md_contents: Vec<Vec<markdown::Content>>,
    /// Markdown text selection state
    pub md_selection: iced_selectable_markdown::SelectionState,
    pub plan_tasks: Vec<freako_core::agent::events::PlanTask>,
    pub plan_task_expanded: HashSet<String>,
    pub plan_task_md_cache: HashMap<String, markdown::Content>,
    pub show_plan_panel: bool,
    pub is_working: bool,
    pub is_thinking: bool,
    pub current_tool: Option<String>,
    pub tool_output_buffer: String,
    pub retry_status: Option<String>,
    pub show_settings: bool,
    pub session_list: Vec<SessionEntry>,
    pub memory_entries: Vec<MemoryEntryView>,
    pub available_models: Vec<String>,
    pub models_loading: bool,
    pub is_at_bottom: bool,
    /// How many messages to display (grows when user scrolls near top)
    pub visible_count: usize,
    /// Messages toggled to selectable text mode (grouped message id -> editor content)
    pub selectable_messages: HashMap<Uuid, text_editor::Content>,
    pub expanded_tool_results: HashSet<String>,
    /// Pre-computed parsed diffs for edit_file results, keyed by tool_call_id.
    pub parsed_diffs: HashMap<String, crate::ui::diff_view::ParsedDiff>,
    /// Animation frame counter for spinner (0–7)
    pub spinner_tick: u8,
    /// True when a plan has been presented and is awaiting user review.
    pub plan_pending_review: bool,
    /// Track whether Shift is held (for Shift+Enter newline in input)
    pub shift_held: bool,

    // OAuth state
    pub oauth_status: OAuthStatus,

    pub pending_approval: Option<crate::ui::approval_dialog::PendingApproval>,
    approval_tx: Option<mpsc::UnboundedSender<ApprovalResponse>>,
    event_rx: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    cancel_tx: Option<mpsc::UnboundedSender<()>>,
    queued_message_tx: Option<mpsc::UnboundedSender<String>>,
    /// Message typed while the agent is working — will be injected after the current tool finishes.
    pub queued_message: Option<String>,
    store: Option<SessionStore>,
    memory_store: Option<MemoryStore>,
}

#[derive(Debug, Clone, Copy)]
pub struct CompactionProgress {
    pub percent: u8,
    pub remaining_messages: usize,
    pub threshold_reached: bool,
}

pub fn compaction_progress(message_count: usize, context: &ContextConfig) -> Option<CompactionProgress> {
    if !context.enable_compaction {
        return None;
    }

    let trigger_at = context.compact_after_messages.max(1);
    let threshold_reached = message_count > trigger_at;
    let remaining_messages = if threshold_reached {
        0
    } else {
        trigger_at.saturating_sub(message_count)
    };
    let percent = if threshold_reached {
        100
    } else {
        ((message_count.saturating_mul(100)) / trigger_at).min(100) as u8
    };

    Some(CompactionProgress {
        percent,
        remaining_messages,
        threshold_reached,
    })
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Message {
    // Input
    InputChanged(String),
    InputAction(iced::widget::text_editor::Action),
    SendMessage,
    QueueMessage,
    StopAgent,
    TogglePlanMode,
    NoOp,
    EnterPressed,

    // Session management
    NewSession,
    LoadSession(String),
    DeleteSession(String),

    // Agent events
    AgentTick,

    // Approval
    ApprovalApprove,
    ApprovalApproveSession,
    ApprovalApproveAlways,
    ApprovalDeny,
    ToggleApprovalExpanded,

    // Settings
    ToggleSettings,
    ProviderTypeChanged(String),
    OpenAIApiBaseChanged(String),
    OpenAIApiKeyChanged(String),
    AnthropicApiKeyChanged(String),
    ModelChanged(String),
    AwsRegionChanged(String),
    AwsProfileChanged(String),
    MaxTokensChanged(String),
    TemperatureChanged(String),
    ContextCompactionEnabledChanged(String),
    CompactAfterMessagesChanged(String),
    KeepRecentMessagesChanged(String),
    SkillsEnabledChanged(String),
    SkillsSourceChanged(usize, String),
    AddSkillsSource,
    RemoveSkillsSource(usize),
    CompactNow,
    SystemPromptChanged(String),
    MemoryBankProjectChanged(String),
    MemoryBankGlobalChanged(String),

    // Models
    FetchModels,
    ModelsFetched(Vec<String>),
    ModelsFetchFailed(String),

    // Scroll
    ChatScrolled(iced::widget::scrollable::Viewport),
    LoadMoreMessages,

    // Clipboard / selection
    CopyMessage(Uuid),
    ToggleSelectMessage(Uuid),
    ToggleToolResult(String),
    ClearSelection,
    EditorAction(Uuid, text_editor::Action),

    // Animation
    AnimationTick,

    // OAuth
    OAuthStart,
    OAuthResult(Result<OAuthCredentials, String>),
    OAuthLogout,

    // Plan panel
    TogglePlanPanel,
    TogglePlanTaskExpanded(String),
    AcceptPlan,

    // Misc
    OpenUrl(String),
    LinkClicked(String),
    MdSelection(iced_selectable_markdown::SelectionAction),
    ShiftState(bool),
}

impl App {
    fn provider_type_from_label(label: &str) -> freako_core::config::types::ProviderType {
        use freako_core::config::types::ProviderType;
        match label {
            "Anthropic" => ProviderType::Anthropic,
            "Bedrock" => ProviderType::Bedrock,
            _ => ProviderType::OpenAI,
        }
    }

    pub fn models_for_current_provider(&self) -> Vec<String> {
        let mut models = if !self.available_models.is_empty() {
            self.available_models.clone()
        } else {
            self.config
                .provider
                .provider_type
                .models()
                .iter()
                .map(|s| s.to_string())
                .collect()
        };

        if !models.contains(&self.config.provider.model) {
            models.insert(0, self.config.provider.model.clone());
        }

        models
    }

    pub fn boot() -> (Self, Task<Message>) {
        let config = config::load_config().unwrap_or_default();
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into());
        let show_settings = !config.is_provider_configured();

        let store = SessionStore::open(&config.data_dir).ok();
        let memory_store = MemoryStore::open(&config.data_dir).ok();
        let session_list = Self::load_session_list(&store, &cwd);
        let memory_entries = Self::load_memory_entries(&memory_store, &cwd);

        let app = Self {
            config,
            session: Session::new(cwd),
            input_text: String::new(),
            input_content: text_editor::Content::new(),
            streaming_text: String::new(),
            streaming_content: markdown::Content::new(),
            streaming_tool_calls: Vec::new(),
            message_contents: Vec::new(),
            grouped_messages: Vec::new(),
            grouped_md_contents: Vec::new(),
            md_selection: iced_selectable_markdown::SelectionState::new(),
            plan_tasks: Vec::new(),
            plan_task_expanded: HashSet::new(),
            plan_task_md_cache: HashMap::new(),
            show_plan_panel: false,
            is_working: false,
            is_thinking: false,
            current_tool: None,
            tool_output_buffer: String::new(),
            retry_status: None,
            show_settings,
            pending_approval: None,
            session_list,
            memory_entries,
            available_models: Vec::new(),
            models_loading: false,
            is_at_bottom: true,
            visible_count: 10,
            selectable_messages: HashMap::new(),
            expanded_tool_results: HashSet::new(),
            parsed_diffs: HashMap::new(),
            spinner_tick: 0,
            plan_pending_review: false,
            shift_held: false,
            oauth_status: OAuthStatus::Idle,
            approval_tx: None,
            event_rx: None,
            cancel_tx: None,
            queued_message_tx: None,
            queued_message: None,
            store,
            memory_store,
        };

        // Kick off initial model fetch if provider is configured
        let task = if app.config.is_provider_configured() {
            Task::done(Message::FetchModels)
        } else {
            Task::none()
        };

        (app, task)
    }

    fn load_session_list(store: &Option<SessionStore>, working_directory: &str) -> Vec<SessionEntry> {
        store
            .as_ref()
            .and_then(|s| s.list_sessions(working_directory).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|(id, title, updated_at)| SessionEntry { id, title, updated_at })
            .collect()
    }

    fn load_memory_entries(memory_store: &Option<MemoryStore>, working_directory: &str) -> Vec<MemoryEntryView> {
        let mut entries = Vec::new();

        if let Some(store) = memory_store {
            if let Ok(project_memories) = store.list_project_memories(working_directory) {
                entries.extend(project_memories.into_iter().map(|entry| MemoryEntryView {
                    title: entry.title,
                    content: entry.content,
                    scope: entry.scope,
                    updated_at: entry.updated_at.to_rfc3339(),
                }));
            }
            if let Ok(global_memories) = store.list_global_memories() {
                entries.extend(global_memories.into_iter().map(|entry| MemoryEntryView {
                    title: entry.title,
                    content: entry.content,
                    scope: entry.scope,
                    updated_at: entry.updated_at.to_rfc3339(),
                }));
            }
        }

        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.title.cmp(&b.title)));
        entries
    }

    fn refresh_session_list(&mut self) {
        self.session_list = Self::load_session_list(&self.store, &self.session.working_directory);
    }

    fn refresh_memory_entries(&mut self) {
        self.memory_entries = Self::load_memory_entries(&self.memory_store, &self.session.working_directory);
    }

    /// Rebuild markdown Content cache for visible messages.
    fn rebuild_message_contents(&mut self) {
        self.visible_count = 10.min(self.session.messages.len());
        self.rebuild_visible_contents();
    }

    fn rebuild_visible_contents(&mut self) {
        use freako_core::session::types::{MessagePart, Role};

        let total = self.session.messages.len();
        let start = total.saturating_sub(self.visible_count);
        self.message_contents = self.session.messages[start..]
            .iter()
            .map(|msg| {
                let text = msg.full_text();
                if text.is_empty() {
                    markdown::Content::new()
                } else {
                    markdown::Content::parse(&text)
                }
            })
            .collect();

        // Build grouped messages: merge consecutive Assistant+Tool into single bubbles.
        // Any uninterrupted sequence of Assistant/Tool messages becomes one bubble.
        let visible = &self.session.messages[start..];
        let mut grouped: Vec<GroupedMessage> = Vec::new();
        for msg in visible {
            match msg.role {
                Role::Tool | Role::Assistant => {
                    if let Some(last) = grouped.last_mut() {
                        if last.message.role == Role::Assistant {
                            last.message.parts.extend(msg.parts.iter().cloned());
                            continue;
                        }
                    }
                    // Start a new assistant bubble (even for Tool messages that
                    // don't follow an assistant — treat them as assistant)
                    let mut merged = msg.clone();
                    if merged.role == Role::Tool {
                        merged.role = Role::Assistant;
                    }
                    grouped.push(GroupedMessage {
                        id: Uuid::new_v4(),
                        message: merged,
                    });
                }
                _ => grouped.push(GroupedMessage {
                    id: Uuid::new_v4(),
                    message: msg.clone(),
                }),
            }
        }
        // Filter out empty bubbles (no text AND no tool calls)
        grouped.retain(|group| {
            let msg = &group.message;
            let has_text = !msg.full_text().is_empty();
            let has_tools = msg.parts.iter().any(|p| matches!(p, MessagePart::ToolCall { .. }));
            has_text || has_tools || msg.role == Role::User || msg.role == Role::System
        });

        // Build per-text-segment markdown contents for each grouped message.
        // Each text part becomes a separate markdown::Content so we can interleave
        // text and tool groups in render order.
        self.grouped_md_contents = grouped.iter().map(|group| {
            group.message.parts.iter().filter_map(|part| {
                if let MessagePart::Text { text } = part {
                    if text.is_empty() {
                        return None;
                    }
                    let md_text = text.clone();
                    Some(markdown::Content::parse(&md_text))
                } else {
                    None
                }
            }).collect()
        }).collect();
        self.grouped_messages = grouped;
    }

    fn save_current_session(&mut self) {
        if self.session.messages.is_empty() {
            return;
        }
        if let Some(title) = maybe_generate_session_title(&self.session.title, &self.session.messages) {
            self.session.title = title;
        }
        self.session.updated_at = chrono::Utc::now();
        if let Some(store) = &self.store {
            let _ = store.save_session(&self.session);
        }
        self.refresh_session_list();
        self.refresh_memory_entries();
    }

    pub fn theme(&self) -> iced::Theme {
        AppTheme::iced_theme()
    }

    fn clear_input(&mut self) {
        self.input_text.clear();
        self.input_content = text_editor::Content::new();
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(text) => {
                self.input_text = text;
                Task::none()
            }

            Message::InputAction(action) => {
                // Intercept Enter key: send message unless Shift is held
                let is_enter = matches!(
                    &action,
                    text_editor::Action::Edit(iced::widget::text_editor::Edit::Enter)
                );

                if is_enter && !self.shift_held {
                    // Enter without Shift: send the message
                    return Task::done(Message::SendMessage);
                }
                // Shift+Enter (or any other action): let the editor handle it

                self.input_content.perform(action);
                self.input_text = self.input_content.text();
                Task::none()
            }

            Message::NoOp => Task::none(),
            Message::EnterPressed => {
                // Global Enter key — only act on it for plan approval
                if self.plan_pending_review && !self.is_working {
                    return Task::done(Message::SendMessage);
                }
                Task::none()
            }
            Message::ShiftState(held) => {
                self.shift_held = held;
                Task::none()
            }

            Message::SendMessage => {
                let text = self.input_text.trim().to_string();

                eprintln!("[SendMessage] plan_pending_review={} is_working={} text={:?}", self.plan_pending_review, self.is_working, text);

                if self.plan_pending_review {
                    if self.is_working {
                        eprintln!("[SendMessage] blocked: still working");
                        return Task::none();
                    }

                    if text.is_empty() {
                        return Task::done(Message::AcceptPlan);
                    }

                    self.plan_pending_review = false;
                    self.clear_input();
                    return self.start_agent_run_with_message(text);
                }

                if text.is_empty() || !self.config.is_provider_configured() {
                    return Task::none();
                }

                // If agent is working, queue the message instead of sending immediately
                if self.is_working {
                    return Task::done(Message::QueueMessage);
                }

                self.clear_input();
                self.streaming_text.clear();
                self.streaming_tool_calls.clear();
                self.streaming_content = markdown::Content::new();
                let content = markdown::Content::parse(&text);
                self.push_message(
                    freako_core::session::types::ConversationMessage::user(&text),
                    content,
                );
                self.rebuild_visible_contents();
                self.is_working = true;

                let (event_tx, event_rx) = mpsc::unbounded_channel();
                let (approval_tx, approval_rx) = mpsc::unbounded_channel();
                let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
                let (queued_message_tx, queued_message_rx) = mpsc::unbounded_channel();
                self.approval_tx = Some(approval_tx);
                self.event_rx = Some(event_rx);
                self.cancel_tx = Some(cancel_tx);
                self.queued_message_tx = Some(queued_message_tx);

                let config = self.config.clone();
                let mut session = self.session.clone();

                tokio::spawn(async move {
                    run_agent_loop(config, &mut session, event_tx, approval_rx, cancel_rx, queued_message_rx).await;
                });

                self.is_at_bottom = true;

                scroll_to_bottom()
            }

            Message::QueueMessage => {
                let text = self.input_text.trim().to_string();
                if text.is_empty() {
                    return Task::none();
                }
                // Replace any existing queued message (last one wins)
                self.queued_message = Some(text.clone());
                self.clear_input();
                // If there's already a queued message channel, send it now;
                // it will be injected after the current tool finishes.
                if let Some(tx) = &self.queued_message_tx {
                    let _ = tx.send(text);
                    // Keep queued_message as the visual indicator until QueuedMessageInjected
                }
                Task::none()
            }

            Message::StopAgent => {
                if let Some(tx) = self.cancel_tx.take() {
                    let _ = tx.send(());
                }
                self.is_working = false;
                self.pending_approval = None;
                self.queued_message = None;
                self.plan_pending_review = false;
                self.plan_tasks.clear();
                self.plan_task_expanded.clear();
                self.plan_task_md_cache.clear();
                self.show_plan_panel = false;
                self.queued_message_tx = None;
                Task::none()
            }

            Message::AgentTick => {
                let mut rx = self.event_rx.take();
                let mut had_events = false;
                if let Some(ref mut rx) = rx {
                    while let Ok(event) = rx.try_recv() {
                        self.handle_agent_event(event);
                        had_events = true;
                    }
                }
                if self.is_working {
                    self.event_rx = rx;
                }
                if had_events && self.is_at_bottom {
                    scroll_to_bottom()
                } else {
                    Task::none()
                }
            }

            Message::ApprovalApprove => {
                if let Some(tx) = &self.approval_tx {
                    let _ = tx.send(ApprovalResponse::Approve);
                }
                self.pending_approval = None;
                Task::none()
            }

            Message::ApprovalApproveSession => {
                if let Some(tx) = &self.approval_tx {
                    let _ = tx.send(ApprovalResponse::ApproveForSession);
                }
                self.pending_approval = None;
                Task::none()
            }

            Message::ApprovalApproveAlways => {
                if let Some(tx) = &self.approval_tx {
                    let _ = tx.send(ApprovalResponse::ApproveAlways);
                }
                
                // Add to config auto_approve list
                if let Some(approval) = &self.pending_approval {
                    if !self.config.auto_approve.contains(&approval.tool_name) {
                        self.config.auto_approve.push(approval.tool_name.clone());
                        // Save config
                        let _ = freako_core::config::save_config(&self.config);
                    }
                }
                
                self.pending_approval = None;
                Task::none()
            }

            Message::ApprovalDeny => {
                if let Some(tx) = &self.approval_tx {
                    let _ = tx.send(ApprovalResponse::Deny);
                }
                self.pending_approval = None;
                Task::none()
            }

            Message::ToggleApprovalExpanded => {
                if let Some(approval) = self.pending_approval.as_mut() {
                    approval.expanded = !approval.expanded;
                }
                Task::none()
            }

            Message::TogglePlanPanel => {
                self.show_plan_panel = !self.show_plan_panel;
                Task::none()
            }

            Message::TogglePlanTaskExpanded(id) => {
                if self.plan_task_expanded.contains(&id) {
                    self.plan_task_expanded.remove(&id);
                } else {
                    self.plan_task_expanded.insert(id);
                }
                Task::none()
            }

            Message::AcceptPlan => {
                eprintln!("[AcceptPlan] switching to execute mode");
                self.config.plan_mode = false;
                self.plan_pending_review = false;
                self.plan_task_expanded.clear(); // Collapse all tasks in execute mode
                let _ = freako_core::config::save_config(&self.config);
                self.clear_input();
                self.start_agent_run_with_message("Plan approved. Execute it.".to_string())
            }

            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
                if !self.show_settings {
                    let _ = config::save_config(&self.config);
                }
                Task::none()
            }

            Message::NewSession => {
                self.save_current_session();
                let cwd = self.session.working_directory.clone();
                self.session = Session::new(cwd);
                self.streaming_text.clear();
                self.streaming_tool_calls.clear();
                self.streaming_content = markdown::Content::new();
                self.message_contents.clear();
                self.grouped_messages.clear();
                self.grouped_md_contents.clear();
                self.visible_count = 0;
                self.is_at_bottom = true;
                self.selectable_messages.clear();
                self.is_working = false;
                self.pending_approval = None;
                self.plan_pending_review = false;
                self.plan_tasks.clear();
                self.plan_task_expanded.clear();
                self.plan_task_md_cache.clear();
                self.show_plan_panel = false;
                self.refresh_session_list();
                Task::none()
            }

            Message::LoadSession(id) => {
                self.save_current_session();
                if let Some(store) = &self.store {
                    if let Ok(Some(session)) = store.load_session(&id) {
                        self.session = session;
                        self.streaming_text.clear();
                        self.streaming_content = markdown::Content::new();
                        self.rebuild_message_contents();
                        self.is_working = false;
                        self.pending_approval = None;
                        self.is_at_bottom = true;
                        self.selectable_messages.clear();
                        return scroll_to_bottom();
                    }
                }
                Task::none()
            }

            Message::DeleteSession(id) => {
                if let Some(store) = &self.store {
                    let _ = store.delete_session(&id);
                }
                // If we deleted the current session, start fresh
                if self.session.id.to_string() == id {
                    let cwd = self.session.working_directory.clone();
                    self.session = Session::new(cwd);
                    self.streaming_text.clear();
                    self.streaming_tool_calls.clear();
                    self.streaming_content = markdown::Content::new();
                    self.message_contents.clear();
                    self.grouped_messages.clear();
                    self.grouped_md_contents.clear();
                    self.visible_count = 0;
                    self.is_at_bottom = true;
                    self.selectable_messages.clear();
                    self.expanded_tool_results.clear();
                    self.parsed_diffs.clear();
                    self.is_working = false;
                    self.is_thinking = false;
                    self.current_tool = None;
                    self.tool_output_buffer.clear();
                    self.retry_status = None;
                    self.pending_approval = None;
                    self.queued_message = None;
                    self.plan_pending_review = false;
                    self.plan_tasks.clear();
                    self.plan_task_expanded.clear();
                    self.plan_task_md_cache.clear();
                    self.show_plan_panel = false;
                }
                self.refresh_session_list();
                Task::none()
            }

            Message::ProviderTypeChanged(label) => {
                let new_type = Self::provider_type_from_label(&label);
                self.config.provider.provider_type = new_type.clone();
                self.available_models.clear();

                if !new_type.models().contains(&self.config.provider.model.as_str()) {
                    self.config.provider.model = new_type.default_model().to_string();
                }

                // Clear conversation — tool call IDs are provider-specific
                self.session.messages.clear();
                self.streaming_text.clear();
                self.streaming_tool_calls.clear();
                self.message_contents.clear();
                self.visible_count = 0;
                self.selectable_messages.clear();

                let _ = config::save_config(&self.config);
                if self.config.is_provider_configured() {
                    Task::done(Message::FetchModels)
                } else {
                    Task::none()
                }
            }

            Message::OpenAIApiBaseChanged(val) => {
                self.config.provider.openai_api_base = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }

            Message::OpenAIApiKeyChanged(val) => {
                self.config.provider.openai_api_key = if val.is_empty() { None } else { Some(val) };
                if self.config.is_provider_configured() {
                    Task::done(Message::FetchModels)
                } else {
                    Task::none()
                }
            }

            Message::AnthropicApiKeyChanged(val) => {
                self.config.provider.anthropic_api_key = if val.is_empty() { None } else { Some(val) };
                if self.config.is_provider_configured() {
                    Task::done(Message::FetchModels)
                } else {
                    Task::none()
                }
            }

            Message::ModelChanged(val) => {
                self.config.provider.model = val;
                let _ = config::save_config(&self.config);
                Task::none()
            }

            Message::AwsRegionChanged(val) => {
                self.config.provider.aws_region = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }

            Message::AwsProfileChanged(val) => {
                self.config.provider.aws_profile = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }

            Message::MaxTokensChanged(val) => {
                if let Ok(n) = val.parse::<u32>() {
                    self.config.provider.max_tokens = n;
                }
                Task::none()
            }

            Message::TemperatureChanged(val) => {
                self.config.provider.temperature = val.parse::<f32>().ok();
                Task::none()
            }

            Message::ContextCompactionEnabledChanged(val) => {
                self.config.context.enable_compaction = matches!(
                    val.trim().to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                );
                Task::none()
            }

            Message::CompactAfterMessagesChanged(val) => {
                if let Ok(n) = val.parse::<usize>() {
                    self.config.context.compact_after_messages = n.max(1);
                }
                Task::none()
            }

            Message::KeepRecentMessagesChanged(val) => {
                if let Ok(n) = val.parse::<usize>() {
                    self.config.context.keep_recent_messages = n.max(1);
                }
                Task::none()
            }

            Message::SkillsEnabledChanged(val) => {
                self.config.skills.enabled = matches!(
                    val.trim().to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                );
                Task::none()
            }

            Message::SkillsSourceChanged(index, val) => {
                if let Some(source) = self.config.skills.sources.get_mut(index) {
                    *source = val.trim().to_string();
                }
                Task::none()
            }

            Message::AddSkillsSource => {
                self.config.skills.sources.push(String::new());
                Task::none()
            }

            Message::RemoveSkillsSource(index) => {
                if index < self.config.skills.sources.len() {
                    self.config.skills.sources.remove(index);
                }
                if self.config.skills.sources.is_empty() {
                    self.config.skills.sources.push("vercel-labs/agent-skills".to_string());
                }
                Task::none()
            }

            Message::CompactNow => {
                let original_len = self.session.messages.len();
                if original_len < 3 {
                    return Task::none();
                }
                // Manual compact: force compaction regardless of threshold
                let mut forced_config = self.config.context.clone();
                forced_config.enable_compaction = true;
                forced_config.compact_after_messages = 0; // always trigger
                let compacted = compact_messages(&self.session.messages, &forced_config);
                if compacted.len() < original_len {
                    self.session.messages = compacted;
                    self.rebuild_message_contents();
                    self.save_current_session();
                    return scroll_to_bottom();
                }
                Task::none()
            }

            Message::SystemPromptChanged(val) => {
                self.config.system_prompt = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }

            Message::MemoryBankProjectChanged(val) => {
                if let Some(store) = &self.memory_store {
                    let id = format!("project:{}", self.session.working_directory);
                    if val.trim().is_empty() {
                        let _ = store.delete_memory(&id);
                    } else {
                        let _ = store.upsert_memory(
                            &id,
                            MemoryScope::Project,
                            &canonicalize_scope_key(&self.session.working_directory),
                            "Project Memory",
                            &val,
                        );
                    }
                    self.refresh_memory_entries();
                }
                Task::none()
            }

            Message::MemoryBankGlobalChanged(val) => {
                if let Some(store) = &self.memory_store {
                    let id = "global:default";
                    if val.trim().is_empty() {
                        let _ = store.delete_memory(id);
                    } else {
                        let _ = store.upsert_memory(
                            id,
                            MemoryScope::Global,
                            "global",
                            "Global Memory",
                            &val,
                        );
                    }
                    self.refresh_memory_entries();
                }
                Task::none()
            }

            Message::FetchModels => {
                if self.models_loading {
                    return Task::none();
                }
                self.models_loading = true;
                let config = self.config.provider.clone();
                Task::perform(
                    async move {
                        freako_core::provider::list_models(&config).await
                    },
                    |result| match result {
                        Ok(models) => Message::ModelsFetched(models),
                        Err(e) => Message::ModelsFetchFailed(e.to_string()),
                    },
                )
            }

            Message::ModelsFetched(models) => {
                self.models_loading = false;
                self.available_models = models;
                // If current model isn't in the list, keep it (user may have typed a custom one)
                Task::none()
            }

            Message::ModelsFetchFailed(_err) => {
                self.models_loading = false;
                // Keep whatever models we had (or empty - settings will show hardcoded fallback)
                Task::none()
            }

            Message::ChatScrolled(viewport) => {
                let content_height = viewport.content_bounds().height;
                let viewport_height = viewport.bounds().height;
                let offset_y = viewport.absolute_offset().y;
                self.is_at_bottom = offset_y + viewport_height >= content_height - 20.0;
                // Load more messages when scrolled near the top
                if offset_y < 100.0 && self.visible_count < self.session.messages.len() {
                    return Task::done(Message::LoadMoreMessages);
                }
                Task::none()
            }

            Message::LoadMoreMessages => {
                let old_count = self.visible_count;
                self.visible_count = (self.visible_count + 10).min(self.session.messages.len());
                if self.visible_count != old_count {
                    // Rebuild markdown contents for newly visible messages
                    self.rebuild_visible_contents();
                }
                Task::none()
            }

            Message::CopyMessage(group_id) => {
                if let Some(group) = self.grouped_messages.iter().find(|group| group.id == group_id) {
                    let text = group.message.full_text();
                    if !text.is_empty() {
                        return iced::clipboard::write(text);
                    }
                }
                Task::none()
            }

            Message::ClearSelection => {
                self.selectable_messages.clear();
                Task::none()
            }

            Message::ToggleToolResult(tool_call_id) => {
                if !self.expanded_tool_results.insert(tool_call_id.clone()) {
                    self.expanded_tool_results.remove(&tool_call_id);
                }
                Task::none()
            }

            Message::ToggleSelectMessage(group_id) => {
                if self.selectable_messages.contains_key(&group_id) {
                    self.selectable_messages.remove(&group_id);
                } else {
                    // Close any other open selectable messages
                    self.selectable_messages.clear();
                    if let Some(group) = self.grouped_messages.iter().find(|group| group.id == group_id) {
                        let full = chat_view::full_message_text(&group.message);
                        self.selectable_messages
                            .insert(group_id, text_editor::Content::with_text(&full));
                    }
                }
                Task::none()
            }

            Message::EditorAction(group_id, action) => {
                // Only allow selection actions, not edits
                let is_edit = action.is_edit();
                if let Some(content) = self.selectable_messages.get_mut(&group_id) {
                    if !is_edit {
                        content.perform(action);
                    }
                }
                Task::none()
            }

            Message::AnimationTick => {
                self.spinner_tick = (self.spinner_tick + 1) % 8;
                Task::none()
            }

            Message::OAuthStart => {
                let pkce = openai_oauth::generate_pkce();
                let url = openai_oauth::build_authorize_url(&pkce);
                let _ = open::that(&url);
                self.oauth_status = OAuthStatus::WaitingForCallback;

                // Spawn async task: wait for callback → exchange code → return result
                let verifier = pkce.code_verifier.clone();
                Task::perform(
                    async move {
                        let code = openai_oauth::wait_for_callback()
                            .await
                            .map_err(|e| e.to_string())?;
                        openai_oauth::exchange_code(&code, &verifier)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::OAuthResult,
                )
            }

            Message::OAuthResult(result) => {
                match result {
                    Ok(creds) => {
                        self.config.provider.openai_oauth = Some(creds);
                        self.oauth_status = OAuthStatus::Idle;
                        let _ = config::save_config(&self.config);
                        Task::done(Message::FetchModels)
                    }
                    Err(e) => {
                        self.oauth_status = OAuthStatus::Error(e);
                        Task::none()
                    }
                }
            }

            Message::OAuthLogout => {
                self.config.provider.openai_oauth = None;
                self.oauth_status = OAuthStatus::Idle;
                let _ = config::save_config(&self.config);
                Task::none()
            }

            Message::OpenUrl(url) => {
                let _ = open::that(&url);
                Task::none()
            }

            Message::LinkClicked(_url) => Task::none(),
            Message::MdSelection(action) => {
                let is_copy = matches!(action, iced_selectable_markdown::SelectionAction::Copy);
                self.md_selection.perform(action);
                if is_copy {
                    let text = self.md_selection.selected_text().to_string();
                    if !text.is_empty() {
                        return iced::clipboard::write(text);
                    }
                }
                Task::none()
            }

            Message::TogglePlanMode => {
                self.config.plan_mode = !self.config.plan_mode;
                let _ = freako_core::config::save_config(&self.config);
                Task::none()
            }
        }
    }

    fn start_agent_run_with_message(&mut self, text: String) -> Task<Message> {
        if text.trim().is_empty() || !self.config.is_provider_configured() {
            return Task::none();
        }

        self.streaming_text.clear();
        self.streaming_tool_calls.clear();
        self.streaming_content = markdown::Content::new();
        let content = markdown::Content::parse(&text);
        self.push_message(
            freako_core::session::types::ConversationMessage::user(&text),
            content,
        );
        self.is_working = true;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (approval_tx, approval_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
        let (queued_message_tx, queued_message_rx) = mpsc::unbounded_channel();
        self.approval_tx = Some(approval_tx);
        self.event_rx = Some(event_rx);
        self.cancel_tx = Some(cancel_tx);
        self.queued_message_tx = Some(queued_message_tx);

        let config = self.config.clone();
        let mut session = self.session.clone();

        tokio::spawn(async move {
            run_agent_loop(config, &mut session, event_tx, approval_rx, cancel_rx, queued_message_rx).await;
        });

        self.is_at_bottom = true;
        scroll_to_bottom()
    }

    /// Push a message and its markdown content, keeping visible_count in sync.
    fn push_message(
        &mut self,
        msg: freako_core::session::types::ConversationMessage,
        content: markdown::Content,
    ) {
        self.session.messages.push(msg);
        self.message_contents.push(content);
        self.visible_count += 1;
    }

    fn flush_streaming_text(&mut self) {
        // Build the assistant message from accumulated streaming data and add
        // it to the GUI's own session so that rebuild_visible_contents can
        // display it.  The agent loop operates on a *cloned* session, so its
        // messages never reach self.session.
        let has_text = !self.streaming_text.is_empty();
        let has_tools = !self.streaming_tool_calls.is_empty();
        if has_text || has_tools {
            use freako_core::session::types::{ConversationMessage, MessagePart};
            let mut parts = Vec::new();
            if has_text {
                parts.push(MessagePart::Text {
                    text: std::mem::take(&mut self.streaming_text),
                });
            }
            for (id, name, arguments) in self.streaming_tool_calls.drain(..) {
                parts.push(MessagePart::ToolCall { id, name, arguments });
            }
            self.session.messages.push(ConversationMessage::assistant(parts));
            self.visible_count += 1;
        }
        self.streaming_text.clear();
        self.streaming_content = markdown::Content::new();
    }

    fn save_streaming_text_as_message(&mut self) {
        // Save any partial streaming text / tool calls as an assistant message.
        // This is used when the agent is cancelled or encounters an error.
        use freako_core::session::types::{ConversationMessage, MessagePart};
        let has_text = !self.streaming_text.is_empty();
        let has_tools = !self.streaming_tool_calls.is_empty();
        if has_text || has_tools {
            let mut parts = Vec::new();
            let text = std::mem::take(&mut self.streaming_text);
            if !text.is_empty() {
                parts.push(MessagePart::Text { text });
            }
            for (id, name, arguments) in self.streaming_tool_calls.drain(..) {
                parts.push(MessagePart::ToolCall { id, name, arguments });
            }
            let display_text = parts.iter().filter_map(|p| match p {
                MessagePart::Text { text } => Some(text.as_str()),
                _ => None,
            }).collect::<Vec<_>>().join("");
            let content = if display_text.is_empty() {
                markdown::Content::new()
            } else {
                markdown::Content::parse(&display_text)
            };
            self.push_message(ConversationMessage::assistant(parts), content);
            self.streaming_content = markdown::Content::new();
        }
    }

    fn handle_agent_event(&mut self, event: AgentEvent) {
        use freako_core::session::types::{ConversationMessage, MessagePart};

        match event {
            AgentEvent::Thinking => {
                self.is_thinking = true;
                self.retry_status = None;
            }
            AgentEvent::RetryScheduled { error, attempt, max_attempts, delay_ms } => {
                self.is_thinking = true;
                self.current_tool = None;
                self.retry_status = Some(format!(
                    "Retrying after error ({}/{}), waiting {}ms… {}",
                    attempt,
                    max_attempts,
                    delay_ms,
                    error
                ));
            }
            AgentEvent::StreamDelta(text) => {
                self.is_thinking = false;
                self.retry_status = None;
                self.streaming_text.push_str(&text);
                self.streaming_content.push_str(&text);
            }
            AgentEvent::ToolCallRequested { id, name, arguments } => {
                // Track the tool call so flush_streaming_text can build the
                // complete assistant message on the GUI's own session.
                self.streaming_tool_calls.push((id, name, arguments));
            }
            AgentEvent::ToolApprovalNeeded { id, name, arguments } => {
                self.pending_approval = Some(PendingApproval {
                    id,
                    tool_name: name,
                    arguments,
                    expanded: false,
                });
            }
            AgentEvent::ToolExecutionStarted { name, .. } => {
                self.is_thinking = false;
                self.retry_status = None;
                self.current_tool = Some(name);
                self.tool_output_buffer.clear();
            }
            AgentEvent::ToolOutputDelta { output, .. } => {
                self.is_thinking = false;
                self.tool_output_buffer.push_str(&output);
            }
            AgentEvent::ToolResult { tool_call_id, name, content, is_error, arguments } => {
                self.is_thinking = false;
                self.current_tool = None;
                let streamed_output = if self.tool_output_buffer.is_empty() {
                    None
                } else {
                    Some(std::mem::take(&mut self.tool_output_buffer))
                };
                // Auto-expand edit_file diffs and pre-compute the parsed diff
                if name == "edit_file" {
                    self.expanded_tool_results.insert(tool_call_id.clone());
                    let path = arguments.as_ref()
                        .and_then(|args| args.get("path").and_then(|v| v.as_str()));
                    let parsed = crate::ui::diff_view::parse_diff(&content, path);
                    self.parsed_diffs.insert(tool_call_id.clone(), parsed);
                } else {
                    self.expanded_tool_results.remove(&tool_call_id);
                }
                if let Some(message) = self.session.messages.iter_mut().rev().find(|message| {
                    message.parts.iter().any(|part| matches!(part,
                        MessagePart::ToolCall { id, .. } if id == &tool_call_id
                    ))
                }) {
                    if let Some(output) = &streamed_output {
                        message.parts.push(MessagePart::ToolOutput {
                            tool_call_id: tool_call_id.clone(),
                            stream: freako_core::agent::events::ToolOutputStream::Stdout,
                            content: output.clone(),
                        });
                    }
                    message.parts.push(MessagePart::ToolResult {
                        tool_call_id,
                        name,
                        content,
                        is_error,
                        arguments,
                    });
                } else {
                    let mut message = ConversationMessage::tool_result(tool_call_id.clone(), name, content, is_error, arguments);
                    if let Some(output) = streamed_output {
                        message.parts.insert(0, MessagePart::ToolOutput {
                            tool_call_id,
                            stream: freako_core::agent::events::ToolOutputStream::Stdout,
                            content: output,
                        });
                    }
                    self.session.messages.push(message);
                    self.visible_count += 1;
                }
                self.rebuild_visible_contents();
                self.is_at_bottom = true;
            }
            AgentEvent::EnteredPlanMode => {
                self.config.plan_mode = true;
                let _ = freako_core::config::save_config(&self.config);
                let notice = "*Agent switched to Plan Mode.*";
                let content = markdown::Content::parse(notice);
                self.push_message(
                    ConversationMessage::assistant(vec![
                        MessagePart::Text { text: notice.to_string() },
                    ]),
                    content,
                );
            }
            AgentEvent::PlanUpdated { tasks } => {
                for task in &tasks {
                    self.plan_task_md_cache.insert(
                        task.id.clone(),
                        markdown::Content::parse(&task.description),
                    );
                }
                self.plan_tasks = tasks;
                self.show_plan_panel = true;
            }
            AgentEvent::PlanReadyForReview { tasks } => {
                for task in &tasks {
                    self.plan_task_md_cache.insert(
                        task.id.clone(),
                        markdown::Content::parse(&task.description),
                    );
                }
                self.plan_tasks = tasks.clone();
                self.plan_pending_review = true;
                self.show_plan_panel = true;
                // Expand all tasks for review
                self.plan_task_expanded = tasks.iter().map(|t| t.id.clone()).collect();
                let notice = "*Plan ready. Review the plan panel, then Accept or type feedback.*";
                let content = markdown::Content::parse(notice);
                self.push_message(
                    ConversationMessage::assistant(vec![
                        MessagePart::Text { text: notice.to_string() },
                    ]),
                    content,
                );
            }
            AgentEvent::PlanTaskStatusChanged { tasks } => {
                self.plan_tasks = tasks;
            }
            AgentEvent::ResponseComplete { usage, .. } => {
                // flush_streaming_text now builds the assistant message from
                // accumulated text + tool calls and adds it to self.session.
                self.flush_streaming_text();
                self.rebuild_visible_contents();
                self.is_at_bottom = true; // ensure auto-scroll after content change
                self.session.total_input_tokens += usage.input_tokens;
                self.session.total_output_tokens += usage.output_tokens;
            }
            AgentEvent::QueuedMessageInjected => {
                // The queued message was injected into the agent's session.
                // Show it in the GUI's own session too.
                if let Some(text) = self.queued_message.take() {
                    let content = markdown::Content::parse(&text);
                    self.push_message(
                        freako_core::session::types::ConversationMessage::user(&text),
                        content,
                    );
                    self.rebuild_visible_contents();
                }
            }
            AgentEvent::Done => {
                self.flush_streaming_text();
                // The agent loop has already added the final assistant message to the session.
                // Rebuild the display to show it.
                self.rebuild_visible_contents();
                self.is_at_bottom = true; // ensure auto-scroll to show final message
                self.is_working = false;
                self.is_thinking = false;
                self.current_tool = None;
                self.retry_status = None;
                self.tool_output_buffer.clear();
                self.queued_message = None;
                // Don't reset plan_pending_review here — it may have just
                // been set by PlanReadyForReview which fires right before Done.
                self.event_rx = None;
                self.approval_tx = None;
                self.cancel_tx = None;
                self.queued_message_tx = None;
                // Auto-compact if threshold is exceeded (count parts/interactions, not just messages)
                let interaction_count: usize = self.session.messages.iter().map(|m| m.parts.len().max(1)).sum();
                if self.config.context.enable_compaction
                    && interaction_count > self.config.context.compact_after_messages
                {
                    let original_len = self.session.messages.len();
                    let compacted = compact_messages(&self.session.messages, &self.config.context);
                    if compacted.len() < original_len {
                        self.session.messages = compacted;
                        self.rebuild_message_contents();
                    }
                }
                // Auto-save after completion
                self.save_current_session();
            }
            AgentEvent::Cancelled => {
                self.save_streaming_text_as_message();
                self.is_working = false;
                self.is_thinking = false;
                self.current_tool = None;
                self.retry_status = None;
                self.tool_output_buffer.clear();
                self.queued_message = None;
                self.plan_pending_review = false;
                let cancelled_text = "*Agent stopped by user*";
                let content = markdown::Content::parse(cancelled_text);
                self.push_message(
                    ConversationMessage::assistant(vec![
                        MessagePart::Text { text: cancelled_text.to_string() },
                    ]),
                    content,
                );
                self.event_rx = None;
                self.approval_tx = None;
                self.cancel_tx = None;
                self.queued_message_tx = None;
                self.save_current_session();
            }
            AgentEvent::Error(err) => {
                self.save_streaming_text_as_message();
                self.is_working = false;
                self.is_thinking = false;
                self.current_tool = None;
                self.retry_status = None;
                self.tool_output_buffer.clear();
                self.queued_message = None;
                self.plan_pending_review = false;
                let error_text = format!("Error: {}", err);
                let content = markdown::Content::parse(&error_text);
                self.push_message(
                    ConversationMessage::assistant(vec![
                        MessagePart::Text { text: error_text },
                    ]),
                    content,
                );
                self.rebuild_visible_contents();
                self.event_rx = None;
                self.approval_tx = None;
                self.cancel_tx = None;
                self.queued_message_tx = None;
                self.save_current_session();
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = Vec::new();

        if self.is_working {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(50)).map(|_| Message::AgentTick),
            );
            subs.push(
                iced::time::every(std::time::Duration::from_millis(120)).map(|_| Message::AnimationTick),
            );
        }

        {
            subs.push(iced::keyboard::listen().map(|event| match event {
                iced::keyboard::Event::ModifiersChanged(modifiers) => {
                    Message::ShiftState(modifiers.shift())
                }
                iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                } => Message::ClearSelection,
                iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(ch),
                    modifiers,
                    ..
                } if modifiers.control() && !modifiers.command() && !modifiers.alt()
                    && ch.eq_ignore_ascii_case("m") => {
                    Message::CompactNow
                }
                // Enter key globally — used for plan approval when text editor may not have focus
                iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                    ..
                } => Message::EnterPressed,
                iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab),
                    modifiers,
                    ..
                } if !modifiers.command() && !modifiers.control() && !modifiers.alt() => {
                    Message::TogglePlanMode
                }
                _ => Message::NoOp,
            }));
        }

        Subscription::batch(subs)
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.show_settings {
            return settings_panel::view(self);
        }

        let mut main_col = column![]
            .width(Length::Fill)
            .height(Length::Fill);

        main_col = main_col.push(status_bar::view(self));
        main_col = main_col.push(chat_view::view(self));
        main_col = main_col.push(input_area::view(self));

        let mut layout_row = row![
            sidebar::view(&self.session_list, self.session.id.to_string()),
            container(main_col).width(Length::Fill),
        ];

        if self.show_plan_panel && !self.plan_tasks.is_empty() {
            layout_row = layout_row.push(plan_panel::view(self));
        }

        let main_view: Element<'_, Message> = layout_row
            .height(Length::Fill)
            .into();

        if let Some(approval) = &self.pending_approval {
            iced::widget::stack![
                main_view,
                approval_dialog::view(approval),
            ]
            .into()
        } else {
            main_view
        }
    }
}
