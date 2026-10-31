//! `tagged-simple`: the smallest correctly tagged document.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const CONTENT: &str = "\
/H1 << /MCID 0 >> BDC BT /F1 24 Tf 20 150 Td (Heading) Tj ET EMC
/P << /MCID 1 >> BDC BT /F1 12 Tf 20 120 Td (Body paragraph) Tj ET EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 200 200] /StructParents 0"));
    w.stream(4, "", CONTENT.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 10 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R] >>",
    );
    w.object(8, struct_elem("H1", 7, Some(3), "0"));
    w.object(9, struct_elem("P", 7, Some(3), "1"));
    w.object(10, "<< /Nums [0 [8 0 R 9 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(StructElemFacts::new(8, "H1", Some("H1")).page(0).mcids(&[0]))
                .child(StructElemFacts::new(9, "P", Some("P")).page(0).mcids(&[1])),
        )
        .parent_tree(0, &[Some(8), Some(9)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 200.0], 3)).mcids(&[0, 1]))
    .with_struct_tree(tree);
    Ok(built(out, expected))
}
