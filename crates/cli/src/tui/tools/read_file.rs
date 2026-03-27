use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::{CODE_STYLE, MUTED_STYLE, ERROR_STYLE};
use crate::tui::message::{highlighted_spans, syntax_for_path};

pub struct ReadFileView {
    info: ToolResultInfo,
    cached_lines: Vec<Line<'static>>,
}

impl ReadFileView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self {
            info,
            cached_lines: Vec::new(),
        }
    }
}

impl ToolView for ReadFileView {
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

        // Parse format: "File: path (N lines total)\n    1 | line1\n    2 | line2\n..."
        let mut lines_iter = content.lines();
        let header = lines_iter.next().unwrap_or("");

        // Extract file path from "File: path (N lines total)"
        let file_path = header
            .strip_prefix("File: ")
            .and_then(|rest| rest.rfind(" (").map(|idx| &rest[..idx]))
            .unwrap_or("");

        // Force syntect to load syntax for this path (unused binding but triggers cache)
        let _ = syntax_for_path(file_path);

        for raw_line in lines_iter {
            // Parse "    N | content" format
            if let Some(pipe_pos) = raw_line.find(" | ") {
                let num_part = raw_line[..pipe_pos].trim();
                let code_part = &raw_line[pipe_pos + 3..];

                let line_num = format!("{:>4} ", num_part);
                let mut spans = vec![Span::styled(line_num, MUTED_STYLE)];
                spans.extend(highlighted_spans(code_part, file_path, CODE_STYLE));

                self.cached_lines.push(Line::from(spans));
            } else {
                // Fallback for lines that don't match the pattern
                self.cached_lines.push(Line::styled(
                    format!("      {}", raw_line),
                    CODE_STYLE,
                ));
            }
        }
    }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        if self.cached_lines.is_empty() {
            return vec![Line::styled("      <empty>", MUTED_STYLE)];
        }
        self.cached_lines.clone()
    }
}
