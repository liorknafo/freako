use std::collections::HashMap;
use std::sync::OnceLock;

use pulldown_cmark::{Event as MarkdownEvent, Options, Parser, Tag, TagEnd};
use ratatui::prelude::*;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SyntectColor, FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use freako_core::agent::events::ToolOutputStream;
use freako_core::session::types::{ConversationMessage, MessagePart, Role};
use freako_core::tools::tool_name::format_tool_presentation;

use super::{
    ASSISTANT_STYLE, CHAT_BG, CODE_BLOCK_BG, ERROR_STYLE, HEADING_STYLE,
    INLINE_CODE_BG, MUTED_STYLE, SUCCESS_STYLE, SYSTEM_STYLE, TOOL_STYLE, USER_STYLE,
    USER_MSG_BG, ASSISTANT_MSG_BG, SYSTEM_MSG_BG, TOOL_MSG_BG,
};

static MARKDOWN_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH);

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| themes.themes.values().next().cloned())
            .unwrap_or_default()
    })
}

pub(crate) fn syntax_for_path(path: &str) -> &'static SyntaxReference {
    let set = syntax_set();
    set.find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

fn syntect_color_to_tui(color: SyntectColor) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

pub(crate) fn highlighted_spans(line: &str, path: &str, fallback: Style) -> Vec<Span<'static>> {
    let syntax = syntax_for_path(path);
    let mut highlighter = HighlightLines::new(syntax, theme());
    let mut out: Vec<Span<'static>> = Vec::new();

    match highlighter.highlight_line(line, syntax_set()) {
        Ok(ranges) => {
            for (style, text) in ranges {
                let mut span_style = fallback;
                span_style.fg = Some(syntect_color_to_tui(style.foreground));
                if style.font_style.contains(FontStyle::BOLD) {
                    span_style = span_style.add_modifier(Modifier::BOLD);
                }
                if style.font_style.contains(FontStyle::ITALIC) {
                    span_style = span_style.add_modifier(Modifier::ITALIC);
                }
                out.push(Span::styled(text.to_string(), span_style));
            }
        }
        Err(_) => {
            out.push(Span::styled(line.to_string(), fallback));
        }
    }

    if out.is_empty() {
        out.push(Span::styled(line.to_string(), fallback));
    }

    out
}

pub(super) fn build_grouped_messages(messages: &[ConversationMessage]) -> Vec<ConversationMessage> {
    let mut grouped: Vec<ConversationMessage> = Vec::new();
    for msg in messages {
        match msg.role {
            Role::Tool | Role::Assistant => {
                if let Some(last) = grouped.last_mut() {
                    if last.role == Role::Assistant {
                        last.parts.extend(msg.parts.iter().cloned());
                        continue;
                    }
                }
                let mut merged = msg.clone();
                if merged.role == Role::Tool {
                    merged.role = Role::Assistant;
                }
                grouped.push(merged);
            }
            _ => grouped.push(msg.clone()),
        }
    }
    grouped.retain(|msg| {
        msg.role == Role::User
            || msg.role == Role::System
            || !msg.full_text().is_empty()
            || msg.parts.iter().any(|p| matches!(p, MessagePart::ToolCall { .. } | MessagePart::ToolResult { .. }))
    });
    grouped
}

pub(super) fn message_style(role: Role) -> Style {
    match role {
        Role::User => USER_STYLE,
        Role::Assistant => ASSISTANT_STYLE,
        Role::Tool => TOOL_STYLE,
        Role::System => SYSTEM_STYLE,
    }
}

fn role_bubble_bg(role: Role) -> Color {
    match role {
        Role::User => USER_MSG_BG,
        Role::Assistant => ASSISTANT_MSG_BG,
        Role::System => SYSTEM_MSG_BG,
        Role::Tool => TOOL_MSG_BG,
    }
}

fn bubble_layout(_role: Role, width: u16) -> (usize, usize) {
    let total_width = width.max(12) as usize;
    // 1-char margin on each side; outer_width shrinks by 2
    (1, total_width.saturating_sub(2))
}

