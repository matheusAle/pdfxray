//! `encrypted-stub`: declares encryption without actually encrypting anything.

use super::common::{HELVETICA, built, page_dict};
use crate::expected::{CatalogFacts, Defect, Expected, PageFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let owner = "AB".repeat(32);
    let user = "CD".repeat(32);

    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 200 100]"));
    w.stream(4, "", b"BT /F1 18 Tf 20 40 Td (Not really secret) Tj ET");
    w.object(5, HELVETICA);
    w.object(
        6,
        format!("<< /Filter /Standard /V 1 /R 2 /O <{owner}> /U <{user}> /P -44 >>"),
    );
    w.finish_table(&Trailer::new(1).encrypt(6))?;
    let out = w.into_output()?;

    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 100.0], 3)))
        .with_defect(Defect::Encrypted {
            handler: "Standard".to_owned(),
            v: 1,
            r: 2,
        });
    Ok(built(out, expected))
}
