//! `hybrid-xref`: a classic xref table whose trailer names a cross-reference stream in `/XRefStm`.
//!
//! The font and the Info dictionary live in an object stream. The classic table lists them as
//! free, so a reader that ignores `/XRefStm` loses the font; the stream restores them.

use super::common::{HELVETICA, built};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_5);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 100] \
         /Resources << /Font << /F1 6 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Hybrid xref) Tj ET");
    w.object_stream(5, &[(6, HELVETICA), (7, "<< /Title (Hybrid xref) >>")]);
    w.finish_hybrid(8, &Trailer::new(1).info(7))?;
    let out = w.into_output()?;

    let expected = Expected::new(PdfVersion::V1_5, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 240.0, 100.0], 3)));
    Ok(built(out, expected))
}