fn wrap_line_to_width(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![Line::raw("")];
    }

    if line.spans.is_empty() {
        return vec![Line::raw("")];
    }

    // Flatten all spans into a sequence of (char, style) pairs
    let mut all_chars: Vec<(char, Style)> = Vec::new();
    for span in &line.spans {
        for ch in span.content.chars() {
            all_chars.push((ch, span.style));
        }
    }

    if all_chars.is_empty() {
        return vec![Line::raw("")];
    }

    let mut wrapped = Vec::new();
    let mut pos = 0usize;

    while pos < all_chars.len() {
        let end = (pos + max_width).min(all_chars.len());

        if end >= all_chars.len() {
            // Remaining content fits in one line
            let mut spans = Vec::new();
            let mut run_start = pos;
            let mut run_style = all_chars[pos].1;
            for i in pos..all_chars.len() {
                if all_chars[i].1 != run_style {
                    let text: String = all_chars[run_start..i].iter().map(|(c, _)| c).collect();
                    spans.push(Span::styled(text, run_style));
                    run_start = i;
                    run_style = all_chars[i].1;
                }
            }
            let text: String = all_chars[run_start..].iter().map(|(c, _)| c).collect();
            spans.push(Span::styled(text, run_style));
            wrapped.push(Line::from(spans).style(line.style));
            break;
        }

        // Look back from `end` for a whitespace break point
        let mut break_at = end;
        let mut found_space = false;
        for i in (pos..end).rev() {
            if all_chars[i].0.is_whitespace() {
                break_at = i + 1; // break after the whitespace
                found_space = true;
                break;
            }
        }
        // If no whitespace found, hard-break at max_width
        if !found_space {
            break_at = end;
        }
        // Avoid zero-length segments
        if break_at == pos {
            break_at = end;
        }

        // Build spans for this wrapped line
        let mut spans = Vec::new();
        let mut run_start = pos;
        let mut run_style = all_chars[pos].1;
        for i in pos..break_at {
            if all_chars[i].1 != run_style {
                let text: String = all_chars[run_start..i].iter().map(|(c, _)| c).collect();
                spans.push(Span::styled(text, run_style));
                run_start = i;
                run_style = all_chars[i].1;
            }
        }
        let text: String = all_chars[run_start..break_at].iter().map(|(c, _)| c).collect();
        spans.push(Span::styled(text, run_style));
        wrapped.push(Line::from(spans).style(line.style));

        pos = break_at;
        // Skip leading whitespace on the next line
        while pos < all_chars.len() && all_chars[pos].0 == ' ' {
            pos += 1;
        }
    }

    if wrapped.is_empty() {
        wrapped.push(Line::raw(""));
    }

    wrapped
}

