//! `table-simple-pdf17`: a PDF 1.7 table of three rows and three columns whose first row is `TH`
//! cells and the rest `TD`, with no `Scope` anywhere. The header cells head their columns by
//! position.

use super::common::{HELVETICA, built, page_dict};
use super::tables::{Cell, Placement, build as build_table};
use crate::expected::{
    CatalogFacts, Expected, NodeFacts, OutlineFacts, PageFacts, PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

pub(super) fn build() -> Result<Built, WriterError> {
    let rows = vec![
        vec![
            Cell::new("TH", "Region", (0, 0)),
            Cell::new("TH", "Q1", (1, 0)),
            Cell::new("TH", "Q2", (2, 0)),
        ],
        vec![
            Cell::new("TD", "North", (0, 1)),
            Cell::new("TD", "1.2", (1, 1)),
            Cell::new("TD", "1.4", (2, 1)),
        ],
        vec![
            Cell::new("TD", "South", (0, 2)),
            Cell::new("TD", "0.9", (1, 2)),
            Cell::new("TD", "1.1", (2, 2)),
        ],
    ];
    let table = build_table(
        &Placement {
            first_obj: 8,
            parent: 7,
            page: 3,
            namespace: None,
        },
        &rows,
    );
    let parent_tree = 21;
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(3, page_dict("/MediaBox [0 0 300 400] /StructParents 0"));
    w.stream(4, "", table.content.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        format!("<< /Type /StructTreeRoot /K 7 0 R /ParentTree {parent_tree} 0 R /ParentTreeNextKey 1 >>"),
    );
    w.object(7, "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R] >>");
    for (obj, body) in &table.objects {
        w.object(*obj, body);
    }
    let owners: Vec<String> = table
        .parents
        .iter()
        .flatten()
        .map(|o| format!("{o} 0 R"))
        .collect();
    w.object(parent_tree, format!("<< /Nums [0 [{}]] >>", owners.join(" ")));
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let mut outline = OutlineFacts::new(6);
    outline.children = vec![NodeFacts::new(7, "Document", Some("Document")).elem(table.node)];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![ParentTreeEntry {
        key: 0,
        parents: table.parents,
    }];
    outline.parent_tree_nodes = vec![parent_tree];
    outline.parent_tree_next_key = Some(1);
    let mcids: Vec<i32> = (0..i32::try_from(table.cells).unwrap_or(0)).collect();
    let expected = Expected::new(
        PdfVersion::V1_7,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&mcids))
    .with_struct_outline(outline);
    Ok(built(out, expected))
}
