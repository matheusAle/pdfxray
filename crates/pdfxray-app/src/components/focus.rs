//! The keyboard focus ring of the controls that paint themselves.
//!
//! egui draws one for its own widgets; a control that fills its own rectangle must call [`ring`]
//! right after the fill. A click does not focus these controls, only Tab does, so the ring never
//! shows after a mouse click.

use egui::{Rect, Response, Stroke, StrokeKind, Ui};

use crate::theme::tokens;

/// Draws the accent ring inside `rect`, with corners of `radius`, while `response` has the
/// keyboard focus.
pub fn ring(ui: &Ui, response: &Response, rect: Rect, radius: f32) {
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, radius, Stroke::new(1.0, tokens::ACCENT), StrokeKind::Inside);
    }
}
