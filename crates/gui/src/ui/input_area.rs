use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Color, Element, Length};

use crate::app::{App, Message};
use crate::ui::theme::AppTheme;

pub fn view(app: &App) -> Element<'_, Message> {
    let provider_configured = app.config.is_provider_configured();
    let has_text = !app.input_text.trim().is_empty();

    // While working: allow queueing a message (but not if one is already queued)
    let can_queue = app.is_working && provider_configured && has_text && app.queued_message.is_none();
    let can_accept_plan = !app.is_working && app.plan_pending_review && app.input_text.trim().is_empty();
    // Normal send when not working
    let can_send = (!app.is_working && provider_configured && has_text) || can_accept_plan;

    let placeholder = if !provider_configured {
        "Configure a provider in Settings..."
    } else if app.plan_pending_review {
        "Press Enter to accept the plan, or type feedback..."
    } else if app.is_working {
        if app.queued_message.is_some() {
            "Message queued — waiting for agent…"
        } else {
            "Type to queue a message…"
        }
    } else {
        "Type a message… (Shift+Enter for new line)"
    };

    let editor = text_editor(&app.input_content)
        .placeholder(placeholder)
        .on_action(Message::InputAction)
        .padding(10)
        .size(14)
        .height(Length::Shrink)
        .style(|theme: &iced::Theme, _status| {
            text_editor::Style {
                background: AppTheme::input_bg().into(),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: AppTheme::border_default(),
                },
                placeholder: AppTheme::text_muted(),
                value: theme.palette().text,
                selection: AppTheme::border_focus(),
            }
        });

    let action_btn = if app.is_working {
        button(
            text("Stop").size(13).color(Color::WHITE),
        )
        .padding([10, 20])
        .on_press(Message::StopAgent)
        .style(move |_t: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => Color::from_rgb(0.9, 0.3, 0.3),
                _ => AppTheme::error(),
            };
            button::Style {
                background: Some(bg.into()),
                text_color: Color::WHITE,
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
    } else {
        let send_btn = button(
            text("Send").size(13).color(Color::WHITE),
        )
        .padding([10, 20])
        .style(move |_t: &iced::Theme, status| {
            let bg = if !can_send {
                AppTheme::border_default()
            } else {
                match status {
                    button::Status::Hovered => AppTheme::accent_hover(),
                    _ => AppTheme::accent(),
                }
            };
            button::Style {
                background: Some(bg.into()),
                text_color: if can_send {
                    Color::WHITE
                } else {
                    AppTheme::text_secondary()
                },
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });

        if can_send {
            send_btn.on_press(Message::SendMessage)
        } else {
            send_btn
        }
    };

    // When working and can queue, show a "Queue" button alongside Stop
    let input_row: Element<'_, Message> = if app.is_working && app.queued_message.is_none() {
        let queue_btn = button(
            text("Queue").size(13).color(Color::WHITE),
        )
        .padding([10, 20])
        .style(move |_t: &iced::Theme, status| {
            let bg = if !can_queue {
                AppTheme::border_default()
            } else {
                match status {
                    button::Status::Hovered => Color::from_rgb(0.3, 0.65, 0.45),
                    _ => AppTheme::success(),
                }
            };
            button::Style {
                background: Some(bg.into()),
                text_color: if can_queue {
                    Color::WHITE
                } else {
                    AppTheme::text_secondary()
                },
                border: iced::Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        });

        let queue_btn = if can_queue {
            queue_btn.on_press(Message::QueueMessage)
        } else {
            queue_btn
        };

        row![editor, queue_btn, action_btn]
            .spacing(10)
            .align_y(iced::Alignment::End)
            .into()
    } else {
        row![editor, action_btn]
            .spacing(10)
            .align_y(iced::Alignment::End)
            .into()
    };

    let mode_hint = if app.plan_pending_review {
        "Plan ready — press Enter to accept, or type feedback to improve it"
    } else if app.config.plan_mode {
        "Plan mode active — research only. Click the status-bar toggle to return to Execute mode"
    } else {
        "Execute mode active — tools can modify files and run commands with approval"
    };

    // Queued message banner
    let mut col = column![
        input_row,
        text(mode_hint)
            .size(12)
            .color(AppTheme::role_assistant())
    ]
    .spacing(6);

    if let Some(queued) = &app.queued_message {
        let preview: String = queued.chars().take(80).collect();
        let suffix = if queued.len() > 80 { "…" } else { "" };
        let banner = container(
            text(format!("⏳ Queued: \"{}{}\"\u{200b}", preview, suffix))
                .size(12)
                .color(AppTheme::warning()),
        )
        .padding([4, 12])
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::plan_bg().into()),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: AppTheme::plan_accent(),
            },
            ..Default::default()
        });
        col = col.push(banner);
    }

    container(col)
        .padding(16)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::input_bg().into()),
            border: iced::Border {
                width: 1.0,
                color: AppTheme::border_subtle(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
