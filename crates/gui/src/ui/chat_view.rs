use std::collections::HashMap;

use iced::widget::{container, markdown, mouse_area, text, Column};
use iced::widget::scrollable;
use iced::{Element, Length, Task};

use freako_core::session::types::{ConversationMessage, MessagePart, Role};
use freako_core::tools::tool_name::format_tool_presentation;
use crate::app::{App, GroupedMessage, Message};
use crate::ui::theme::AppTheme;
use crate::ui::tool_view::ToolResultOwned;


/// Braille spinner frames — same set as status_bar.
const SPINNER_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠇"];

fn md_settings() -> markdown::Settings {
    markdown::Settings::with_text_size(
        14,
        markdown::Style::from_palette(iced::Theme::Dark.palette()),
    )
}

pub fn chat_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("chat-scroll")
}

pub fn scroll_to_bottom() -> Task<Message> {
    // Chain two snap_to_end calls: the first fires immediately, the second
    // fires after the next layout pass to catch any content height changes.
    iced::widget::operation::snap_to_end(chat_scroll_id())
        .chain(iced::widget::operation::snap_to_end(chat_scroll_id()))
}

/// Build the full copyable text for a message, including tool parts.
pub fn full_message_text(msg: &ConversationMessage) -> String {
    let mut out = String::new();
    for part in &msg.parts {
        match part {
            MessagePart::Text { text } => out.push_str(text),
            MessagePart::ToolCall { name, arguments, .. } => {
                let (title, summary) = tool_presentation(name, arguments);
                out.push_str(&format!("[Calling {}] {}\n", title, summary));
            }
            MessagePart::ToolResult { name, content, is_error, arguments, .. } => {
                let prefix = if *is_error { "Error" } else { "Result" };
                let (title, summary) = arguments
                    .as_ref()
                    .map(|args| tool_presentation(name, args))
                    .unwrap_or_else(|| (name.clone(), String::new()));
                let content_preview = if *name == "read_file" {
                    "(collapsed by default)".to_string()
                } else {
                    truncate_middle(content, 240)
                };
                out.push_str(&format!("[{} {}] {} {}\n", title, prefix, summary, content_preview));
            }
            MessagePart::Image { media_type, .. } => {
                out.push_str(&format!("[image: {}]\n", media_type));
            }
            MessagePart::ToolOutput { stream, content, .. } => {
                let label = match stream {
                    freako_core::agent::events::ToolOutputStream::Stdout => "stdout",
                    freako_core::agent::events::ToolOutputStream::Stderr => "stderr",
                };
                out.push_str(&format!("[tool output:{}] {}\n", label, truncate_middle(content, 240)));
            }
        }
    }
    out
}

pub fn view(app: &App) -> Element<'_, Message> {
    let mut chat_col = Column::new().spacing(16).padding(24);

    if app.session.messages.is_empty() && app.streaming_text.is_empty() {
        chat_col = chat_col.push(
            container(
                Column::new()
                    .spacing(8)
                    .align_x(iced::Alignment::Center)
                    .push(text("freako").size(32).color(crate::ui::theme::AppTheme::text_secondary()))
                    .push(text("Type a message to get started").size(14).color(crate::ui::theme::AppTheme::text_muted()))
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding([120, 0]),
        );
    }

    if app.visible_count < app.session.messages.len() {
        let hidden = app.session.messages.len() - app.visible_count;
        chat_col = chat_col.push(
            container(
                text(format!("Scroll up to load {} earlier messages...", hidden))
                    .size(12)
                    .color(crate::ui::theme::AppTheme::text_muted()),
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding([8, 0]),
        );
    }

    for (i, group) in app.grouped_messages.iter().enumerate() {
        let md_sections = app.grouped_md_contents.get(i)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        chat_col = chat_col.push(message_bubble_owned(app, group, md_sections));
    }

    if !app.streaming_text.is_empty() {
        use crate::ui::theme::AppTheme;
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        let md_el: Element<'_, markdown::Uri> = markdown::view(
            app.streaming_content.items(),
            md_settings(),
        )
        .into();

        let bubble = container(
            Column::new()
                .spacing(4)
                .push(
                    text(spinner)
                        .size(11)
                        .color(AppTheme::thinking()),
                )
                .push(md_el.map(|_url| Message::LinkClicked(String::new())))
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::assistant_bubble().into()),
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.0,
                color: AppTheme::assistant_bubble_border(),
            },
            ..Default::default()
        });
        chat_col = chat_col.push(bubble);
    }

    if app.is_thinking && app.streaming_text.is_empty() {
        use crate::ui::theme::AppTheme;
        let spinner = SPINNER_FRAMES[app.spinner_tick as usize];
        let bubble = container(
            Column::new()
                .spacing(4)
                .push(
                    text(format!("{} Thinking…", spinner))
                        .size(12)
                        .color(AppTheme::thinking()),
                )
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::assistant_bubble().into()),
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.0,
                color: AppTheme::thinking(),
            },
            ..Default::default()
        });
        chat_col = chat_col.push(bubble);
    }

    if app.current_tool.is_some() || !app.tool_output_buffer.is_empty() {
        use crate::ui::theme::AppTheme;
        let bubble = container(
            crate::ui::tool_view::streaming_tool_group_view(
                &app.streaming_tool_calls,
                app.current_tool.as_deref(),
                &app.tool_output_buffer,
                SPINNER_FRAMES[app.spinner_tick as usize],
            )
        )
        .padding(16)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::tool_bubble().into()),
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.0,
                color: AppTheme::tool_bubble_border(),
            },
            ..Default::default()
        });
        chat_col = chat_col.push(bubble);
    }

    // Add bottom spacer so the last message isn't cut off at the edge
    chat_col = chat_col.push(container(text("")).height(60));

    let scroller = scrollable(chat_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .on_scroll(Message::ChatScrolled)
        .id(chat_scroll_id());

    let chat_area = mouse_area(
        container(scroller)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_t: &iced::Theme| container::Style {
                background: Some(AppTheme::chat_bg().into()),
                ..Default::default()
            }),
    )
    .on_press(Message::ClearSelection);

    chat_area.into()
}

