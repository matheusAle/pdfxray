//! The table model: grids, spans and the header cells of every cell under the PDF 2.0 and PDF 1.7
//! rules, through the public query API.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail loudly on unexpected input"
)]

use std::fmt::Write as _;

use pdfxray_core::{
    Document, HeaderSource, IrregularRow, NodeId, ObjRef, RowGroup, TableModel, TableRules, TableScope,
};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

fn table_of(doc: &Document, n: u32) -> TableModel {
    doc.table(&elem(n)).unwrap()
}

/// `(row, column, headers as "object via")` of every cell.
fn associations(model: &TableModel) -> Vec<(u32, u32, Vec<String>)> {
    model
        .cells
        .iter()
        .map(|c| {
            (
                c.row,
                c.col,
                c.headers
                    .iter()
                    .map(|h| {
                        let via = match h.via {
                            HeaderSource::Headers => "headers",
                            HeaderSource::Scope => "scope",
                            HeaderSource::Position => "position",
                        };
                        format!("{} {via}", h.cell)
                    })
                    .collect(),
            )
        })
        .collect()
}

// ---- table-simple-pdf17 -----------------------------------------------------------------------

#[test]
fn a_th_row_without_scope_heads_its_columns_by_position_under_the_pdf_17_rules() {
    let doc = fixture("table-simple-pdf17");

    let model = table_of(&doc, 8);

    assert_eq!(model.rules, TableRules::Pdf17);
    assert_eq!((model.rows, model.cols), (3, 3));
    assert_eq!(
        associations(&model),
        [
            (0, 0, vec![]),
            (0, 1, vec![]),
            (0, 2, vec![]),
            (1, 0, vec!["struct:12:0 position".to_owned()]),
            (1, 1, vec!["struct:13:0 position".to_owned()]),
            (1, 2, vec!["struct:14:0 position".to_owned()]),
            (2, 0, vec!["struct:12:0 position".to_owned()]),
            (2, 1, vec!["struct:13:0 position".to_owned()]),
            (2, 2, vec!["struct:14:0 position".to_owned()]),
        ]
    );
    assert_eq!(
        model.cells.iter().map(|c| c.header).collect::<Vec<_>>(),
        [true, true, true, false, false, false, false, false, false]
    );
    assert_eq!(model.slots, (0..9).map(Some).collect::<Vec<_>>());
    assert_eq!(model.irregular, []);
    // Smoke: nine cells, three of them headers.
    assert_eq!(model.cells.iter().filter(|c| c.header).count(), 3);
}

#[test]
fn a_cell_is_found_by_position_and_by_element() {
    let doc = fixture("table-simple-pdf17");
    let model = table_of(&doc, 8);

    assert_eq!(model.cell_at(1, 2).map(|c| c.node.clone()), Some(elem(17)));
    assert_eq!(model.cell_of(&elem(20)).map(|c| (c.row, c.col)), Some((2, 2)));
    assert_eq!(model.cell_at(3, 0), None);
    assert_eq!(model.cell_at(0, 3), None);
    assert_eq!(model.cell_of(&elem(8)), None);
    // Smoke.
    assert!(model.cell_at(0, 0).is_some_and(|c| c.header));
}

// ---- table-scope-pdf2 -------------------------------------------------------------------------

#[test]
fn the_pdf_2_algorithm_finds_headers_by_scope_and_skips_a_header_whose_scope_conflicts() {
    let doc = fixture("table-scope-pdf2");

    let model = table_of(&doc, 9);

    assert_eq!(model.rules, TableRules::Pdf2);
    assert_eq!(
        associations(&model),
        [
            (0, 0, vec![]),
            (0, 1, vec![]),
            (0, 2, vec![]),
            // A row header sees the column header above it ...
            (1, 0, vec!["struct:13:0 scope".to_owned()]),
            // ... and a data cell sees its column header, then its row header.
            (
                1,
                1,
                vec!["struct:14:0 scope".to_owned(), "struct:16:0 scope".to_owned()]
            ),
            (
                1,
                2,
                vec!["struct:15:0 scope".to_owned(), "struct:16:0 scope".to_owned()]
            ),
            // Scope Row on "North" conflicts with the upward search from "South" and does not
            // stop it: the search goes on to "Region".
            (2, 0, vec!["struct:13:0 scope".to_owned()]),
            (
                2,
                1,
                vec!["struct:14:0 scope".to_owned(), "struct:19:0 scope".to_owned()]
            ),
            (
                2,
                2,
                vec!["struct:15:0 scope".to_owned(), "struct:19:0 scope".to_owned()]
            ),
        ]
    );
    assert_eq!(
        model.cells.iter().map(|c| c.scope).collect::<Vec<_>>(),
        [
            Some(TableScope::Column),
            Some(TableScope::Column),
            Some(TableScope::Column),
            Some(TableScope::Row),
            None,
            None,
            Some(TableScope::Row),
            None,
            None
        ]
    );
    // Smoke: every data cell has two headers.
    assert!(
        model
            .cells
            .iter()
            .filter(|c| !c.header)
            .all(|c| c.headers.len() == 2)
    );
}

