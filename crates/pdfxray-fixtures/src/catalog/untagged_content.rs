//! `untagged-content`: content that is neither tagged nor artifact, an MCID used twice and tagged
//! content nested inside an artifact.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{
    ArtifactFacts, CatalogFacts, Defect, Expected, PageFacts, Sourced, StructElemFacts, StructFacts,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// One operator per line, so the line number is the operator index (the comments in `build`
/// refer to them).
const CONTENT: &str = "\
/Artifact << /Type /Pagination /Subtype /Header >> BDC
BT
/F1 10 Tf
20 180 Td
(Header) Tj
ET
EMC
/P << /MCID 0 >> BDC
BT
/F1 12 Tf
20 150 Td
(Tagged one) Tj
ET
EMC
BT
/F1 12 Tf
20 120 Td
(Untagged text) Tj
ET
/P << /MCID 1 >> BDC
BT
/F1 12 Tf
20 100 Td
(Tagged two) Tj
ET
EMC
0.8 g
20 80 160 4 re
f
/P << /MCID 1 >> BDC
BT
/F1 12 Tf
20 60 Td
(Tagged again) Tj
ET
EMC
/Artifact BMC
/Span << /MCID 2 >> BDC
BT
/F1 12 Tf
20 40 Td
(Nested) Tj
ET
EMC
EMC";

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
    w.object(8, struct_elem("P", 7, Some(3), "0"));
    w.object(9, struct_elem("P", 7, Some(3), "1"));
    w.object(10, struct_elem("P", 7, Some(3), "2"));
    w.object(11, "<< /Nums [0 [8 0 R 9 0 R 10 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(StructElemFacts::new(8, "P", Some("P")).page(0).mcids(&[0]))
                .child(StructElemFacts::new(9, "P", Some("P")).page(0).mcids(&[1]))
                .child(StructElemFacts::new(10, "P", Some("P")).page(0).mcids(&[2])),
        )
        .parent_tree(0, &[Some(8), Some(9), Some(10)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 200.0], 3)).mcids(&[0, 1, 1, 2]))
    .with_struct_tree(tree)
    .with_artifact(ArtifactFacts::new(0, Some("Pagination"), Some("Header")))
    .with_artifact(ArtifactFacts::new(0, None, None))
    // Operator 17 is the `Tj` of "Untagged text", the only painting operator of its run.
    .with_defect(Defect::UntaggedContent {
        page: 0,
        op_index_range: [17, 18],
    })
    // Operator 28 is the `f` of the grey bar; 26 and 27 only set the colour and build the path.
    .with_defect(Defect::UntaggedContent {
        page: 0,
        op_index_range: [28, 29],
    })
    // MCID 1 opens at operators 19 and 29.
    .with_defect(Defect::DuplicateMcid {
        page: 0,
        mcid: 1,
        count: 2,
    })
    // Operator 36 opens the artifact, operator 37 the tagged Span inside it.
    .with_defect(Defect::MarkedContentInsideArtifact {
        page: 0,
        op_index: 37,
        tag: "Span".to_owned(),
    });
    Ok(built(out, expected))
}
