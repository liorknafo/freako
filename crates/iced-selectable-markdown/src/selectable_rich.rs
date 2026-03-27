//! A selectable rich text widget — forked from iced's Rich widget with added
//! mouse selection and highlight rendering.

use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::{self, Paragraph, Span};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::widget::text::{Catalog, LineHeight, Shaping, Wrapping};
use iced::advanced::{Clipboard, Layout, Renderer as AdvancedRenderer, Shell, Widget};
use iced::widget::markdown;
use iced::{
    alignment, Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Vector,
};

// Re-import traits needed for method resolution
use iced::advanced::text::Renderer as TextRendererTrait;

use crate::state::{SelectionAction, SelectionState};

type IRenderer = iced::Renderer;
type ITheme = iced::Theme;

/// A rich text widget with text selection support.
pub struct SelectableRich<'a, Message> {
    spans: Box<dyn AsRef<[Span<'a, markdown::Uri, iced::Font>]> + 'a>,
    size: Option<Pixels>,
    line_height: LineHeight,
    width: Length,
    height: Length,
    font: Option<iced::Font>,
    wrapping: Wrapping,
    item_index: usize,
    selection: &'a SelectionState,
    on_action: Box<dyn Fn(SelectionAction) -> Message + 'a>,
    on_link_click: Option<Box<dyn Fn(markdown::Uri) -> Message + 'a>>,
    hovered_link: Option<usize>,
    selection_color: Color,
}

