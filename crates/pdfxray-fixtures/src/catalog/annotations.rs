//! `annotations`: two links, a text field widget with an appearance stream, a text note and a
//! hidden annotation, tagged through `OBJR` references and the parent tree.

use super::common::{HELVETICA, built};
use crate::expected::{
    AnnotationFacts, ArtifactFacts, CatalogFacts, Expected, NodeFacts, ObjectParentFacts, OutlineFacts,
    PageFacts, PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const PAGE1_CONTENT: &str = "\
/Link << /MCID 0 >> BDC BT /F1 12 Tf 22 155 Td (External link) Tj ET EMC
/Link << /MCID 1 >> BDC BT /F1 12 Tf 22 125 Td (Internal link) Tj ET EMC
/P << /MCID 2 >> BDC BT /F1 12 Tf 22 105 Td (Name field below) Tj ET EMC";

const PAGE2_CONTENT: &str = "/Artifact BMC BT /F1 12 Tf 20 100 Td (Destination page) Tj ET EMC";

const APPEARANCE: &str = "/Tx BMC q BT /Helv 12 Tf 0 g 2 6 Td (Alice) Tj ET Q EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /AcroForm 9 0 R \
         /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 7 0 R] /Count 2 /MediaBox [0 0 200 200] >>",
    );
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /StructParents 0 /Annots [17 0 R 18 0 R 19 0 R 20 0 R 21 0 R] \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", PAGE1_CONTENT.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 10 0 R /RoleMap << /Annot /Span >> /ParentTree 16 0 R \
         /ParentTreeNextKey 5 >>",
    );
    w.object(
        7,
        "<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 8 0 R >>",
    );
    w.stream(8, "", PAGE2_CONTENT.as_bytes());
    w.object(
        9,
        "<< /Fields [19 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>",
    );
    w.object(
        10,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [11 0 R 12 0 R 13 0 R 14 0 R 15 0 R] >>",
    );
    w.object(
        11,
        "<< /Type /StructElem /S /Link /P 10 0 R /Pg 3 0 R /K [0 << /Type /OBJR /Pg 3 0 R /Obj 17 0 R >>] >>",
    );
    w.object(
        12,
        "<< /Type /StructElem /S /Link /P 10 0 R /Pg 3 0 R /K [1 << /Type /OBJR /Pg 3 0 R /Obj 18 0 R >>] >>",
    );
    w.object(
        13,
        "<< /Type /StructElem /S /Form /P 10 0 R /Pg 3 0 R /K [<< /Type /OBJR /Pg 3 0 R /Obj 19 0 R >>] >>",
    );
    w.object(
        14,
        "<< /Type /StructElem /S /Annot /P 10 0 R /Pg 3 0 R /K [<< /Type /OBJR /Pg 3 0 R /Obj 20 0 R >>] >>",
    );
    w.object(15, "<< /Type /StructElem /S /P /P 10 0 R /Pg 3 0 R /K 2 >>");
    w.object(
        16,
        "<< /Nums [0 [11 0 R 12 0 R 15 0 R] 1 11 0 R 2 12 0 R 3 13 0 R 4 14 0 R] >>",
    );
    w.object(
        17,
        "<< /Type /Annot /Subtype /Link /Rect [20 150 100 170] /Border [0 0 1] /F 4 \
         /A << /S /URI /URI (https://example.com/a) >> /StructParent 1 /P 3 0 R >>",
    );
    w.object(
        18,
        "<< /Type /Annot /Subtype /Link /Rect [20 120 100 140] /Border [0 0 1] /F 4 \
         /A << /S /GoTo /D [7 0 R /Fit] >> /StructParent 2 /P 3 0 R >>",
    );
    w.object(
        19,
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /V (Alice) /Rect [20 80 120 100] /F 4 \
         /DA (/Helv 12 Tf 0 g) /MK << /BC [0 0 0] >> /AP << /N 22 0 R >> /StructParent 3 /P 3 0 R >>",
    );
    w.object(
        20,
        "<< /Type /Annot /Subtype /Text /Rect [150 150 170 170] /Contents (A note) /Name /Note /F 4 \
         /StructParent 4 /P 3 0 R >>",
    );
    w.object(
        21,
        "<< /Type /Annot /Subtype /Square /Rect [150 100 190 130] /C [1 0 0] /F 2 /P 3 0 R >>",
    );
    w.stream(
        22,
        "/Type /XObject /Subtype /Form /BBox [0 0 100 20] /Resources << /Font << /Helv 5 0 R >> >>",
        APPEARANCE.as_bytes(),
    );
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let link = |object: u32, mcid: i32, annot: u32| {
        NodeFacts::new(object, "Link", Some("Link"))
            .page(0)
            .mcr(0, mcid)
            .objr(Some(0), annot)
    };
    let document = NodeFacts::new(10, "Document", Some("Document"))
        .elem(link(11, 0, 17))
        .elem(link(12, 1, 18))
        .elem(NodeFacts::new(13, "Form", Some("Form")).page(0).objr(Some(0), 19))
        // `Annot` is not a PDF 1.7 type; the RoleMap sends it to Span.
        .elem(
            NodeFacts::new(14, "Annot", Some("Span"))
                .page(0)
                .objr(Some(0), 20),
        )
        .elem(NodeFacts::new(15, "P", Some("P")).page(0).mcr(0, 2));

    let mut outline = OutlineFacts::new(6);
    outline.role_map.insert("Annot".to_owned(), "Span".to_owned());
    outline.children = vec![document];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![ParentTreeEntry {
        key: 0,
        parents: [11, 12, 15].map(Some).to_vec(),
    }];
    outline.parent_tree_objects = [(1, 11), (2, 12), (3, 13), (4, 14)]
        .iter()
        .map(|(key, parent)| ObjectParentFacts {
            key: *key,
            parent: *parent,
        })
        .collect();
    outline.parent_tree_nodes = vec![16];
    outline.parent_tree_next_key = Some(5);

    let annot = |object: u32, subtype: &str, rect: [f64; 4], flags: u32, struct_parent: Option<i32>| {
        AnnotationFacts {
            page: 0,
            object,
            subtype: subtype.to_owned(),
            rect,
            flags,
            struct_parent,
            uri: None,
            goto_page: None,
            has_appearance: false,
            field_type: None,
            field_name: None,
        }
    };
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::inherited([0.0, 0.0, 200.0, 200.0], 2)).mcids(&[0, 1, 2]))
    .with_page(PageFacts::new(
        1,
        7,
        Sourced::inherited([0.0, 0.0, 200.0, 200.0], 2),
    ))
    .with_struct_outline(outline)
    .with_artifact(ArtifactFacts::new(1, None, None))
    .with_annotation(AnnotationFacts {
        uri: Some("https://example.com/a".to_owned()),
        ..annot(17, "Link", [20.0, 150.0, 100.0, 170.0], 4, Some(1))
    })
    .with_annotation(AnnotationFacts {
        goto_page: Some(1),
        ..annot(18, "Link", [20.0, 120.0, 100.0, 140.0], 4, Some(2))
    })
    .with_annotation(AnnotationFacts {
        has_appearance: true,
        field_type: Some("Tx".to_owned()),
        field_name: Some("name".to_owned()),
        ..annot(19, "Widget", [20.0, 80.0, 120.0, 100.0], 4, Some(3))
    })
    .with_annotation(annot(20, "Text", [150.0, 150.0, 170.0, 170.0], 4, Some(4)))
    // The hidden flag (bit value 2) keeps this annotation out of the structure tree.
    .with_annotation(annot(21, "Square", [150.0, 100.0, 190.0, 130.0], 2, None));
    Ok(built(out, expected))
}
