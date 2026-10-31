//! `artifacts`: pagination, layout and bare artifacts around one tagged paragraph.

use super::common::{HELVETICA, built, page_dict, struct_elem};
use crate::expected::{
    ArtifactFacts, CatalogFacts, Expected, PageFacts, Sourced, StructElemFacts, StructFacts,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const CONTENT: &str = "\
/Artifact << /Type /Pagination /Subtype /Header >> BDC BT /F1 10 Tf 20 180 Td (Page header) Tj ET EMC
/P << /MCID 0 >> BDC BT /F1 12 Tf 20 100 Td (Body text) Tj ET EMC
/Artifact << /Type /Layout >> BDC 0.8 g 20 90 160 1 re f EMC
/Artifact << /Type /Pagination /Subtype /Footer >> BDC BT /F1 10 Tf 20 10 Td (Page footer) Tj ET EMC
/Artifact BMC 0.5 G 5 5 190 190 re S EMC";

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
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 9 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(7, "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R] >>");
    w.object(8, struct_elem("P", 7, Some(3), "0"));
    w.object(9, "<< /Nums [0 [8 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let tree = StructFacts::new(6)
        .child(
            StructElemFacts::new(7, "Document", Some("Document"))
                .child(StructElemFacts::new(8, "P", Some("P")).page(0).mcids(&[0])),
        )
        .parent_tree(0, &[Some(8)]);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 200.0], 3)).mcids(&[0]))
    .with_struct_tree(tree)
    .with_artifact(ArtifactFacts::new(0, Some("Pagination"), Some("Header")))
    .with_artifact(ArtifactFacts::new(0, Some("Layout"), None))
    .with_artifact(ArtifactFacts::new(0, Some("Pagination"), Some("Footer")))
    .with_artifact(ArtifactFacts::new(0, None, None));
    Ok(built(out, expected))
}