fn bubble_content_line(
    left_pad: usize,
    inner_width: usize,
    bubble_bg: Color,
    line: Line<'static>,
) -> Line<'static> {
    let is_code_block = line.style.bg == Some(CODE_BLOCK_BG);

    let content_width: usize = line
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum();

    // If the line itself has a bg (e.g. code block), use it for padding areas too
    let fill_bg = line.style.bg.unwrap_or(bubble_bg);

    if is_code_block {
        // Code block lines get: margin (bubble_bg) + padding (CODE_BLOCK_BG) on each side
        let code_inner = inner_width.saturating_sub(4); // 1 margin + 1 padding each side
        let right_pad = code_inner.saturating_sub(content_width);

        let mut spans = Vec::with_capacity(line.spans.len() + 9);
        // Left margin (CHAT_BG)
        if left_pad > 0 {
            spans.push(Span::styled(" ".repeat(left_pad), Style::new().bg(CHAT_BG)));
        }
        // Left braille (bubble_bg)
        spans.push(Span::styled("\u{2800}", Style::new().bg(bubble_bg)));
        // Left code margin (bubble_bg)
        spans.push(Span::styled(" ", Style::new().bg(bubble_bg)));
        // Left code padding (CODE_BLOCK_BG)
        spans.push(Span::styled(" ", Style::new().bg(CODE_BLOCK_BG)));
        // Content spans — preserve explicit bg, otherwise apply CODE_BLOCK_BG
        spans.extend(line.spans.into_iter().map(|span| {
            let style = if span.style.bg.is_some() {
                span.style
            } else {
                span.style.patch(Style::new().bg(CODE_BLOCK_BG))
            };
            Span::styled(span.content.into_owned(), style)
        }));
        // Right code fill + padding (CODE_BLOCK_BG)
        spans.push(Span::styled(" ".repeat(right_pad + 1), Style::new().bg(CODE_BLOCK_BG)));
        // Right code margin (bubble_bg)
        spans.push(Span::styled(" ", Style::new().bg(bubble_bg)));
        // Right braille (bubble_bg)
        spans.push(Span::styled("\u{2800}", Style::new().bg(bubble_bg)));
        // Right margin (CHAT_BG)
        if left_pad > 0 {
            spans.push(Span::styled(" ", Style::new().bg(CHAT_BG)));
        }
        Line::from(spans).style(Style::new().bg(bubble_bg))
    } else {
        let right_pad = inner_width.saturating_sub(content_width);

        let mut spans = Vec::with_capacity(line.spans.len() + 5);
        // Left margin (CHAT_BG)
        if left_pad > 0 {
            spans.push(Span::styled(" ".repeat(left_pad), Style::new().bg(CHAT_BG)));
        }
        // Left bubble padding — Braille blank (U+2800) prevents ratatui's WordWrapper
        // from splitting blank lines into empty wrapped lines
        spans.push(Span::styled("\u{2800}", Style::new().bg(fill_bg)));
        // Content spans — preserve explicit bg (e.g. code block bg), otherwise apply bubble_bg
        spans.extend(line.spans.into_iter().map(|span| {
            let style = if span.style.bg.is_some() {
                span.style
            } else {
                span.style.patch(Style::new().bg(fill_bg))
            };
            Span::styled(span.content.into_owned(), style)
        }));
        // Right bubble padding — fill with the same bg as content
        spans.push(Span::styled(format!("{}\u{2800}", " ".repeat(right_pad)), Style::new().bg(fill_bg)));
        // Right margin (CHAT_BG)
        if left_pad > 0 {
            spans.push(Span::styled(" ", Style::new().bg(CHAT_BG)));
        }
        // Line bg = fill_bg: any uncovered position gets the correct color
        Line::from(spans).style(Style::new().bg(fill_bg))
    }
}

pub(super) fn compact_shell_output(content: &str, head: usize, tail: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= head + tail + 1 {
        return content.to_string();
    }

    let omitted = lines.len().saturating_sub(head + tail);
    let mut compacted: Vec<String> = Vec::new();
    compacted.extend(lines.iter().take(head).map(|line| (*line).to_string()));
    compacted.push(format!("… <{} lines omitted> …", omitted));
    compacted.extend(lines.iter().skip(lines.len().saturating_sub(tail)).map(|line| (*line).to_string()));
    compacted.join("\n")
}

pub(crate) fn ansi_line_spans(text: &str, fallback: Style, _stream: Option<ToolOutputStream>) -> Vec<Line<'static>> {
    let base_style = fallback;

    text.lines()
        .map(|line| {
            let spans = parse_ansi_spans(line, base_style);
            Line::from(spans)
        })
        .collect()
}

fn parse_ansi_spans(line: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current_style = base;
    let mut buf = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), current_style));
            }
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut seq = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphabetic() {
                        let cmd = c;
                        chars.next();
                        if cmd == 'm' {
                            current_style = apply_sgr(&seq, base);
                        }
                        break;
                    }
                    seq.push(c);
                    chars.next();
                }
            }
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, current_style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base));
    }
    spans
}

