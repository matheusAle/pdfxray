//! The keyboard focus ring of the controls that paint themselves: one style for every control.
//!
//! egui draws one for its own widgets; a control that fills its own rectangle must call [`ring`]
//! right after the fill, [`row_ring`] when it is a full-width row, or [`page_ring`] when it is
//! drawn on the page. A click does not focus these controls, only Tab does, so the ring never
//! shows after a mouse click.
//!
//! The ring is a [`RING_WIDTH`] accent line standing [`RING_GAP`] off the control, with corners
//! of the control's radius plus the gap: the gap shows the background, so the 1 px outline of a
//! selected item and the ring stay two separate lines. Full-width rows touch their neighbours, so
//! their ring sits the same gap inside the row's edge instead.

use egui::{Painter, Rect, Response, Stroke, StrokeKind, Ui};

use crate::theme::tokens;

/// Width of the ring.
pub const RING_WIDTH: f32 = 2.0;
/// The gap between a control and its ring, and between a row's edge and its inset ring.
pub const RING_GAP: f32 = 2.0;

/// Where the ring of a control in `rect` lies: the rectangle its inner edge follows, outside
/// `rect` by [`RING_GAP`], and that edge's corner radius. The stroke goes outward from it.
pub fn ring_rect(rect: Rect, radius: f32) -> (Rect, f32) {
    (rect.expand(RING_GAP), radius + RING_GAP)
}

/// Where the ring of a full-width row in `rect` lies: [`RING_GAP`] inside the row's edge. The
/// stroke goes inward from it.
pub fn row_ring_rect(rect: Rect, radius: f32) -> (Rect, f32) {
    (rect.shrink(RING_GAP), (radius - RING_GAP).max(0.0))
}

/// Draws the accent ring [`RING_GAP`] outside `rect` (a chip, a crumb, a cell, a card, a button
/// with corners of `radius`) while `response` has the keyboard focus.
pub fn ring(ui: &Ui, response: &Response, rect: Rect, radius: f32) {
    if response.has_focus() {
        let (at, radius) = ring_rect(rect, radius);
        ui.painter().rect_stroke(
            at,
            radius,
            Stroke::new(RING_WIDTH, tokens::ACCENT),
            StrokeKind::Outside,
        );
    }
}

/// Draws the accent ring [`RING_GAP`] inside the edge of a full-width row in `rect` (a tree,
/// property, drawer or menu row) while `response` has the keyboard focus: a row touches its
/// neighbours, so a ring outside it would cover them.
pub fn row_ring(ui: &Ui, response: &Response, rect: Rect, radius: f32) {
    if response.has_focus() {
        let (at, radius) = row_ring_rect(rect, radius);
        ui.painter().rect_stroke(
            at,
            radius,
            Stroke::new(RING_WIDTH, tokens::ACCENT),
            StrokeKind::Inside,
        );
    }
}

/// The ring of a control drawn on the page with `painter`: the same accent ring as [`ring`], with
/// the [`tokens::FOCUS_HALO`] ([`RING_WIDTH`] wide) around it, since the accent alone is 1.7:1 on
/// a white page.
pub fn page_ring(painter: &Painter, response: &Response, rect: Rect, radius: f32) {
    if response.has_focus() {
        let (at, radius) = ring_rect(rect, radius);
        painter.rect_stroke(
            at,
            radius,
            Stroke::new(RING_WIDTH, tokens::ACCENT),
            StrokeKind::Outside,
        );
        painter.rect_stroke(
            at.expand(RING_WIDTH),
            radius + RING_WIDTH,
            Stroke::new(RING_WIDTH, tokens::FOCUS_HALO),
            StrokeKind::Outside,
        );
    }
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_control_ring_stands_two_points_off_with_the_radius_grown_by_the_gap() {
        let chip = Rect::from_min_size(pos2(10.0, 20.0), vec2(40.0, 16.0));

        let (at, radius) = ring_rect(chip, 4.0);

        assert_eq!(at, Rect::from_min_max(pos2(8.0, 18.0), pos2(52.0, 38.0)));
        assert!((radius - 6.0).abs() < f32::EPSILON, "{radius}");
        // Smoke: the ring is outside the control, never on it.
        assert!(at.contains_rect(chip) && at != chip);
    }

    #[test]
    fn a_row_ring_sits_two_points_inside_the_row() {
        let row = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 28.0));

        let (at, radius) = row_ring_rect(row, 4.0);

        assert_eq!(at, Rect::from_min_max(pos2(2.0, 2.0), pos2(298.0, 26.0)));
        assert!((radius - 2.0).abs() < f32::EPSILON, "{radius}");
        // Smoke: the ring stays inside the row, so it never covers its neighbours.
        assert!(row.contains_rect(at));
    }
}
