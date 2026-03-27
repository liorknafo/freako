use ratatui::prelude::*;

use super::{ToolResultInfo, ToolView};
use crate::tui::CODE_STYLE;
use super::super::message::highlighted_spans;

// ── Diff style constants ────────────────────────────────────────
const DIFF_ADD_STYLE: Style = Style::new().fg(Color::Rgb(180, 255, 180)).bg(Color::Rgb(20, 45, 20));
const DIFF_REMOVE_STYLE: Style = Style::new().fg(Color::Rgb(255, 180, 180)).bg(Color::Rgb(55, 18, 18));
const DIFF_HUNK_STYLE: Style = Style::new().fg(Color::Indexed(141)).add_modifier(Modifier::BOLD);
const DIFF_HEADER_STYLE: Style = Style::new().fg(Color::Indexed(146)).add_modifier(Modifier::BOLD);
const DIFF_FILENAME_STYLE: Style = Style::new().fg(Color::Indexed(110)).add_modifier(Modifier::BOLD);
const DIFF_PLAIN_STYLE: Style = Style::new().fg(Color::Indexed(250));
const DIFF_MODIFIED_STYLE: Style = Style::new().fg(Color::Rgb(180, 180, 205)).bg(Color::Rgb(30, 30, 40));
const INLINE_DIFF_ADD_STYLE: Style = Style::new().fg(Color::Rgb(210, 255, 210)).bg(Color::Rgb(28, 90, 36)).add_modifier(Modifier::BOLD);
const INLINE_DIFF_REMOVE_STYLE: Style = Style::new().fg(Color::Rgb(255, 210, 210)).bg(Color::Rgb(105, 28, 28));

// ── Inline diff types ───────────────────────────────────────────

#[derive(Clone, Copy)]
enum InlineDiffKind {
    Unchanged,
    Added,
    Removed,
}

struct InlineDiffSegment {
    kind: InlineDiffKind,
    text: String,
}

// ── EditFileView ────────────────────────────────────────────────

pub struct EditFileView {
    info: ToolResultInfo,
    cached_lines: Vec<Line<'static>>,
}

impl EditFileView {
    pub fn new(info: ToolResultInfo) -> Self {
        Self {
            info,
            cached_lines: Vec::new(),
        }
    }
}

impl ToolView for EditFileView {
    fn info(&self) -> &ToolResultInfo { &self.info }

    fn default_collapsed(&self) -> bool { false }

    fn render_body(&self, _width: u16) -> Vec<Line<'static>> {
        self.cached_lines.clone()
    }

    fn set_result(&mut self, content: &str, is_error: bool) {
        self.info.content = content.to_string();
        self.info.is_error = is_error;
        self.cached_lines = parse_diff_content(content);
    }
}

// ── Diff parsing and rendering ──────────────────────────────────

fn parse_diff_content(content: &str) -> Vec<Line<'static>> {
    let path = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("Edited "))
        .unwrap_or("diff.rs");

    // Rejoin lines where inline markers got split across multiple lines
    let rejoined = rejoin_diff_lines(content);

    rejoined.iter().map(|line| {
        if let Some(rest) = line.strip_prefix("~ ") {
            render_diff_line("~ ", DIFF_MODIFIED_STYLE, rest, path, CODE_STYLE)
        } else if let Some(rest) = line.strip_prefix("+ ") {
            render_diff_line("+ ", DIFF_ADD_STYLE, rest, path, CODE_STYLE)
        } else if let Some(rest) = line.strip_prefix("- ") {
            render_diff_line("- ", DIFF_REMOVE_STYLE, rest, path, CODE_STYLE)
        } else if let Some(rest) = line.strip_prefix("  ") {
            render_diff_line("  ", DIFF_PLAIN_STYLE, rest, path, CODE_STYLE)
        } else if line.starts_with("@@") {
            Line::styled(format!("    {}", line), DIFF_HUNK_STYLE)
        } else if line.starts_with("---") || line.starts_with("+++") {
            Line::styled(format!("    {}", line), DIFF_HEADER_STYLE)
        } else if line.starts_with("Edited ") {
            Line::styled(format!("    {}", line), DIFF_FILENAME_STYLE)
        } else {
            Line::styled(format!("    {}", line), DIFF_PLAIN_STYLE)
        }
    }).collect()
}

