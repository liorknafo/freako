use std::sync::OnceLock;

use iced::widget::{Column, Row, container, text};
use iced::{Color, Element, Font, Length};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SyntectColor, FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::app::Message;
use crate::ui::theme::AppTheme;

// Subtle line-level background — just a faint tint to show which lines changed
const DIFF_ADD_BG: Color = Color::from_rgb(0.06, 0.14, 0.08);
const DIFF_REMOVE_BG: Color = Color::from_rgb(0.16, 0.06, 0.06);
const DIFF_ADD_FG: Color = Color::from_rgb(0.72, 0.96, 0.74);
const DIFF_REMOVE_FG: Color = Color::from_rgb(0.98, 0.72, 0.72);
const DIFF_HUNK_FG: Color = Color::from_rgb(0.76, 0.62, 0.96);
const DIFF_HEADER_FG: Color = Color::from_rgb(0.72, 0.76, 0.86);
const DIFF_FILENAME_FG: Color = Color::from_rgb(0.52, 0.84, 0.74);
const DIFF_PLAIN_FG: Color = Color::from_rgb(0.78, 0.80, 0.84);
// Bright inline backgrounds — highlight the exact characters that changed
const INLINE_ADD_BG: Color = Color::from_rgb(0.14, 0.40, 0.18);
const INLINE_REMOVE_BG: Color = Color::from_rgb(0.45, 0.14, 0.14);
// Modified line (merged inline diff) — subtle neutral background
const DIFF_MODIFIED_BG: Color = Color::from_rgb(0.12, 0.12, 0.16);
const DIFF_MODIFIED_FG: Color = Color::from_rgb(0.70, 0.70, 0.80);
const GUTTER_FG: Color = Color::from_rgb(0.40, 0.42, 0.46);
const GUTTER_BG: Color = Color::from_rgb(0.10, 0.10, 0.14);

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

// ---------------------------------------------------------------------------
// Pre-computed diff data (computed once per tool result)
// ---------------------------------------------------------------------------

/// A single text span with pre-computed color and background.
#[derive(Debug, Clone)]
pub struct DiffSpan {
    pub text: String,
    pub color: Color,
    pub bg: Option<Color>,
    pub italic: bool,
}

/// The type of a parsed diff line.
#[derive(Debug, Clone, Copy)]
pub enum ParsedLineKind {
    Added,
    Removed,
    Modified,
    Context,
    Hunk,
    Header,
    Filename,
    Plain,
}

/// A fully parsed and syntax-highlighted diff line, ready for cheap rendering.
#[derive(Debug, Clone)]
pub struct ParsedDiffLine {
    pub kind: ParsedLineKind,
    pub old_num: Option<usize>,
    pub new_num: Option<usize>,
    /// Pre-highlighted spans (prefix + code segments with colors baked in).
    pub spans: Vec<DiffSpan>,
}

/// Pre-computed diff for a single edit_file result. Stored in App, rendered in view().
#[derive(Debug, Clone)]
pub struct ParsedDiff {
    pub lines: Vec<ParsedDiffLine>,
}

