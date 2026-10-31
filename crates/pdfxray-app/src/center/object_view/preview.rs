//! The picture of an object under its dictionary, for objects without instructions or text to
//! list: images, fonts, colour spaces, shadings, patterns and pages.

use egui::{Frame, Margin, Ui, vec2};
use pdfxray_core::Visual;

use crate::components::object_preview;
use crate::state::AppState;

/// The preview card, filling the room left under the dictionary.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState, visual: &Visual) {
    // A long dictionary line can widen the view past what is on screen; the card keeps to what
    // is visible.
    let visible = ui.clip_rect().right() - ui.cursor().left();
    let room = vec2(ui.available_width().min(visible), ui.available_height());
    ui.allocate_ui(room, |ui| {
        Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
            let room = vec2(ui.available_width(), ui.available_height() - 14.0);
            object_preview::centre(ui, state, visual, room);
        });
    });
}
