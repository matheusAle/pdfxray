//! `broken-xref-offsets`: a valid document whose xref entries for two objects lie.

use super::common::{HELVETICA, built, page_dict};
use crate::expected::{CatalogFacts, Defect, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Entry of object 3 (the page) is shifted forward by this many bytes.
const PAGE_SHIFT: i64 = 7;
/// Entry of object 4 (the content stream) is shifted backward by this many bytes.
const CONTENT_SHIFT: i64 = -3;

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_4);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 200 100]"));
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Broken xref) Tj ET");
    w.object(5, HELVETICA);
    w.corrupt_offset(3, PAGE_SHIFT);
    w.corrupt_offset(4, CONTENT_SHIFT);
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let mut expected = Expected::new(PdfVersion::V1_4, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 100.0], 3)));
    for m in &out.revisions[0].offset_mismatches {
        expected = expected.with_defect(Defect::XrefOffsetWrong {
            obj: m.obj,
            recorded: m.recorded,
            actual: m.actual,
        });
    }
    Ok(built(out, expected))
}
