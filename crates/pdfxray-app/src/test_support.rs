//! Helpers shared by the unit tests of this crate.

use std::time::{Duration, Instant};

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

/// Calls `probe` until it answers, for at most 30 s; a worker thread answers on its own time.
pub(crate) fn wait_until<T>(mut probe: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(answer) = probe() {
            return answer;
        }
        assert!(Instant::now() < deadline, "no answer within 30 s");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// [`pdfxray_fixtures::paint_pages::two_overlapping_pages`] opened in `state`, as opening it
/// from disk does.
pub(crate) fn install_two_overlapping_pages(state: &mut AppState) {
    let doc = Document::from_bytes(
        "two-pages.pdf",
        pdfxray_fixtures::paint_pages::two_overlapping_pages().unwrap(),
    )
    .unwrap();
    state.install(doc, std::path::Path::new("two-pages.pdf"), Duration::ZERO, false);
}