fn apply_sgr(seq: &str, base: Style) -> Style {
    let mut style = base;
    if seq.is_empty() || seq == "0" {
        return base;
    }
    for code_str in seq.split(';') {
        let code: u8 = match code_str.parse() {
            Ok(c) => c,
            Err(_) => continue,
        };
        match code {
            0 => style = base,
            1 => style = style.add_modifier(Modifier::BOLD),
            2 => style = style.add_modifier(Modifier::DIM),
            3 => style = style.add_modifier(Modifier::ITALIC),
            4 => style = style.add_modifier(Modifier::UNDERLINED),
            22 => style = style.remove_modifier(Modifier::BOLD).remove_modifier(Modifier::DIM),
            23 => style = style.remove_modifier(Modifier::ITALIC),
            24 => style = style.remove_modifier(Modifier::UNDERLINED),
            30 => style = style.fg(Color::Black),
            31 => style = style.fg(Color::Red),
            32 => style = style.fg(Color::Green),
            33 => style = style.fg(Color::Yellow),
            34 => style = style.fg(Color::Blue),
            35 => style = style.fg(Color::Magenta),
            36 => style = style.fg(Color::Cyan),
            37 => style = style.fg(Color::White),
            39 => style.fg = base.fg,
            40 => style = style.bg(Color::Black),
            41 => style = style.bg(Color::Red),
            42 => style = style.bg(Color::Green),
            43 => style = style.bg(Color::Yellow),
            44 => style = style.bg(Color::Blue),
            45 => style = style.bg(Color::Magenta),
            46 => style = style.bg(Color::Cyan),
            47 => style = style.bg(Color::White),
            49 => style.bg = base.bg,
            _ => {}
        }
    }
    style
}

pub(super) fn render_message_lines(
    msg: &ConversationMessage,
    style: Style,
    _show_prefix: bool,
    width: u16,
    tool_views: &HashMap<String, Box<dyn super::tools::ToolView>>,
    tool_collapse_state: &HashMap<String, bool>,
    tool_header_indices: &mut Vec<(String, usize)>,
) -> Vec<Line<'static>> {
    let bubble_bg = role_bubble_bg(msg.role);
    let (left_pad, outer_width) = bubble_layout(msg.role, width);
    let inner_width = outer_width.saturating_sub(2);
    let mut lines = Vec::new();
    let mut bubble_lines = Vec::new();
    let mut pending_tool_header_positions: Vec<(String, usize)> = Vec::new();

    // Collect tool result / output metadata
    let mut tool_results: HashMap<String, (String, bool)> = HashMap::new();
    let mut tool_outputs: HashMap<String, String> = HashMap::new();
    for part in &msg.parts {
        match part {
            MessagePart::ToolResult { tool_call_id, content, is_error, .. } => {
                tool_results.insert(tool_call_id.clone(), (content.clone(), *is_error));
            }
            MessagePart::ToolOutput { tool_call_id, content, .. } => {
                tool_outputs.insert(tool_call_id.clone(), content.clone());
            }
            _ => {}
        }
    }

    for part in &msg.parts {
        match part {
            MessagePart::Text { text } if !text.is_empty() => {
                if msg.role == Role::User {
                    for l in text.lines() {
                        if l.is_empty() {
                            bubble_lines.push(Line::raw(""));
                        } else {
                            bubble_lines.push(Line::styled(l.to_string(), style));
                        }
                    }
                } else {
                    bubble_lines.extend(markdown_to_lines(text, inner_width as u16));
                }
            }
            MessagePart::ToolCall { id, name, arguments, .. } => {
                // Use tool view if available, otherwise fall back to simple header.
                if let Some(view) = tool_views.get(id) {
                    let collapsed = tool_collapse_state
                        .get(id)
                        .copied()
                        .unwrap_or_else(|| view.default_collapsed());
                    pending_tool_header_positions.push((id.clone(), bubble_lines.len()));
                    bubble_lines.extend(super::tools::render_tool(view.as_ref(), collapsed, inner_width as u16));
                } else {
                    let (title, summary) = format_tool_presentation(name, arguments)
                        .unwrap_or_else(|| (name.clone().into(), fallback_tool_summary(name, arguments)));
                    let status = if let Some((_, is_err)) = tool_results.get(id) {
                        if *is_err { "error" } else { "done" }
                    } else {
                        "pending"
                    };
                    let status_style = match status {
                        "done" => SUCCESS_STYLE,
                        "error" => ERROR_STYLE,
                        _ => TOOL_STYLE,
                    };
                    bubble_lines.push(Line::from(vec![
                        Span::styled("  ▶ ", MUTED_STYLE),
                        Span::styled(title.to_string(), TOOL_STYLE.add_modifier(Modifier::BOLD)),
                        Span::styled(format!(" - {}", summary), TOOL_STYLE),
                        Span::styled(format!(" - {}", status), status_style),
                    ]));
                }
            }
            MessagePart::ToolResult { tool_call_id, .. } => {
                // Already rendered via ToolCall above when a tool view exists.
                if tool_views.contains_key(tool_call_id) {
                    // no-op
                }
            }
            MessagePart::Image { media_type, .. } => {
                bubble_lines.push(Line::styled(
                    format!("[image: {}]", media_type),
                    Style::new().fg(Color::Indexed(75)),
                ));
            }
            MessagePart::ToolOutput { .. } => {
                // ToolOutput is surfaced via dedicated tool views; no inline rendering here.
            }
            _ => {}
        }
    }

    // Top padding
    lines.push(bubble_content_line(left_pad, inner_width, bubble_bg, Line::raw("")));

    for (raw_idx, bubble_line) in bubble_lines.into_iter().enumerate() {
        for (tool_call_id, header_raw_idx) in &pending_tool_header_positions {
            if *header_raw_idx == raw_idx {
                tool_header_indices.push((tool_call_id.clone(), lines.len()));
            }
        }
        let is_code = bubble_line.style.bg == Some(CODE_BLOCK_BG);
        let wrap_w = if is_code { inner_width.saturating_sub(4) } else { inner_width };
        for wrapped in wrap_line_to_width(bubble_line, wrap_w) {
            lines.push(bubble_content_line(
                left_pad,
                inner_width,
                bubble_bg,
                wrapped,
            ));
        }
    }

    // Bottom padding
    lines.push(bubble_content_line(left_pad, inner_width, bubble_bg, Line::raw("")));

    lines
}

