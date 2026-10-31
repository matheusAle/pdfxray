//! `orphan-mcid`: the three ways content and structure can disagree about MCIDs.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{CatalogFacts, Defect, Expected, PageFacts, Sourced, StructElemFacts, StructFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// MCIDs 0 and 2 are referenced from `/K`; MCID 1 is not referenced by any struct element.
const CONTENT: &str = "\
/P << /MCID 0 >> BDC BT /F1 12 Tf 20 150 Td (Properly tagged) Tj ET EMC
/Span << /MCID 1 >> BDC BT /F1 12 Tf 20 120 Td (Orphan content) Tj ET EMC
/P << /MCID 2 >> BDC BT /F1 12 Tf 20 90 Td (Parent tree disagrees) Tj ET EMC";

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
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 11 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R] >>",
    );
    // Correct: K says MCID 0, the parent tree says object 8.
    w.object(8, struct_elem("P", 7, Some(3), "0"));
    // Points at MCID 5, which the content never defines.
    w.object(9, struct_elem("P", 7, Some(3), "5"));
    // K says MCID 2 but the parent tree entry for MCID 2 names object 8 instead of 10.
    w.object(10, struct_elem("P", 7, Some(3), "2"));
    w.object(11, "<< /Nums [0 [8 0 R null 8 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(StructElemFacts::new(8, "P", Some("P")).page(0).mcids(&[0]))
                .child(StructElemFacts::new(9, "P", Some("P")).page(0).mcids(&[5]))
                .child(StructElemFacts::new(10, "P", Some("P")).page(0).mcids(&[2])),
        )
        .parent_tree(0, &[Some(8), None, Some(8)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 200.0], 3)).mcids(&[0, 1, 2]))
    .with_struct_tree(tree)
    .with_defect(Defect::OrphanMcid { page: 0, mcid: 1 })
    .with_defect(Defect::MissingMcid {
        elem: 9,
        page: 0,
        mcid: 5,
    })
    .with_defect(Defect::ParentTreeMismatch {
        key: 0,
        mcid: 2,
        parent_tree: Some(8),
        k_parent: 10,
    });
    Ok(built(out, expected))
}
