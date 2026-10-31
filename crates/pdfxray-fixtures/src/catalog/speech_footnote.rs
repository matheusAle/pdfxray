//! `speech-footnote`: a paragraph with a footnote reference, and its `Note` tagged after the next
//! paragraph. Screen readers read the note where it sits in the tree, one paragraph after the
//! reference, and never move it.

use super::common::{HELVETICA, built, page_dict, struct_elem, tagged_text};
use crate::expected::{
    CatalogFacts, Expected, NodeFacts, OutlineFacts, PageFacts, PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let content = [
        tagged_text("P", 0, 12, (40, 340), "Revenue grew in every region."),
        tagged_text("Reference", 1, 8, (220, 344), "1"),
        tagged_text("P", 2, 12, (40, 310), "Closing remarks follow."),
        tagged_text("Lbl", 3, 8, (40, 60), "1"),
        tagged_text("P", 4, 8, (50, 60), "Source: annual report."),
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
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 14 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 10 0 R 11 0 R] >>",
    );
    w.object(8, "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K [0 9 0 R] >>");
    w.object(9, struct_elem("Reference", 8, Some(3), "1"));
    w.object(10, struct_elem("P", 7, Some(3), "2"));
    w.object(
        11,
        "<< /Type /StructElem /S /Note /P 7 0 R /Pg 3 0 R /K [12 0 R 13 0 R] >>",
    );
    w.object(12, struct_elem("Lbl", 11, Some(3), "3"));
    w.object(13, struct_elem("P", 11, Some(3), "4"));
    w.object(14, "<< /Nums [0 [8 0 R 9 0 R 10 0 R 12 0 R 13 0 R]] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let leaf =
        |object: u32, role: &str, mcid: i32| NodeFacts::new(object, role, Some(role)).page(0).mcr(0, mcid);
    let document = NodeFacts::new(7, "Document", Some("Document"))
        .elem(
            NodeFacts::new(8, "P", Some("P"))
                .page(0)
                .mcr(0, 0)
                .elem(leaf(9, "Reference", 1)),
        )
        .elem(leaf(10, "P", 2))
        .elem(
            NodeFacts::new(11, "Note", Some("Note"))
                .page(0)
                .elem(leaf(12, "Lbl", 3))
                .elem(leaf(13, "P", 4)),
        );
    let mut outline = OutlineFacts::new(6);
    outline.children = vec![document];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![ParentTreeEntry {
        key: 0,
        parents: [8, 9, 10, 12, 13].map(Some).to_vec(),
    }];
    outline.parent_tree_nodes = vec![14];
    outline.parent_tree_next_key = Some(1);
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2, 3, 4]))
    .with_struct_outline(outline);
    Ok(built(out, expected))
}
