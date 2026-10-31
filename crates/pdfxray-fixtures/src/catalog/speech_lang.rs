//! `speech-lang`: a document in `en-US` with a `Span` in German (`/Lang` on the element) and a
//! paragraph in French (`/Lang` in the marked-content property list), so the announced language
//! changes four times.

use super::common::{HELVETICA, built, page_dict, struct_elem, tagged_text};
use crate::expected::{
    CatalogFacts, Expected, NodeFacts, OutlineFacts, PageFacts, PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let content = [
        tagged_text("P", 0, 12, (40, 340), "The German word"),
        tagged_text("Span", 1, 12, (150, 340), "Gesamtumsatz"),
        tagged_text("P", 2, 12, (250, 340), "means total revenue."),
        "/P << /MCID 3 /Lang (fr-FR) >> BDC BT /F1 12 Tf 40 310 Td (Bonjour tout le monde.) Tj ET EMC"
            .to_owned(),
        tagged_text("P", 4, 12, (40, 280), "Back in English."),
    ]
    .join("\n");
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 400 400] /StructParents 0"));
    w.stream(4, "", content.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 12 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 10 0 R 11 0 R] >>",
    );
    w.object(
        8,
        "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K [0 9 0 R 2] >>",
    );
    w.object(
        9,
        "<< /Type /StructElem /S /Span /P 8 0 R /Pg 3 0 R /Lang (de-DE) /K 1 >>",
    );
    w.object(10, struct_elem("P", 7, Some(3), "3"));
    w.object(11, struct_elem("P", 7, Some(3), "4"));
    w.object(12, "<< /Nums [0 [8 0 R 9 0 R 8 0 R 10 0 R 11 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let leaf =
        |object: u32, role: &str, mcid: i32| NodeFacts::new(object, role, Some(role)).page(0).mcr(0, mcid);
    let document = NodeFacts::new(7, "Document", Some("Document"))
        .elem(
            NodeFacts::new(8, "P", Some("P"))
                .page(0)
                .mcr(0, 0)
                .elem(leaf(9, "Span", 1).lang("de-DE"))
                .mcr(0, 2),
        )
        .elem(leaf(10, "P", 3))
        .elem(leaf(11, "P", 4));
    let mut outline = OutlineFacts::new(6);
    outline.children = vec![document];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![ParentTreeEntry {
        key: 0,
        parents: [8, 9, 8, 10, 11].map(Some).to_vec(),
    }];
    outline.parent_tree_nodes = vec![12];
    outline.parent_tree_next_key = Some(1);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 400.0, 400.0], 3)).mcids(&[0, 1, 2, 3, 4]))
    .with_struct_outline(outline);
    Ok(built(out, expected))
}