/// Parse raw diff content into a `ParsedDiff`. This does all the expensive work
/// (inline segment parsing, syntax highlighting) exactly once.
pub fn parse_diff(content: &str, path: Option<&str>) -> ParsedDiff {
    let fallback_path = path.unwrap_or("diff.rs");
    let syntax_path = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("Edited "))
        .unwrap_or(fallback_path);

    let rejoined = rejoin_diff_lines(content);

    let mut old_lineno: usize = 1;
    let mut new_lineno: usize = 1;
    let mut lines = Vec::with_capacity(rejoined.len());

    for line in &rejoined {
        if let Some(rest) = line.strip_prefix("~ ") {
            // Modified line — merged inline diff with both removed and added chars
            let old_num = old_lineno;
            let new_num = new_lineno;
            old_lineno += 1;
            new_lineno += 1;
            let spans = build_code_spans("~ ", DIFF_MODIFIED_FG, Some(DIFF_MODIFIED_BG), rest, syntax_path);
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Modified,
                old_num: Some(old_num),
                new_num: Some(new_num),
                spans,
            });
        } else if let Some(rest) = line.strip_prefix("+ ") {
            let new_num = new_lineno;
            new_lineno += 1;
            let spans = build_code_spans("+ ", DIFF_ADD_FG, Some(DIFF_ADD_BG), rest, syntax_path);
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Added,
                old_num: None,
                new_num: Some(new_num),
                spans,
            });
        } else if let Some(rest) = line.strip_prefix("- ") {
            let old_num = old_lineno;
            old_lineno += 1;
            let spans = build_code_spans("- ", DIFF_REMOVE_FG, Some(DIFF_REMOVE_BG), rest, syntax_path);
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Removed,
                old_num: Some(old_num),
                new_num: None,
                spans,
            });
        } else if let Some(rest) = line.strip_prefix("  ") {
            let old_num = old_lineno;
            let new_num = new_lineno;
            old_lineno += 1;
            new_lineno += 1;
            let spans = build_code_spans("  ", DIFF_PLAIN_FG, None, rest, syntax_path);
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Context,
                old_num: Some(old_num),
                new_num: Some(new_num),
                spans,
            });
        } else if line.starts_with("@@") {
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Hunk,
                old_num: None,
                new_num: None,
                spans: vec![DiffSpan { text: line.clone(), color: DIFF_HUNK_FG, bg: None, italic: false }],
            });
        } else if line.starts_with("---") || line.starts_with("+++") {
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Header,
                old_num: None,
                new_num: None,
                spans: vec![DiffSpan { text: line.clone(), color: DIFF_HEADER_FG, bg: None, italic: false }],
            });
        } else if line.starts_with("Edited ") {
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Filename,
                old_num: None,
                new_num: None,
                spans: vec![DiffSpan { text: line.clone(), color: DIFF_FILENAME_FG, bg: None, italic: false }],
            });
        } else {
            lines.push(ParsedDiffLine {
                kind: ParsedLineKind::Plain,
                old_num: None,
                new_num: None,
                spans: vec![DiffSpan { text: line.clone(), color: DIFF_PLAIN_FG, bg: None, italic: false }],
            });
        }
    }

    ParsedDiff { lines }
}

/// Build pre-highlighted spans for a code diff line (added/removed/context).
fn build_code_spans(
    prefix: &str,
    prefix_color: Color,
    line_bg: Option<Color>,
    raw: &str,
    syntax_path: &str,
) -> Vec<DiffSpan> {
    let mut spans = Vec::new();

    // Prefix span ("+ ", "- ", "  ")
    spans.push(DiffSpan {
        text: prefix.to_string(),
        color: prefix_color,
        bg: line_bg,
        italic: false,
    });

    // Parse inline change markers and syntax-highlight each segment
    for segment in parse_inline_segments(raw) {
        let seg_bg = match segment.kind {
            InlineChangeKind::Unchanged => line_bg,
            InlineChangeKind::Added => Some(INLINE_ADD_BG),
            InlineChangeKind::Removed => Some(INLINE_REMOVE_BG),
        };

        for (text_segment, color, _bold, italic) in highlighted_segments(&segment.text, syntax_path) {
            spans.push(DiffSpan {
                text: text_segment,
                color,
                bg: seg_bg,
                italic,
            });
        }
    }

    spans
}

// ---------------------------------------------------------------------------
// Rendering (called every frame — just maps pre-computed data to widgets)
// ---------------------------------------------------------------------------

/// Render a pre-computed `ParsedDiff` into an Iced element. Cheap — no parsing or highlighting.
pub fn render_parsed_diff(diff: &ParsedDiff) -> Element<'static, Message> {
    let mut col = Column::new().spacing(0);

    for line in &diff.lines {
        col = col.push(render_parsed_line(line));
    }

    if diff.lines.is_empty() {
        col = col.push(text("<empty>").size(12).font(Font::MONOSPACE).color(AppTheme::text_muted()));
    }

    container(col)
        .padding(8)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::bg().into()),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: AppTheme::border_subtle(),
            },
            ..Default::default()
        })
        .into()
}

fn render_parsed_line(line: &ParsedDiffLine) -> Element<'static, Message> {
    match line.kind {
        ParsedLineKind::Added | ParsedLineKind::Removed | ParsedLineKind::Modified | ParsedLineKind::Context => {
            let line_bg = match line.kind {
                ParsedLineKind::Added => Some(DIFF_ADD_BG),
                ParsedLineKind::Removed => Some(DIFF_REMOVE_BG),
                ParsedLineKind::Modified => Some(DIFF_MODIFIED_BG),
                _ => None,
            };
            let code = render_spans(&line.spans, line_bg);
            Row::new()
                .spacing(0)
                .push(gutter_cell(line.old_num, line_bg))
                .push(gutter_cell(line.new_num, line_bg))
                .push(code)
                .into()
        }
        _ => {
            // Plain/hunk/header/filename — single styled text, no gutter
            let span = line.spans.first().cloned().unwrap_or(DiffSpan {
                text: String::new(),
                color: DIFF_PLAIN_FG,
                bg: None,
                italic: false,
            });
            render_plain_row(span.text, span.color, span.bg)
        }
    }
}

