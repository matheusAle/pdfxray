//! Helpers shared by the unit tests of this crate.

use crate::Document;

/// Opens the generated fixture `name` (see [`pdfxray_fixtures::pdf_path`]).
pub(crate) fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}
