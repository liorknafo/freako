use std::collections::HashMap;

use iced::widget::{button, container, mouse_area, text, Column, Row};
use iced::{Color, Element, Length};
use serde_json::Value;

use freako_core::tools::tool_name::format_tool_presentation;

use crate::app::{App, Message};
use crate::ui::theme::AppTheme;

#[derive(Clone)]
pub struct ToolResultOwned {
    pub content: String,
    pub is_error: bool,
    pub arguments: Option<Value>,
    pub output: Option<String>,
}

pub fn tool_group_view_owned(
    app: &App,
    calls: Vec<(String, String, Value)>,
    results: &HashMap<String, ToolResultOwned>,
) -> Element<'static, Message> {
    let mut col = Column::new().spacing(4);

    let title = if calls.len() == 1 {
        format!("{} tool call", calls.len())
    } else {
        format!("{} tool calls", calls.len())
    };
    col = col.push(text(title).size(11).color(AppTheme::text_secondary()));

    for (tool_call_id, name, arguments) in calls {
        let result = results.get(&tool_call_id).cloned();
        let expanded = app.expanded_tool_results.contains(&tool_call_id);
        let parsed_diff = app.parsed_diffs.get(&tool_call_id).cloned();
        col = col.push(tool_entry_view_owned(tool_call_id, name, arguments, result, expanded, parsed_diff));
    }

    container(col)
        .padding(10)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::tool_bubble().into()),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: AppTheme::tool_bubble_border(),
            },
            ..Default::default()
        })
        .into()
}

pub fn streaming_tool_group_view<'a>(
    calls: &'a [(String, String, Value)],
    current_tool: Option<&'a str>,
    tool_output_buffer: &'a str,
    spinner: &'a str,
) -> Element<'a, Message> {
    let mut col = Column::new().spacing(8);
    col = col.push(text(format!("{} Tools", spinner)).size(12).color(AppTheme::tool_active()));

    if calls.is_empty() {
        let label = current_tool.unwrap_or("Tool");
        let body = if tool_output_buffer.is_empty() {
            format!("Running {}…", label)
        } else {
            tool_output_buffer.to_string()
        };
        col = col.push(compact_row(label, body, AppTheme::text_primary()));
    } else {
        for (_, name, arguments) in calls {
            let status = if current_tool == Some(name.as_str()) {
                if tool_output_buffer.is_empty() {
                    "running…".to_string()
                } else {
                    tool_output_buffer.to_string()
                }
            } else {
                "queued…".to_string()
            };
            let (title, summary) = tool_presentation(name, arguments);
            let details = format!("{} — {}", summary, status);
            col = col.push(compact_row(&title, details, AppTheme::text_primary()));
        }
    }

    col.into()
}

