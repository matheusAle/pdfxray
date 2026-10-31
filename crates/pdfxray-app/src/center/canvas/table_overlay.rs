//! The table overlay of the canvas: while a table, a row, a cell or anything inside a cell is
//! selected, the table's grid is outlined in the table role colour, the selected cell is tinted
//! and a thin connector runs from it to each header cell that is read with it, labelled with the
//! rule that found the header: `Headers` (the cell's own `/Headers` list), `Scope` (the header's
//! `/Scope`) or `position` (a header without scope, found by where it sits). Cells that cannot be
//! read with a header are outlined in amber: a data cell no header was found for, or one whose
//! `/Headers` names an ID that no cell has.
//!
//! The line style says the same as the label: solid for `Headers`, dashed for `Scope`, dotted for
//! `position`. A label is a button that selects the header it points to.

use std::collections::HashMap;

use egui::{Color32, CursorIcon, Rect as UiRect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};
use pdfxray_core::{HeaderSource, NodeId, PageIx, Rect, RoleClass, TableCell};

use super::connector::{self, LineStyle};
use super::geometry::PageGeometry;
use crate::components::{Icon, tooltip};
use crate::roles::role_color;
use crate::state::{AppState, TableData};
use crate::theme::tokens;

/// Size of the amber marker on a cell without a header.
const MARK: f32 = 14.0;

/// The label of a connector: the rule that found the header.
pub const fn rule_label(via: HeaderSource) -> &'static str {
    match via {
        HeaderSource::Headers => "Headers",
        HeaderSource::Scope => "Scope",
        HeaderSource::Position => "position",
    }
}

/// What the rule means, for the tooltip of a label.
pub const fn rule_hint(via: HeaderSource) -> &'static str {
    match via {
        HeaderSource::Headers => "Named by this cell's /Headers list",
        HeaderSource::Scope => "The header's /Scope reaches this cell",
        HeaderSource::Position => "A header without /Scope, found by where it sits",
    }
}

const fn style_of(via: HeaderSource) -> LineStyle {
    match via {
        HeaderSource::Headers => LineStyle::Solid,
        HeaderSource::Scope => LineStyle::Dashed,
        HeaderSource::Position => LineStyle::Dotted,
    }
}

/// `text`, or `empty cell` for a cell with none.
fn shown(text: &str) -> &str {
    if text.is_empty() { "empty cell" } else { text }
}

/// The accessible name of a connector label: `Header via Scope: Q1`.
pub fn connector_name(header_text: &str, via: HeaderSource) -> String {
    format!("Header via {}: {}", rule_label(via), shown(header_text))
}

/// The accessible name of the marker of a cell without a header: `No header: 1.1`.
pub fn no_header_name(text: &str) -> String {
    format!("No header: {}", shown(text))
}

/// Why a cell is marked: what is wrong with its headers.
pub fn no_header_reason(cell: &pdfxray_core::TableCell) -> String {
    if cell.missing_header_ids.is_empty() {
        "No header cell was found for this cell".to_owned()
    } else {
        format!(
            "/Headers names {}, which no cell of the table has",
            cell.missing_header_ids.join(", ")
        )
    }
}

/// Room left outside the text of the outermost cells, in points.
const EDGE: f64 = 3.0;

/// A cell of the selected table that is on the page.
#[derive(Debug, Clone)]
struct CellBox {
    node: NodeId,
    /// The cell's slot of the grid (see [`snap`]).
    rect: Rect,
    text: String,
    /// Why the cell cannot be read with a header, when it cannot.
    missing: Option<String>,
}

/// A connector from the selected cell to one of its header cells, as indexes into the cells.
#[derive(Debug, Clone, Copy)]
struct Link {
    to: usize,
    via: HeaderSource,
}

/// What the overlay draws on one page.
#[derive(Debug, Default)]
pub(super) struct TableLayer {
    cells: Vec<CellBox>,
    /// The selected cell, as an index into `cells`.
    selected: Option<usize>,
    links: Vec<Link>,
    color: Option<Color32>,
}