impl<'a, Message> SelectableRich<'a, Message> {
    pub fn new(
        spans: impl AsRef<[Span<'a, markdown::Uri, iced::Font>]> + 'a,
        item_index: usize,
        selection: &'a SelectionState,
        on_action: impl Fn(SelectionAction) -> Message + 'a,
    ) -> Self {
        Self {
            spans: Box::new(spans),
            size: None,
            line_height: LineHeight::default(),
            width: Length::Shrink,
            height: Length::Shrink,
            font: None,
            wrapping: Wrapping::default(),
            item_index,
            selection,
            on_action: Box::new(on_action),
            on_link_click: None,
            hovered_link: None,
            selection_color: Color::from_rgba(0.25, 0.45, 0.75, 0.35),
        }
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }

    pub fn font(mut self, font: impl Into<iced::Font>) -> Self {
        self.font = Some(font.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn wrapping(mut self, wrapping: Wrapping) -> Self {
        self.wrapping = wrapping;
        self
    }

    pub fn on_link_click(mut self, f: impl Fn(markdown::Uri) -> Message + 'a) -> Self {
        self.on_link_click = Some(Box::new(f));
        self
    }
}

/// Internal widget state.
struct State {
    spans: Vec<Span<'static, markdown::Uri, iced::Font>>,
    span_pressed: Option<usize>,
    paragraph: <IRenderer as iced::advanced::text::Renderer>::Paragraph,
}

impl<Message> Widget<Message, ITheme, IRenderer> for SelectableRich<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            spans: Vec::new(),
            span_pressed: None,
            paragraph: <<IRenderer as iced::advanced::text::Renderer>::Paragraph as Default>::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &IRenderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        let spans = self.spans.as_ref().as_ref();
        let size = self.size.unwrap_or_else(|| renderer.default_size());
        let font = self.font.unwrap_or_else(|| renderer.default_font());

        layout::sized(limits, self.width, self.height, |limits| {
            let bounds = limits.max();

            let text_with_spans = || iced::advanced::Text {
                content: spans,
                bounds,
                size,
                line_height: self.line_height,
                font,
                align_x: text::Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: self.wrapping,
            };

            if state.spans != spans {
                state.paragraph = Paragraph::with_spans(text_with_spans());
                state.spans = spans.iter().cloned().map(Span::to_static).collect();
            } else {
                match state.paragraph.compare(iced::advanced::Text {
                    content: (),
                    bounds,
                    size,
                    line_height: self.line_height,
                    font,
                    align_x: text::Alignment::Default,
                    align_y: alignment::Vertical::Top,
                    shaping: Shaping::Advanced,
                    wrapping: self.wrapping,
                }) {
                    iced::advanced::text::Difference::None => {}
                    iced::advanced::text::Difference::Bounds => {
                        state.paragraph.resize(bounds);
                    }
                    iced::advanced::text::Difference::Shape => {
                        state.paragraph = Paragraph::with_spans(text_with_spans());
                    }
                }
            }

            // Register the plain text with the selection state so it can be
            // used for clipboard copy and word boundary detection.
            let plain: String = spans.iter()
                .filter_map(|s| {
                    let t = s.text.as_ref();
                    if t.is_empty() { None } else { Some(t) }
                })
                .collect::<Vec<_>>()
                .join("");
            self.selection.register_item_text(self.item_index, plain);

            state.paragraph.min_bounds()
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut IRenderer,
        theme: &ITheme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }

        let state = tree.state.downcast_ref::<State>();
        let style = theme.style(&<ITheme as Catalog>::default());

        // Draw span decorations (highlights, underlines, strikethroughs)
        let spans = self.spans.as_ref().as_ref();
        for (index, span) in spans.iter().enumerate() {
            let is_hovered_link = self.on_link_click.is_some()
                && Some(index) == self.hovered_link;

            if span.highlight.is_some()
                || span.underline
                || span.strikethrough
                || is_hovered_link
            {
                let translation = layout.position() - Point::ORIGIN;
                let regions = state.paragraph.span_bounds(index);

                if let Some(highlight) = span.highlight {
                    for bounds in &regions {
                        let bounds = Rectangle::new(
                            bounds.position()
                                - Vector::new(span.padding.left, span.padding.top),
                            bounds.size()
                                + Size::new(span.padding.x(), span.padding.y()),
                        );
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: bounds + translation,
                                border: highlight.border,
                                ..Default::default()
                            },
                            highlight.background,
                        );
                    }
                }

                if span.underline || span.strikethrough || is_hovered_link {
                    let sz = span.size.or(self.size).unwrap_or(renderer.default_size());
                    let lh = span.line_height.unwrap_or(self.line_height).to_absolute(sz);
                    let color = span.color.or(style.color).unwrap_or(defaults.text_color);
                    let baseline = translation + Vector::new(0.0, sz.0 + (lh.0 - sz.0) / 2.0);

                    if span.underline || is_hovered_link {
                        for bounds in &regions {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        bounds.position() + baseline - Vector::new(0.0, sz.0 * 0.08),
                                        Size::new(bounds.width, 1.0),
                                    ),
                                    ..Default::default()
                                },
                                color,
                            );
                        }
                    }
                    if span.strikethrough {
                        for bounds in &regions {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        bounds.position() + baseline - Vector::new(0.0, sz.0 / 2.0),
                                        Size::new(bounds.width, 1.0),
                                    ),
                                    ..Default::default()
                                },
                                color,
                            );
                        }
                    }
                }
            }
        }

        // Draw selection highlight BEFORE text so text renders on top
        if let Some(sel_range) = self.selection.item_selection(self.item_index) {
            let translation = layout.position() - Point::ORIGIN;
            let bounds = layout.bounds();
            let sz = self.size.unwrap_or(renderer.default_size());
            let lh = self.line_height.to_absolute(sz);

            let start_pos = state.paragraph.grapheme_position(0, sel_range.start);
            let end_pos = state.paragraph.grapheme_position(0, sel_range.end);

            match (start_pos, end_pos) {
                (Some(start), Some(end)) => {
                    if (start.y - end.y).abs() < 1.0 {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(start.x, start.y) + translation,
                                    Size::new(end.x - start.x, lh.0),
                                ),
                                ..Default::default()
                            },
                            self.selection_color,
                        );
                    } else {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(start.x, start.y) + translation,
                                    Size::new(bounds.width - start.x, lh.0),
                                ),
                                ..Default::default()
                            },
                            self.selection_color,
                        );
                        let mut y = start.y + lh.0;
                        while y + lh.0 <= end.y + 0.5 {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        Point::new(0.0, y) + translation,
                                        Size::new(bounds.width, lh.0),
                                    ),
                                    ..Default::default()
                                },
                                self.selection_color,
                            );
                            y += lh.0;
                        }
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle::new(
                                    Point::new(0.0, end.y) + translation,
                                    Size::new(end.x, lh.0),
                                ),
                                ..Default::default()
                            },
                            self.selection_color,
                        );
                    }
                }
                _ => {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            ..Default::default()
                        },
                        self.selection_color,
                    );
                }
            }
        }

        // Draw the actual text
        iced::advanced::widget::text::draw(
            renderer,
            defaults,
            layout.bounds(),
            &state.paragraph,
            style,
            viewport,
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &IRenderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        let was_hovered = self.hovered_link.is_some();
        if let Some(position) = cursor.position_in(layout.bounds()) {
            self.hovered_link = state.paragraph.hit_span(position).and_then(|span| {
                if self.spans.as_ref().as_ref().get(span)?.link.is_some() {
                    Some(span)
                } else {
                    None
                }
            });
        } else {
            self.hovered_link = None;
        }
        if was_hovered != self.hovered_link.is_some() {
            shell.request_redraw();
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(layout.bounds()) {
                    if self.hovered_link.is_some() {
                        state.span_pressed = self.hovered_link;
                    }

                    if let Some(hit) = state.paragraph.hit_test(position) {
                        shell.publish((self.on_action)(SelectionAction::Press {
                            item_index: self.item_index,
                            char_offset: hit.cursor(),
                        }));
                        shell.request_redraw();
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if self.selection.is_dragging() {
                    if let Some(position) = cursor.position_in(layout.bounds()) {
                        if let Some(hit) = state.paragraph.hit_test(position) {
                            shell.publish((self.on_action)(SelectionAction::Drag {
                                item_index: self.item_index,
                                char_offset: hit.cursor(),
                            }));
                            shell.request_redraw();
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(on_link) = &self.on_link_click {
                    if let Some(span) = state.span_pressed.take() {
                        if Some(span) == self.hovered_link {
                            if let Some(link) = self.spans.as_ref().as_ref()
                                .get(span).and_then(|s| s.link.clone())
                            {
                                shell.publish(on_link(link));
                            }
                        }
                    }
                }
                shell.publish((self.on_action)(SelectionAction::Release));
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Character(c),
                modifiers,
                ..
            }) if modifiers.command() && c.as_str() == "c" => {
                if self.selection.has_selection() {
                    shell.publish((self.on_action)(SelectionAction::Copy));
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &IRenderer,
    ) -> mouse::Interaction {
        if self.hovered_link.is_some() {
            mouse::Interaction::Pointer
        } else if cursor.position_in(layout.bounds()).is_some() {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message: 'a> From<SelectableRich<'a, Message>>
    for Element<'a, Message, ITheme, IRenderer>
{
    fn from(widget: SelectableRich<'a, Message>) -> Self {
        Element::new(widget)
    }
}
