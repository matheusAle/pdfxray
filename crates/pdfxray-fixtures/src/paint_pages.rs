//! A two-page file for the paint-order stepper across pages: overlapping, untagged fills.
//!
//! Page 1 runs `q Q 0 0 10 10 re f` (4 operators, the fill at 3); page 2 runs a red and then a
//! blue fill over part of it, `rg re f rg re f` (the fills at 2 and 5). In the paint order of the
//! whole document page 2's operators are positions 4 to 9.
//!
//! It is not part of the [`crate::catalog`]: no expected facts are generated for it. Tests build
//! it with [`two_overlapping_pages`] and assert on the operators by the `PAINT_PAGES_*` constants.

use crate::WriterError;
use crate::writer::{PdfVersion, PdfWriter, Trailer};

/// Operators on page 1.
pub const PAINT_PAGES_FIRST_OPS: u32 = 4;
/// The red fill on page 2.
pub const PAINT_PAGES_RED_FILL: u32 = 2;
/// The blue fill on page 2, painted over part of the red one.
pub const PAINT_PAGES_BLUE_FILL: u32 = 5;

/// The file's bytes.
///
/// # Errors
///
/// When the writer cannot lay the file out (never for these fixed objects).
pub fn two_overlapping_pages() -> Result<Vec<u8>, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 200] >>",
    );
    w.object(3, "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>");
    w.object(4, "<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>");
    w.stream(5, "", b"q Q 0 0 10 10 re f");
    w.stream(6, "", b"1 0 0 rg 10 10 50 50 re f 0 0 1 rg 30 30 50 50 re f");
    w.finish_table(&Trailer::new(1))?;
    Ok(w.into_output()?.bytes)
}