/// Rejoin lines where inline markers (`⟦...⟧`) got split across multiple lines.
fn rejoin_diff_lines(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in content.lines() {
        let is_diff_line = raw.starts_with("~ ")
            || raw.starts_with("+ ")
            || raw.starts_with("- ")
            || raw.starts_with("  ")
            || raw.starts_with("@@")
            || raw.starts_with("---")
            || raw.starts_with("+++")
            || raw.starts_with("Edited ")
            || raw.is_empty();
        if is_diff_line || lines.is_empty() {
            lines.push(raw.to_string());
        } else {
            if let Some(last) = lines.last_mut() {
                last.push_str(raw);
            }
        }
    }
    lines
}

fn render_diff_line(prefix: &str, prefix_style: Style, line: &str, path: &str, code_style: Style) -> Line<'static> {
    let line_bg = prefix_style.bg.unwrap_or(Color::Reset);
    let mut spans = vec![
        Span::styled("    ", Style::new().bg(line_bg)),
        Span::styled(prefix.to_string(), prefix_style.add_modifier(Modifier::BOLD)),
    ];
    for span in inline_highlighted_spans(line, path, code_style, line_bg) {
        spans.push(span);
    }
    Line::from(spans)
}

fn inline_highlighted_spans(line: &str, path: &str, fallback: Style, line_bg: Color) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    for segment in parse_inline_diff_segments(line) {
        let base = match segment.kind {
            InlineDiffKind::Unchanged => fallback.bg(line_bg),
            InlineDiffKind::Added => INLINE_DIFF_ADD_STYLE,
            InlineDiffKind::Removed => INLINE_DIFF_REMOVE_STYLE,
        };

        for mut span in highlighted_spans(&segment.text, path, fallback) {
            span.style = match segment.kind {
                InlineDiffKind::Unchanged => span.style.bg(line_bg),
                InlineDiffKind::Added => span.style
                    .fg(INLINE_DIFF_ADD_STYLE.fg.unwrap_or(Color::Reset))
                    .bg(INLINE_DIFF_ADD_STYLE.bg.unwrap_or(line_bg))
                    .add_modifier(Modifier::BOLD),
                InlineDiffKind::Removed => span.style
                    .fg(INLINE_DIFF_REMOVE_STYLE.fg.unwrap_or(Color::Reset))
                    .bg(INLINE_DIFF_REMOVE_STYLE.bg.unwrap_or(line_bg))
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::CROSSED_OUT),
            };
            if span.content.is_empty() {
                span.style = base;
            }
            spans.push(span);
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), fallback.bg(line_bg)));
    }

    spans
}

fn parse_inline_diff_segments(input: &str) -> Vec<InlineDiffSegment> {
    let mut segments = Vec::new();
    let mut rest = input;

    while let Some(start) = rest.find('\u{27E6}') {
        if start > 0 {
            segments.push(InlineDiffSegment {
                kind: InlineDiffKind::Unchanged,
                text: rest[..start].to_string(),
            });
        }

        let after_start = &rest[start + '\u{27E6}'.len_utf8()..];
        let Some(end) = after_start.find('\u{27E7}') else {
            segments.push(InlineDiffSegment {
                kind: InlineDiffKind::Unchanged,
                text: format!("\u{27E6}{}", after_start),
            });
            rest = "";
            break;
        };

        let token = &after_start[..end];
        if let Some((marker, text)) = token.split_once(':') {
            let kind = match marker {
                "=" => InlineDiffKind::Unchanged,
                "+" => InlineDiffKind::Added,
                "-" => InlineDiffKind::Removed,
                _ => InlineDiffKind::Unchanged,
            };
            segments.push(InlineDiffSegment {
                kind,
                text: text.to_string(),
            });
        } else {
            segments.push(InlineDiffSegment {
                kind: InlineDiffKind::Unchanged,
                text: format!("\u{27E6}{}\u{27E7}", token),
            });
        }

        rest = &after_start[end + '\u{27E7}'.len_utf8()..];
    }

    if !rest.is_empty() {
        segments.push(InlineDiffSegment {
            kind: InlineDiffKind::Unchanged,
            text: rest.to_string(),
        });
    }

    if segments.is_empty() {
        segments.push(InlineDiffSegment {
            kind: InlineDiffKind::Unchanged,
            text: String::new(),
        });
    }

    segments
}
