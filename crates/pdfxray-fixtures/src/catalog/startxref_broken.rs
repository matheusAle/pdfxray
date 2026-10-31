//! `startxref-broken`: a valid document whose `startxref` points 12 bytes into the xref table.

use super::common::{HELVETICA, built, page_dict};
use crate::expected::{CatalogFacts, Defect, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// `startxref` is moved forward by this many bytes, landing inside the first xref entries.
const STARTXREF_SHIFT: i64 = 12;

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_4);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 200 100]"));
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Wrong startxref) Tj ET");
    w.object(5, HELVETICA);
    w.corrupt_startxref(STARTXREF_SHIFT);
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let mut expected = Expected::new(PdfVersion::V1_4, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 100.0], 3)));
    if let Some(m) = out.revisions[0].startxref_mismatch {
        expected = expected.with_defect(Defect::StartxrefWrong {
            recorded: m.recorded,
            actual: m.actual,
        });
    }
    Ok(built(out, expected))
}
