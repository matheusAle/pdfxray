//! `speech-link-objr`: three object references in the structure tree: a `Link` with text and an
//! external address, a `Link` with only `/Contents` that goes to a page, and a `Form` holding a
//! text field whose name is its `/TU`.

use super::common::{HELVETICA, built, page_dict, struct_elem, tagged_text};
use crate::expected::{
    AnnotationFacts, CatalogFacts, Expected, NodeFacts, ObjectParentFacts, OutlineFacts, PageFacts,
    PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let content = [
        tagged_text("P", 0, 12, (40, 340), "Please read the following."),
        tagged_text("Link", 1, 12, (40, 310), "Annual report"),
        tagged_text("P", 2, 12, (40, 220), "Thank you."),
    ]
    .join("\n");
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        page_dict("/MediaBox [0 0 300 400] /StructParents 0 /Annots [14 0 R 15 0 R 16 0 R]"),
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 13 0 R /ParentTreeNextKey 4 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 9 0 R 10 0 R 11 0 R 12 0 R] >>",
    );
    w.object(8, struct_elem("P", 7, Some(3), "0"));
    w.object(
        9,
        "<< /Type /StructElem /S /Link /P 7 0 R /Pg 3 0 R /K [1 << /Type /OBJR /Pg 3 0 R /Obj 14 0 R >>] >>",
    );
    w.object(
        10,
        "<< /Type /StructElem /S /Link /P 7 0 R /Pg 3 0 R /K [<< /Type /OBJR /Pg 3 0 R /Obj 15 0 R >>] >>",
    );
    w.object(
        11,
        "<< /Type /StructElem /S /Form /P 7 0 R /Pg 3 0 R /K [<< /Type /OBJR /Pg 3 0 R /Obj 16 0 R >>] >>",
    );
    w.object(12, struct_elem("P", 7, Some(3), "2"));
    w.object(
        13,
        "<< /Nums [0 [8 0 R 9 0 R 12 0 R] 1 9 0 R 2 10 0 R 3 11 0 R] >>",
    );
    w.object(
        14,
        "<< /Type /Annot /Subtype /Link /Rect [40 305 140 325] /Border [0 0 0] /F 4 \
         /A << /S /URI /URI (https://example.com/report) >> /StructParent 1 /P 3 0 R >>",
    );
    w.object(
        15,
        "<< /Type /Annot /Subtype /Link /Rect [40 270 140 290] /Border [0 0 0] /F 4 \
         /Contents (Downloads page) /Dest [3 0 R /Fit] /StructParent 2 /P 3 0 R >>",
    );
    w.object(
        16,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /TU (Your name) /Rect [40 240 200 260] /F 4 \
         /StructParent 3 /P 3 0 R >>",
    );
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let document = NodeFacts::new(7, "Document", Some("Document"))
        .elem(NodeFacts::new(8, "P", Some("P")).page(0).mcr(0, 0))
        .elem(
            NodeFacts::new(9, "Link", Some("Link"))
                .page(0)
                .mcr(0, 1)
                .objr(Some(0), 14),
        )
        .elem(NodeFacts::new(10, "Link", Some("Link")).page(0).objr(Some(0), 15))
        .elem(NodeFacts::new(11, "Form", Some("Form")).page(0).objr(Some(0), 16))
        .elem(NodeFacts::new(12, "P", Some("P")).page(0).mcr(0, 2));
    let mut outline = OutlineFacts::new(6);
    outline.children = vec![document];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![ParentTreeEntry {
        key: 0,
        parents: [8, 9, 12].map(Some).to_vec(),
    }];
    outline.parent_tree_objects = [(1, 9), (2, 10), (3, 11)]
        .iter()
        .map(|&(key, parent)| ObjectParentFacts { key, parent })
        .collect();
    outline.parent_tree_nodes = vec![13];
    outline.parent_tree_next_key = Some(4);

    let annot = |object: u32, subtype: &str, rect: [f64; 4], key: i32| AnnotationFacts {
        page: 0,
        object,
        subtype: subtype.to_owned(),
        rect,
        flags: 4,
        struct_parent: Some(key),
        uri: None,
        goto_page: None,
        has_appearance: false,
        field_type: None,
        field_name: None,
    };
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&[0, 1, 2]))
    .with_struct_outline(outline)
    .with_annotation(AnnotationFacts {
        uri: Some("https://example.com/report".to_owned()),
        ..annot(14, "Link", [40.0, 305.0, 140.0, 325.0], 1)
    })
    .with_annotation(AnnotationFacts {
        goto_page: Some(0),
        ..annot(15, "Link", [40.0, 270.0, 140.0, 290.0], 2)
    })
    .with_annotation(AnnotationFacts {
        field_type: Some("Tx".to_owned()),
        field_name: Some("name".to_owned()),
        ..annot(16, "Widget", [40.0, 240.0, 200.0, 260.0], 3)
    });
    Ok(built(out, expected))
}
