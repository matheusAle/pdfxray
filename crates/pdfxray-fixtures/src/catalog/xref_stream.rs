//! `xref-stream-objstm`: PDF 1.5 with an xref stream and an object stream.

use super::common::{HELVETICA, built, page_dict};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_5);
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Hello object streams) Tj ET");
    w.object_stream(
        6,
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>"),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
            (3, &page_dict("/MediaBox [0 0 240 100]")),
            (5, HELVETICA),
        ],
    );
    w.finish_xref_stream(7, &Trailer::new(1))?;
    let out = w.into_output()?;

    let expected = Expected::new(PdfVersion::V1_5, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 240.0, 100.0], 3)));
    Ok(built(out, expected))
}
