//! Tables: the grid of a `Table` element and the header cells of every cell.
//!
//! A table is read from `Table` → (`THead` | `TBody` | `TFoot`)? → `TR` → `TH` | `TD` (ISO 32000-1
//! §14.8.4.3.4, Table 337; the attributes are those of §14.8.5.7, Table 349). Standard types are
//! the ones the cells' roles resolve to, so a custom role mapped to `TD` is a cell.
//!
//! # Grid
//!
//! Cells are placed row by row, left to right, in the first free slot, skipping the slots that a
//! `RowSpan` from a row above still covers. `ColSpan` and `RowSpan` (attribute owner `Table`, both
//! default 1) widen and lengthen a cell; a span that leaves the table is clipped to it. A row
//! whose width, spans included, differs from the widest row is *irregular*.
//!
//! # Header cells
//!
//! Three rules resolve which header cells belong to a cell, in this order, and the model keeps
//! the rule that did ([`HeaderSource`]):
//!
//! 1. **`/Headers`.** An explicit list of `/ID`s of header cells of the table overrides everything
//!    else. A listed cell that has a `/Headers` list of its own adds that list, recursively.
//! 2. **`/Scope`.** A `TH` with `Scope` `Row`, `Column` or `Both` heads the cells to its right, the
//!    cells below it or both.
//! 3. **Position.** A `TH` without `Scope` heads by where it sits.
//!
//! What 2 and 3 mean depends on the [`TableRules`] of the document:
//!
//! - **PDF 2.0** (a PDF 2.0 file whose structure tree declares namespaces), following the
//!   header-finding algorithm of ISO 32000-2 §14.8.4.8.3 as the PDF Association's errata describe
//!   it: from the cell, search toward the start of the table, upward for each column the cell
//!   spans, then leftward for each row it spans. Header cells met on the way belong to the cell.
//!   The search of a line stops at the table edge, at a data cell met after a header cell was
//!   found, and at a header cell that has a `/Headers` list of its own (that header is not counted
//!   either: it speaks for itself). A `Scope` that conflicts
//!   with the direction (`Row` while searching upward, `Column` while searching leftward) neither
//!   counts nor stops the search. A `TH` without `Scope` counts in both directions. Results run
//!   from the nearest, most specific header to the farthest, the column headers first.
//! - **PDF 1.x** (and any file without namespaces), the PDF 1.7 rules: every `TH` is a column
//!   header, a row header or both. Its `Scope` says which; without one, a `TH` is a column header
//!   when it sits in a table head or in a row that holds no `TD`, otherwise a row header. A column
//!   header heads every cell below it in the columns it spans, a row header every cell to its
//!   right in the rows it spans. No search stops early, and no namespace is consulted.
//!
//! The ISO text itself was read only through its public errata and the Well-Tagged PDF
//! specification; the searches are an implementation of that description.

use std::collections::{HashMap, HashSet};

use pdfxray_raw::Dict;
use serde::Serialize;

use super::parse::{Builder, Rd};
use super::text;
use super::{Kid, TagTree};
use crate::numeric::usize_from_u32;
use crate::{Document, NodeId, PageIx, Severity};

/// Longest span honoured, so a hostile `RowSpan` cannot allocate a huge grid.
const MAX_SPAN: i64 = 1000;
/// Deepest nesting of row groups inside a table that is followed.
const MAX_GROUP_DEPTH: u32 = 4;

/// The `Scope` of a header cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableScope {
    /// The header applies to the cells of its row.
    Row,
    /// The header applies to the cells of its column.
    Column,
    /// The header applies to both.
    Both,
}

impl TableScope {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "Row" => Some(Self::Row),
            "Column" => Some(Self::Column),
            "Both" => Some(Self::Both),
            _ => None,
        }
    }

    /// True when a header with this scope applies to the cells below it (`up` is true) or to the
    /// cells to its right.
    fn applies(self, up: bool) -> bool {
        matches!(
            (self, up),
            (Self::Both, _) | (Self::Column, true) | (Self::Row, false)
        )
    }
}

