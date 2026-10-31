//! `incremental-update`: two revisions, the second replacing a content stream and adding an object.

use super::common::{HELVETICA, built, page_dict};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_4);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 240 100]"));
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Hello pdfxray) Tj ET");
    w.object(5, HELVETICA);
    w.finish_table(&Trailer::new(1))?;

    // Revision 2: new content for the page and a new Info dictionary.
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Hello pdfxray rev 2) Tj ET");
    w.object(6, "<< /Title (pdfxray incremental fixture) >>");
    w.finish_table(&Trailer::new(1).info(6))?;
    let out = w.into_output()?;

    let expected = Expected::new(PdfVersion::V1_4, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 240.0, 100.0], 3)));
    Ok(built(out, expected))
}