fn tool_entry_view_owned(
    tool_call_id: String,
    name: String,
    arguments: Value,
    result: Option<ToolResultOwned>,
    expanded: bool,
    parsed_diff: Option<crate::ui::diff_view::ParsedDiff>,
) -> Element<'static, Message> {
    let mut col = Column::new().spacing(4);

    let (status_text, status_color) = match &result {
        Some(r) if r.is_error => ("✗ error", AppTheme::error()),
        Some(_) => ("✓ done", AppTheme::success()),
        None => ("… pending", AppTheme::text_secondary()),
    };

    let (title, summary) = tool_presentation(&name, &arguments);
    // Header row: icon + name + summary + status
    let mut header = Row::new()
        .spacing(6)
        .push(text("⚙").size(12))
        .push(text(title).size(12).color(AppTheme::role_user()))
        .push(text(summary).size(11).color(AppTheme::text_muted()))
        .push(text(status_text).size(11).color(status_color));

    // Add diff summary for edit_file (e.g. "+5 / -3 lines")
    if name == "edit_file" {
        if let Some(ref diff) = parsed_diff {
            let adds = diff.lines.iter().filter(|l| matches!(l.kind, crate::ui::diff_view::ParsedLineKind::Added)).count();
            let removes = diff.lines.iter().filter(|l| matches!(l.kind, crate::ui::diff_view::ParsedLineKind::Removed)).count();
            let diff_summary = format!("+{} / -{} lines", adds, removes);
            header = header.push(
                text(diff_summary).size(11).color(AppTheme::text_secondary())
            );
        }
    }
    col = col.push(header);

    if let Some(result) = result {
        if expanded {
            let color = if result.is_error {
                AppTheme::error()
            } else {
                AppTheme::text_secondary()
            };
            let body = match name.as_str() {
                "edit_file" => {
                    if let Some(ref diff) = parsed_diff {
                        crate::ui::diff_view::render_parsed_diff(diff)
                    } else {
                        // Fallback: parse on the fly (for old sessions without cached diff)
                        let path = result
                            .arguments
                            .as_ref()
                            .and_then(|args| args.get("path").and_then(|v| v.as_str()));
                        let diff = crate::ui::diff_view::parse_diff(&result.content, path);
                        crate::ui::diff_view::render_parsed_diff(&diff)
                    }
                }
                _ => {
                    let body = if name == "shell" {
                        format_result_body(&name, result.output.as_deref().unwrap_or(&result.content), result.arguments.as_ref())
                    } else {
                        format_result_body(&name, &result.content, result.arguments.as_ref())
                    };
                    container(text(body).size(12).font(iced::Font::MONOSPACE).color(color))
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
            };
            col = col.push(mouse_area(body).on_press(Message::NoOp));
            col = col.push(
                button(text("Hide").size(10))
                    .padding([2, 8])
                    .on_press(Message::ToggleToolResult(tool_call_id))
                    .style(|_t: &iced::Theme, _s| button::Style {
                        text_color: AppTheme::text_muted(),
                        background: None,
                        ..Default::default()
                    })
            );
        } else {
            col = col.push(
                button(text("Show result").size(10))
                    .padding([2, 8])
                    .on_press(Message::ToggleToolResult(tool_call_id))
                    .style(|_t: &iced::Theme, _s| button::Style {
                        text_color: AppTheme::text_muted(),
                        background: None,
                        ..Default::default()
                    })
            );
        }
    }

    container(col)
        .padding(6)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(Color::from_rgba(0.1, 0.1, 0.14, 0.4).into()),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn compact_row(name: &str, details: String, color: Color) -> Element<'static, Message> {
    Row::new()
        .spacing(8)
        .push(text("🔧").size(14))
        .push(text(name.to_string()).size(13).color(AppTheme::role_user()))
        .push(text(details).size(11).color(color))
        .into()
}

fn tool_presentation(name: &str, arguments: &Value) -> (String, String) {
    format_tool_presentation(name, arguments)
        .map(|(title, summary)| (title.into_owned(), summary))
        .unwrap_or_else(|| {
            let fallback = serde_json::to_string(arguments)
                .map(|s| truncate_middle(&s, 120))
                .unwrap_or_default();
            (name.to_string(), fallback)
        })
}

fn format_result_body(name: &str, content: &str, arguments: Option<&Value>) -> String {
    match name {
        "read_file" => {
            let summary = arguments
                .map(|args| tool_presentation(name, args).1)
                .unwrap_or_else(|| "read_file".to_string());
            format!("{}\n\n{}", summary, truncate_middle(content, 2000))
        }
        "edit_file" | "write_file" => {
            let path = arguments
                .and_then(|args| args.get("path").and_then(|v| v.as_str()))
                .unwrap_or("?");
            format!("{}\n\n{}", path, truncate_middle(content, 4000))
        }
        "shell" => {
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() <= 8 {
                content.to_string()
            } else {
                let head: Vec<&str> = lines.iter().take(4).copied().collect();
                let tail: Vec<&str> = lines.iter().skip(lines.len() - 4).copied().collect();
                let omitted = lines.len() - 8;
                format!("{}\n… {} lines omitted …\n{}", head.join("\n"), omitted, tail.join("\n"))
            }
        }
        _ => truncate_middle(content, 4000),
    }
}

fn truncate_middle(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }

    let keep = max.saturating_sub(1) / 2;
    let start: String = s.chars().take(keep).collect();
    let end: String = s
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}…{}", start, end)
}