/// The typed attributes of an attribute object owned by `Table`. See [`TableAttrs::read`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct TableAttrs {
    row_span: Option<i64>,
    col_span: Option<i64>,
    scope: Option<TableScope>,
    /// `/Headers`: the `/ID`s of header cells.
    headers: Option<Vec<String>>,
}

impl TableAttrs {
    /// Reads `/RowSpan`, `/ColSpan`, `/Scope` and `/Headers` of an attribute dictionary.
    pub(super) fn read(rd: Rd<'_>, d: &Dict) -> Self {
        Self {
            row_span: rd.int(d, b"RowSpan"),
            col_span: rd.int(d, b"ColSpan"),
            scope: rd.name(d, b"Scope").and_then(|n| TableScope::parse(&n)),
            headers: rd.get(d, b"Headers").map(|v| {
                rd.list(v)
                    .into_iter()
                    .filter_map(|id| match rd.deref(id) {
                        pdfxray_raw::Object::String(s) => Some(text::decode(&s.bytes)),
                        _ => None,
                    })
                    .collect()
            }),
        }
    }
}

/// Which header-finding rules a document's tables follow. See the module documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TableRules {
    /// PDF 1.7: `Scope`, else position; no namespaces.
    #[default]
    Pdf17,
    /// PDF 2.0: the header-finding algorithm of ISO 32000-2 §14.8.4.8.3.
    Pdf2,
}

/// Which rule associated a header cell with a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderSource {
    /// The cell's `/Headers` list (or the `/Headers` list of a header it named).
    Headers,
    /// The header's `/Scope`.
    Scope,
    /// The header's position: it has no `Scope`.
    Position,
}

/// A header cell that belongs to a cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeaderRef {
    /// The `TH` (or other cell) element.
    pub cell: NodeId,
    /// The rule that made it a header of the cell.
    pub via: HeaderSource,
}

/// Where a row sits in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowGroup {
    /// Inside `THead`.
    Head,
    /// Inside `TBody`, or directly inside the `Table`.
    Body,
    /// Inside `TFoot`.
    Foot,
}

/// One cell of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableCell {
    /// The `TH` or `TD` element.
    pub node: NodeId,
    /// True for a header cell (`TH`).
    pub header: bool,
    /// The row group the cell's row sits in.
    pub group: RowGroup,
    /// First row, from 0.
    pub row: u32,
    /// First column, from 0.
    pub col: u32,
    /// Rows covered (`RowSpan`, at least 1).
    pub row_span: u32,
    /// Columns covered (`ColSpan`, at least 1).
    pub col_span: u32,
    /// The cell's `Scope`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<TableScope>,
    /// The cell's `/ID`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The header cells of this cell, nearest and most specific first.
    pub headers: Vec<HeaderRef>,
    /// `/Headers` entries that name no cell of this table.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_header_ids: Vec<String>,
}

/// A row whose width, spans included, differs from the table's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IrregularRow {
    /// The row, from 0.
    pub row: u32,
    /// Slots the row covers.
    pub width: u32,
}

/// The grid of a table and the header cells of each cell. See [`Document::table`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableModel {
    /// The `Table` element.
    pub table: NodeId,
    /// The rules the header cells were resolved with.
    pub rules: TableRules,
    /// Number of rows (`TR` elements).
    pub rows: u32,
    /// Number of columns: the widest row, spans included.
    pub cols: u32,
    /// The cells in reading order.
    pub cells: Vec<TableCell>,
    /// Row-major `rows * cols` slots: the index into [`TableModel::cells`] of the cell covering
    /// the slot, `None` for a slot no cell covers.
    pub slots: Vec<Option<usize>>,
    /// The rows that are not as wide as the table.
    pub irregular: Vec<IrregularRow>,
}

