mod message;
mod render;
mod settings;
mod tools;

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::{
    event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use freako_core::agent::events::{AgentEvent, ToolOutputStream};
use freako_core::agent::loop_::{run_agent_loop, ApprovalResponse};
use freako_core::config::types::AppConfig;
use freako_core::session::store::SessionStore;
use freako_core::session::title::maybe_generate_session_title;
use freako_core::session::types::{ConversationMessage, MessagePart, Session};

// ── TUI Color Palette (256-color safe) ──────────────────────────
const USER_STYLE: Style = Style::new().fg(Color::Indexed(250));
const ASSISTANT_STYLE: Style = Style::new().fg(Color::Indexed(252));
const TOOL_STYLE: Style = Style::new().fg(Color::Indexed(180));
const SYSTEM_STYLE: Style = Style::new().fg(Color::Indexed(145));
const INPUT_CURSOR_STYLE: Style = Style::new().bg(Color::Indexed(240)).fg(Color::White);
const CHAT_BG: Color = Color::Rgb(18, 18, 20);
const INPUT_BG: Color = Color::Rgb(24, 24, 27);
const APPROVAL_BG: Color = Color::Rgb(30, 30, 34);
const SIDEBAR_BG: Color = Color::Rgb(22, 22, 25);
const SELECTED_BG: Color = Color::Rgb(58, 58, 64);
const SHELL_CONSOLE_BG: Color = Color::Black;
const SHELL_CONSOLE_FG: Color = Color::Indexed(252);
const SHELL_CONSOLE_BORDER: Color = Color::Indexed(240);
// Per-role message bubble backgrounds
const USER_MSG_BG: Color = Color::Rgb(28, 28, 34);
const ASSISTANT_MSG_BG: Color = Color::Rgb(20, 26, 34);
const SYSTEM_MSG_BG: Color = Color::Rgb(34, 26, 26);
const TOOL_MSG_BG: Color = Color::Rgb(22, 24, 30);
const THINKING_STYLE: Style = Style::new().fg(Color::Indexed(145));
const SUCCESS_STYLE: Style = Style::new().fg(Color::Indexed(78));
const ERROR_STYLE: Style = Style::new().fg(Color::Indexed(167));
const HEADING_STYLE: Style = Style::new().fg(Color::Indexed(223));
const CODE_STYLE: Style = Style::new().fg(Color::Indexed(151));
const CODE_BLOCK_BG: Color = Color::Rgb(12, 12, 16);
const INLINE_CODE_BG: Color = Color::Rgb(38, 38, 46);
const MUTED_STYLE: Style = Style::new().fg(Color::Indexed(243));
const SPINNER_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠇"];

const SHELL_COMPACT_HEAD_LINES: usize = 4;
const SHELL_COMPACT_TAIL_LINES: usize = 4;
const SHELL_CONSOLE_VISIBLE_LINES: usize = 10;
const SHELL_CONSOLE_SCROLL_STEP: usize = 3;

#[derive(Clone, Debug)]
pub(super) struct LiveShellChunk {
    pub stream: ToolOutputStream,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct LiveShellOutput {
    pub chunks: Vec<LiveShellChunk>,
}

impl LiveShellOutput {
    fn push(&mut self, stream: ToolOutputStream, text: String) {
        self.chunks.push(LiveShellChunk { stream, text });
    }

    fn plain_text(&self) -> String {
        self.chunks.iter().map(|chunk| chunk.text.as_str()).collect()
    }
}

#[derive(Clone, Debug)]
pub(super) struct CompletedShellOutput {
    pub plain: String,
    pub compacted: String,
    pub chunks: Vec<LiveShellChunk>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PendingShellViewport {
    pub tool_call_id: Option<String>,
    pub scroll_lines_from_bottom: usize,
}

enum InputMode {
    Normal,
    Editing,
    WaitingApproval,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalKind {
    File,
    Shell,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    tool_name: String,
    args_json: String,
    kind: ApprovalKind,
}

struct App {
    input: String,
    input_mode: InputMode,
    session: Session,
    messages: Vec<String>,
    event_rx: mpsc::UnboundedReceiver<AgentEvent>,
    approval_tx: mpsc::UnboundedSender<ApprovalResponse>,
    pending_approval: Option<PendingApproval>,
    approval_cursor: usize,
    store: Option<SessionStore>,
    session_list: Vec<(String, String, String)>,
    session_list_selected: usize,
    selecting_session: bool,
    streaming_text: String,
    streaming_tool_calls: Vec<(String, String, serde_json::Value)>,
    current_tool: Option<String>,
    current_plan_text: Option<String>,
    tool_output_buffer: String,
    live_shell_outputs: HashMap<String, LiveShellOutput>,
    completed_shell_outputs: HashMap<String, CompletedShellOutput>,
    active_shell_tool_call_id: Option<String>,
    shell_tool_order: VecDeque<String>,
    hovered_shell_tool_call_id: Option<String>,
    shell_mouse_regions: Vec<(String, Rect)>,
    shell_viewport: PendingShellViewport,
    is_thinking: bool,
    is_working: bool,
    spinner_tick: u8,
    scroll_offset: u16,
    status_message: String,
    config: AppConfig,
    plan_pending_review: bool,
    settings_state: settings::SettingsState,
    /// Images pending to be sent with the next message (media_type, base64_data).
    pending_images: Vec<(String, String)>,
    /// Channel for receiving OAuth results from background task.
    oauth_result_tx: mpsc::UnboundedSender<Result<freako_core::config::types::OAuthCredentials, String>>,
    oauth_result_rx: mpsc::UnboundedReceiver<Result<freako_core::config::types::OAuthCredentials, String>>,
    /// Channel for receiving LLM compaction results from background task.
    compact_result_tx: mpsc::UnboundedSender<Vec<freako_core::session::types::ConversationMessage>>,
    compact_result_rx: mpsc::UnboundedReceiver<Vec<freako_core::session::types::ConversationMessage>>,
    queued_message: Option<String>,
    queued_message_tx: Option<mpsc::UnboundedSender<String>>,
    cancel_tx: Option<mpsc::UnboundedSender<()>>,
    /// All tool views, keyed by tool_call_id. Created on ToolExecutionStarted.
    tool_views: HashMap<String, Box<dyn tools::ToolView>>,
    /// Per-tool collapse state: tool_call_id -> is_collapsed.
    tool_collapse_state: HashMap<String, bool>,
    /// Bumped on collapse/scroll toggle to invalidate chat cache.
    collapse_generation: u64,
    /// Tool header screen regions for click-to-toggle (computed each frame).
    tool_header_regions: Vec<(String, Rect)>,
    /// Tool header line indices in chat_lines_cache (recorded during cache build).
    tool_header_line_indices: Vec<(String, usize)>,
    /// Pre-computed wrapped visual positions for tool headers (computed during cache build).
    tool_header_wrapped_positions: Vec<(String, u32)>,
    /// Render-time values needed for click-to-logical-line mapping.
    last_scroll: u16,
    last_inner_width: u16,
    last_chat_area_y: u16,
    last_chat_area_height: usize,
    /// Cached rendered chat lines. Invalidated when `chat_cache_msg_count` changes.
    chat_lines_cache: Vec<Line<'static>>,
    /// Line ranges for each grouped message in the cache (start, end).
    chat_ranges_cache: Vec<(usize, usize)>,
    /// The message count when the cache was built.
    chat_cache_msg_count: usize,
    /// The terminal width when the cache was built.
    chat_cache_width: u16,
    /// The collapse generation when the cache was built.
    chat_cache_collapse_gen: u64,
}

fn approval_options(_kind: ApprovalKind) -> &'static [&'static str] {
    &["Approve", "Approve for Session", "Always Approve", "Deny"]
}

fn next_approval_cursor(kind: ApprovalKind, cursor: usize) -> usize {
    (cursor + 1) % approval_options(kind).len()
}

fn prev_approval_cursor(kind: ApprovalKind, cursor: usize) -> usize {
    let len = approval_options(kind).len();
    (cursor + len - 1) % len
}

fn build_approval_response(kind: ApprovalKind, cursor: usize) -> ApprovalResponse {
    match approval_options(kind)[cursor] {
        "Approve" => ApprovalResponse::Approve,
        "Approve for Session" => ApprovalResponse::ApproveForSession,
        "Always Approve" => ApprovalResponse::ApproveAlways,
        _ => ApprovalResponse::Deny,
    }
}

fn open_store(data_dir: &PathBuf) -> Option<SessionStore> {
    SessionStore::open(data_dir).ok()
}

fn load_session_list(app: &mut App) {
    if let Some(store) = &app.store {
        if let Ok(list) = store.list_sessions(&app.session.working_directory) {
            app.session_list = list;
            if app.session_list_selected >= app.session_list.len() && !app.session_list.is_empty() {
                app.session_list_selected = app.session_list.len() - 1;
            }
        }
    }
}

fn load_session(app: &mut App, session_id: &str) {
    if let Some(store) = &app.store {
        if let Ok(Some(session)) = store.load_session(session_id) {
            app.session = session;
            app.messages.clear();
            app.streaming_text.clear();
            app.input_mode = InputMode::Normal;
            app.selecting_session = false;
            app.pending_approval = None;
            app.live_shell_outputs.clear();
            app.completed_shell_outputs.clear();
            app.active_shell_tool_call_id = None;
            app.shell_tool_order.clear();
            app.hovered_shell_tool_call_id = None;
            app.shell_mouse_regions.clear();
            app.shell_viewport = PendingShellViewport::default();
            populate_tool_views_from_session(app);
        }
    }
}

fn reset_shell_console_state(app: &mut App) {
    app.live_shell_outputs.clear();
    app.completed_shell_outputs.clear();
    app.active_shell_tool_call_id = None;
    app.shell_tool_order.clear();
    app.hovered_shell_tool_call_id = None;
    app.shell_mouse_regions.clear();
    app.shell_viewport = PendingShellViewport::default();
}

fn shell_output_for<'a>(app: &'a App, tool_call_id: &str) -> Option<&'a str> {
    if app.live_shell_outputs.contains_key(tool_call_id) {
        None
    } else {
        app.completed_shell_outputs.get(tool_call_id).map(|output| output.plain.as_str())
    }
}

fn sync_shell_viewport_target(app: &mut App) {
    let preferred = app
        .hovered_shell_tool_call_id
        .clone()
        .or_else(|| app.active_shell_tool_call_id.clone())
        .or_else(|| app.shell_tool_order.back().cloned());

    if app.shell_viewport.tool_call_id != preferred {
        app.shell_viewport.tool_call_id = preferred;
        app.shell_viewport.scroll_lines_from_bottom = 0;
    }
}

fn update_shell_hover(app: &mut App, column: u16, row: u16) {
    app.hovered_shell_tool_call_id = app
        .shell_mouse_regions
        .iter()
        .find(|(_, rect)| {
            column >= rect.x
                && column < rect.x.saturating_add(rect.width)
                && row >= rect.y
                && row < rect.y.saturating_add(rect.height)
        })
        .map(|(id, _)| id.clone());
    sync_shell_viewport_target(app);
}

/// Find which tool header was clicked by mapping screen row to logical line.
/// Uses ratatui's Paragraph to measure wrapped line heights accurately.
fn find_clicked_tool_header(app: &App, row: u16) -> Option<String> {
    use ratatui::widgets::{Paragraph, Wrap};

    if row < app.last_chat_area_y {
        return None;
    }
    let visual_row_in_chat = (row - app.last_chat_area_y) as usize;
    if visual_row_in_chat >= app.last_chat_area_height {
        return None;
    }
    // The absolute visual line in the wrapped content
    let target_visual = visual_row_in_chat + app.last_scroll as usize;

    // Build a set of header line indices
    let header_map: std::collections::HashMap<usize, &str> = app.tool_header_line_indices
        .iter()
        .map(|(id, idx)| (*idx, id.as_str()))
        .collect();

    if header_map.is_empty() {
        return None;
    }

    // Walk logical lines, accumulating wrapped heights
    let mut cumulative_visual: usize = 0;
    let w = app.last_inner_width;
    for (i, line) in app.chat_lines_cache.iter().enumerate() {
        let p = Paragraph::new(vec![line.clone()]).wrap(Wrap { trim: false });
        let line_height = p.line_count(w);

        if target_visual >= cumulative_visual && target_visual < cumulative_visual + line_height {
            return header_map.get(&i).map(|id| id.to_string());
        }
        cumulative_visual += line_height;
    }
    None
}

/// Find a tool_call_id if the mouse is over a tool header region (for scrollable tools).
fn find_hovered_scrollable_tool(app: &App, column: u16, row: u16) -> Option<String> {
    // Check tool header regions — tool body is below the header
    for (tool_call_id, rect) in &app.tool_header_regions {
        // The body region starts right after the header and extends some lines
        // For simplicity, check if mouse is anywhere near the tool's area
        if column >= rect.x && column < rect.x.saturating_add(rect.width)
            && row >= rect.y
        {
            if let Some(view) = app.tool_views.get(tool_call_id) {
                if view.is_scrollable() {
                    return Some(tool_call_id.clone());
                }
            }
        }
    }
    None
}

fn scroll_hovered_shell(app: &mut App, delta: isize) -> bool {
    let Some(tool_call_id) = app.hovered_shell_tool_call_id.clone() else {
        return false;
    };
    let Some(output) = shell_output_for(app, &tool_call_id) else {
        return false;
    };

    let total_lines = output.lines().count().max(1);
    let max_offset = total_lines.saturating_sub(SHELL_CONSOLE_VISIBLE_LINES);
    app.shell_viewport.tool_call_id = Some(tool_call_id);
    let current = app.shell_viewport.scroll_lines_from_bottom as isize;
    let next = (current + delta).clamp(0, max_offset as isize) as usize;
    app.shell_viewport.scroll_lines_from_bottom = next;
    true
}

fn populate_tool_views_from_session(app: &mut App) {
    app.tool_views.clear();
    app.tool_collapse_state.clear();
    // Build args map across ALL messages (ToolCall and ToolResult may be in different messages)
    let mut tool_call_args: HashMap<String, serde_json::Value> = HashMap::new();
    for msg in &app.session.messages {
        for part in &msg.parts {
            if let MessagePart::ToolCall { id, arguments, .. } = part {
                tool_call_args.insert(id.clone(), arguments.clone());
            }
        }
    }
    for msg in &app.session.messages {
        for part in &msg.parts {
            if let MessagePart::ToolResult {
                tool_call_id, name, content, is_error, arguments, ..
            } = part {
                let args = arguments.clone()
                    .or_else(|| tool_call_args.get(tool_call_id).cloned())
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                if let Some(tool_call) = freako_core::tools::tool_name::ToolCall::from_raw(name, &args) {
                    let mut view = tools::create_tool_view(tool_call_id.clone(), tool_call);
                    view.set_result(content, *is_error);
                    let default_collapsed = view.default_collapsed();
                    app.tool_views.insert(tool_call_id.clone(), view);
                    app.tool_collapse_state.insert(tool_call_id.clone(), default_collapsed);
                }
            }
        }
    }
}

fn start_agent(app: &mut App, config: AppConfig) {
    app.is_working = true;
    app.is_thinking = true;
    app.streaming_text.clear();
    app.streaming_tool_calls.clear();
    app.current_tool = None;
    app.tool_output_buffer.clear();
    reset_shell_console_state(app);
    app.tool_views.clear();
    app.tool_collapse_state.clear();
    app.status_message = "Thinking…".into();
    app.scroll_offset = 0;

    let (event_tx, event_rx) = mpsc::unbounded_channel();
    app.event_rx = event_rx;

    let (approval_tx, approval_rx) = mpsc::unbounded_channel();
    app.approval_tx = approval_tx;

    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();
    app.cancel_tx = Some(cancel_tx);

    let (queued_message_tx, queued_message_rx) = mpsc::unbounded_channel();
    app.queued_message_tx = Some(queued_message_tx);

    let mut session = app.session.clone();

    tokio::spawn(async move {
        run_agent_loop(config, &mut session, event_tx, approval_rx, cancel_rx, queued_message_rx).await;
    });
}

/// Try to read an image from the clipboard. Returns (media_type, base64_data) or None.
fn clipboard_image_base64() -> Option<(String, String)> {
    use arboard::Clipboard;
    use base64::Engine;
    use image::ImageEncoder;

    let mut clipboard = Clipboard::new().ok()?;
    let img_data = clipboard.get_image().ok()?;

    // Convert RGBA bytes to PNG
    let mut png_buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png_buf));
    encoder.write_image(
        &img_data.bytes,
        img_data.width as u32,
        img_data.height as u32,
        image::ExtendedColorType::Rgba8,
    ).ok()?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);
    Some(("image/png".to_string(), b64))
}

