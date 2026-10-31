//! `struct-rich`: role and class maps, attributes, alternate and actual text, language, IDs, a
//! table, a list, a link with an object reference, content continued across two pages and a
//! parent tree that is a number tree with `/Kids`.

use super::common::{HELVETICA, built};
use crate::expected::{
    AnnotationFacts, AttributeSetFacts, CatalogFacts, Expected, NodeFacts, ObjectParentFacts, OutlineFacts,
    PageFacts, PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const PAGE1_CONTENT: &str = "\
/H1 << /MCID 0 >> BDC BT /F1 24 Tf 20 360 Td (Rich structure) Tj ET EMC
/P << /MCID 1 >> BDC BT /F1 12 Tf 20 330 Td (Un paragraphe qui continue) Tj ET EMC
/Figure << /MCID 2 >> BDC 0.8 0.9 1 rg 20 230 120 80 re f EMC
/TH << /MCID 3 >> BDC BT /F1 12 Tf 20 200 Td (Name) Tj ET EMC
/TD << /MCID 4 >> BDC BT /F1 12 Tf 120 200 Td (Value) Tj ET EMC
/Lbl << /MCID 5 >> BDC BT /F1 12 Tf 20 170 Td (1.) Tj ET EMC
/LBody << /MCID 6 >> BDC BT /F1 12 Tf 40 170 Td (First item) Tj ET EMC";

const PAGE2_CONTENT: &str = "\
/P << /MCID 0 >> BDC BT /F1 12 Tf 20 360 Td (sur la page suivante) Tj ET EMC
/Link << /MCID 1 >> BDC BT /F1 12 Tf 20 330 Td (A link) Tj ET EMC
/Span << /MCID 2 >> BDC BT /F1 12 Tf 20 300 Td (W3C) Tj ET EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 8 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 400 400] >>",
    );
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /StructParents 0 /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>",
    );
    w.object(
        4,
        "<< /Type /Page /Parent 2 0 R /StructParents 1 /Annots [9 0 R] \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>",
    );
    w.object(5, HELVETICA);
    w.stream(6, "", PAGE1_CONTENT.as_bytes());
    w.stream(7, "", PAGE2_CONTENT.as_bytes());
    w.object(
        8,
        "<< /Type /StructTreeRoot /K 10 0 R /RoleMap << /Cell /TD /Callout /Remark /Remark /Span >> \
         /ClassMap << /Body << /O /Layout /TextAlign /Justify >> \
         /Wide [<< /O /Layout /Placement /Block >> << /O /Layout /Width 120 >>] >> \
         /IDTree 25 0 R /ParentTree 26 0 R /ParentTreeNextKey 3 >>",
    );
    w.object(
        9,
        "<< /Type /Annot /Subtype /Link /Rect [20 325 70 345] /Border [0 0 0] /F 4 \
         /A << /S /URI /URI (https://example.com/) >> /StructParent 2 /P 4 0 R >>",
    );
    w.object(10, "<< /Type /StructElem /S /Document /P 8 0 R /K [11 0 R] >>");
    w.object(
        11,
        "<< /Type /StructElem /S /Sect /P 10 0 R /K [12 0 R 13 0 R 14 0 R 15 0 R 19 0 R 23 0 R 24 0 R] >>",
    );
    w.object(12, "<< /Type /StructElem /S /H1 /P 11 0 R /Pg 3 0 R /K 0 >>");
    w.object(
        13,
        "<< /Type /StructElem /S /P /P 11 0 R /Lang (fr-FR) /C /Body \
         /K [<< /Type /MCR /Pg 3 0 R /MCID 1 >> << /Type /MCR /Pg 4 0 R /MCID 0 >>] >>",
    );
    w.object(
        14,
        "<< /Type /StructElem /S /Figure /P 11 0 R /Pg 3 0 R /Alt (A pale blue rectangle) /ID (fig-1) \
         /C /Wide /A << /O /Layout /BBox [20 230 140 310] >> /K 2 >>",
    );
    w.object(
        15,
        "<< /Type /StructElem /S /Table /P 11 0 R /ID (tbl-1) /K [16 0 R] >>",
    );
    w.object(16, "<< /Type /StructElem /S /TR /P 15 0 R /K [17 0 R 18 0 R] >>");
    w.object(
        17,
        "<< /Type /StructElem /S /TH /P 16 0 R /Pg 3 0 R /ID (th-1) /A << /O /Table /Scope /Column >> /K 3 >>",
    );
    w.object(18, "<< /Type /StructElem /S /Cell /P 16 0 R /Pg 3 0 R /K 4 >>");
    w.object(19, "<< /Type /StructElem /S /L /P 11 0 R /K [20 0 R] >>");
    w.object(20, "<< /Type /StructElem /S /LI /P 19 0 R /K [21 0 R 22 0 R] >>");
    w.object(21, "<< /Type /StructElem /S /Lbl /P 20 0 R /Pg 3 0 R /K 5 >>");
    w.object(22, "<< /Type /StructElem /S /LBody /P 20 0 R /Pg 3 0 R /K 6 >>");
    w.object(
        23,
        "<< /Type /StructElem /S /Link /P 11 0 R /Pg 4 0 R /K [1 << /Type /OBJR /Pg 4 0 R /Obj 9 0 R >>] >>",
    );
    w.object(
        24,
        "<< /Type /StructElem /S /Callout /P 11 0 R /Pg 4 0 R /ActualText (World Wide Web Consortium) /K 2 >>",
    );
    w.object(25, "<< /Names [(fig-1) 14 0 R (tbl-1) 15 0 R (th-1) 17 0 R] >>");
    w.object(26, "<< /Kids [27 0 R 28 0 R] >>");
    w.object(
        27,
        "<< /Limits [0 1] /Nums [0 [12 0 R 13 0 R 14 0 R 17 0 R 18 0 R 21 0 R 22 0 R] \
         1 [13 0 R 23 0 R 24 0 R]] >>",
    );
    w.object(28, "<< /Limits [2 2] /Nums [2 23 0 R] >>");
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let layout = |entries: &[(&str, &str)]| AttributeSetFacts::new("Layout", entries);
    let table = NodeFacts::new(15, "Table", Some("Table")).id("tbl-1").elem(
        NodeFacts::new(16, "TR", Some("TR"))
            .elem(
                NodeFacts::new(17, "TH", Some("TH"))
                    .page(0)
                    .id("th-1")
                    .direct(AttributeSetFacts::new("Table", &[("Scope", "/Column")]))
                    .mcr(0, 3),
            )
            // `Cell` is mapped to TD through the RoleMap.
            .elem(NodeFacts::new(18, "Cell", Some("TD")).page(0).mcr(0, 4)),
    );
    let list = NodeFacts::new(19, "L", Some("L")).elem(
        NodeFacts::new(20, "LI", Some("LI"))
            .elem(NodeFacts::new(21, "Lbl", Some("Lbl")).page(0).mcr(0, 5))
            .elem(NodeFacts::new(22, "LBody", Some("LBody")).page(0).mcr(0, 6)),
    );
    let sect = NodeFacts::new(11, "Sect", Some("Sect"))
        .elem(NodeFacts::new(12, "H1", Some("H1")).page(0).mcr(0, 0))
        // One paragraph, two marked-content references on two pages, a class and a language.
        .elem(
            NodeFacts::new(13, "P", Some("P"))
                .lang("fr-FR")
                .class("Body", layout(&[("TextAlign", "/Justify")]))
                .mcr(0, 1)
                .mcr(1, 0),
        )
        .elem(
            NodeFacts::new(14, "Figure", Some("Figure"))
                .page(0)
                .id("fig-1")
                .alt("A pale blue rectangle")
                .direct(layout(&[("BBox", "[20 230 140 310]")]))
                .class("Wide", layout(&[("Placement", "/Block")]))
                .class("Wide", layout(&[("Width", "120")]))
                .mcr(0, 2),
        )
        .elem(table)
        .elem(list)
        // A bare MCID in /K takes its page from the element's /Pg; the OBJR names the annotation.
        .elem(
            NodeFacts::new(23, "Link", Some("Link"))
                .page(1)
                .mcr(1, 1)
                .objr(Some(1), 9),
        )
        // `Callout` maps to `Remark`, which maps to `Span`.
        .elem(
            NodeFacts::new(24, "Callout", Some("Span"))
                .page(1)
                .actual_text("World Wide Web Consortium")
                .mcr(1, 2),
        );

    let mut outline = OutlineFacts::new(8);
    outline.role_map = [("Cell", "TD"), ("Callout", "Remark"), ("Remark", "Span")]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    outline
        .class_map
        .insert("Body".to_owned(), vec![layout(&[("TextAlign", "/Justify")])]);
    outline.class_map.insert(
        "Wide".to_owned(),
        vec![layout(&[("Placement", "/Block")]), layout(&[("Width", "120")])],
    );
    outline.id_tree = [("fig-1", 14), ("tbl-1", 15), ("th-1", 17)]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), *v))
        .collect();
    outline.children = vec![NodeFacts::new(10, "Document", Some("Document")).elem(sect)];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }, PageKeyFacts { page: 1, key: 1 }];
    outline.parent_tree = vec![
        ParentTreeEntry {
            key: 0,
            parents: [12, 13, 14, 17, 18, 21, 22].map(Some).to_vec(),
        },
        ParentTreeEntry {
            key: 1,
            parents: [13, 23, 24].map(Some).to_vec(),
        },
    ];
    outline.parent_tree_objects = vec![ObjectParentFacts { key: 2, parent: 23 }];
    outline.parent_tree_nodes = vec![26, 27, 28];
    outline.parent_tree_next_key = Some(3);

    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(
        PageFacts::new(0, 3, Sourced::inherited([0.0, 0.0, 400.0, 400.0], 2)).mcids(&[0, 1, 2, 3, 4, 5, 6]),
    )
    .with_page(PageFacts::new(1, 4, Sourced::inherited([0.0, 0.0, 400.0, 400.0], 2)).mcids(&[0, 1, 2]))
    .with_struct_outline(outline)
    .with_annotation(AnnotationFacts {
        page: 1,
        object: 9,
        subtype: "Link".to_owned(),
        rect: [20.0, 325.0, 70.0, 345.0],
        flags: 4,
        struct_parent: Some(2),
        uri: Some("https://example.com/".to_owned()),
        goto_page: None,
        has_appearance: false,
        field_type: None,
        field_name: None,
    });
    Ok(built(out, expected))
}
