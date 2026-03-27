use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::{MUTED_STYLE, ERROR_STYLE};

const FILE_COLOR: Color = Color::Indexed(75);
const LINENO_COLOR: Color = Color::Indexed(221);
const CONTENT_COLOR: Color = Color::Indexed(252);

pub struct GrepView {
    info: ToolResultInfo,
    cached_lines: Vec<Line<'static>>,
}

impl GrepView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self {
            info,
            cached_lines: Vec::new(),
        }
    }
}

impl ToolView for GrepView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { true }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.info.content = content.to_string();
        self.info.is_error = is_error;

        self.cached_lines.clear();

        if is_error {
            for l in content.lines() {
                self.cached_lines.push(Line::styled(format!("      {}", l), ERROR_STYLE));
            }
            return;
        }

        if content.trim().is_empty() || content.contains("No matches") {
            self.cached_lines.push(Line::styled("      No matches", MUTED_STYLE));
            return;
        }

        // Parse "filepath:lineno: content" lines
        for raw_line in content.lines() {
            // Try to parse filepath:lineno: content
            // Find the first colon after a path (could contain drive letter on Windows)
            if let Some(line_spans) = parse_grep_line(raw_line) {
                self.cached_lines.push(Line::from(line_spans));
            } else {
                // Fallback: render as plain text
                self.cached_lines.push(Line::styled(
                    format!("      {}", raw_line),
                    Style::default().fg(CONTENT_COLOR),
                ));
            }
        }
    }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        if self.cached_lines.is_empty() {
            return vec![Line::styled("      No matches", MUTED_STYLE)];
        }
        self.cached_lines.clone()
    }
}

fn parse_grep_line(line: &str) -> Option<Vec<Span<'static>>> {
    // Format: "filepath:lineno: content"
    // Handle Windows paths like C:\foo\bar:10: content
    // Strategy: find ":digits:" pattern
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            // Check if digits follow
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start && end < bytes.len() && bytes[end] == b':' {
                let filepath = &line[..i];
                let lineno = &line[start..end];
                let content = if end + 1 < line.len() {
                    &line[end + 1..]
                } else {
                    ""
                };
                let content = content.strip_prefix(' ').unwrap_or(content);
                return Some(vec![
                    Span::raw("      "),
                    Span::styled(filepath.to_string(), Style::default().fg(FILE_COLOR)),
                    Span::styled(format!(":{}:", lineno), Style::default().fg(LINENO_COLOR)),
                    Span::styled(format!(" {}", content), Style::default().fg(CONTENT_COLOR)),
                ]);
            }
        }
        i += 1;
    }
    None
}
