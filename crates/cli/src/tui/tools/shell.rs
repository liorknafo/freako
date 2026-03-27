use ratatui::prelude::*;

use freako_core::agent::events::ToolOutputStream;

use super::{ToolResultInfo, ToolView};
use super::super::message::{ansi_line_spans, truncate_middle};
use crate::tui::{
    MUTED_STYLE, SHELL_CONSOLE_BG, SHELL_CONSOLE_BORDER, SHELL_CONSOLE_FG,
    SHELL_CONSOLE_VISIBLE_LINES, THINKING_STYLE,
};

/// TUI tool view for shell command output with a scrollable black-background console.
pub struct ShellView {
    info: ToolResultInfo,
    chunks: Vec<(ToolOutputStream, String)>,
    running: bool,
    scroll_offset: usize,
}

impl ShellView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self {
            info,
            chunks: Vec::new(),
            running: true,
            scroll_offset: 0,
        }
    }

    /// Collect all chunks into ANSI-parsed lines.
    fn collect_lines(&self) -> Vec<Line<'static>> {
        let fallback = Style::new().fg(SHELL_CONSOLE_FG).bg(SHELL_CONSOLE_BG);
        let mut rendered = Vec::new();
        for (stream, text) in &self.chunks {
            rendered.extend(ansi_line_spans(text, fallback, Some(*stream)));
        }
        if rendered.is_empty() {
            rendered.push(Line::styled(
                "<no output yet>",
                MUTED_STYLE.bg(SHELL_CONSOLE_BG),
            ));
        }
        rendered
    }
}

impl ToolView for ShellView {
    fn info(&self) -> &ToolResultInfo {
        &self.info
    }

    fn default_collapsed(&self) -> bool {
        false
    }

    fn is_scrollable(&self) -> bool {
        true
    }

    fn scroll(&mut self, delta: isize) {
        // delta > 0 means scroll up (increase offset from bottom),
        // delta < 0 means scroll down (decrease offset from bottom).
        let lines = self.collect_lines();
        let total = lines.len();
        let max_offset = total.saturating_sub(1);
        let new = self.scroll_offset as isize + delta;
        self.scroll_offset = (new.max(0) as usize).min(max_offset);
    }

    fn push_stream_delta(&mut self, stream: ToolOutputStream, text: &str) {
        self.chunks.push((stream, text.to_string()));
    }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.running = false;
        self.info.content = content.to_string();
        self.info.is_error = is_error;
        // If no streaming chunks were received (e.g., loaded from session),
        // populate from the final content so render_body has something to show.
        if self.chunks.is_empty() && !content.is_empty() {
            self.chunks.push((ToolOutputStream::Stdout, content.to_string()));
        }
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        let all_lines = self.collect_lines();
        let total_lines = all_lines.len();
        let from_bottom = self.scroll_offset.min(total_lines.saturating_sub(1));
        let window_size = SHELL_CONSOLE_VISIBLE_LINES.min(total_lines.max(1));
        let end = total_lines.saturating_sub(from_bottom);
        let start = end.saturating_sub(window_size);

        let command = self.info.summary();
        let header = truncate_middle(&command, 72);
        let status = if self.running { "running" } else { "done" };

        let border_style = Style::new().fg(SHELL_CONSOLE_BORDER).bg(SHELL_CONSOLE_BG);
        let header_text_style = Style::new()
            .fg(SHELL_CONSOLE_FG)
            .bg(SHELL_CONSOLE_BG)
            .add_modifier(Modifier::BOLD);
        let status_style = if self.running {
            THINKING_STYLE.bg(SHELL_CONSOLE_BG)
        } else {
            MUTED_STYLE.bg(SHELL_CONSOLE_BG)
        };

        let mut rendered = Vec::new();

        // Header line: ┌─ command [status]
        rendered.push(Line::from(vec![
            Span::styled("      \u{250c}\u{2500} ", border_style),
            Span::styled(header, header_text_style),
            Span::styled(format!(" [{}]", status), status_style),
        ]));

        // Visible output lines
        for line in all_lines.into_iter().skip(start).take(end.saturating_sub(start)) {
            let mut spans = vec![Span::styled("      \u{2502} ", border_style)];
            spans.extend(line.spans.into_iter().map(|span| {
                let s = span.style;
                Span::styled(
                    span.content.into_owned(),
                    s.fg(s.fg.unwrap_or(SHELL_CONSOLE_FG))
                        .bg(s.bg.unwrap_or(SHELL_CONSOLE_BG)),
                )
            }));
            rendered.push(Line::from(spans));
        }

        // Footer line: └─ hint
        let hint = if total_lines <= window_size {
            format!(
                "{} line{}",
                total_lines,
                if total_lines == 1 { "" } else { "s" }
            )
        } else if from_bottom == 0 {
            format!("tailing latest {} / {} lines", window_size, total_lines)
        } else {
            format!("showing older output (+{} from bottom)", from_bottom)
        };
        rendered.push(Line::from(vec![
            Span::styled("      \u{2514}\u{2500} ", border_style),
            Span::styled(hint, MUTED_STYLE.bg(SHELL_CONSOLE_BG)),
        ]));

        rendered
    }
}