fn render_spans(spans: &[DiffSpan], line_bg: Option<Color>) -> Element<'static, Message> {
    let mut row = Row::new().spacing(0);
    for span in spans {
        let bg = span.bg;
        let mut txt = text(span.text.clone()).size(12).font(Font::MONOSPACE).color(span.color);
        if span.italic {
            txt = txt.shaping(iced::widget::text::Shaping::Advanced);
        }
        row = row.push(
            container(txt)
                .style(move |_t: &iced::Theme| container::Style {
                    background: bg.map(Into::into),
                    ..Default::default()
                })
        );
    }
    container(row)
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: line_bg.map(Into::into),
            ..Default::default()
        })
        .into()
}

fn gutter_cell(num: Option<usize>, bg: Option<Color>) -> Element<'static, Message> {
    let label = match num {
        Some(n) => format!("{:>4}", n),
        None => "    ".to_string(),
    };
    let line_bg = bg.unwrap_or(GUTTER_BG);
    container(
        text(label).size(12).font(Font::MONOSPACE).color(GUTTER_FG)
    )
    .style(move |_t: &iced::Theme| container::Style {
        background: Some(line_bg.into()),
        ..Default::default()
    })
    .into()
}

fn render_plain_row(line: String, color: Color, bg: Option<Color>) -> Element<'static, Message> {
    container(text(line).size(12).font(Font::MONOSPACE).color(color))
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: bg.map(Into::into),
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Inline marker parsing (private helpers)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum InlineChangeKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
struct InlineSegment {
    kind: InlineChangeKind,
    text: String,
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

fn parse_inline_segments(input: &str) -> Vec<InlineSegment> {
    let mut segments = Vec::new();
    let mut rest = input;

    while let Some(start) = rest.find('⟦') {
        if start > 0 {
            segments.push(InlineSegment {
                kind: InlineChangeKind::Unchanged,
                text: rest[..start].to_string(),
            });
        }

        let after_start = &rest[start + '⟦'.len_utf8()..];
        let Some(end) = after_start.find('⟧') else {
            segments.push(InlineSegment {
                kind: InlineChangeKind::Unchanged,
                text: format!("⟦{}", after_start),
            });
            rest = "";
            break;
        };

        let token = &after_start[..end];
        if let Some((marker, text)) = token.split_once(':') {
            let kind = match marker {
                "=" => InlineChangeKind::Unchanged,
                "+" => InlineChangeKind::Added,
                "-" => InlineChangeKind::Removed,
                _ => InlineChangeKind::Unchanged,
            };
            segments.push(InlineSegment {
                kind,
                text: text.to_string(),
            });
        } else {
            segments.push(InlineSegment {
                kind: InlineChangeKind::Unchanged,
                text: format!("⟦{}⟧", token),
            });
        }

        rest = &after_start[end + '⟧'.len_utf8()..];
    }

    if !rest.is_empty() {
        segments.push(InlineSegment {
            kind: InlineChangeKind::Unchanged,
            text: rest.to_string(),
        });
    }

    if segments.is_empty() {
        segments.push(InlineSegment {
            kind: InlineChangeKind::Unchanged,
            text: String::new(),
        });
    }

    segments
}

// ---------------------------------------------------------------------------
// Syntax highlighting
// ---------------------------------------------------------------------------

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

fn syntax_for_path(path: &str) -> &'static SyntaxReference {
    let set = syntax_set();
    set.find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

fn syntect_color_to_iced(color: SyntectColor) -> Color {
    Color::from_rgba8(color.r, color.g, color.b, f32::from(color.a) / 255.0)
}

fn highlighted_segments(line: &str, path: &str) -> Vec<(String, Color, bool, bool)> {
    let syntax = syntax_for_path(path);
    let mut highlighter = HighlightLines::new(syntax, theme());
    let mut segments = Vec::new();

    for segment in LinesWithEndings::from(line) {
        match highlighter.highlight_line(segment, syntax_set()) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    segments.push((
                        text.to_string(),
                        syntect_color_to_iced(style.foreground),
                        style.font_style.contains(FontStyle::BOLD),
                        style.font_style.contains(FontStyle::ITALIC),
                    ));
                }
            }
            Err(_) => segments.push((segment.to_string(), AppTheme::text_primary(), false, false)),
        }
    }

    if segments.is_empty() {
        segments.push((line.to_string(), AppTheme::text_primary(), false, false));
    }

    segments
}
