//! Helpers shared by the unit tests of this crate.

use std::time::Duration;

use pdfxray_core::Document;

use crate::state::AppState;

/// Opens the generated fixture `name` (see [`pdfxray_fixtures::pdf_path`]).
pub(crate) fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

/// Opens the generated fixture `name` in `state`, as opening it from disk does.
pub(crate) fn install_fixture(state: &mut AppState, name: &str) {
    state.install(
        fixture(name),
        &pdfxray_fixtures::pdf_path(name),
        Duration::ZERO,
        false,
    );
}
