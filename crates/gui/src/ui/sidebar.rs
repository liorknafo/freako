use iced::widget::{button, column, container, row, scrollable, text, Column};
use iced::{Color, Element, Length};

use crate::app::{Message, SessionEntry};
use crate::ui::theme::AppTheme;

pub fn view<'a>(sessions: &'a [SessionEntry], current_id: String) -> Element<'a, Message> {
    let mut list = Column::new().spacing(4).padding([0, 8]);

    let header = container(
        column![
            text("Sessions").size(14).color(AppTheme::text_secondary()),
            button(
                text("+ New Session").size(13).color(AppTheme::accent_hover()),
            )
            .on_press(Message::NewSession)
            .padding([8, 12])
            .width(Length::Fill)
            .style(|_t: &iced::Theme, status| {
                let bg = match status {
                    button::Status::Hovered => AppTheme::sidebar_item_hover(),
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 6.0.into(),
                        width: 1.0,
                        color: AppTheme::border_default(),
                    },
                    ..Default::default()
                }
            })
        ]
        .spacing(10),
    )
    .padding([12, 16]);

    if sessions.is_empty() {
        list = list.push(
            container(
                text("No sessions yet").size(12).color(AppTheme::text_muted()),
            )
            .padding([12, 8]),
        );
    }

    for entry in sessions {
        let is_current = entry.id == current_id.as_str();
        let id = entry.id.clone();

        let bg = if is_current {
            AppTheme::sidebar_item_selected()
        } else {
            Color::TRANSPARENT
        };
        let border_color = if is_current {
            AppTheme::sidebar_item_selected_border()
        } else {
            Color::TRANSPARENT
        };

        let title_text = if entry.title.is_empty() { "Untitled" } else { &entry.title };
        let delete_id = entry.id.clone();

        let entry_btn = button(
            column![
                text(title_text.to_string()).size(13).color(AppTheme::text_primary()),
                text(&entry.updated_at).size(10).color(AppTheme::text_muted()),
            ]
            .spacing(2),
        )
        .on_press(Message::LoadSession(id))
        .padding([8, 10])
        .width(Length::Fill)
        .style(move |_t: &iced::Theme, status| {
            let hover_bg = match status {
                button::Status::Hovered if !is_current => AppTheme::sidebar_item_hover(),
                _ => bg,
            };
            button::Style {
                background: Some(hover_bg.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    width: if is_current { 1.0 } else { 0.0 },
                    color: border_color,
                },
                ..Default::default()
            }
        });

        let delete_btn = button(
            text("\u{1F5D1}").size(13),
        )
        .on_press(Message::DeleteSession(delete_id))
        .padding([4, 6])
        .style(|_t: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered => AppTheme::error(),
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

        list = list.push(
            row![entry_btn, delete_btn]
                .spacing(2)
                .align_y(iced::Alignment::Center),
        );
    }

    let content = column![
        header,
        scrollable(list).height(Length::Fill),
    ];

    container(content)
        .width(Length::Fixed(240.0))
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
