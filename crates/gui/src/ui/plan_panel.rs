use iced::widget::{button, column, container, markdown, row, scrollable, text, Column};
use iced::{Color, Element, Length};

use freako_core::agent::events::TaskStatus;
use crate::app::{App, Message};
use crate::ui::theme::AppTheme;

fn md_settings() -> markdown::Settings {
    markdown::Settings::with_text_size(
        13,
        markdown::Style::from_palette(iced::Theme::Dark.palette()),
    )
}

fn status_indicator(status: &TaskStatus) -> Element<'static, Message> {
    match status {
        TaskStatus::NotStarted => text("\u{2014}") // em-dash
            .size(14)
            .color(AppTheme::text_muted())
            .into(),
        TaskStatus::InProgress => text("\u{25CF}") // filled circle
            .size(12)
            .color(AppTheme::tool_active())
            .into(),
        TaskStatus::Done => text("\u{2713}") // checkmark
            .size(14)
            .color(AppTheme::success())
            .into(),
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let is_review = app.plan_pending_review;
    let is_execute = !app.config.plan_mode && !app.plan_pending_review && !app.plan_tasks.is_empty();

    // Header
    let header = row![
        text("Plan").size(14).color(AppTheme::plan_accent()),
        iced::widget::space::horizontal().width(Length::Fill),
        button(text("\u{2715}").size(12).color(AppTheme::text_muted()))
            .on_press(Message::TogglePlanPanel)
            .padding([4, 8])
            .style(|_t: &iced::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => AppTheme::sidebar_item_hover(),
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
    ]
    .align_y(iced::Alignment::Center)
    .padding([12, 16]);

    // Task list
    let mut task_list = Column::new().spacing(2).padding([0, 8]);

    for task in &app.plan_tasks {
        let is_expanded = app.plan_task_expanded.contains(&task.id);
        let chevron = if is_expanded { "\u{25BC}" } else { "\u{25B6}" }; // down / right triangle
        let task_id = task.id.clone();

        let mut header_row = row![
            text(chevron).size(10).color(AppTheme::text_muted()),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);

        // Show status indicator in execute mode
        if is_execute {
            header_row = header_row.push(status_indicator(&task.status));
        }

        header_row = header_row.push(
            text(task.header.clone())
                .size(13)
                .color(AppTheme::text_primary()),
        );

        let header_btn = button(header_row)
            .on_press(Message::TogglePlanTaskExpanded(task_id))
            .padding([6, 8])
            .width(Length::Fill)
            .style(|_t: &iced::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => AppTheme::sidebar_item_hover(),
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            });

        task_list = task_list.push(header_btn);

        if is_expanded {
            if let Some(md_content) = app.plan_task_md_cache.get(&task.id) {
                let md_el: Element<'_, markdown::Uri> = markdown::view(
                    md_content.items(),
                    md_settings(),
                )
                .into();
                let desc_container = container(
                    md_el.map(|_url| Message::LinkClicked(String::new()))
                )
                .padding(iced::Padding { top: 4.0, right: 8.0, bottom: 8.0, left: 26.0 })
                .width(Length::Fill);
                task_list = task_list.push(desc_container);
            }
        }
    }

    // Footer (review mode only)
    let mut content_col = column![
        header,
        scrollable(task_list).height(Length::Fill),
    ];

    if is_review {
        let accept_btn = button(
            text("Accept Plan").size(13).color(Color::WHITE),
        )
        .on_press(Message::AcceptPlan)
        .padding([8, 16])
        .width(Length::Fill)
        .style(|_t: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => AppTheme::plan_accent_hover(),
                _ => AppTheme::plan_accent(),
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                text_color: Color::BLACK,
                ..Default::default()
            }
        });

        content_col = content_col.push(
            container(accept_btn).padding([8, 16]),
        );
    }

    container(content_col)
        .width(Length::Fixed(300.0))
        .height(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(AppTheme::sidebar_bg().into()),
            border: iced::Border {
                width: 1.0,
                color: AppTheme::border_subtle(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