impl TableModel {
    /// The cell covering row `row`, column `col`.
    #[must_use]
    pub fn cell_at(&self, row: u32, col: u32) -> Option<&TableCell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        let slot = usize_from_u32(row) * usize_from_u32(self.cols) + usize_from_u32(col);
        self.cells.get((*self.slots.get(slot)?)?)
    }

    /// The cell of the structure element `node`.
    #[must_use]
    pub fn cell_of(&self, node: &NodeId) -> Option<&TableCell> {
        self.cells.iter().find(|c| c.node == *node)
    }
}

/// A cell as placed, before header cells are resolved.
struct Raw {
    ei: usize,
    header: bool,
    group: RowGroup,
    scope: Option<TableScope>,
    id: Option<String>,
    explicit: Option<Vec<String>>,
    row: usize,
    col: usize,
    row_span: usize,
    col_span: usize,
    /// True when the cell's row holds a data cell.
    row_has_data: bool,
}

impl Raw {
    /// The scope the cell heads by: its `Scope`, else what its position says.
    fn kind(&self) -> TableScope {
        self.scope
            .unwrap_or(if self.group == RowGroup::Head || !self.row_has_data {
                TableScope::Column
            } else {
                TableScope::Row
            })
    }

    fn via(&self) -> HeaderSource {
        if self.scope.is_some() {
            HeaderSource::Scope
        } else {
            HeaderSource::Position
        }
    }
}

/// The placed cells of a table.
struct Grid {
    raws: Vec<Raw>,
    slots: Vec<Option<usize>>,
    rows: usize,
    cols: usize,
}

impl Grid {
    fn at(&self, row: usize, col: usize) -> Option<usize> {
        self.slots.get(row * self.cols + col).copied().flatten()
    }

    /// Resolves the header cells of cell `i`. Returns them with the `/Headers` entries that name
    /// no cell of the table.
    fn headers_of(
        &self,
        i: usize,
        rules: TableRules,
        by_id: &HashMap<&str, usize>,
    ) -> (Vec<(usize, HeaderSource)>, Vec<String>) {
        let cell = &self.raws[i];
        if cell.explicit.as_ref().is_some_and(|ids| !ids.is_empty()) {
            let mut found = Vec::new();
            let mut missing = Vec::new();
            let mut seen = HashSet::from([i]);
            self.explicit(i, by_id, &mut seen, &mut found, &mut missing);
            return (found, missing);
        }
        let found = match rules {
            TableRules::Pdf2 => self.search_pdf2(i),
            TableRules::Pdf17 => self.search_pdf17(i),
        };
        (found, Vec::new())
    }

    /// Adds the cells named by the `/Headers` list of `i`, and theirs in turn.
    fn explicit(
        &self,
        i: usize,
        by_id: &HashMap<&str, usize>,
        seen: &mut HashSet<usize>,
        found: &mut Vec<(usize, HeaderSource)>,
        missing: &mut Vec<String>,
    ) {
        for id in self.raws[i].explicit.iter().flatten() {
            match by_id.get(id.as_str()) {
                Some(&j) => {
                    if seen.insert(j) {
                        found.push((j, HeaderSource::Headers));
                        self.explicit(j, by_id, seen, found, missing);
                    }
                }
                None => missing.push(id.clone()),
            }
        }
    }