pub(crate) fn fallback_tool_summary(name: &str, args: &serde_json::Value) -> String {
    if let Some(obj) = args.as_object() {
        for (_, v) in obj {
            if let Some(s) = v.as_str() {
                return truncate_middle(s, 50);
            }
        }
    }
    name.to_string()
}

pub(super) fn format_messages(messages: &[ConversationMessage]) -> Vec<String> {
    let mut out = Vec::new();
    for message in messages {
        let role = match message.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
            Role::System => "System",
        };

        let mut parts = Vec::new();
        for part in &message.parts {
            match part {
                MessagePart::Text { text } => parts.push(text.clone()),
                MessagePart::ToolCall { name, arguments, .. } => {
                    let args = serde_json::to_string_pretty(arguments)
                        .unwrap_or_else(|_| arguments.to_string());
                    parts.push(format!("[tool call] {} {}", name, args));
                }
                MessagePart::ToolResult { name, content, is_error, .. } => {
                    let status = if *is_error { "error" } else { "ok" };
                    parts.push(format!("[tool result:{}] {}\n{}", status, name, content));
                }
                MessagePart::Image { media_type, .. } => {
                    parts.push(format!("[image: {}]", media_type));
                }
                _ => {}
            }
        }

        out.push(format!("{}: {}", role, parts.join("\n")));
    }
    out
}

pub(super) fn truncate_middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    if len <= max {
        return s.to_string();
    }
    if max <= 3 {
        return "…".repeat(max);
    }
    let keep = max - 3;
    let front = keep / 2;
    let back = keep - front;
    let start: String = chars[..front].iter().collect();
    let end: String = chars[len - back..].iter().collect();
    format!("{}...{}", start, end)
}

