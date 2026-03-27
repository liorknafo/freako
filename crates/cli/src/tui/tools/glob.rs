use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::{MUTED_STYLE, ERROR_STYLE};

const FILE_COLOR: Color = Color::Indexed(75);

pub struct GlobView {
    info: ToolResultInfo,
}

impl GlobView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self { info }
    }
}

impl ToolView for GlobView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { true }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.info.content = content.to_string();
        self.info.is_error = is_error;
    }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        if self.info.content.is_empty() {
            return vec![Line::styled("      <empty>", MUTED_STYLE)];
        }

        if self.info.is_error {
            return self.info.content
                .lines()
                .map(|l| Line::styled(format!("      {}", l), ERROR_STYLE))
                .collect();
        }

        // Check for "No files matching" message
        if self.info.content.starts_with("No files matching") || self.info.content.starts_with("No files found") {
            return vec![Line::styled(
                format!("      {}", self.info.content.lines().next().unwrap_or("")),
                MUTED_STYLE,
            )];
        }

        // Parse "Found N file(s):\npath1\npath2\n..." format
        let mut lines = Vec::new();
        let mut line_iter = self.info.content.lines();

        // Skip the "Found N file(s):" header line; if first line isn't a header, show it
        if let Some(first) = line_iter.next() && !first.starts_with("Found ") {
            lines.push(Line::styled(
                format!("      {}", first),
                Style::default().fg(FILE_COLOR),
            ));
        }

        for path in line_iter {
            if path.trim().is_empty() {
                continue;
            }
            lines.push(Line::styled(
                format!("      {}", path),
                Style::default().fg(FILE_COLOR),
            ));
        }

        if lines.is_empty() {
            return vec![Line::styled("      No files matching", MUTED_STYLE)];
        }

        lines
    }
}
