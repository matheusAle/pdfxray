//! Table structure shared by the `table-*` fixtures: a `Table` of `TR` rows and `TH`/`TD` cells,
//! each with a line of text, optional `/ID` and `Table` attributes.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::common::tagged_text;
use crate::expected::{AttributeSetFacts, NodeFacts};

/// Where the table's first row is painted, and the distance between rows and columns.
const ORIGIN: (i32, i32) = (40, 340);
const ROW_STEP: i32 = 20;
const COL_STEP: i32 = 80;

/// One cell of a table.
pub(super) struct Cell {
    /// `TH` or `TD`.
    pub role: &'static str,
    /// The text painted in the cell.
    pub text: &'static str,
    /// Grid position, `(column, row)`, where the text is painted.
    pub at: (i32, i32),
    /// `/ID`.
    pub id: Option<&'static str>,
    /// Entries of the `Table` attribute object, `(key, value in PDF syntax)`.
    pub attrs: &'static [(&'static str, &'static str)],
}

impl Cell {
    /// A cell with nothing but text.
    pub(super) const fn new(role: &'static str, text: &'static str, at: (i32, i32)) -> Self {
        Self {
            role,
            text,
            at,
            id: None,
            attrs: &[],
        }
    }

    /// The same cell with an `/ID`.
    pub(super) const fn id(mut self, id: &'static str) -> Self {
        self.id = Some(id);
        self
    }

    /// The same cell with `Table` attributes.
    pub(super) const fn attrs(mut self, attrs: &'static [(&'static str, &'static str)]) -> Self {
        self.attrs = attrs;
        self
    }
}

/// Where a table's objects go and which namespace they belong to.
pub(super) struct Placement {
    /// Object number of the `Table`; its rows and then its cells follow.
    pub first_obj: u32,
    /// Object number of the parent element.
    pub parent: u32,
    /// Object number of the page.
    pub page: u32,
    /// Object number and URI of the namespace of every element, for PDF 2.0.
    pub namespace: Option<(u32, &'static str)>,
}

/// Everything a table adds to a fixture.
pub(super) struct TableParts {
    /// The content-stream lines, one per cell, MCIDs counting from 0 in cell order.
    pub content: String,
    /// The objects, `(number, body)`: table, rows, cells.
    pub objects: Vec<(u32, String)>,
    /// The facts of the `Table` element.
    pub node: NodeFacts,
    /// The owner of each MCID, for the parent tree.
    pub parents: Vec<Option<u32>>,
    /// `/ID` to object number, for the `/IDTree`.
    pub ids: BTreeMap<String, u32>,
    /// Number of cells.
    pub cells: usize,
}

/// Builds the table of `rows`.
pub(super) fn build(place: &Placement, rows: &[Vec<Cell>]) -> TableParts {
    let ns_entry = place
        .namespace
        .map_or_else(String::new, |(obj, _)| format!(" /NS {obj} 0 R"));
    let with_ns = |node: NodeFacts| match place.namespace {
        Some((_, uri)) => node.namespaces(uri, uri),
        None => node,
    };
    let row_objs: Vec<u32> = (1..).take(rows.len()).map(|i| place.first_obj + i).collect();
    let mut next = place.first_obj + 1 + u32::try_from(rows.len()).unwrap_or(0);
    let mut content = Vec::new();
    let mut objects = Vec::new();
    let mut parents = Vec::new();
    let mut ids = BTreeMap::new();
    let mut table = with_ns(NodeFacts::new(place.first_obj, "Table", Some("Table")));
    let mut mcid = 0;
    for (row, row_obj) in rows.iter().zip(&row_objs) {
        let mut tr = with_ns(NodeFacts::new(*row_obj, "TR", Some("TR")));
        let mut kids = String::new();
        for cell in row {
            let obj = next;
            next += 1;
            let mut entries = format!(
                "/Type /StructElem /S /{} /P {row_obj} 0 R /Pg {} 0 R",
                cell.role, place.page
            );
            let mut facts = with_ns(NodeFacts::new(obj, cell.role, Some(cell.role)).page(0));
            if let Some(id) = cell.id {
                let _ = write!(entries, " /ID ({id})");
                facts = facts.id(id);
                ids.insert(id.to_owned(), obj);
            }
            if !cell.attrs.is_empty() {
                entries.push_str(" /A << /O /Table");
                for (key, value) in cell.attrs {
                    let _ = write!(entries, " /{key} {value}");
                }
                entries.push_str(" >>");
                facts = facts.direct(AttributeSetFacts::new("Table", cell.attrs));
            }
            let _ = write!(entries, "{ns_entry} /K {mcid}");
            objects.push((obj, format!("<< {entries} >>")));
            let (col, line) = cell.at;
            content.push(tagged_text(
                cell.role,
                mcid,
                12,
                (ORIGIN.0 + COL_STEP * col, ORIGIN.1 - ROW_STEP * line),
                cell.text,
            ));
            parents.push(Some(obj));
            let _ = write!(kids, "{obj} 0 R ");
            tr = tr.elem(facts.mcr(0, mcid));
            mcid += 1;
        }
        objects.push((
            *row_obj,
            format!(
                "<< /Type /StructElem /S /TR /P {} 0 R{ns_entry} /K [{}] >>",
                place.first_obj,
                kids.trim_end()
            ),
        ));
        table = table.elem(tr);
    }
    let row_refs: Vec<String> = row_objs.iter().map(|o| format!("{o} 0 R")).collect();
    objects.push((
        place.first_obj,
        format!(
            "<< /Type /StructElem /S /Table /P {} 0 R{ns_entry} /K [{}] >>",
            place.parent,
            row_refs.join(" ")
        ),
    ));
    objects.sort_by_key(|(obj, _)| *obj);
    TableParts {
        content: content.join("\n"),
        objects,
        node: table,
        parents,
        ids,
        cells: usize::try_from(mcid).unwrap_or(0),
    }
}