// ---- table-headers-ids ------------------------------------------------------------------------

#[test]
fn spans_place_cells_and_headers_lists_resolve_recursively_with_dangling_ids_reported() {
    let doc = fixture("table-headers-ids");

    let model = table_of(&doc, 8);

    assert_eq!(model.rules, TableRules::Pdf17);
    assert_eq!((model.rows, model.cols), (4, 3));
    assert_eq!(
        model
            .cells
            .iter()
            .map(|c| (c.row, c.col, c.row_span, c.col_span))
            .collect::<Vec<_>>(),
        [
            (0, 0, 2, 1),
            (0, 1, 1, 2),
            (1, 1, 1, 1),
            (1, 2, 1, 1),
            (2, 0, 1, 1),
            (2, 1, 1, 1),
            (2, 2, 1, 1),
            (3, 0, 1, 1),
            (3, 1, 1, 1),
            (3, 2, 1, 1),
        ]
    );
    // The merged corner covers both of its rows, the merged header both of its columns.
    assert_eq!(
        model.slots,
        [
            Some(0),
            Some(1),
            Some(1),
            Some(0),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
            Some(9)
        ]
    );
    assert_eq!(model.cell_at(1, 0).map(|c| c.node.clone()), Some(elem(13)));
    assert_eq!(model.irregular, []);
    assert_eq!(
        associations(&model),
        [
            (0, 0, vec![]),
            (0, 1, vec![]),
            (1, 1, vec!["struct:14:0 headers".to_owned()]),
            (1, 2, vec!["struct:14:0 headers".to_owned()]),
            // No Scope and no list: a header by position, here the column header "Region".
            (2, 0, vec!["struct:13:0 position".to_owned()]),
            // The list names North and Q1; Q1's own list adds Sales.
            (
                2,
                1,
                vec![
                    "struct:17:0 headers".to_owned(),
                    "struct:15:0 headers".to_owned(),
                    "struct:14:0 headers".to_owned()
                ]
            ),
            (
                2,
                2,
                vec![
                    "struct:17:0 headers".to_owned(),
                    "struct:16:0 headers".to_owned(),
                    "struct:14:0 headers".to_owned()
                ]
            ),
            (3, 0, vec!["struct:13:0 position".to_owned()]),
            (
                3,
                1,
                vec![
                    "struct:20:0 headers".to_owned(),
                    "struct:15:0 headers".to_owned(),
                    "struct:14:0 headers".to_owned()
                ]
            ),
            (
                3,
                2,
                vec![
                    "struct:20:0 headers".to_owned(),
                    "struct:16:0 headers".to_owned(),
                    "struct:14:0 headers".to_owned()
                ]
            ),
        ]
    );
    assert_eq!(
        model
            .cells
            .iter()
            .map(|c| c.missing_header_ids.clone())
            .collect::<Vec<_>>()
            .concat(),
        ["h-missing"]
    );
    assert_eq!(
        model
            .cells
            .iter()
            .filter_map(|c| c.id.as_deref())
            .collect::<Vec<_>>(),
        ["h-region", "h-sales", "h-q1", "h-q2", "h-north", "h-south"]
    );
    // Smoke: only the last cell has a list entry nobody answers.
    assert_eq!(model.cells[9].missing_header_ids.len(), 1);
}

// ---- pdf 2.0 against pdf 1.7 ------------------------------------------------------------------