/// Try to read text from the clipboard.
fn clipboard_text() -> Option<String> {
    use arboard::Clipboard;
    let mut clipboard = Clipboard::new().ok()?;
    clipboard.get_text().ok()
}

fn save_session(app: &mut App) {
    if app.session.messages.is_empty() {
        return;
    }
    if let Some(title) = maybe_generate_session_title(&app.session.title, &app.session.messages) {
        app.session.title = title;
    }
    app.session.updated_at = chrono::Utc::now();
    if let Some(store) = &app.store {
        let _ = store.save_session(&app.session);
    }
}

fn flush_streaming_assistant_parts(app: &mut App) {
    if !app.streaming_text.is_empty() || !app.streaming_tool_calls.is_empty() {
        let mut parts = Vec::new();
        let text = std::mem::take(&mut app.streaming_text);
        if !text.is_empty() {
            parts.push(MessagePart::Text { text });
        }
        for (id, name, arguments) in app.streaming_tool_calls.drain(..) {
            parts.push(MessagePart::ToolCall { id, name, arguments });
        }
        app.session.messages.push(ConversationMessage::assistant(parts));
    }
}

pub async fn run(config: AppConfig, working_directory: String) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;
    stdout.execute(EnableBracketedPaste)?;
    // Enable keyboard enhancement so Ctrl+V arrives as a key event (for image paste)
    let keyboard_enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if keyboard_enhanced {
        let _ = stdout.execute(PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
        ));
    }
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let (_event_tx, event_rx) = mpsc::unbounded_channel();
    let (oauth_tx, oauth_rx) = mpsc::unbounded_channel();
    let (compact_tx, compact_rx) = mpsc::unbounded_channel();
    let (approval_tx, approval_rx_dummy) = mpsc::unbounded_channel::<ApprovalResponse>();
    drop(approval_rx_dummy);

    let mut app = App {
        input: String::new(),
        input_mode: InputMode::Editing,
        session: Session::new(working_directory.clone()),
        messages: Vec::new(),
        event_rx,
        approval_tx,
        pending_approval: None,
        approval_cursor: 0,
        store: open_store(&config.data_dir),
        session_list: Vec::new(),
        session_list_selected: 0,
        selecting_session: false,
        streaming_text: String::new(),
        streaming_tool_calls: Vec::new(),
        current_tool: None,
        current_plan_text: None,
        tool_output_buffer: String::new(),
        live_shell_outputs: HashMap::new(),
        completed_shell_outputs: HashMap::new(),
        active_shell_tool_call_id: None,
        shell_tool_order: VecDeque::new(),
        hovered_shell_tool_call_id: None,
        shell_mouse_regions: Vec::new(),
        shell_viewport: PendingShellViewport::default(),
        is_thinking: false,
        is_working: false,
        spinner_tick: 0,
        scroll_offset: 0,
        status_message: String::new(),
        config: config.clone(),
        plan_pending_review: false,
        settings_state: settings::SettingsState::new(),
        pending_images: Vec::new(),
        oauth_result_tx: oauth_tx,
        oauth_result_rx: oauth_rx,
        compact_result_tx: compact_tx,
        compact_result_rx: compact_rx,
        queued_message: None,
        queued_message_tx: None,
        cancel_tx: None,
        tool_views: HashMap::new(),
        tool_collapse_state: HashMap::new(),
        collapse_generation: 0,
        tool_header_regions: Vec::new(),
        tool_header_line_indices: Vec::new(),
        tool_header_wrapped_positions: Vec::new(),
        last_scroll: 0,
        last_inner_width: 0,
        last_chat_area_y: 0,
        last_chat_area_height: 0,
        chat_lines_cache: Vec::new(),
        chat_ranges_cache: Vec::new(),
        chat_cache_msg_count: 0,
        chat_cache_width: 0,
        chat_cache_collapse_gen: 0,
    };

    load_session_list(&mut app);

    loop {
        terminal.draw(|f| render::ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match app.input_mode {
                    InputMode::WaitingApproval => {
                        if let Some(pending) = app.pending_approval.as_ref() {
                            match key.code {
                                KeyCode::Left => app.approval_cursor = prev_approval_cursor(pending.kind, app.approval_cursor),
                                KeyCode::Right | KeyCode::Tab => app.approval_cursor = next_approval_cursor(pending.kind, app.approval_cursor),
                                KeyCode::Enter => {
                                    let response = build_approval_response(pending.kind, app.approval_cursor);
                                    let _ = app.approval_tx.send(response);
                                    app.pending_approval = None;
                                    app.input_mode = InputMode::Editing;
                                }
                                KeyCode::Esc => {
                                    let _ = app.approval_tx.send(ApprovalResponse::Deny);
                                    app.pending_approval = None;
                                    app.input_mode = InputMode::Editing;
                                }
                                _ => {}
                            }
                        }
                    }
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('i') | KeyCode::Char('e') => app.input_mode = InputMode::Editing,
                        KeyCode::Char('n') => {
                            app.session = Session::new(app.session.working_directory.clone());
                            app.messages.clear();
                            app.streaming_text.clear();
                            app.input.clear();
                            app.selecting_session = false;
                            reset_shell_console_state(&mut app);
                            app.tool_views.clear();
                            app.tool_collapse_state.clear();
                        }
                        KeyCode::Char('l') => {
                            load_session_list(&mut app);
                            app.selecting_session = !app.selecting_session;
                        }
                        KeyCode::Char('p') => {
                            app.config.plan_mode = !app.config.plan_mode;
                            let _ = freako_core::config::save_config(&app.config);
                        }
                        KeyCode::Char('o') => {
                            app.input_mode = InputMode::Settings;
                            app.settings_state = settings::SettingsState::new();
                        }
                        KeyCode::Char('s') => {
                            if let Some(cancel_tx) = &app.cancel_tx {
                                let _ = cancel_tx.send(());
                            }
                        }
                        KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            let original_len = app.session.messages.len();
                            if original_len >= 3 {
                                app.status_message = "Compacting context...".into();
                                app.is_thinking = true;
                                app.current_tool = Some("Compacting context...".to_string());
                                let messages = app.session.messages.clone();
                                let provider_config = app.config.provider.clone();
                                let mut forced = app.config.context.clone();
                                forced.enable_compaction = true;
                                forced.compact_after_messages = 0;
                                let tx = app.compact_result_tx.clone();
                                tokio::spawn(async move {
                                    let provider = match freako_core::provider::build_provider(&provider_config) {
                                        Ok(p) => p,
                                        Err(_) => return,
                                    };
                                    if let Ok(compacted) = freako_core::agent::context::llm_compact_messages(
                                        &messages,
                                        &forced,
                                        provider.as_ref(),
                                        &provider_config.model,
                                        provider_config.max_tokens,
                                    ).await {
                                        let _ = tx.send(compacted);
                                    }
                                });
                            }
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(cancel_tx) = &app.cancel_tx {
                                let _ = cancel_tx.send(());
                            } else {
                                break;
                            }
                        }
                        KeyCode::Up if app.selecting_session => {
                            if app.session_list_selected > 0 {
                                app.session_list_selected -= 1;
                            }
                        }
                        KeyCode::Down if app.selecting_session => {
                            if app.session_list_selected + 1 < app.session_list.len() {
                                app.session_list_selected += 1;
                            }
                        }
                        KeyCode::Enter if app.selecting_session => {
                            if let Some((id, _, _)) = app.session_list.get(app.session_list_selected).cloned() {
                                load_session(&mut app, &id);
                            }
                        }
                        KeyCode::Delete | KeyCode::Char('x') if app.selecting_session => {
                            if let Some((id, _, _)) = app.session_list.get(app.session_list_selected).cloned() {
                                // Don't delete the currently active session
                                if id != app.session.id.to_string() {
                                    if let Some(store) = &app.store {
                                        let _ = store.delete_session(&id);
                                    }
                                    load_session_list(&mut app);
                                }
                            }
                        }
                        KeyCode::Up if !app.selecting_session => app.scroll_offset = app.scroll_offset.saturating_add(1),
                        KeyCode::Down if !app.selecting_session => app.scroll_offset = app.scroll_offset.saturating_sub(1),
                        KeyCode::PageUp => app.scroll_offset = app.scroll_offset.saturating_add(10),
                        KeyCode::PageDown => app.scroll_offset = app.scroll_offset.saturating_sub(10),
                        KeyCode::Home => app.scroll_offset = u16::MAX,
                        KeyCode::End => app.scroll_offset = 0,
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Esc => app.input_mode = InputMode::Normal,
                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                            app.input.push('\n');
                            app.scroll_offset = 0;
                        }
                        KeyCode::Enter => {
                            let text = app.input.trim().to_string();
                            let has_images = !app.pending_images.is_empty();
                            if app.plan_pending_review {
                                app.plan_pending_review = false;
                                if text.is_empty() {
                                    app.config.plan_mode = false;
                                    let _ = freako_core::config::save_config(&app.config);
                                    app.input.clear();
                                    app.pending_images.clear();
                                    app.session.messages.push(ConversationMessage::user("Plan approved. Execute it."));
                                    save_session(&mut app);
                                    let cfg = app.config.clone();
                                    start_agent(&mut app, cfg);
                                } else {
                                    app.input.clear();
                                    app.pending_images.clear();
                                    app.session.messages.push(ConversationMessage::user(text));
                                    save_session(&mut app);
                                    let cfg = app.config.clone();
                                    start_agent(&mut app, cfg);
                                }
                            } else if !text.is_empty() || has_images {
                                app.input.clear();
                                // Build message with text + images
                                let mut parts = Vec::new();
                                if !text.is_empty() {
                                    parts.push(MessagePart::Text { text });
                                }
                                for (media_type, data) in app.pending_images.drain(..) {
                                    parts.push(MessagePart::Image { media_type, data });
                                }
                                app.session.messages.push(ConversationMessage {
                                    role: freako_core::session::types::Role::User,
                                    parts,
                                    timestamp: chrono::Utc::now(),
                                });
                                save_session(&mut app);
                                let cfg = app.config.clone();
                                start_agent(&mut app, cfg);
                            }
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                            app.scroll_offset = 0;
                        }
                        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+V: paste image from clipboard (requires keyboard enhancement)
                            if let Some((media_type, data)) = clipboard_image_base64() {
                                app.pending_images.push((media_type, data));
                            } else if let Some(text) = clipboard_text() {
                                app.input.push_str(&text);
                            }
                            app.scroll_offset = 0;
                        }
                        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::ALT) => {
                            // Alt+V: fallback image paste for terminals without keyboard enhancement
                            if let Some((media_type, data)) = clipboard_image_base64() {
                                app.pending_images.push((media_type, data));
                            }
                            app.scroll_offset = 0;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(cancel_tx) = &app.cancel_tx {
                                let _ = cancel_tx.send(());
                            } else {
                                break;
                            }
                        }
                        KeyCode::Char(ch) => {
                            app.input.push(ch);
                            app.scroll_offset = 0;
                        }
                        _ => {}
                    },
                    InputMode::Settings => {
                        if app.settings_state.editing {
                            match key.code {
                                KeyCode::Esc => app.settings_state.cancel_editing(),
                                KeyCode::Enter => {
                                    app.settings_state.apply_edit(&mut app.config);
                                    let _ = freako_core::config::save_config(&app.config);
                                }
                                KeyCode::Backspace => { app.settings_state.edit_buffer.pop(); }
                                KeyCode::Char(ch) => app.settings_state.edit_buffer.push(ch),
                                _ => {}
                            }
                        } else {
                            let field = app.settings_state.selected_field(&app.config);
                            let kind = field.map(|f| f.kind());
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('o') => {
                                    let _ = freako_core::config::save_config(&app.config);
                                    app.input_mode = InputMode::Normal;
                                }
                                KeyCode::Up | KeyCode::Char('k') => app.settings_state.move_up(&app.config),
                                KeyCode::Down | KeyCode::Char('j') => app.settings_state.move_down(&app.config),
                                // ←/→/Tab cycle Select fields
                                KeyCode::Right | KeyCode::Tab if kind == Some(settings::FieldKind::Select) => {
                                    app.settings_state.cycle_select(&mut app.config, 1);
                                    let _ = freako_core::config::save_config(&app.config);
                                }
                                KeyCode::Left if kind == Some(settings::FieldKind::Select) => {
                                    app.settings_state.cycle_select(&mut app.config, -1);
                                    let _ = freako_core::config::save_config(&app.config);
                                }
                                // Tab/Enter toggle Toggle fields
                                KeyCode::Tab | KeyCode::Enter if kind == Some(settings::FieldKind::Toggle) => {
                                    app.settings_state.toggle(&mut app.config);
                                    let _ = freako_core::config::save_config(&app.config);
                                }
                                // Enter on Action fields
                                KeyCode::Enter if kind == Some(settings::FieldKind::Action) => {
                                    if let Some(action) = app.settings_state.do_action(&app.config) {
                                        match action {
                                            settings::SettingsAction::OpenUrl(url) => {
                                                let _ = open::that(url);
                                            }
                                            settings::SettingsAction::StartOAuth => {
                                                use freako_core::provider::openai_oauth;
                                                let pkce = openai_oauth::generate_pkce();
                                                let url = openai_oauth::build_authorize_url(&pkce);
                                                let _ = open::that(&url);
                                                app.status_message = "Waiting for browser login...".into();
                                                // Spawn async task to wait for OAuth callback
                                                let verifier = pkce.code_verifier.clone();
                                                let oauth_tx = app.oauth_result_tx.clone();
                                                tokio::spawn(async move {
                                                    let result = async {
                                                        let code = openai_oauth::wait_for_callback()
                                                            .await
                                                            .map_err(|e| e.to_string())?;
                                                        openai_oauth::exchange_code(&code, &verifier)
                                                            .await
                                                            .map_err(|e| e.to_string())
                                                    }.await;
                                                    let _ = oauth_tx.send(result);
                                                });
                                            }
                                            settings::SettingsAction::OAuthLogout => {
                                                app.config.provider.openai_oauth = None;
                                                let _ = freako_core::config::save_config(&app.config);
                                            }
                                        }
                                    }
                                }
                                // Enter on text/secret fields starts editing
                                KeyCode::Enter => {
                                    app.settings_state.start_editing(&app.config);
                                }
                                _ => {}
                            }
                        }
                    }
                },
                Event::Mouse(mouse) => {
                    update_shell_hover(&mut app, mouse.column, mouse.row);
                    match mouse.kind {
                        MouseEventKind::Moved => {}
                        MouseEventKind::Down(MouseButton::Left) => {
                            if let Some(tool_call_id) = find_clicked_tool_header(&app, mouse.row) {
                                let was_collapsed = app.tool_collapse_state.get(&tool_call_id).copied().unwrap_or(true);
                                // Measure body lines to adjust scroll
                                let body_visual_lines = if let Some(view) = app.tool_views.get(&tool_call_id) {
                                    use ratatui::widgets::{Paragraph, Wrap};
                                    // Body is rendered at bubble content width (width - 2 for padding)
                                    let bubble_width = app.last_inner_width.saturating_sub(2);
                                    let body = view.render_body(bubble_width);
                                    let p = Paragraph::new(body).wrap(Wrap { trim: false });
                                    p.line_count(bubble_width)
                                } else {
                                    0
                                };
                                // Toggle
                                let entry = app.tool_collapse_state.entry(tool_call_id).or_insert(true);
                                *entry = !*entry;
                                app.collapse_generation += 1;
                                // Adjust scroll to keep the header at the same screen position
                                if was_collapsed {
                                    // Uncollapsing: body lines added below header → push content down
                                    app.scroll_offset = app.scroll_offset.saturating_add(body_visual_lines as u16);
                                } else {
                                    // Collapsing: body lines removed → pull content up
                                    app.scroll_offset = app.scroll_offset.saturating_sub(body_visual_lines as u16);
                                }
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            // Try scrolling a hovered tool view first, fall back to chat scroll
                            let mut scrolled = false;
                            if let Some(hovered_id) = find_hovered_scrollable_tool(&app, mouse.column, mouse.row) {
                                if let Some(view) = app.tool_views.get_mut(&hovered_id) {
                                    if view.is_scrollable() {
                                        view.scroll(SHELL_CONSOLE_SCROLL_STEP as isize);
                                        app.collapse_generation += 1;
                                        scrolled = true;
                                    }
                                }
                            }
                            if !scrolled {
                                if !scroll_hovered_shell(&mut app, SHELL_CONSOLE_SCROLL_STEP as isize) {
                                    app.scroll_offset = app.scroll_offset.saturating_add(3);
                                }
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            let mut scrolled = false;
                            if let Some(hovered_id) = find_hovered_scrollable_tool(&app, mouse.column, mouse.row) {
                                if let Some(view) = app.tool_views.get_mut(&hovered_id) {
                                    if view.is_scrollable() {
                                        view.scroll(-(SHELL_CONSOLE_SCROLL_STEP as isize));
                                        app.collapse_generation += 1;
                                        scrolled = true;
                                    }
                                }
                            }
                            if !scrolled {
                                if !scroll_hovered_shell(&mut app, -(SHELL_CONSOLE_SCROLL_STEP as isize)) {
                                    app.scroll_offset = app.scroll_offset.saturating_sub(3);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::Paste(text) => {
                    // Bracketed paste: terminal sent pasted text
                    if matches!(app.input_mode, InputMode::Editing) {
                        // Check clipboard for image first
                        if let Some((media_type, data)) = clipboard_image_base64() {
                            app.pending_images.push((media_type, data));
                        } else {
                            app.input.push_str(&text);
                        }
                        app.scroll_offset = 0;
                    }
                }
                _ => {}
            }
        }

        app.spinner_tick = app.spinner_tick.wrapping_add(1) % 8;

        // Poll for compaction results
        if let Ok(compacted) = app.compact_result_rx.try_recv() {
            let original_len = app.session.messages.len();
            if compacted.len() < original_len {
                app.session.messages = compacted;
                app.status_message = format!("Compacted {} → {} messages", original_len, app.session.messages.len());
                save_session(&mut app);
            } else {
                app.status_message = "Nothing to compact".into();
            }
            app.is_thinking = false;
            app.current_tool = None;
        }

        // Poll for OAuth results
        if let Ok(result) = app.oauth_result_rx.try_recv() {
            match result {
                Ok(creds) => {
                    app.config.provider.openai_oauth = Some(creds);
                    let _ = freako_core::config::save_config(&app.config);
                    app.status_message = "ChatGPT login successful!".into();
                }
                Err(e) => {
                    app.status_message = format!("OAuth error: {}", e);
                }
            }
        }

        while let Ok(event) = app.event_rx.try_recv() {
            match event {
                AgentEvent::Thinking => {
                    app.is_thinking = true;
                    app.status_message = "Thinking…".into();
                }
                AgentEvent::Compacting => {
                    app.is_thinking = true;
                    app.current_tool = Some("Compacting context...".to_string());
                    app.status_message = "Compacting context…".into();
                }
                AgentEvent::StreamDelta(text) => {
                    app.is_thinking = false;
                    app.streaming_text.push_str(&text);
                    app.scroll_offset = 0;
                }
                AgentEvent::ToolCallRequested { id, name, arguments } => {
                    app.streaming_tool_calls.push((id, name, arguments));
                }
                AgentEvent::ToolApprovalNeeded { name, arguments, .. } => {
                    let kind = if matches!(name.as_str(), "write_file" | "edit_file") { ApprovalKind::File } else { ApprovalKind::Shell };
                    let args_json = serde_json::to_string_pretty(&arguments).unwrap_or_else(|_| arguments.to_string());
                    app.pending_approval = Some(PendingApproval { tool_name: name, args_json, kind });
                    app.approval_cursor = 0;
                    app.input_mode = InputMode::WaitingApproval;
                }
                AgentEvent::ToolExecutionStarted { tool_call_id, name } => {
                    app.is_thinking = false;
                    app.current_tool = Some(name.clone());
                    app.tool_output_buffer.clear();
                    app.status_message = format!("Running: {}", name);

                    // Create tool view if not already present
                    if !app.tool_views.contains_key(&tool_call_id) {
                        let args = app.streaming_tool_calls.iter()
                            .find(|(id, _, _)| id == &tool_call_id)
                            .map(|(_, _, a)| a.clone())
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        if let Some(tool_call) = freako_core::tools::tool_name::ToolCall::from_raw(&name, &args) {
                            let view = tools::create_tool_view(tool_call_id.clone(), tool_call);
                            if !app.tool_collapse_state.contains_key(&tool_call_id) {
                                let default = view.default_collapsed();
                                app.tool_collapse_state.insert(tool_call_id.clone(), default);
                            }
                            app.tool_views.insert(tool_call_id.clone(), view);
                        }
                    }

                    if name == "shell" {
                        app.active_shell_tool_call_id = Some(tool_call_id.clone());
                        app.live_shell_outputs.entry(tool_call_id.clone()).or_default();
                        if !app.shell_tool_order.iter().any(|id| id == &tool_call_id) {
                            app.shell_tool_order.push_back(tool_call_id.clone());
                        }
                        app.shell_viewport.tool_call_id = Some(tool_call_id);
                        app.shell_viewport.scroll_lines_from_bottom = 0;
                    }
                }
                AgentEvent::ToolOutputDelta { tool_call_id, stream, output } => {
                    app.tool_output_buffer.push_str(&output);
                    // Forward to tool view (for use after completion)
                    if let Some(view) = app.tool_views.get_mut(&tool_call_id) {
                        view.push_stream_delta(stream, &output);
                        // Don't bump collapse_generation here — streaming output
                        // is rendered via tail_lines, not the cache.
                    }
                    app.live_shell_outputs.entry(tool_call_id.clone()).or_default().push(stream, output);
                    if app.shell_viewport.tool_call_id.as_deref() == Some(tool_call_id.as_str())
                        && app.shell_viewport.scroll_lines_from_bottom == 0
                    {
                        app.shell_viewport.scroll_lines_from_bottom = 0;
                    }
                    app.scroll_offset = 0;
                }
                AgentEvent::ToolResult { tool_call_id, name, content, is_error, arguments } => {
                    app.current_tool = None;
                    app.tool_output_buffer.clear();
                    // Create tool view if not already present (fallback for when ToolExecutionStarted didn't create one)
                    if !app.tool_views.contains_key(&tool_call_id) {
                        let args = arguments.clone()
                            .or_else(|| app.streaming_tool_calls.iter()
                                .find(|(id, _, _)| id == &tool_call_id)
                                .map(|(_, _, a)| a.clone()))
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        if let Some(tool_call) = freako_core::tools::tool_name::ToolCall::from_raw(&name, &args) {
                            let view = tools::create_tool_view(tool_call_id.clone(), tool_call);
                            if !app.tool_collapse_state.contains_key(&tool_call_id) {
                                let default = view.default_collapsed();
                                app.tool_collapse_state.insert(tool_call_id.clone(), default);
                            }
                            app.tool_views.insert(tool_call_id.clone(), view);
                        }
                    }
                    // Forward result to tool view
                    if let Some(view) = app.tool_views.get_mut(&tool_call_id) {
                        view.set_result(&content, is_error);
                    }
                    app.collapse_generation += 1;
                    if name == "shell" {
                        let live = app.live_shell_outputs.remove(&tool_call_id);
                        let plain = live.as_ref()
                            .map(|output| output.plain_text())
                            .unwrap_or_else(|| content.clone());
                        let chunks = live.map(|o| o.chunks).unwrap_or_default();
                        let compacted = message::compact_shell_output(&plain, SHELL_COMPACT_HEAD_LINES, SHELL_COMPACT_TAIL_LINES);
                        app.completed_shell_outputs.insert(
                            tool_call_id.clone(),
                            CompletedShellOutput {
                                plain: plain.clone(),
                                compacted: compacted.clone(),
                                chunks,
                            },
                        );
                        if app.active_shell_tool_call_id.as_deref() == Some(tool_call_id.as_str()) {
                            app.active_shell_tool_call_id = None;
                        }
                        if app.shell_viewport.tool_call_id.is_none() {
                            app.shell_viewport.tool_call_id = Some(tool_call_id.clone());
                        }
                    }
                    if let Some(message) = app.session.messages.iter_mut().rev().find(|m| {
                        m.parts.iter().any(|p| matches!(p, MessagePart::ToolCall { id, .. } if id == &tool_call_id))
                    }) {
                        if let Some(completed) = app.completed_shell_outputs.get(&tool_call_id) {
                            message.parts.push(MessagePart::ToolOutput {
                                tool_call_id: tool_call_id.clone(),
                                stream: ToolOutputStream::Stdout,
                                content: completed.plain.clone(),
                            });
                        }
                        message.parts.push(MessagePart::ToolResult {
                            tool_call_id: tool_call_id.clone(),
                            name: name.clone(),
                            content: if let Some(completed) = app.completed_shell_outputs.get(&tool_call_id) {
                                completed.compacted.clone()
                            } else {
                                content.clone()
                            },
                            is_error,
                            arguments: arguments.clone(),
                        });
                    }
                    sync_shell_viewport_target(&mut app);
                    save_session(&mut app);
                    app.scroll_offset = 0;
                }
                AgentEvent::ResponseComplete { usage, .. } => {
                    flush_streaming_assistant_parts(&mut app);
                    app.session.total_input_tokens += usage.input_tokens;
                    app.session.total_output_tokens += usage.output_tokens;
                    save_session(&mut app);
                    app.scroll_offset = 0;
                }
                AgentEvent::Done => {
                    flush_streaming_assistant_parts(&mut app);
                    app.is_working = false;
                    app.is_thinking = false;
                    app.current_tool = None;
                    app.status_message = "Done".into();
                    save_session(&mut app);
                    app.cancel_tx = None;
                    app.queued_message_tx = None;
                    app.scroll_offset = 0;
                }
                AgentEvent::Cancelled => {
                    app.is_working = false;
                    app.is_thinking = false;
                    app.current_tool = None;
                    app.streaming_text.clear();
                    app.streaming_tool_calls.clear();
                    app.status_message = "Cancelled".into();
                    save_session(&mut app);
                    app.cancel_tx = None;
                    app.queued_message_tx = None;
                }
                AgentEvent::Error(err) => {
                    app.messages.push(format!("Error: {}", err));
                    app.is_working = false;
                    app.is_thinking = false;
                    app.current_tool = None;
                    app.status_message = format!("Error: {}", err);
                    app.cancel_tx = None;
                    app.queued_message_tx = None;
                }
                AgentEvent::RetryScheduled { error, attempt, max_attempts, .. } => {
                    app.status_message = format!("Retry {}/{}: {}", attempt, max_attempts, error);
                }
                AgentEvent::EnteredPlanMode => app.config.plan_mode = true,
                AgentEvent::PlanUpdated { content } => {
                    app.current_plan_text = Some(content);
                }
                AgentEvent::PlanReadyForReview { content } => {
                    app.current_plan_text = Some(content);
                    app.plan_pending_review = true;
                    app.status_message = "Plan ready — Enter to approve".into();
                }
                AgentEvent::QueuedMessageInjected => {}
            }
        }
    }

    disable_raw_mode()?;
    if keyboard_enhanced {
        let _ = terminal.backend_mut().execute(PopKeyboardEnhancementFlags);
    }
    terminal.backend_mut().execute(DisableBracketedPaste)?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.backend_mut().execute(DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