pub(super) fn markdown_to_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    let parser = Parser::new_ext(text, MARKDOWN_OPTIONS);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();

    let mut in_code_block = false;
    let mut code_block_lang: Option<String> = None;
    let mut in_heading = false;
    let mut list_stack: Vec<(bool, u64)> = Vec::new(); // (is_ordered, counter)
    let mut item_needs_prefix = false;
    let mut bold_depth: u32 = 0;
    let mut italic_depth: u32 = 0;
    let mut link_url: Option<String> = None;

    macro_rules! flush {
        () => {
            if !current_spans.is_empty() {
                let mut line = Line::from(std::mem::take(&mut current_spans));
                if in_code_block {
                    line = line.style(Style::new().bg(CODE_BLOCK_BG));
                }
                lines.push(line);
            }
        };
    }

    for event in parser {
        match event {
            // ── Code blocks ──
            MarkdownEvent::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_block_lang = match &kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        let l = lang.split_whitespace().next().unwrap_or("").to_string();
                        if l.is_empty() { None } else { Some(l) }
                    }
                    _ => None,
                };
                flush!();
            }
            MarkdownEvent::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                code_block_lang = None;
                flush!();
            }

            // ── Headings ──
            MarkdownEvent::Start(Tag::Heading { .. }) => {
                in_heading = true;
                flush!();
            }
            MarkdownEvent::End(TagEnd::Heading(_)) => {
                in_heading = false;
                flush!();
            }

            // ── Paragraphs ──
            MarkdownEvent::Start(Tag::Paragraph) => {}
            MarkdownEvent::End(TagEnd::Paragraph) => {
                flush!();
                if list_stack.is_empty() {
                    lines.push(Line::raw(""));
                }
            }

            // ── Lists ──
            MarkdownEvent::Start(Tag::List(first)) => {
                flush!();
                let is_ordered = first.is_some();
                let start = first.unwrap_or(0);
                list_stack.push((is_ordered, start));
            }
            MarkdownEvent::End(TagEnd::List(_)) => {
                list_stack.pop();
                flush!();
                if list_stack.is_empty() {
                    lines.push(Line::raw(""));
                }
            }
            MarkdownEvent::Start(Tag::Item) => {
                flush!();
                item_needs_prefix = true;
            }
            MarkdownEvent::End(TagEnd::Item) => {
                flush!();
            }

            // ── Emphasis ──
            MarkdownEvent::Start(Tag::Emphasis) => { italic_depth += 1; }
            MarkdownEvent::End(TagEnd::Emphasis) => { italic_depth = italic_depth.saturating_sub(1); }
            MarkdownEvent::Start(Tag::Strong) => { bold_depth += 1; }
            MarkdownEvent::End(TagEnd::Strong) => { bold_depth = bold_depth.saturating_sub(1); }

            // ── Links ──
            MarkdownEvent::Start(Tag::Link { dest_url, .. }) => {
                link_url = Some(dest_url.to_string());
            }
            MarkdownEvent::End(TagEnd::Link) => {
                if let Some(url) = link_url.take() {
                    let last_text = current_spans.last()
                        .map(|s| s.content.as_ref().to_string())
                        .unwrap_or_default();
                    if last_text != url && !url.is_empty() {
                        current_spans.push(Span::styled(
                            format!(" ({})", url),
                            MUTED_STYLE,
                        ));
                    }
                }
            }

            // ── Text content ──
            MarkdownEvent::Text(t) => {
                // Code blocks: use syntax highlighting
                if in_code_block {
                    let fallback = Style::new().fg(Color::Indexed(151)).bg(CODE_BLOCK_BG);
                    let ext = code_block_lang.as_deref().unwrap_or("txt");
                    // Map common language names to file extensions for syntect
                    let path = match ext {
                        "rust" | "rs" => "file.rs",
                        "python" | "py" => "file.py",
                        "javascript" | "js" => "file.js",
                        "typescript" | "ts" => "file.ts",
                        "bash" | "sh" | "shell" | "zsh" => "file.sh",
                        "json" => "file.json",
                        "toml" => "file.toml",
                        "yaml" | "yml" => "file.yaml",
                        "html" => "file.html",
                        "css" => "file.css",
                        "sql" => "file.sql",
                        "go" => "file.go",
                        "java" => "file.java",
                        "c" => "file.c",
                        "cpp" | "c++" | "cxx" => "file.cpp",
                        "ruby" | "rb" => "file.rb",
                        "markdown" | "md" => "file.md",
                        "xml" => "file.xml",
                        "diff" | "patch" => "file.diff",
                        other => {
                            // Try as extension directly
                            if other.contains('.') { other } else { "file.txt" }
                        }
                    };
                    for (i, line_text) in t.split('\n').enumerate() {
                        if i > 0 { flush!(); }
                        if !line_text.is_empty() {
                            let mut spans = highlighted_spans(line_text, path, fallback);
                            // Ensure all spans have CODE_BLOCK_BG
                            for span in &mut spans {
                                if span.style.bg.is_none() {
                                    span.style = span.style.bg(CODE_BLOCK_BG);
                                }
                            }
                            current_spans.extend(spans);
                        } else if i > 0 {
                            // Empty line within code block — push a styled blank line
                            lines.push(Line::raw("").style(Style::new().bg(CODE_BLOCK_BG)));
                        }
                    }
                } else {
                    let mut style = if in_heading {
                        HEADING_STYLE.add_modifier(Modifier::BOLD)
                    } else if link_url.is_some() {
                        Style::new().fg(Color::Indexed(75)).add_modifier(Modifier::UNDERLINED)
                    } else {
                        ASSISTANT_STYLE
                    };
                    if bold_depth > 0 { style = style.add_modifier(Modifier::BOLD); }
                    if italic_depth > 0 { style = style.add_modifier(Modifier::ITALIC); }

                    // List item prefix
                    if item_needs_prefix && !list_stack.is_empty() {
                        item_needs_prefix = false;
                        let depth = list_stack.len();
                        let indent = "  ".repeat(depth.saturating_sub(1));
                        if let Some((is_ordered, counter)) = list_stack.last_mut() {
                            let prefix = if *is_ordered {
                                let n = *counter;
                                *counter += 1;
                                format!("{}{}. ", indent, n)
                            } else {
                                format!("{}\u{2022} ", indent)
                            };
                            current_spans.push(Span::styled(prefix, MUTED_STYLE));
                        }
                    }

                    for (i, line_text) in t.split('\n').enumerate() {
                        if i > 0 { flush!(); }
                        if !line_text.is_empty() {
                            current_spans.push(Span::styled(line_text.to_string(), style));
                        }
                    }
                }
            }

            // ── Inline code ──
            MarkdownEvent::Code(t) => {
                current_spans.push(Span::styled(
                    format!(" {} ", t),
                    Style::new().fg(Color::Indexed(151)).bg(INLINE_CODE_BG),
                ));
            }

            // ── Line breaks ──
            MarkdownEvent::SoftBreak => {
                // Soft break = space between words in a paragraph
                current_spans.push(Span::raw(" "));
            }
            MarkdownEvent::HardBreak => {
                flush!();
            }

            _ => {}
        }
    }

    flush!();

    if lines.is_empty() {
        lines.push(Line::raw(""));
    }

    // Strip trailing blank lines
    while lines.len() > 1 && lines.last().map_or(false, |l| {
        l.spans.is_empty() || l.spans.iter().all(|s| s.content.trim().is_empty())
    }) {
        lines.pop();
    }

    let wrap_width = width.max(1) as usize;
    let mut wrapped = Vec::new();
    for line in lines {
        let is_code = line.style.bg == Some(CODE_BLOCK_BG);
        let w = if is_code { wrap_width.saturating_sub(4) } else { wrap_width };
        wrapped.extend(wrap_line_to_width(line, w));
    }

    wrapped
}