    /// The columns `cell` covers and the rows it covers, clipped to the grid.
    fn spans(&self, cell: &Raw) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        (
            cell.col..(cell.col + cell.col_span).min(self.cols),
            cell.row..(cell.row + cell.row_span).min(self.rows),
        )
    }

    /// The PDF 2.0 header-finding algorithm (see the module documentation).
    fn search_pdf2(&self, i: usize) -> Vec<(usize, HeaderSource)> {
        let cell = &self.raws[i];
        let (cols, rows) = self.spans(cell);
        let mut out: Vec<(usize, HeaderSource)> = Vec::new();
        let mut seen = HashSet::new();
        // Upward for each column, then leftward for each row.
        for up in [true, false] {
            let lines = if up { cols.clone() } else { rows.clone() };
            let start = if up { cell.row } else { cell.col };
            for line in lines {
                let mut found = false;
                for pos in (0..start).rev() {
                    let at = if up {
                        self.at(pos, line)
                    } else {
                        self.at(line, pos)
                    };
                    let Some(j) = at.filter(|&j| j != i) else { continue };
                    let other = &self.raws[j];
                    if !other.header {
                        if found {
                            break;
                        }
                        continue;
                    }
                    if other.explicit.is_some() {
                        break;
                    }
                    if other.scope.is_some_and(|s| !s.applies(up)) {
                        continue;
                    }
                    found = true;
                    if seen.insert(j) {
                        out.push((j, other.via()));
                    }
                }
            }
        }
        out
    }

    /// The PDF 1.7 rules: every header cell heads by its scope or position, nothing stops a search.
    fn search_pdf17(&self, i: usize) -> Vec<(usize, HeaderSource)> {
        let cell = &self.raws[i];
        let (cols, rows) = self.spans(cell);
        let mut out: Vec<(usize, HeaderSource)> = Vec::new();
        let mut seen = HashSet::new();
        for up in [true, false] {
            let lines = if up { cols.clone() } else { rows.clone() };
            let start = if up { cell.row } else { cell.col };
            for line in lines {
                for pos in (0..start).rev() {
                    let at = if up {
                        self.at(pos, line)
                    } else {
                        self.at(line, pos)
                    };
                    let Some(j) = at.filter(|&j| j != i) else { continue };
                    let other = &self.raws[j];
                    if other.header && other.kind().applies(up) && seen.insert(j) {
                        out.push((j, other.via()));
                    }
                }
            }
        }
        out
    }
}

impl TagTree {
    fn standard(&self, ei: usize) -> Option<&str> {
        self.elems[ei].resolved.standard.as_deref()
    }

    /// The typed table attributes of element `ei`: the first attribute object that sets each.
    fn table_attrs(&self, ei: usize) -> TableAttrs {
        let mut out = TableAttrs::default();
        for set in &self.elems[ei].attrs {
            let t = &set.table;
            out.row_span = out.row_span.or(t.row_span);
            out.col_span = out.col_span.or(t.col_span);
            out.scope = out.scope.or(t.scope);
            if out.headers.is_none() {
                out.headers.clone_from(&t.headers);
            }
        }
        out
    }

    /// The `TR` elements of table (or row group) `ei`, in order, with their row groups.
    fn table_rows(&self, ei: usize, group: RowGroup, depth: u32, out: &mut Vec<(usize, RowGroup)>) {
        for kid in &self.elems[ei].kids {
            let Kid::Elem(k) = kid else { continue };
            match (self.standard(*k), depth < MAX_GROUP_DEPTH) {
                (Some("TR"), _) => out.push((*k, group)),
                (Some("THead"), true) => self.table_rows(*k, RowGroup::Head, depth + 1, out),
                (Some("TBody"), true) => self.table_rows(*k, RowGroup::Body, depth + 1, out),
                (Some("TFoot"), true) => self.table_rows(*k, RowGroup::Foot, depth + 1, out),
                _ => {}
            }
        }
    }

