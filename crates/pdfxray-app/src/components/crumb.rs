//! Breadcrumb crumbs: a kind dot and a mono name, with chevrons between them.

use egui::{
    Align, Color32, Frame, Label, Layout, Margin, Response, RichText, Role, Sense, Shape, Stroke, Ui,
    WidgetInfo, epaint::RectShape, vec2,
};

use super::icon::Icon;
use super::tooltip;
use crate::theme::{fonts, tokens};

/// What a crumb shows for the crumbs that were folded away.
pub const MORE: &str = "···";

/// Widest a crumb's name gets; a longer one is cut with an ellipsis (its tooltip has it all), so
/// a long file name cannot push the selection out of the row.
pub const MAX_TEXT_W: f32 = 140.0;

/// One crumb. The current one (the last) is filled with the accent-dim tint.
#[derive(Debug, Clone)]
#[must_use = "call `show`"]
pub struct Crumb<'a> {
    text: &'a str,
    dot: Option<Color32>,
    current: bool,
    max_text: f32,
}

impl<'a> Crumb<'a> {
    /// A crumb with `text` and a kind dot.
    pub const fn new(text: &'a str, dot: Color32) -> Self {
        Self {
            text,
            dot: Some(dot),
            current: false,
            max_text: MAX_TEXT_W,
        }
    }

    /// The crumb that stands for the folded ones: dim dots and no kind dot, as it is no node.
    pub const fn more() -> Self {
        Self {
            text: MORE,
            dot: None,
            current: false,
            max_text: MAX_TEXT_W,
        }
    }

    /// Marks the crumb as the selected node itself.
    pub const fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    /// Cuts the name at `width` points instead of [`MAX_TEXT_W`].
    pub const fn max_text(mut self, width: f32) -> Self {
        self.max_text = width;
        self
    }

    /// The size of the name on one line, uncut.
    fn text_size(&self, ui: &Ui) -> egui::Vec2 {
        ui.painter()
            .layout_no_wrap(
                self.text.to_owned(),
                fonts::mono(tokens::mono::SMALL),
                Color32::WHITE,
            )
            .size()
    }

    /// The width [`Self::show`] will take, so rows of crumbs can wrap before they overflow.
    pub fn width(&self, ui: &Ui) -> f32 {
        let dot = if self.dot.is_some() { 6.0 + 5.0 } else { 0.0 };
        // The frame margins, the dot, the gap between dot and text, and the frame stroke.
        self.text_size(ui).x.min(self.max_text) + 2.0 * 5.0 + dot + 2.0
    }

    /// Lays the crumb out; `clicked()` selects that ancestor.
    pub fn show(self, ui: &mut Ui) -> Response {
        // Every crumb has a border (clear on the others), so they all have the same box and
        // their names sit on one baseline.
        let (fill, stroke, ink) = if self.current {
            (
                tokens::ACCENT_DIM,
                Stroke::new(1.0, tokens::ACCENT),
                tokens::ACCENT,
            )
        } else if self.dot.is_none() {
            (
                Color32::TRANSPARENT,
                Stroke::new(1.0, Color32::TRANSPARENT),
                tokens::TEXT_3,
            )
        } else {
            (
                Color32::TRANSPARENT,
                Stroke::new(1.0, Color32::TRANSPARENT),
                tokens::TEXT_2,
            )
        };
        let size = self.text_size(ui);
        let cut = size.x > self.max_text;
        // The hover plate goes behind the frame, but is only known after it: reserve its place.
        let plate = ui.painter().add(Shape::Noop);
        let inner = Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(tokens::RADIUS_SM)
            .inner_margin(Margin::symmetric(5, 2))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                // The row is as tall as its text (up to an even number of points, so the pill
                // centres on whole pixels in a row of an even height), not as a 28 px control.
                ui.spacing_mut().interact_size.y = (size.y / 2.0).ceil() * 2.0;
                ui.horizontal(|ui| {
                    if let Some(dot) = self.dot {
                        let (rect, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 3.0, dot);
                    }
                    let name = RichText::new(self.text)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(ink);
                    if cut {
                        ui.allocate_ui_with_layout(
                            vec2(self.max_text, size.y),
                            Layout::left_to_right(Align::Center),
                            |ui| ui.add(Label::new(name).truncate()),
                        );
                    } else {
                        ui.label(name);
                    }
                });
            });
        let text = self.text;
        let click = ui.interact(
            inner.response.rect,
            inner.response.id.with("crumb"),
            Sense::click(),
        );
        click.widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
        if click.hovered() && !self.current {
            ui.painter().set(
                plate,
                RectShape::filled(inner.response.rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER),
            );
        }
        let click = click.on_hover_cursor(egui::CursorIcon::PointingHand);
        if cut {
            return tooltip::text(click, text);
        }
        click
    }
}

/// Width of [`separator`].
pub const SEPARATOR_WIDTH: f32 = 11.0;

/// The chevron between two crumbs.
pub fn separator(ui: &mut Ui) {
    Icon::ChevronRight.show(ui, SEPARATOR_WIDTH, tokens::TEXT_3);
}
