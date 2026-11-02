//! The multi-selection and the marquee on one page: every selected node's outline, the group
//! hairline with its "4 selected" badge, and the marquee being drawn with what it will catch and
//! the count chip that trails the cursor (board "2.12 Canvas · Marquee multi-selection").

use egui::{
    Align2, Area, CornerRadius, CursorIcon, Frame, Id, Key, Label, Margin, Order, Painter, Pos2,
    Rect as UiRect, RichText, Stroke, StrokeKind, Ui, vec2,
};
use pdfxray_core::{PageIx, Rect};

use super::geometry::PageGeometry;
use crate::state::AppState;
use crate::state::selection::SUMMARY_FROM;
use crate::theme::{fonts, tokens};

/// Space between the bounds of a multi-selection and its hairline, in points on screen.
const GROUP_PAD: f32 = 4.0;

/// Where the count chip sits from the cursor: left of it and below.
const CHIP_OFFSET: egui::Vec2 = vec2(-14.0, 14.0);

/// The text of the badge on the group hairline, e.g. `4 selected`.
pub fn badge_text(count: usize) -> String {
    format!("{count} selected")
}

/// What to draw for the selection and the marquee on one page.
#[derive(Debug, Default)]
pub(super) struct MultiLayer {
    /// The selected nodes drawn with the multi-selection outline (the primary too, unless the
    /// inspector stepped into it).
    pub(super) members: Vec<Rect>,
    /// The bounds of every selected node on the page, with the hairline and badge, from three
    /// selected on (two are the compare view).
    pub(super) group: Option<Rect>,
    /// How many nodes are selected in all, for the badge.
    pub(super) count: usize,
    /// The bounds the action bar goes under while several nodes are selected.
    pub(super) bar_anchor: Option<Rect>,
    /// The marquee being drawn on this page.
    pub(super) marquee: Option<Rect>,
    /// What the marquee will catch.
    pub(super) catch: Vec<Rect>,
    /// The count chip's text.
    pub(super) chip: Option<String>,
}

impl MultiLayer {
    /// True while a marquee is drawn on this page.
    pub(super) const fn marquee_on(&self) -> bool {
        self.marquee.is_some()
    }
}

/// Reads what [`draw`] needs for `page`.
pub(super) fn collect(state: &AppState, page: PageIx) -> MultiLayer {
    let count = state.selection.len();
    let mut layer = MultiLayer {
        count,
        ..MultiLayer::default()
    };
    if count >= 2 {
        let rects: Vec<_> = state
            .selection_rects()
            .into_iter()
            .filter(|(_, s)| s.page == page)
            .collect();
        layer.bar_anchor = union(rects.iter().map(|(_, s)| s.rect));
        if count >= SUMMARY_FROM {
            let primary = state.selection.primary();
            let stepped_in = state.selection.stepped_in();
            layer.members = rects
                .iter()
                .filter(|(id, _)| !(stepped_in && Some(id) == primary))
                .map(|(_, s)| s.rect)
                .collect();
            layer.group = layer.bar_anchor;
        }
    }
    if let Some(marquee) = state.canvas.marquee.as_ref().filter(|m| m.page == page) {
        layer.marquee = Some(marquee.rect());
        layer.chip = state.marquee_count();
        if let Some(loaded) = &state.document {
            layer.catch = marquee
                .catch
                .iter()
                .filter_map(|id| crate::selection::selection_rect(&loaded.doc, id))
                .filter(|s| s.page == page)
                .map(|s| s.rect)
                .collect();
        }
    }
    layer
}

/// The smallest rectangle around `rects`.
fn union(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    rects.reduce(|a, b| Rect::new(a.x0.min(b.x0), a.y0.min(b.y0), a.x1.max(b.x1), a.y1.max(b.y1)))
}

/// Draws the members' outlines and the group hairline with its badge.
pub(super) fn draw_selection(ui: &Ui, painter: &Painter, geo: &PageGeometry, layer: &MultiLayer) {
    for member in &layer.members {
        painter.rect(
            geo.rect_to_screen(*member),
            0.0,
            tokens::MULTI_FILL,
            Stroke::new(tokens::MULTI_STROKE, tokens::ACCENT),
            StrokeKind::Inside,
        );
    }
    let Some(group) = layer.group else {
        return;
    };
    let hairline = geo.rect_to_screen(group).expand(GROUP_PAD);
    painter.rect_stroke(
        hairline,
        tokens::MARQUEE_RADIUS,
        Stroke::new(1.0, tokens::GROUP_HAIRLINE),
        StrokeKind::Middle,
    );
    Area::new(Id::unique("canvas-group-badge"))
        .order(Order::Middle)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(hairline.left_top() + vec2(0.0, -2.0))
        .interactable(false)
        .constrain_to(ui.clip_rect())
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(tokens::ACCENT)
                .corner_radius(CornerRadius::same(3))
                .inner_margin(Margin::symmetric(5, 1))
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(badge_text(layer.count))
                                .font(fonts::mono(tokens::mono::SMALL))
                                .color(tokens::ON_ACCENT),
                        )
                        .selectable(false)
                        .extend(),
                    );
                });
        });
}