    /// Places the cells of the rows in a grid.
    fn place_cells(&self, trs: &[(usize, RowGroup)]) -> Grid {
        let mut raws: Vec<Raw> = Vec::new();
        // Per row, the slots taken so far: `Some(cell)`.
        let mut taken: Vec<Vec<Option<usize>>> = vec![Vec::new(); trs.len()];
        for (r, &(tr, group)) in trs.iter().enumerate() {
            let cells: Vec<usize> = self.elems[tr]
                .kids
                .iter()
                .filter_map(|k| match k {
                    Kid::Elem(c) if matches!(self.standard(*c), Some("TH" | "TD")) => Some(*c),
                    _ => None,
                })
                .collect();
            let row_has_data = cells.iter().any(|&c| self.standard(c) == Some("TD"));
            let mut col = 0;
            for ei in cells {
                while taken[r].get(col).is_some_and(Option::is_some) {
                    col += 1;
                }
                let attrs = self.table_attrs(ei);
                let span = |v: Option<i64>| usize::try_from(v.unwrap_or(1).clamp(1, MAX_SPAN)).unwrap_or(1);
                let (row_span, col_span) = (span(attrs.row_span), span(attrs.col_span));
                let index = raws.len();
                for taken_row in taken.iter_mut().skip(r).take(row_span) {
                    if taken_row.len() < col + col_span {
                        taken_row.resize(col + col_span, None);
                    }
                    for slot in &mut taken_row[col..col + col_span] {
                        slot.get_or_insert(index);
                    }
                }
                raws.push(Raw {
                    ei,
                    header: self.standard(ei) == Some("TH"),
                    group,
                    scope: attrs.scope,
                    id: self.elems[ei].id.clone(),
                    explicit: attrs.headers,
                    row: r,
                    col,
                    row_span,
                    col_span,
                    row_has_data,
                });
                col += col_span;
            }
        }
        let cols = taken.iter().map(Vec::len).max().unwrap_or(0);
        let slots = taken
            .into_iter()
            .flat_map(|mut row| {
                row.resize(cols, None);
                row
            })
            .collect();
        Grid {
            raws,
            slots,
            rows: trs.len(),
            cols,
        }
    }

