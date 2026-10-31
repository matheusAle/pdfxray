//! Tables: the grid of the `Table` element behind the selection, for the canvas overlay and the
//! inspector's Table section.

use std::sync::Arc;

use pdfxray_core::{Document, NodeId, TableCell, TableModel};

use super::AppState;

/// Characters of a cell's text kept for the mini grid and the header list.
const CELL_TEXT: usize = 24;

/// A table's grid with the text of each cell.
#[derive(Debug)]
pub struct TableData {
    /// The grid, the header cells and the rules they were found with.
    pub model: TableModel,
    /// The start of the text of each cell of `model.cells`, in the same order.
    texts: Vec<String>,
}

impl TableData {
    /// Reads the table `id`; `None` when it is not a `Table` element.
    pub(super) fn read(doc: &Document, id: &NodeId) -> Option<Self> {
        let model = doc.table(id)?;
        let texts = model
            .cells
            .iter()
            .map(|cell| doc.text_preview(&cell.node, CELL_TEXT).unwrap_or_default())
            .collect();
        Some(Self { model, texts })
    }

    /// The start of the text of cell `at` of the model; empty for an empty cell.
    pub fn text(&self, at: usize) -> &str {
        self.texts.get(at).map_or("", String::as_str)
    }

    /// The index in `model.cells` of the cell of element `node`.
    pub fn index_of(&self, node: &NodeId) -> Option<usize> {
        self.model.cells.iter().position(|cell| cell.node == *node)
    }

    /// True when the cell cannot be read with its header: a data cell no header was found for, or
    /// one whose `/Headers` names an ID that no cell of the table has.
    pub fn lacks_header(cell: &TableCell) -> bool {
        (!cell.header && cell.headers.is_empty()) || !cell.missing_header_ids.is_empty()
    }
}

/// The table a selection is in and the cell it is, or is inside.
#[derive(Debug, Clone)]
pub struct TableContext {
    /// The innermost table that holds the selection.
    pub data: Arc<TableData>,
    /// The cell, as an index into `data.model.cells`; `None` when the selection is the table or a
    /// row group.
    pub cell: Option<usize>,
}

impl AppState {
    /// The table that holds `id` (a structure element, or marked content owned by one), and the
    /// cell it is part of; `None` when it is in no table.
    pub fn table_context(&mut self, id: &NodeId) -> Option<TableContext> {
        let doc = &self.document.as_ref()?.doc;
        let element = match id {
            NodeId::StructElem(_) => id.clone(),
            NodeId::MarkedContent { page, mcid } => self.derived.tags()?.owner_of(*page, *mcid)?.clone(),
            _ => return None,
        };
        let chain = doc.breadcrumb(&element);
        for (at, node) in chain.iter().enumerate().rev() {
            if !matches!(node, NodeId::StructElem(_)) {
                continue;
            }
            let Some(data) = self.derived.table(doc, node) else {
                continue;
            };
            let cell = chain[at + 1..]
                .iter()
                .rev()
                .find_map(|inner| data.index_of(inner));
            return Some(TableContext { data, cell });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::ObjRef;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    fn table(name: &str, id: u32) -> TableData {
        let doc = fixture(name);
        TableData::read(&doc, &NodeId::StructElem(ObjRef::new(id, 0))).unwrap()
    }

    #[test]
    fn a_cell_lacks_its_header_when_none_was_found_or_an_id_names_no_cell() {
        let ids = table("table-headers-ids", 8);
        let cell = |n: u32| {
            let node = NodeId::StructElem(ObjRef::new(n, 0));
            &ids.model.cells[ids.index_of(&node).unwrap()]
        };

        // North is a header cell and 1.2 has its headers; 1.1 names an ID that no cell has.
        assert_eq!(
            [17, 18, 22].map(|n| TableData::lacks_header(cell(n))),
            [false, false, true]
        );
        let last = ids.index_of(&NodeId::StructElem(ObjRef::new(22, 0))).unwrap();
        assert_eq!(ids.text(last), "1.1");
        // Smoke: the table with every header resolved has no such cell.
        let simple = table("table-simple-pdf17", 8);
        assert!(!simple.model.cells.iter().any(TableData::lacks_header));
    }

    #[test]
    fn a_node_that_is_not_a_table_has_no_data() {
        let doc = fixture("table-headers-ids");

        assert!(TableData::read(&doc, &NodeId::StructElem(ObjRef::new(13, 0))).is_none());
        assert!(TableData::read(&doc, &NodeId::Document).is_none());
    }
}
