use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::{MUTED_STYLE, ERROR_STYLE};

const URL_COLOR: Color = Color::Indexed(75);
const SNIPPET_COLOR: Color = Color::Indexed(243);

pub struct WebView {
    info: ToolResultInfo,
}

impl WebView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self { info }
    }
}

impl ToolView for WebView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { true }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.info.content = content.to_string();
        self.info.is_error = is_error;
    }

    fn render_body(&self, width: u16) -> Vec<Line<'static>> {
        if self.info.content.is_empty() {
            return vec![Line::styled("      <empty>", MUTED_STYLE)];
        }

        if self.info.is_error {
            return self.info.content
                .lines()
                .map(|l| Line::styled(format!("      {}", l), ERROR_STYLE))
                .collect();
        }

        match self.info.name() {
            "web_search" => self.render_search_results(),
            "web_fetch" => self.render_fetch_result(width),
            _ => self.render_fallback(),
        }
    }
}

impl WebView {
    fn render_search_results(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut current_block: Vec<&str> = Vec::new();

        // Split content into result blocks separated by blank lines
        for raw_line in self.info.content.lines() {
            if raw_line.trim().is_empty() {
                if !current_block.is_empty() {
                    lines.extend(render_search_block(&current_block));
                    lines.push(Line::raw(""));
                    current_block.clear();
                }
            } else {
                current_block.push(raw_line);
            }
        }
        // Last block
        if !current_block.is_empty() {
            lines.extend(render_search_block(&current_block));
        }

        if lines.is_empty() {
            return vec![Line::styled("      No results", MUTED_STYLE)];
        }

        lines
    }

    fn render_fetch_result(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let mut body_start = None;

        // Render metadata header (Title, URL, Content-Type)
        for (i, raw_line) in self.info.content.lines().enumerate() {
            if raw_line.trim().is_empty() {
                body_start = Some(i + 1);
                lines.push(Line::raw(""));
                break;
            }

            if let Some(colon_pos) = raw_line.find(": ") {
                let label = &raw_line[..colon_pos + 1];
                let value = &raw_line[colon_pos + 2..];
                let value_style = if label.starts_with("URL") {
                    Style::default().fg(URL_COLOR)
                } else {
                    Style::default()
                };
                lines.push(Line::from(vec![
                    Span::raw("      "),
                    Span::styled(
                        label.to_string(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!(" {}", value), value_style),
                ]));
            } else {
                lines.push(Line::styled(format!("      {}", raw_line), Style::default()));
            }
        }

        // Render body as markdown
        if let Some(start) = body_start {
            let body: String = self.info.content
                .lines()
                .skip(start)
                .collect::<Vec<_>>()
                .join("\n");
            if !body.trim().is_empty() {
                let md_width = width.saturating_sub(8); // account for indentation
                for line in super::super::message::markdown_to_lines(&body, md_width) {
                    let mut indented: Vec<Span<'static>> = vec![Span::raw("      ")];
                    indented.extend(line.spans);
                    lines.push(Line::from(indented));
                }
            }
        }

        if lines.is_empty() {
            return vec![Line::styled("      <empty>", MUTED_STYLE)];
        }

        lines
    }

    fn render_fallback(&self) -> Vec<Line<'static>> {
        self.info.content
            .lines()
            .map(|l| Line::styled(format!("      {}", l), Style::default()))
            .collect()
    }
}

/// Render a single search result block (group of non-empty lines).
/// Expected format:
///   "N. Title"
///   "URL: url"
///   "Published: date"  (optional)
///   "Snippet: text"
fn render_search_block(block: &[&str]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for raw_line in block {
        let line = *raw_line;

        // "N. Title" — starts with a digit
        if line.chars().next().is_some_and(|c| c.is_ascii_digit()) && line.contains(". ") {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(
                    line.to_string(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]));
        } else if let Some(url) = line.strip_prefix("URL: ") {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled("URL: ", Style::default()),
                Span::styled(url.to_string(), Style::default().fg(URL_COLOR)),
            ]));
        } else if let Some(snippet) = line.strip_prefix("Snippet: ") {
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled("Snippet: ", Style::default()),
                Span::styled(snippet.to_string(), Style::default().fg(SNIPPET_COLOR)),
            ]));
        } else if line.starts_with("Published: ") {
            lines.push(Line::styled(
                format!("      {}", line),
                Style::default().fg(SNIPPET_COLOR),
            ));
        } else {
            lines.push(Line::styled(format!("      {}", line), Style::default()));
        }
    }

    lines
}
