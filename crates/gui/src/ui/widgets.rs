use iced::widget::{container, text, Column};
use iced::{Color, Element, Length};

use crate::app::Message;

/// A labeled form row: label on the left, field on the right.
pub fn labeled_field<'a>(
    label: &str,
    field: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Column::new()
        .spacing(4)
        .push(
            text(label.to_string())
                .size(12)
                .color(Color::from_rgb(0.55, 0.55, 0.6)),
        )
        .push(field)
        .into()
}

/// A card-style section with title and content inside a rounded container.
pub fn card<'a>(
    title: &str,
    content: Column<'a, Message>,
) -> Element<'a, Message> {
    let inner = Column::new()
        .spacing(16)
        .push(
            text(title.to_string())
                .size(15)
                .color(Color::from_rgb(0.75, 0.75, 0.82)),
        )
        .push(content);

    container(inner)
        .padding(20)
        .width(Length::Fill)
        .style(|_t: &iced::Theme| container::Style {
            background: Some(Color::from_rgb(0.14, 0.14, 0.18).into()),
            border: iced::Border {
                radius: 10.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.22, 0.22, 0.27),
            },
            ..Default::default()
        })
        .into()
}