fn message_bubble_owned<'a>(
    app: &'a App,
    group: &'a GroupedMessage,
    md_sections: &'a [markdown::Content],
) -> Element<'a, Message> {
    use crate::ui::theme::AppTheme;
    let msg = &group.message;
    let (bg, border_color, role_label, role_color) = match msg.role {
        Role::User => (
            AppTheme::user_bubble(),
            AppTheme::user_bubble_border(),
            "You",
            AppTheme::role_user(),
        ),
        _ => (
            AppTheme::assistant_bubble(),
            AppTheme::assistant_bubble_border(),
            if msg.role == Role::System { "System" } else { "Assistant" },
            AppTheme::role_assistant(),
        ),
    };

    let mut content_col = Column::new().spacing(4);

    content_col = content_col.push(
        text(role_label).size(11).color(role_color),
    );

    // Walk parts in order, rendering text sections and tool groups inline.
    // We collect tool results into a map first so we can pair them with calls.
    let mut results_map: HashMap<String, ToolResultOwned> = HashMap::new();
    for part in &msg.parts {
        match part {
            MessagePart::ToolResult { tool_call_id, content, is_error, arguments, .. } => {
                results_map.insert(
                    tool_call_id.clone(),
                    ToolResultOwned {
                        content: content.clone(),
                        is_error: *is_error,
                        arguments: arguments.clone(),
                        output: results_map.get(tool_call_id).and_then(|existing| existing.output.clone()),
                    },
                );
            }
            MessagePart::ToolOutput { tool_call_id, content, .. } => {
                let entry = results_map.entry(tool_call_id.clone()).or_insert_with(|| ToolResultOwned {
                    content: String::new(),
                    is_error: false,
                    arguments: None,
                    output: None,
                });
                match &mut entry.output {
                    Some(output) => output.push_str(content),
                    None => entry.output = Some(content.clone()),
                }
            }
            _ => {}
        }
    }

    let mut text_idx = 0;
    let mut pending_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

    for part in &msg.parts {
        match part {
            MessagePart::Text { text: t } => {
                // Flush any pending tool calls before this text section
                if !pending_calls.is_empty() {
                    let calls = std::mem::take(&mut pending_calls);
                    content_col = content_col.push(
                        crate::ui::tool_view::tool_group_view_owned(app, calls, &results_map)
                    );
                }
                // Render the text section
                if !t.is_empty() {
                    if let Some(md_content) = md_sections.get(text_idx) {
                        if !md_content.items().is_empty() {
                            let sel_md = iced_selectable_markdown::view(
                                md_content.items(),
                                md_settings(),
                                &app.md_selection,
                                Message::MdSelection,
                                |_uri| Message::LinkClicked(String::new()),
                            );
                            content_col = content_col.push(sel_md);
                        } else {
                            content_col = content_col.push(text(t.as_str()).size(14));
                        }
                    } else {
                        content_col = content_col.push(text(t.as_str()).size(14));
                    }
                    text_idx += 1;
                }
            }
            MessagePart::ToolCall { id, name, arguments } => {
                pending_calls.push((id.clone(), name.clone(), arguments.clone()));
            }
            MessagePart::ToolResult { .. } => {
                // Results are already in results_map, handled by tool_group_view
            }
            MessagePart::Image { media_type, .. } => {
                // Flush pending tool calls
                if !pending_calls.is_empty() {
                    let calls = std::mem::take(&mut pending_calls);
                    content_col = content_col.push(
                        crate::ui::tool_view::tool_group_view_owned(app, calls, &results_map)
                    );
                }
                content_col = content_col.push(
                    text(format!("[📎 image: {}]", media_type)).size(12).color(AppTheme::text_muted())
                );
            }
            MessagePart::ToolOutput { .. } => {
                // Tool output is rendered via the paired tool result/group view.
            }
        }
    }

    // Flush any remaining tool calls at the end
    if !pending_calls.is_empty() {
        content_col = content_col.push(
            crate::ui::tool_view::tool_group_view_owned(app, pending_calls, &results_map)
        );
    }

    let bubble = container(content_col)
        .padding(16)
        .width(Length::Fill)
        .style(move |_t: &iced::Theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.0,
                color: border_color,
            },
            ..Default::default()
        });

    bubble.into()
}



fn tool_presentation(name: &str, arguments: &serde_json::Value) -> (String, String) {
    format_tool_presentation(name, arguments)
        .map(|(title, summary)| (title.into_owned(), summary))
        .unwrap_or_else(|| {
            let fallback = serde_json::to_string(arguments)
                .map(|s| truncate_middle(&s, 120))
                .unwrap_or_default();
            (name.to_string(), fallback)
        })
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