/// Draws the marquee, the pre-highlight of what it will catch and the count chip at `pointer`.
pub(super) fn draw_marquee(
    ui: &Ui,
    painter: &Painter,
    geo: &PageGeometry,
    layer: &MultiLayer,
    cursor: Option<Pos2>,
) {
    let Some(marquee) = layer.marquee else {
        return;
    };
    for caught in &layer.catch {
        painter.rect_stroke(
            geo.rect_to_screen(*caught),
            0.0,
            Stroke::new(tokens::MULTI_STROKE, tokens::MARQUEE_CATCH),
            StrokeKind::Inside,
        );
    }
    let rect = geo.rect_to_screen(marquee);
    // The halo first, a point outside the accent stroke, so the marquee reads on white.
    painter.rect_stroke(
        rect.expand(1.0),
        tokens::MARQUEE_RADIUS + 1.0,
        Stroke::new(1.0, tokens::MARQUEE_HALO),
        StrokeKind::Outside,
    );
    painter.rect(
        rect,
        tokens::MARQUEE_RADIUS,
        tokens::MARQUEE_FILL,
        Stroke::new(1.0, tokens::ACCENT),
        StrokeKind::Inside,
    );
    let (Some(text), Some(at)) = (&layer.chip, cursor) else {
        return;
    };
    Area::new(Id::unique("canvas-marquee-count"))
        .order(Order::Tooltip)
        .pivot(Align2::RIGHT_TOP)
        .fixed_pos(at + CHIP_OFFSET)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(tokens::MARQUEE_CHIP)
                .stroke(Stroke::new(1.0, tokens::POPOVER_HAIRLINE))
                .corner_radius(CornerRadius::same(3))
                .inner_margin(Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(text)
                                .font(fonts::mono(tokens::mono::SMALL))
                                .color(tokens::TEXT),
                        )
                        .selectable(false)
                        .extend(),
                    );
                });
        });
}

/// True while Space is held: a drag pans the canvas instead of measuring or drawing a marquee.
pub(super) fn panning(ui: &Ui) -> bool {
    ui.input(|i| i.key_down(Key::Space))
}

/// Whether the page should take drags as marquees this frame. Always while a marquee is drawn;
/// never while Space is held, which pans; else when the drag starts (or would start, where the
/// pointer is) on empty space, or anywhere with ⌘.
pub(super) fn takes_drag(ui: &Ui, state: &AppState, page: PageIx, geo: &PageGeometry) -> bool {
    if state.canvas.marquee.is_some() {
        return true;
    }
    if panning(ui) {
        return false;
    }
    let (command, at) = ui.input(|i| {
        let at = if i.pointer.any_down() {
            i.pointer.press_origin()
        } else {
            i.pointer.hover_pos()
        };
        (i.modifiers.command, at)
    });
    let Some(at) = at.filter(|at| geo.rect.contains(*at)) else {
        return false;
    };
    command || state.marquee_starts_at(page, geo.screen_to_page(at))
}

/// The crosshair while the marquee is drawn, wherever the pointer is.
pub(super) fn cursor(ctx: &egui::Context, layer: &MultiLayer) {
    if layer.marquee_on() {
        ctx.set_cursor_icon(CursorIcon::Crosshair);
    }
}

/// The screen rectangle the action bar goes under while several nodes are selected.
pub(super) fn bar_anchor(geo: &PageGeometry, layer: &MultiLayer) -> Option<UiRect> {
    layer.bar_anchor.map(|rect| {
        let rect = geo.rect_to_screen(rect);
        if layer.group.is_some() {
            rect.expand(GROUP_PAD)
        } else {
            rect
        }
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_union_spans_every_rectangle_and_none_is_no_bounds() {
        let rects = [
            Rect::new(10.0, 10.0, 20.0, 20.0),
            Rect::new(5.0, 15.0, 12.0, 40.0),
        ];
        assert_eq!(union(rects.into_iter()), Some(Rect::new(5.0, 10.0, 20.0, 40.0)));
        // Smoke: no rectangles, no bounds.
        assert_eq!(union(std::iter::empty()), None);
        assert_eq!(badge_text(4), "4 selected");
    }
}