    /// The model of table element `ei` under `rules`; `None` when it is not a `Table`.
    pub(super) fn table_model(&self, ei: usize, rules: TableRules) -> Option<TableModel> {
        if self.standard(ei) != Some("Table") {
            return None;
        }
        let mut rows = Vec::new();
        self.table_rows(ei, RowGroup::Body, 0, &mut rows);
        let grid = self.place_cells(&rows);
        let by_id: HashMap<&str, usize> = grid
            .raws
            .iter()
            .enumerate()
            .filter_map(|(i, c)| Some((c.id.as_deref()?, i)))
            .collect();
        let node = |i: usize| self.id_of(grid.raws[i].ei);
        let cells = (0..grid.raws.len())
            .map(|i| {
                let raw = &grid.raws[i];
                let (found, missing) = grid.headers_of(i, rules, &by_id);
                TableCell {
                    node: node(i),
                    header: raw.header,
                    group: raw.group,
                    row: to_u32(raw.row),
                    col: to_u32(raw.col),
                    row_span: to_u32(raw.row_span),
                    col_span: to_u32(raw.col_span),
                    scope: raw.scope,
                    id: raw.id.clone(),
                    headers: found
                        .into_iter()
                        .map(|(j, via)| HeaderRef { cell: node(j), via })
                        .collect(),
                    missing_header_ids: missing,
                }
            })
            .collect();
        let irregular = (0..grid.rows)
            .filter_map(|r| {
                let width = (0..grid.cols).filter(|&c| grid.at(r, c).is_some()).count();
                (width != grid.cols).then(|| IrregularRow {
                    row: to_u32(r),
                    width: to_u32(width),
                })
            })
            .collect();
        Some(TableModel {
            table: self.id_of(ei),
            rules,
            rows: to_u32(grid.rows),
            cols: to_u32(grid.cols),
            cells,
            slots: grid.slots,
            irregular,
        })
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl Builder<'_> {
    /// The header-finding rules for this document: PDF 2.0's when the file is PDF 2.0 and its
    /// structure tree declares namespaces, otherwise PDF 1.7's.
    pub(super) fn table_rules(&self) -> TableRules {
        if self.rd.doc.raw.header().version.major >= 2 && !self.tree.namespaces.is_empty() {
            TableRules::Pdf2
        } else {
            TableRules::Pdf17
        }
    }

    /// Reports tables without headers, data cells without header cells, `/Headers` entries that
    /// name no cell, and irregular rows.
    pub(super) fn check_tables(&mut self) {
        /// A finding about element `ei`.
        type Found = (&'static str, Severity, String, usize);
        let rules = self.tree.table_rules;
        let mut found: Vec<Found> = Vec::new();
        for ei in 0..self.tree.elems.len() {
            let Some(model) = self.tree.table_model(ei, rules) else {
                continue;
            };
            let label = |node: &NodeId| match node {
                NodeId::StructElem(r) => r.to_string(),
                other => other.to_string(),
            };
            let at = |node: &NodeId| match node {
                NodeId::StructElem(r) => self.tree.index.get(r).copied().unwrap_or(ei),
                _ => ei,
            };
            let has_header = model.cells.iter().any(|c| c.header);
            if !has_header && model.rows >= 2 && model.cols >= 2 {
                found.push((
                    "table-header-missing",
                    Severity::Warning,
                    format!(
                        "{} is a table of {} rows and {} columns with no header cell (TH)",
                        label(&model.table),
                        model.rows,
                        model.cols
                    ),
                    ei,
                ));
            }
            for cell in &model.cells {
                for id in &cell.missing_header_ids {
                    found.push((
                        "table-headers-id-missing",
                        Severity::Warning,
                        format!(
                            "{} lists /Headers ID ({id}), which no cell of its table has",
                            label(&cell.node)
                        ),
                        at(&cell.node),
                    ));
                }
                if has_header && !cell.header && cell.headers.is_empty() && cell.missing_header_ids.is_empty()
                {
                    found.push((
                        "table-header-unresolved",
                        Severity::Info,
                        format!(
                            "{} (row {}, column {}) has no header cell",
                            label(&cell.node),
                            cell.row + 1,
                            cell.col + 1
                        ),
                        at(&cell.node),
                    ));
                }
            }
            if !model.irregular.is_empty() {
                let rows: Vec<String> = model
                    .irregular
                    .iter()
                    .map(|r| format!("row {} covers {}", r.row + 1, r.width))
                    .collect();
                found.push((
                    "table-irregular",
                    Severity::Warning,
                    format!(
                        "{} is {} columns wide, but {}",
                        label(&model.table),
                        model.cols,
                        rows.join(", ")
                    ),
                    ei,
                ));
            }
        }
        for (code, severity, message, ei) in found {
            let obj = self.tree.elems[ei].obj;
            self.find_on(code, severity, message, Some(obj), Some(ei));
        }
    }
}

impl Document {
    /// The grid and header cells of the `Table` element `id`: rows by columns from its `TR`, `TH`
    /// and `TD` (inside `THead`, `TBody` and `TFoot` or not), `RowSpan` and `ColSpan` applied, and
    /// for each cell the header cells found by `/Headers`, then `/Scope`, then position. A PDF 2.0
    /// file with namespaces follows the header-finding algorithm of ISO 32000-2; any other file
    /// the PDF 1.7 rules (see [`TableRules`]). `None` when `id` is not a table of the structure
    /// tree.
    #[must_use]
    pub fn table(&self, id: &NodeId) -> Option<TableModel> {
        let NodeId::StructElem(r) = id else {
            return None;
        };
        let ei = *self.tags.index.get(r)?;
        self.tags.table_model(ei, self.tags.table_rules)
    }

    /// The `Table` elements with content on `page`, in document order.
    #[must_use]
    pub fn tables(&self, page: PageIx) -> Vec<NodeId> {
        let tree = &self.tags;
        (0..tree.elems.len())
            .filter(|&ei| tree.standard(ei) == Some("Table"))
            .filter(|&ei| tree.bbox_on(self, ei, page).is_some())
            .map(|ei| tree.id_of(ei))
            .collect()
    }
}