/// A table of `rows` of `(role, extra entries)` cells under a `Document` (object 7), one line of
/// text per cell. Objects: table 8, rows from 9, then the cells.
fn table_doc(version: PdfVersion, namespaced: bool, rows: &[&[(&str, &str)]]) -> Document {
    let ns = if namespaced { " /NS 91 0 R" } else { "" };
    let mut content = String::new();
    let mut objects: Vec<(u32, String)> = Vec::new();
    let mut owners = Vec::new();
    let mut next = 9 + u32::try_from(rows.len()).unwrap();
    let mut row_refs = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let row_obj = 9 + u32::try_from(r).unwrap();
        row_refs.push(format!("{row_obj} 0 R"));
        let mut cell_refs = Vec::new();
        for (c, (role, extra)) in row.iter().enumerate() {
            let mcid = owners.len();
            let _ = writeln!(
                content,
                "/{role} << /MCID {mcid} >> BDC BT /F1 12 Tf {} {} Td (r{r}c{c}) Tj ET EMC",
                20 + 60 * c,
                380 - 20 * r
            );
            objects.push((
                next,
                format!(
                    "<< /Type /StructElem /S /{role} /P {row_obj} 0 R /Pg 3 0 R {extra}{ns} /K {mcid} >>"
                ),
            ));
            owners.push(next);
            cell_refs.push(format!("{next} 0 R"));
            next += 1;
        }
        objects.push((
            row_obj,
            format!(
                "<< /Type /StructElem /S /TR /P 8 0 R{ns} /K [{}] >>",
                cell_refs.join(" ")
            ),
        ));
    }
    objects.push((
        8,
        format!(
            "<< /Type /StructElem /S /Table /P 7 0 R{ns} /K [{}] >>",
            row_refs.join(" ")
        ),
    ));
    objects.push((
        7,
        format!("<< /Type /StructElem /S /Document /P 6 0 R{ns} /K [8 0 R] >>"),
    ));
    objects.sort_by_key(|(n, _)| *n);
    let mut w = PdfWriter::new(version);
    let ns_root = if namespaced { "/Namespaces [91 0 R]" } else { "" };
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /StructParents 0 \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(
        6,
        format!("<< /Type /StructTreeRoot /K 7 0 R {ns_root} /ParentTree 90 0 R /ParentTreeNextKey 1 >>"),
    );
    for (n, body) in &objects {
        w.object(*n, body);
    }
    w.object(
        90,
        format!(
            "<< /Nums [0 [{}]] >>",
            owners
                .iter()
                .map(|o| format!("{o} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    );
    w.object(91, "<< /Type /Namespace /NS (http://iso.org/pdf2/ssn) >>");
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("table.pdf", w.into_output().unwrap().bytes).unwrap()
}

/// A header row, a data row, a sub-header row and a data row, with no `Scope` anywhere.
const SUBHEADED: [&[(&str, &str)]; 4] = [
    &[("TH", ""), ("TH", "")],
    &[("TD", ""), ("TD", "")],
    &[("TH", ""), ("TH", "")],
    &[("TD", ""), ("TD", "")],
];

#[test]
fn the_pdf_2_search_stops_at_data_after_a_header_and_pdf_17_collects_every_column_header() {
    // Cells: row 0 objects 13 and 14, row 1 15 and 16, row 2 17 and 18, row 3 19 and 20.
    let pdf2 = table_doc(PdfVersion::V2_0, true, &SUBHEADED);
    let pdf17 = table_doc(PdfVersion::V1_7, false, &SUBHEADED);

    let last = |doc: &Document| {
        let model = table_of(doc, 8);
        (model.rules, associations(&model).pop().unwrap())
    };

    assert_eq!(
        last(&pdf2),
        (TableRules::Pdf2, (3, 1, vec!["struct:18:0 position".to_owned()]))
    );
    assert_eq!(
        last(&pdf17),
        (
            TableRules::Pdf17,
            (
                3,
                1,
                vec![
                    "struct:18:0 position".to_owned(),
                    "struct:14:0 position".to_owned()
                ]
            )
        )
    );
    // Smoke: a PDF 2.0 file without namespaces follows the PDF 1.7 rules.
    let no_namespaces = table_doc(PdfVersion::V2_0, false, &SUBHEADED);
    assert_eq!(table_of(&no_namespaces, 8).rules, TableRules::Pdf17);
}

#[test]
fn a_header_with_its_own_headers_list_ends_the_pdf_2_search_and_is_not_counted() {
    let rows: [&[(&str, &str)]; 3] = [
        &[("TH", "/ID (top)")],
        &[("TH", "/A << /O /Table /Headers [(top)] >>")],
        &[("TD", "")],
    ];
    let pdf2 = table_doc(PdfVersion::V2_0, true, &rows);
    let pdf17 = table_doc(PdfVersion::V1_7, false, &rows);

    // Cells: 12 (top), 13 (the header with a list), 14 (the data cell).
    assert_eq!(associations(&table_of(&pdf2, 8))[2], (2, 0, vec![]));
    assert_eq!(
        associations(&table_of(&pdf17, 8))[2],
        (
            2,
            0,
            vec![
                "struct:13:0 position".to_owned(),
                "struct:12:0 position".to_owned()
            ]
        )
    );
}

// ---- grids ------------------------------------------------------------------------------------

#[test]
fn a_short_row_is_irregular_and_leaves_an_empty_slot() {
    let rows: [&[(&str, &str)]; 2] = [&[("TH", ""), ("TH", ""), ("TH", "")], &[("TD", ""), ("TD", "")]];
    let doc = table_doc(PdfVersion::V1_7, false, &rows);

    let model = table_of(&doc, 8);

    assert_eq!((model.rows, model.cols), (2, 3));
    assert_eq!(model.slots, [Some(0), Some(1), Some(2), Some(3), Some(4), None]);
    assert_eq!(model.irregular, [IrregularRow { row: 1, width: 2 }]);
    assert_eq!(model.cell_at(1, 2), None);
    // Smoke.
    assert_eq!(model.irregular.len(), 1);
}

#[test]
fn a_row_span_pushes_the_cells_of_the_next_row_and_a_span_past_the_table_is_clipped() {
    let rows: [&[(&str, &str)]; 2] = [
        &[("TD", "/A << /O /Table /RowSpan 5 >>"), ("TD", "")],
        &[("TD", "")],
    ];
    let doc = table_doc(PdfVersion::V1_7, false, &rows);

    let model = table_of(&doc, 8);

    assert_eq!(
        model
            .cells
            .iter()
            .map(|c| (c.row, c.col, c.row_span))
            .collect::<Vec<_>>(),
        [(0, 0, 5), (0, 1, 1), (1, 1, 1)]
    );
    assert_eq!((model.rows, model.cols), (2, 2));
    assert_eq!(model.slots, [Some(0), Some(1), Some(0), Some(2)]);
    assert_eq!(model.irregular, []);
}

#[test]
fn row_groups_and_custom_cell_roles_are_followed_through_the_role_map() {
    let content = "\
/Cell << /MCID 0 >> BDC BT /F1 12 Tf 20 380 Td (head) Tj ET EMC
/TD << /MCID 1 >> BDC BT /F1 12 Tf 20 360 Td (body) Tj ET EMC
/TD << /MCID 2 >> BDC BT /F1 12 Tf 20 340 Td (foot) Tj ET EMC";
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << /Font << /F1 5 0 R >> >> \
         /Contents 4 0 R >>",
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(6, "<< /Type /StructTreeRoot /K 7 0 R /RoleMap << /Cell /TD >> >>");
    w.object(7, "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R] >>");
    w.object(
        8,
        "<< /Type /StructElem /S /Table /P 7 0 R /K [9 0 R 11 0 R 13 0 R] >>",
    );
    for (group, row, cell, mcid, role) in [
        (9, 10, 15, 0, "Cell"),
        (11, 12, 16, 1, "TD"),
        (13, 14, 17, 2, "TD"),
    ] {
        let section = match group {
            9 => "THead",
            11 => "TBody",
            _ => "TFoot",
        };
        w.object(
            group,
            format!("<< /Type /StructElem /S /{section} /P 8 0 R /K [{row} 0 R] >>"),
        );
        w.object(
            row,
            format!("<< /Type /StructElem /S /TR /P {group} 0 R /K [{cell} 0 R] >>"),
        );
        w.object(
            cell,
            format!("<< /Type /StructElem /S /{role} /P {row} 0 R /Pg 3 0 R /K {mcid} >>"),
        );
    }
    w.finish_table(&Trailer::new(1)).unwrap();
    let doc = Document::from_bytes("groups.pdf", w.into_output().unwrap().bytes).unwrap();

    let model = table_of(&doc, 8);

    assert_eq!(
        model
            .cells
            .iter()
            .map(|c| (c.node.clone(), c.group, c.row))
            .collect::<Vec<_>>(),
        [
            (elem(15), RowGroup::Head, 0),
            (elem(16), RowGroup::Body, 1),
            (elem(17), RowGroup::Foot, 2)
        ]
    );
    // Smoke: a custom role mapped to TD is a data cell.
    assert!(model.cells.iter().all(|c| !c.header));
}

// ---- queries ----------------------------------------------------------------------------------

#[test]
fn only_table_elements_have_a_model_and_a_page_lists_its_tables() {
    let doc = fixture("table-simple-pdf17");

    assert_eq!(doc.table(&elem(7)), None);
    assert_eq!(doc.table(&elem(12)), None);
    assert_eq!(doc.table(&NodeId::Page(0)), None);
    assert_eq!(doc.table(&elem(999)), None);
    assert_eq!(doc.tables(0), [elem(8)]);
    assert_eq!(doc.tables(1), []);
    // Smoke: another fixture has no table at all.
    assert_eq!(fixture("speech-lang").tables(0), []);
}