/// Collects the table behind the selection that is on `page`: nothing when the selection is in no
/// table or the table has nothing on the page.
pub(super) fn collect(state: &mut AppState, page: PageIx) -> TableLayer {
    let Some(selection) = state.selection.primary.clone() else {
        return TableLayer::default();
    };
    let Some(context) = state.table_context(&selection) else {
        return TableLayer::default();
    };
    let boxes = state.tag_boxes(page);
    let by_node: HashMap<&NodeId, Rect> = boxes.iter().map(|tag| (&tag.id, tag.rect)).collect();
    let data: &TableData = &context.data;
    let mut layer = TableLayer {
        color: Some(role_color(RoleClass::Table)),
        ..TableLayer::default()
    };
    // The index of each model cell among the cells on this page.
    let mut placed: HashMap<usize, usize> = HashMap::new();
    let on_page: Vec<(usize, &TableCell, Rect)> = data
        .model
        .cells
        .iter()
        .enumerate()
        .filter_map(|(at, cell)| Some((at, cell, *by_node.get(&cell.node)?)))
        .collect();
    let slots = snap(
        &on_page
            .iter()
            .map(|&(_, cell, rect)| (cell, rect))
            .collect::<Vec<_>>(),
    );
    for ((at, cell, _), rect) in on_page.into_iter().zip(slots) {
        placed.insert(at, layer.cells.len());
        layer.cells.push(CellBox {
            node: cell.node.clone(),
            rect,
            text: data.text(at).to_owned(),
            missing: TableData::lacks_header(cell).then(|| no_header_reason(cell)),
        });
    }
    if let Some(at) = context.cell {
        layer.selected = placed.get(&at).copied();
        if layer.selected.is_some() {
            layer.links = data.model.cells[at]
                .headers
                .iter()
                .filter_map(|header| {
                    let to = *placed.get(&data.index_of(&header.cell)?)?;
                    Some(Link { to, via: header.via })
                })
                .collect();
        }
    }
    layer
}

/// The extent `(low, high)` along one axis of each column or row that has a cell covering only it.
type Extents = HashMap<u32, (f64, f64)>;

/// The extents along an axis of the cells that cover one slot of it: `at` and `span` say where a
/// cell is on the axis, `range` what it covers there. Rows are measured downwards.
fn extents(
    cells: &[(&TableCell, Rect)],
    at: impl Fn(&TableCell) -> (u32, u32),
    range: impl Fn(Rect) -> (f64, f64),
) -> Extents {
    let mut found = Extents::new();
    for (cell, rect) in cells {
        let (index, span) = at(cell);
        if span != 1 {
            continue;
        }
        let (lo, hi) = range(*rect);
        let e = found.entry(index).or_insert((lo, hi));
        *e = (e.0.min(lo), e.1.max(hi));
    }
    found
}

/// The low (`low`) or high edge of slot `at`: midway to its neighbour, or `EDGE` outside the
/// outermost. `None` when no cell tells where the slot is.
fn boundary(extents: &Extents, at: u32, low: bool) -> Option<f64> {
    let (lo, hi) = *extents.get(&at)?;
    Some(if low {
        match at.checked_sub(1).and_then(|before| extents.get(&before)) {
            Some(&(_, before_hi)) => f64::midpoint(before_hi, lo),
            None => lo - EDGE,
        }
    } else {
        match extents.get(&(at + 1)) {
            Some(&(after_lo, _)) => f64::midpoint(hi, after_lo),
            None => hi + EDGE,
        }
    })
}

/// The slot of each cell in the grid. A cell's own box hugs its text, so the boxes of a table are
/// small islands; the slots tile the table instead: each column is as wide as the widest text in
/// it and each row as tall as its tallest, and a boundary falls midway between two neighbours.
/// Spanning cells take the columns and rows they cover. A cell whose column or row cannot be told
/// (no cell that covers only it is on the page) keeps its own box.
fn snap(cells: &[(&TableCell, Rect)]) -> Vec<Rect> {
    let cols = extents(cells, |c| (c.col, c.col_span), |r| (r.x0, r.x1));
    // Page y grows upwards and rows run down, so rows are measured on `-y`.
    let rows = extents(cells, |c| (c.row, c.row_span), |r| (-r.y1, -r.y0));
    cells
        .iter()
        .map(|(cell, own)| {
            let x0 = boundary(&cols, cell.col, true);
            let x1 = boundary(&cols, cell.col + cell.col_span - 1, false);
            let top = boundary(&rows, cell.row, true);
            let bottom = boundary(&rows, cell.row + cell.row_span - 1, false);
            match (x0, x1, top, bottom) {
                (Some(x0), Some(x1), Some(top), Some(bottom)) if x0 < x1 && top < bottom => {
                    Rect::new(x0, -bottom, x1, -top)
                }
                _ => *own,
            }
        })
        .collect()
}

