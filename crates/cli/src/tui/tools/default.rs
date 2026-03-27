use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::{TOOL_STYLE, ERROR_STYLE};

/// Fallback view for tools without specialized rendering.
/// Used for: write_file, delete_memory, write_memory, list_memories,
/// read_memory, list_skills, plan tools, and unknown tools.
pub struct DefaultView {
    info: ToolResultInfo,
}

impl DefaultView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self { info }
    }
}

impl ToolView for DefaultView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { true }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        if self.info.content.is_empty() {
            return vec![Line::styled("      <empty>", TOOL_STYLE)];
        }
        let style = if self.info.is_error { ERROR_STYLE } else { TOOL_STYLE };
        self.info.content
            .lines()
            .map(|l| Line::styled(format!("      {}", l), style))
            .collect()
    }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.info.content = content.to_string();
        self.info.is_error = is_error;
    }
}
