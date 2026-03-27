pub mod default;
pub mod shell;
pub mod edit_file;
pub mod read_file;
pub mod grep;
pub mod glob;
pub mod list_dir;
pub mod web;

use ratatui::prelude::*;
use freako_core::agent::events::ToolOutputStream;
use freako_core::tools::tool_name::{ToolCall, ToolPresentation};

use super::{TOOL_STYLE, SUCCESS_STYLE, ERROR_STYLE, MUTED_STYLE};

/// Common info every tool view has.
pub struct ToolResultInfo {
    pub tool_call_id: String,
    pub tool_call: ToolCall,
    pub content: String,
    pub is_error: bool,
}

impl ToolResultInfo {
    pub fn name(&self) -> &'static str {
        self.tool_call.display_name()
    }

    pub fn summary(&self) -> String {
        ToolPresentation::summary(&self.tool_call)
    }
}

/// The trait each tool renderer implements.
pub trait ToolView {
    /// Access the common info.
    fn info(&self) -> &ToolResultInfo;

    /// Default collapsed state for this tool type.
    fn default_collapsed(&self) -> bool;

    /// Render the body (shown when uncollapsed). Must be cheap — use pre-computed data.
    fn render_body(&self, width: u16) -> Vec<Line<'static>>;

    /// Whether this tool's body is scrollable (mouse wheel captures scroll).
    fn is_scrollable(&self) -> bool { false }

    /// Scroll the body (delta > 0 = scroll up/back, delta < 0 = scroll down/forward).
    fn scroll(&mut self, _delta: isize) {}

    /// Push a streaming delta (e.g. shell stdout/stderr chunk).
    fn push_stream_delta(&mut self, _stream: ToolOutputStream, _text: &str) {}

    /// Mark execution as complete with final content.
    /// Do expensive work here (syntax highlighting, diff parsing) — called once.
    fn set_result(&mut self, _content: &str, _is_error: bool) {}

    /// Whether this tool is still running.
    fn is_running(&self) -> bool { false }
}

/// Create the appropriate tool view for a typed ToolCall.
pub fn create_tool_view(tool_call_id: String, tool_call: ToolCall) -> Box<dyn ToolView> {
    let info = ToolResultInfo {
        tool_call_id,
        tool_call,
        content: String::new(),
        is_error: false,
    };
    match &info.tool_call {
        ToolCall::Shell { .. } => Box::new(shell::ShellView::new(info)),
        ToolCall::EditFile { .. } => Box::new(edit_file::EditFileView::new(info)),
        ToolCall::ReadFile { .. } => Box::new(read_file::ReadFileView::new(info)),
        ToolCall::Grep { .. } => Box::new(grep::GrepView::new(info)),
        ToolCall::Glob { .. } => Box::new(glob::GlobView::new(info)),
        ToolCall::ListDir { .. } => Box::new(list_dir::ListDirView::new(info)),
        ToolCall::WebSearch { .. } | ToolCall::WebFetch { .. } => Box::new(web::WebView::new(info)),
        ToolCall::WriteFile { .. }
        | ToolCall::ListMemories { .. }
        | ToolCall::ReadMemory { .. }
        | ToolCall::WriteMemory { .. }
        | ToolCall::DeleteMemory { .. }
        | ToolCall::ListSkills
        | ToolCall::EnterPlanMode
        | ToolCall::EditPlan { .. }
        | ToolCall::ReadPlan
        | ToolCall::ReviewPlan => Box::new(default::DefaultView::new(info)),
    }
}

/// Render the shared tool header line (icon + name + summary + status).
pub fn render_tool_header(view: &dyn ToolView, collapsed: bool) -> Line<'static> {
    let info = view.info();
    let summary = info.summary();

    let (status_text, status_style) = if view.is_running() {
        ("running…", TOOL_STYLE)
    } else if info.is_error {
        ("✗ error", ERROR_STYLE)
    } else if info.content.is_empty() {
        ("… pending", MUTED_STYLE)
    } else {
        ("✓ done", SUCCESS_STYLE)
    };

    let collapse_icon = if collapsed { "▶" } else { "▼" };

    Line::from(vec![
        Span::styled(format!("  {} ", collapse_icon), MUTED_STYLE),
        Span::styled(info.name().to_string(), TOOL_STYLE.add_modifier(Modifier::BOLD)),
        Span::styled(format!(" – {}", summary), TOOL_STYLE),
        Span::styled(format!(" – {}", status_text), status_style),
    ])
}

/// Render a complete tool: header + body (if uncollapsed).
pub fn render_tool(
    view: &dyn ToolView,
    collapsed: bool,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(render_tool_header(view, collapsed));
    if !collapsed {
        lines.extend(view.render_body(width));
    }
    lines
}