/// What the user did on the overlay this frame.
#[derive(Debug, Default)]
pub(super) struct Output {
    /// The cell whose marker or connector label was clicked.
    pub(super) clicked: Option<NodeId>,
}

/// Draws the overlay over the page at `geo`.
pub(super) fn draw(ui: &Ui, geo: &PageGeometry, page: PageIx, layer: &TableLayer) -> Output {
    let mut out = Output::default();
    let Some(color) = layer.color.filter(|_| !layer.cells.is_empty()) else {
        return out;
    };
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    let rects: Vec<UiRect> = layer
        .cells
        .iter()
        .map(|cell| geo.rect_to_screen(cell.rect))
        .collect();

    // The grid: every cell thin, the whole table a little stronger.
    for rect in &rects {
        painter.rect_stroke(
            *rect,
            0.0,
            Stroke::new(1.0, color.gamma_multiply(0.55)),
            StrokeKind::Outside,
        );
    }
    if let Some(table) = rects.iter().copied().reduce(UiRect::union) {
        painter.rect_stroke(
            table.expand(1.0),
            0.0,
            Stroke::new(1.5, color),
            StrokeKind::Outside,
        );
    }

    // Cells that cannot be read with a header.
    for (at, cell) in layer.cells.iter().enumerate() {
        let Some(reason) = &cell.missing else {
            continue;
        };
        let rect = rects[at];
        painter.rect(
            rect,
            0.0,
            tokens::WARN.gamma_multiply(0.10),
            Stroke::new(1.5, tokens::WARN),
            StrokeKind::Outside,
        );
        // On the cell's bottom-right corner, half outside: a value is often right-aligned, and
        // the selection's tag sits on the top edge.
        let mark = UiRect::from_center_size(rect.max, vec2(MARK, MARK));
        let response = ui
            .interact(
                mark,
                ui.make_persistent_id(("table-no-header", page, &cell.node)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        painter.circle_filled(mark.center(), MARK / 2.0, tokens::BG.gamma_multiply(0.9));
        if response.hovered() {
            painter.circle_filled(mark.center(), MARK / 2.0, tokens::WARN.gamma_multiply(0.25));
        }
        Icon::Warning.paint(ui, mark.shrink(1.0), MARK - 2.0, tokens::WARN);
        let name = no_header_name(&cell.text);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
        let response = tooltip::text(response, reason.clone());
        if response.clicked() {
            out.clicked = Some(cell.node.clone());
        }
    }

    let Some(selected) = layer.selected else {
        return out;
    };
    let from = rects[selected];
    painter.rect(
        from,
        0.0,
        tokens::ACCENT.gamma_multiply(0.18),
        Stroke::new(2.0, tokens::ACCENT),
        StrokeKind::Outside,
    );

    // Connectors, then their labels over them.
    let mut placed: Vec<UiRect> = Vec::new();
    for (n, link) in layer.links.iter().enumerate() {
        let to = rects[link.to];
        let (start, end) = (
            connector::edge_toward(from, to.center()),
            connector::edge_toward(to, from.center()),
        );
        connector::line(&painter, start, end, tokens::ACCENT, style_of(link.via));
        connector::anchor(&painter, start, tokens::ACCENT);
        connector::arrow_head(&painter, end, start, tokens::ACCENT);
        let header = &layer.cells[link.to];
        let name = connector_name(&header.text, link.via);
        let response = connector::label(
            ui,
            &painter,
            &mut placed,
            // Near the header's end: the labels of several headers spread out instead of
            // piling up on the cell they all start from.
            connector::beside(start.lerp(end, 0.7), end),
            &connector::Label {
                text: rule_label(link.via),
                ink: tokens::ACCENT,
                id: ui.make_persistent_id(("table-connector", page, n)),
                name: &name,
            },
        );
        let response = tooltip::text(response, rule_hint(link.via));
        if response.clicked() {
            out.clicked = Some(header.node.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{RowGroup, TableCell};
    use pretty_assertions::assert_eq;

    use super::*;

    fn cell(row: u32, col: u32, row_span: u32, col_span: u32) -> TableCell {
        TableCell {
            node: NodeId::Document,
            header: false,
            group: RowGroup::Body,
            row,
            col,
            row_span,
            col_span,
            scope: None,
            id: None,
            headers: Vec::new(),
            missing_header_ids: Vec::new(),
        }
    }

    #[test]
    fn the_rules_are_named_as_the_connector_labels_say() {
        assert_eq!(
            [HeaderSource::Headers, HeaderSource::Scope, HeaderSource::Position].map(rule_label),
            ["Headers", "Scope", "position"]
        );
        assert_eq!(connector_name("Q1", HeaderSource::Scope), "Header via Scope: Q1");
        assert_eq!(
            connector_name("", HeaderSource::Position),
            "Header via position: empty cell"
        );
        assert_eq!(no_header_name("1.1"), "No header: 1.1");
    }

    #[test]
    fn a_missing_id_is_named_in_the_reason_and_a_plain_miss_is_not() {
        let mut lacking = cell(1, 1, 1, 1);
        assert_eq!(
            no_header_reason(&lacking),
            "No header cell was found for this cell"
        );
        lacking.missing_header_ids = vec!["h-a".to_owned(), "h-b".to_owned()];
        assert_eq!(
            no_header_reason(&lacking),
            "/Headers names h-a, h-b, which no cell of the table has"
        );
    }

    #[test]
    fn cells_that_hug_their_text_are_snapped_to_a_grid_that_tiles_the_table() {
        let cells = [
            (cell(0, 0, 1, 1), Rect::new(10.0, 90.0, 30.0, 100.0)),
            (cell(0, 1, 1, 1), Rect::new(50.0, 90.0, 70.0, 100.0)),
            (cell(1, 0, 1, 1), Rect::new(10.0, 70.0, 25.0, 80.0)),
            (cell(1, 1, 1, 1), Rect::new(50.0, 70.0, 60.0, 80.0)),
        ];
        let refs: Vec<(&TableCell, Rect)> = cells.iter().map(|(c, r)| (c, *r)).collect();

        let slots = snap(&refs);

        // A boundary falls midway between neighbours, the table edge 3 points outside the text.
        assert_eq!(
            slots,
            [
                Rect::new(7.0, 85.0, 40.0, 103.0),
                Rect::new(40.0, 85.0, 73.0, 103.0),
                Rect::new(7.0, 67.0, 40.0, 85.0),
                Rect::new(40.0, 67.0, 73.0, 85.0),
            ]
        );
    }

    #[test]
    fn a_spanning_cell_takes_the_columns_it_covers_and_a_cell_nothing_places_keeps_its_box() {
        let cells = [
            (cell(0, 0, 1, 2), Rect::new(10.0, 90.0, 70.0, 100.0)),
            (cell(1, 0, 1, 1), Rect::new(10.0, 70.0, 30.0, 80.0)),
            (cell(1, 1, 1, 1), Rect::new(50.0, 70.0, 70.0, 80.0)),
        ];
        let refs: Vec<(&TableCell, Rect)> = cells.iter().map(|(c, r)| (c, *r)).collect();

        let slots = snap(&refs);

        // The wide cell covers both columns, down to the middle between the rows.
        assert_eq!(slots[0], Rect::new(7.0, 85.0, 73.0, 103.0));
        assert_eq!(slots[1], Rect::new(7.0, 67.0, 40.0, 85.0));
        // Smoke: with no cell that covers one column only, nothing says where its columns are.
        let alone = [(cell(0, 0, 1, 2), Rect::new(1.0, 2.0, 3.0, 4.0))];
        let alone: Vec<(&TableCell, Rect)> = alone.iter().map(|(c, r)| (c, *r)).collect();
        assert_eq!(snap(&alone), [Rect::new(1.0, 2.0, 3.0, 4.0)]);
    }
}
