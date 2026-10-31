//! The card over the canvas that counts the failed checks on the page.

use egui::{RichText, Sense, Ui, vec2};

use super::cards::{Card, Column, title};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Title of the card.
pub const TITLE: &str = "On this page";

/// Why the passed checks are not split by page: a veraPDF report counts them for the whole file
/// and never says which page they were on, so only the failed ones can be shown per page.
pub const PASSED_NOTE: &str = "veraPDF counts passed checks for the whole file, not per page.";

/// The text of the failed line: `2 failed`.
pub fn failed_text(count: usize) -> String {
    format!("{count} failed")
}

/// Draws the card; nothing without a run.
pub(super) fn show(column: &mut Column, state: &AppState) {
    let Some(run) = state.document.as_ref().and_then(|loaded| loaded.doc.validation()) else {
        return;
    };
    let failed = state.failed_on_page(state.canvas.page);
    let passed = crate::navigator::thousands(usize::try_from(run.checks_passed).unwrap_or(usize::MAX));
    column.card(&Card::new(TITLE), |ui| {
        title(ui, TITLE);
        row(ui, tokens::ERR, &failed_text(failed), tokens::TEXT_2);
        row(
            ui,
            tokens::OK,
            &format!("{passed} passed in the file"),
            tokens::TEXT_3,
        )
        .on_hover_text(PASSED_NOTE);
    });
}

fn row(ui: &mut Ui, color: egui::Color32, text: &str, ink: egui::Color32) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(
            RichText::new(text)
                .font(fonts::mono(tokens::mono::BODY))
                .color(ink),
        );
    })
    .response
}
