//! The centre: the canvas, or the object view when the Objects entry of the rail is active.

pub mod canvas;
pub mod object_view;
pub mod pick;
pub mod toolbar;
pub mod view_menu;

use egui::Ui;

use crate::state::AppState;

/// Draws the centre host.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    if state.nav.rail.replaces_canvas() {
        object_view::ui(ui, state);
    } else {
        canvas::ui(ui, state);
    }
}
