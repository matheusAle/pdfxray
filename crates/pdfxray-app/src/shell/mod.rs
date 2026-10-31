//! The fixed shell: title bar, activity rail, navigator, centre, inspector, drawer and status
//! bar. Each region is a host that draws one panel from [`crate::AppState`]; the shell only
//! decides where they go.

mod keys;
pub(crate) mod layout;
mod rail;
pub mod status_bar;
mod title_bar;

use std::path::PathBuf;

use crate::state::AppState;

pub use title_bar::ua_verdict;

/// Render the whole app into the root `ui` and apply input (file drops, clicks) to `state`.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let dropped: Vec<PathBuf> = ui.ctx().input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect()
    });
    if let Some(path) = dropped.first() {
        state.accept_drop(path);
    }

    state.poll_open(ui.ctx());
    state.poll_background(&ui.ctx().clone());
    keys::handle(ui, state);
    state.refresh_object_facts();
    layout::show(ui, state);
}

/// The hairline between a bar and the body, drawn inside the bar's rectangle along `edge` (its
/// top or bottom). Tone does the separating; this is only the faint edge light of the spec.
fn separator(ui: &egui::Ui, edge: f32) {
    let bar = ui.max_rect();
    let y = if (edge - bar.top()).abs() < (edge - bar.bottom()).abs() {
        edge + 0.5
    } else {
        edge - 0.5
    };
    ui.painter().hline(
        bar.x_range(),
        y,
        egui::Stroke::new(1.0, crate::theme::tokens::HAIRLINE),
    );
}
