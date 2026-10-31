//! `table-headers-ids`: a PDF 1.7 table with a merged corner cell (`RowSpan 2`), a merged header
//! (`ColSpan 2`) and data cells that name their header cells by `/Headers` and `/ID`. One header
//! (`Q1`) names the merged header in turn, and the last cell names an ID no cell has.

use super::common::{HELVETICA, built, page_dict};
use super::tables::{Cell, Placement, build as build_table};
use crate::expected::{
    CatalogFacts, Expected, NodeFacts, OutlineFacts, PageFacts, PageKeyFacts, ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

const ROW_SPAN_2: &[(&str, &str)] = &[("RowSpan", "2")];
const COL_SPAN_2: &[(&str, &str)] = &[("ColSpan", "2")];
const UNDER_SALES: &[(&str, &str)] = &[("Headers", "[(h-sales)]")];
const NORTH_Q1: &[(&str, &str)] = &[("Headers", "[(h-north) (h-q1)]")];
const NORTH_Q2: &[(&str, &str)] = &[("Headers", "[(h-north) (h-q2)]")];
const SOUTH_Q1: &[(&str, &str)] = &[("Headers", "[(h-south) (h-q1)]")];
const SOUTH_Q2_AND_A_GHOST: &[(&str, &str)] = &[("Headers", "[(h-south) (h-q2) (h-missing)]")];

pub(super) fn build() -> Result<Built, WriterError> {
    let rows = vec![
        vec![
            Cell::new("TH", "Region", (0, 0)).id("h-region").attrs(ROW_SPAN_2),
            Cell::new("TH", "Sales", (1, 0)).id("h-sales").attrs(COL_SPAN_2),
        ],
        vec![
            Cell::new("TH", "Q1", (1, 1)).id("h-q1").attrs(UNDER_SALES),
            Cell::new("TH", "Q2", (2, 1)).id("h-q2").attrs(UNDER_SALES),
        ],
        vec![
            Cell::new("TH", "North", (0, 2)).id("h-north"),
            Cell::new("TD", "1.2", (1, 2)).attrs(NORTH_Q1),
            Cell::new("TD", "1.4", (2, 2)).attrs(NORTH_Q2),
        ],
        vec![
            Cell::new("TH", "South", (0, 3)).id("h-south"),
            Cell::new("TD", "0.9", (1, 3)).attrs(SOUTH_Q1),
            Cell::new("TD", "1.1", (2, 3)).attrs(SOUTH_Q2_AND_A_GHOST),
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
    let parent_tree = 23;
    let names: Vec<String> = table
        .ids
        .iter()
        .map(|(id, obj)| format!("({id}) {obj} 0 R"))
        .collect();
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
        format!(
            "<< /Type /StructTreeRoot /K 7 0 R /IDTree << /Names [{}] >> /ParentTree {parent_tree} 0 R \
             /ParentTreeNextKey 1 >>",
            names.join(" ")
        ),
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
    outline.id_tree = table.ids;
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
