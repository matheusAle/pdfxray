//! `speech-artifacts`: a page header and footer painted as pagination artifacts and a layout rule
//! around a heading and a paragraph. Tag-following readers skip them; the default Chrome and Edge
//! viewer reads them where they are painted.

use super::common::{HELVETICA, built, page_dict, struct_elem, tagged_text};
use crate::expected::{
    ArtifactFacts, CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let content = [
        "/Artifact << /Type /Pagination /Subtype /Header >> BDC BT /F1 10 Tf 40 380 Td (Annual Report 2025) Tj ET EMC"
            .to_owned(),
        tagged_text("H1", 0, 18, (40, 340), "Results"),
        tagged_text("P", 1, 12, (40, 310), "Revenue grew in every region."),
        "/Artifact << /Type /Layout >> BDC 0.8 g 40 60 220 1 re f EMC".to_owned(),
        "/Artifact << /Type /Pagination /Subtype /Footer >> BDC BT /F1 10 Tf 40 30 Td (Page 1 of 9) Tj ET EMC"
            .to_owned(),
    ]
    .join("\n");
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 300 400] /StructParents 0"));
    w.stream(4, "", content.as_bytes());
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
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1]))
    .with_struct_tree(tree)
    .with_artifact(ArtifactFacts::new(0, Some("Pagination"), Some("Header")))
    .with_artifact(ArtifactFacts::new(0, Some("Layout"), None))
    .with_artifact(ArtifactFacts::new(0, Some("Pagination"), Some("Footer")));
    Ok(built(out, expected))
}
