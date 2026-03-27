use std::sync::OnceLock;

use iced::widget::{Column, Row, button, column, container, row, text};
use iced::{Color, Element, Font, Length};
use serde_json::Value;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SyntectColor, FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::app::Message;
use crate::ui::theme::AppTheme;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

pub fn approval_arguments_view(arguments: &Value) -> Element<'static, Message> {
    let pretty = serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string());
    let is_write_file = arguments.get("content").and_then(|v| v.as_str()).is_some();
    let expanded = true;

    let controls = row![
        button(text(if expanded { "Hide full file" } else { "Show full file" }).size(11))
            .padding([4, 10])
            .on_press(Message::ToggleApprovalExpanded)
    ]
    .spacing(8);

    let body: Element<'static, Message> = if is_write_file && expanded {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("untitled.txt")
            .to_string();
        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        syntax_highlighted_block(&path, &content)
    } else {
        plain_block(&pretty)
    };

    column![controls, body].spacing(8).into()
}

pub fn approval_arguments_view_collapsible(arguments: &Value, expanded: bool) -> Element<'static, Message> {
    let pretty = serde_json::to_string_pretty(arguments).unwrap_or_else(|_| arguments.to_string());
    let is_write_file = arguments.get("content").and_then(|v| v.as_str()).is_some();

    let label = if expanded { "Hide full file" } else { "Show full file" };
    let controls = row![
        button(text(label).size(11))
            .padding([4, 10])
            .on_press(Message::ToggleApprovalExpanded)
    ]
    .spacing(8);

    let body: Element<'static, Message> = if is_write_file && expanded {
        let path = arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("untitled.txt")
            .to_string();
        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        syntax_highlighted_block(&path, &content)
    } else {
        plain_block(&pretty)
    };

    column![controls, body].spacing(8).into()
}

fn plain_block(content: &str) -> Element<'static, Message> {
    container(
        text(content.to_string())
            .size(12)
            .font(Font::MONOSPACE)
            .color(AppTheme::text_primary()),
    )
    .padding(10)
    .width(Length::Fill)
    .style(|_t: &iced::Theme| container::Style {
        background: Some(AppTheme::bg().into()),
        border: iced::Border {
            radius: 6.0.into(),
            width: 1.0,
            color: AppTheme::border_subtle(),
        },
        ..Default::default()
    })
    .into()
}

fn syntax_highlighted_block(path: &str, content: &str) -> Element<'static, Message> {
    let mut col = Column::new().spacing(0);

    for line in LinesWithEndings::from(content) {
        col = col.push(render_code_line(path, line));
    }

    if content.is_empty() {
        col = col.push(
            text("<empty>")
                .size(12)
                .font(Font::MONOSPACE)
                .color(AppTheme::text_muted()),
        );
    }

    container(col)
        .padding(10)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::bg().into()),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: AppTheme::border_subtle(),
            },
            ..Default::default()
        })
        .into()
}

fn render_code_line(path: &str, line: &str) -> Element<'static, Message> {
    let mut row = Row::new().spacing(0);

    for (segment, color, _bold, italic) in highlighted_segments(line, path) {
        let mut txt = text(segment).size(12).font(Font::MONOSPACE).color(color);
        if italic {
            txt = txt.shaping(iced::widget::text::Shaping::Advanced);
        }
        row = row.push(txt);
    }

    row.into()
}

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

    match highlighter.highlight_line(line, syntax_set()) {
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
        Err(_) => segments.push((line.to_string(), AppTheme::text_primary(), false, false)),
    }

    if segments.is_empty() {
        segments.push((line.to_string(), AppTheme::text_primary(), false, false));
    }

    segments
}
