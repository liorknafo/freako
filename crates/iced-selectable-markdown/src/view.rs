//! Forked markdown view functions that use SelectableRichText.

use std::cell::Cell;

use iced::widget::{
    checkbox, column, container, rich_text, row, scrollable, text,
};
use iced::widget::markdown::{Bullet, Catalog, Item, Settings, Uri};
use iced::{alignment, padding, Element, Length, Pixels};

use crate::selectable_rich::SelectableRich;
use crate::state::{SelectionAction, SelectionState};

/// Helper to get items from a Bullet (since .items() is private).
fn bullet_items(bullet: &Bullet) -> &[Item] {
    match bullet {
        Bullet::Point { items } | Bullet::Task { items, .. } => items,
    }
}

fn bullet_done(bullet: &Bullet) -> Option<bool> {
    match bullet {
        Bullet::Task { done, .. } => Some(*done),
        _ => None,
    }
}

/// Render markdown items with text selection support.
pub fn view<'a, Message: 'a>(
    items: impl IntoIterator<Item = &'a Item>,
    settings: impl Into<Settings>,
    selection: &'a SelectionState,
    on_action: impl Fn(SelectionAction) -> Message + Clone + 'a,
    on_link_click: impl Fn(Uri) -> Message + Clone + 'a,
) -> Element<'a, Message> {
    let settings = settings.into();
    let item_counter = Cell::new(0usize);

    column(
        items
            .into_iter()
            .enumerate()
            .map(|(i, content)| {
                render_item(
                    settings,
                    content,
                    i,
                    selection,
                    &on_action,
                    &on_link_click,
                    &item_counter,
                )
            }),
    )
    .spacing(settings.spacing.0)
    .into()
}

fn render_item<'a, Message: 'a>(
    settings: Settings,
    item: &'a Item,
    index: usize,
    selection: &'a SelectionState,
    on_action: &(impl Fn(SelectionAction) -> Message + Clone + 'a),
    on_link_click: &(impl Fn(Uri) -> Message + Clone + 'a),
    item_counter: &Cell<usize>,
) -> Element<'a, Message> {
    let item_idx = item_counter.get();
    item_counter.set(item_idx + 1);

    match item {
        Item::Heading(level, md_text) => {
            let size = match level {
                pulldown_cmark::HeadingLevel::H1 => settings.h1_size,
                pulldown_cmark::HeadingLevel::H2 => settings.h2_size,
                pulldown_cmark::HeadingLevel::H3 => settings.h3_size,
                pulldown_cmark::HeadingLevel::H4 => settings.h4_size,
                pulldown_cmark::HeadingLevel::H5 => settings.h5_size,
                pulldown_cmark::HeadingLevel::H6 => settings.h6_size,
            };

            container(
                SelectableRich::new(md_text.spans(settings.style), item_idx, selection, on_action.clone())
                    .on_link_click(on_link_click.clone())
                    .size(size)
            )
            .padding(padding::top(if index > 0 {
                settings.text_size / 2.0
            } else {
                Pixels::ZERO
            }))
            .into()
        }
        Item::Paragraph(md_text) => {
            SelectableRich::new(md_text.spans(settings.style), item_idx, selection, on_action.clone())
                .size(settings.text_size)
                .on_link_click(on_link_click.clone())
                .into()
        }
        Item::CodeBlock { lines, .. } => {
            container(
                scrollable(
                    container(column(lines.iter().map(|line| {
                        SelectableRich::new(
                            line.spans(settings.style),
                            item_idx,
                            selection,
                            on_action.clone(),
                        )
                        .on_link_click(on_link_click.clone())
                        .font(settings.style.code_block_font)
                        .size(settings.code_size)
                        .into()
                    })))
                    .padding(settings.code_size),
                )
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default()
                        .width(settings.code_size / 2)
                        .scroller_width(settings.code_size / 2),
                )),
            )
            .width(Length::Fill)
            .padding(settings.code_size / 4)
            .class(iced::Theme::code_block())
            .into()
        }
        Item::List { start, bullets } => {
            if let Some(start_num) = start {
                let digits = ((*start_num + bullets.len() as u64).max(1) as f32).log10().ceil();

                column(bullets.iter().enumerate().map(|(i, bullet)| {
                    row![
                        text!("{}.", i as u64 + start_num)
                            .size(settings.text_size)
                            .align_x(alignment::Horizontal::Right)
                            .width(settings.text_size * ((digits / 2.0).ceil() + 1.0)),
                        view_bullet_items(
                            bullet_items(bullet),
                            Settings {
                                spacing: settings.spacing * 0.6,
                                ..settings
                            },
                            selection,
                            on_action,
                            on_link_click,
                            item_counter,
                        )
                    ]
                    .spacing(settings.spacing)
                    .into()
                }))
                .spacing(settings.spacing * 0.75)
                .into()
            } else {
                column(bullets.iter().map(|bullet| {
                    row![
                        if let Some(done) = bullet_done(bullet) {
                            Element::from(
                                container(checkbox(done).size(settings.text_size))
                                    .center_y(
                                        iced::widget::text::LineHeight::default()
                                            .to_absolute(settings.text_size),
                                    ),
                            )
                        } else {
                            text("•").size(settings.text_size).into()
                        },
                        view_bullet_items(
                            bullet_items(bullet),
                            Settings {
                                spacing: settings.spacing * 0.6,
                                ..settings
                            },
                            selection,
                            on_action,
                            on_link_click,
                            item_counter,
                        )
                    ]
                    .spacing(settings.spacing)
                    .into()
                }))
                .spacing(settings.spacing * 0.75)
                .padding([0.0, settings.spacing.0])
                .into()
            }
        }
        Item::Quote(contents) => {
            row![
                iced::widget::rule::vertical(4),
                column(
                    contents
                        .iter()
                        .enumerate()
                        .map(|(i, content)| render_item(
                            settings,
                            content,
                            i,
                            selection,
                            on_action,
                            on_link_click,
                            item_counter,
                        )),
                )
                .spacing(settings.spacing.0),
            ]
            .height(Length::Shrink)
            .spacing(settings.spacing)
            .into()
        }
        Item::Rule => {
            iced::widget::rule::horizontal(2).into()
        }
        Item::Image { alt, .. } => {
            container(
                rich_text(alt.spans(settings.style))
                    .on_link_click(on_link_click.clone()),
            )
            .padding(settings.spacing)
            .class(iced::Theme::code_block())
            .into()
        }
        Item::Table { .. } => {
            text("(table)").into()
        }
    }
}

fn view_bullet_items<'a, Message: 'a>(
    items: &'a [Item],
    settings: Settings,
    selection: &'a SelectionState,
    on_action: &(impl Fn(SelectionAction) -> Message + Clone + 'a),
    on_link_click: &(impl Fn(Uri) -> Message + Clone + 'a),
    item_counter: &Cell<usize>,
) -> Element<'a, Message> {
    column(
        items
            .iter()
            .enumerate()
            .map(|(i, content)| {
                render_item(settings, content, i, selection, on_action, on_link_click, item_counter)
            }),
    )
    .spacing(settings.spacing.0)
    .into()
}
