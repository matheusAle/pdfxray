//! `table-scope-pdf2`: a PDF 2.0 table whose structure elements are in the PDF 2.0 namespace. The
//! first row heads its columns (`Scope /Column`) and the first column heads its rows
//! (`Scope /Row`), so the header-finding algorithm of ISO 32000-2 applies.

use super::common::{HELVETICA, built};
use super::tables::{Cell, Placement, build as build_table};
use crate::expected::{
    CatalogFacts, Expected, NamespaceFacts, NodeFacts, OutlineFacts, PageFacts, PageKeyFacts,
    ParentTreeEntry, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Namespace URI of the PDF 2.0 structure types.
const PDF2_NS: &str = "http://iso.org/pdf2/ssn";

const COLUMN: &[(&str, &str)] = &[("Scope", "/Column")];
const ROW: &[(&str, &str)] = &[("Scope", "/Row")];

pub(super) fn build() -> Result<Built, WriterError> {
    let rows = vec![
        vec![
            Cell::new("TH", "Region", (0, 0)).attrs(COLUMN),
            Cell::new("TH", "Q1", (1, 0)).attrs(COLUMN),
            Cell::new("TH", "Q2", (2, 0)).attrs(COLUMN),
        ],
        vec![
            Cell::new("TH", "North", (0, 1)).attrs(ROW),
            Cell::new("TD", "1.2", (1, 1)),
            Cell::new("TD", "1.4", (2, 1)),
        ],
        vec![
            Cell::new("TH", "South", (0, 2)).attrs(ROW),
            Cell::new("TD", "0.9", (1, 2)),
            Cell::new("TD", "1.1", (2, 2)),
        ],
    ];
    let table = build_table(
        &Placement {
            first_obj: 9,
            parent: 8,
            page: 3,
            namespace: Some((7, PDF2_NS)),
        },
        &rows,
    );
    let parent_tree = 22;
    let mut w = PdfWriter::new(PdfVersion::V2_0);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /StructParents 0 \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", table.content.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        format!(
            "<< /Type /StructTreeRoot /K 8 0 R /Namespaces [7 0 R] /ParentTree {parent_tree} 0 R \
             /ParentTreeNextKey 1 >>"
        ),
    );
    w.object(7, format!("<< /Type /Namespace /NS ({PDF2_NS}) >>"));
    w.object(
        8,
        "<< /Type /StructElem /S /Document /NS 7 0 R /P 6 0 R /K [9 0 R] >>",
    );
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
    outline.namespaces = vec![NamespaceFacts {
        object: 7,
        uri: PDF2_NS.to_owned(),
        role_map: std::collections::BTreeMap::new(),
    }];
    outline.children = vec![
        NodeFacts::new(8, "Document", Some("Document"))
            .namespaces(PDF2_NS, PDF2_NS)
            .elem(table.node),
    ];
    outline.page_keys = vec![PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![ParentTreeEntry {
        key: 0,
        parents: table.parents,
    }];
    outline.parent_tree_nodes = vec![parent_tree];
    outline.parent_tree_next_key = Some(1);
    let mcids: Vec<i32> = (0..i32::try_from(table.cells).unwrap_or(0)).collect();
    let expected = Expected::new(
        PdfVersion::V2_0,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 300.0, 400.0], 3)).mcids(&mcids))
    .with_struct_outline(outline);
    Ok(built(out, expected))
}
