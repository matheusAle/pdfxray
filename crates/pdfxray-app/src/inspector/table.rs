//! "Table": what the inspector shows for a selected table, row, cell or anything inside a cell.
//!
//! A mini grid of the table (rows by columns, spans drawn as one wide or tall cell, header cells
//! bold, the selected cell accented, a cell that cannot be read with a header outlined in amber;
//! a click selects the cell), the rule set the header cells were found with, and for a selected
//! cell the header cells that are read with it, each with the rule that found it.

use std::fmt::Write as _;

use egui::{Frame, Margin, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use pdfxray_core::numeric::f32_from_u32;
use pdfxray_core::{NodeId, TableCell, TableRules};

use super::Action;
use crate::center::canvas::table_overlay::{no_header_reason, rule_hint, rule_label};
use crate::components::{PropRow, section, tooltip};
use crate::state::{AppState, TableContext, TableData};
use crate::theme::{fonts, tokens};

/// Title of the section.
pub const TITLE: &str = "Table";

/// What the rule set is called: `PDF 2.0 algorithm`.
pub const fn rules_name(rules: TableRules) -> &'static str {
    match rules {
        TableRules::Pdf2 => "PDF 2.0 algorithm",
        TableRules::Pdf17 => "PDF 1.7 rules",
    }
}

/// What the rule set does, in a line.
pub const fn rules_hint(rules: TableRules) -> &'static str {
    match rules {
        TableRules::Pdf2 => {
            "ISO 32000-2: from the cell, search up each column and left along each row; /Headers wins"
        }
        TableRules::Pdf17 => "PDF 1.7: /Headers, else a header's /Scope, else where it sits; no search",
    }
}

/// Most rows and columns the mini grid draws; a bigger table shows the part around the selection.
const MAX_ROWS: u32 = 10;
const MAX_COLS: u32 = 8;
const CELL_H: f32 = 24.0;
const GAP: f32 = 2.0;

/// The table the selection is in, read before the panel is drawn.
#[derive(Debug, Clone)]
pub(super) struct Data {
    context: TableContext,
}

impl Data {
    /// The table that holds `selection`, if any.
    pub(super) fn of(state: &mut AppState, selection: &NodeId) -> Option<Self> {
        Some(Self {
            context: state.table_context(selection)?,
        })
    }
}

/// The accessible name of a cell of the mini grid: `Cell row 2, column 3: 1.2`, `Header cell row 1,
/// column 2, 2 columns wide: Sales` for a header that spans.
pub fn cell_name(cell: &TableCell, text: &str) -> String {
    let kind = if cell.header { "Header cell" } else { "Cell" };
    let mut name = format!("{kind} row {}, column {}", cell.row + 1, cell.col + 1);
    if cell.col_span > 1 {
        let _ = write!(name, ", {} columns wide", cell.col_span);
    }
    if cell.row_span > 1 {
        let _ = write!(name, ", {} rows tall", cell.row_span);
    }
    if !text.is_empty() {
        name.push_str(": ");
        name.push_str(text);
    }
    name
}

/// `4 rows × 3 columns`.
pub fn size_text(rows: u32, cols: u32) -> String {
    format!(
        "{rows} row{} × {cols} column{}",
        if rows == 1 { "" } else { "s" },
        if cols == 1 { "" } else { "s" }
    )
}

/// The first row or column shown of `total`, when at most `max` fit and `focus` (the selected one)
/// should be among them, in the middle where it can.
fn window(total: u32, max: u32, focus: Option<u32>) -> u32 {
    if total <= max {
        return 0;
    }
    let focus = focus.unwrap_or(0).min(total - 1);
    focus.saturating_sub(max / 2).min(total - max)
}

/// Draws the section.
pub(super) fn show(ui: &mut Ui, data: &Data, action: &mut Action) {
    let TableContext { data: table, cell } = &data.context;
    let model = &table.model;
    section::show_with(ui, TITLE, TITLE, true, |ui| {
        PropRow::new("Grid", &size_text(model.rows, model.cols)).show(ui);
        let rules = PropRow::new("Rules", rules_name(model.rules))
            .copy_text(rules_name(model.rules))
            .show(ui);
        tooltip::text(rules.value, rules_hint(model.rules));
        if !model.irregular.is_empty() {
            let rows: Vec<String> = model.irregular.iter().map(|r| (r.row + 1).to_string()).collect();
            PropRow::new("Irregular", &format!("row {}", rows.join(", ")))
                .warn(true)
                .show(ui);
        }
        ui.add_space(6.0);
        // The grid lines up with the text of the rows around it.
        Frame::new()
            .inner_margin(Margin::symmetric(8, 0))
            .show(ui, |ui| grid(ui, table, *cell, action));
        if let Some(at) = *cell {
            ui.add_space(10.0);
            headers(ui, table, at, action);
        }
    });
}

/// The mini grid.
fn grid(ui: &mut Ui, table: &TableData, selected: Option<usize>, action: &mut Action) {
    let model = &table.model;
    if model.rows == 0 || model.cols == 0 {
        return;
    }
    let focus = selected.map(|at| &model.cells[at]);
    let first_row = window(model.rows, MAX_ROWS, focus.map(|c| c.row));
    let first_col = window(model.cols, MAX_COLS, focus.map(|c| c.col));
    let rows = model.rows.min(MAX_ROWS);
    let cols = model.cols.min(MAX_COLS);
    let (rows_f, cols_f) = (f32_from_u32(rows), f32_from_u32(cols));
    let width = ui.available_width();
    let cell_w = ((width - GAP * (cols_f - 1.0)) / cols_f).max(18.0);
    let size = vec2(width, rows_f * CELL_H + (rows_f - 1.0) * GAP);
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    let slot = |row: u32, col: u32| {
        pos2(
            area.min.x + f32_from_u32(col - first_col) * (cell_w + GAP),
            area.min.y + f32_from_u32(row - first_row) * (CELL_H + GAP),
        )
    };
    for (at, cell) in model.cells.iter().enumerate() {
        // The part of the cell inside the window.
        let (from_row, from_col) = (cell.row.max(first_row), cell.col.max(first_col));
        let (to_row, to_col) = (
            (cell.row + cell.row_span).min(first_row + rows),
            (cell.col + cell.col_span).min(first_col + cols),
        );
        if from_row >= to_row || from_col >= to_col {
            continue;
        }
        let rect = Rect::from_min_size(
            slot(from_row, from_col),
            vec2(
                f32_from_u32(to_col - from_col) * (cell_w + GAP) - GAP,
                f32_from_u32(to_row - from_row) * (CELL_H + GAP) - GAP,
            ),
        );
        let text = table.text(at);
        let response = ui
            .interact(
                rect,
                ui.make_persistent_id(("table-cell", &cell.node)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let name = cell_name(cell, text);
        response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected == Some(at), name.clone()));
        let lacking = TableData::lacks_header(cell);
        if ui.is_rect_visible(rect) {
            paint_cell(
                ui,
                rect,
                cell,
                text,
                selected == Some(at),
                lacking,
                response.hovered(),
            );
        }
        let response = if lacking {
            tooltip::text(response, no_header_reason(cell))
        } else {
            response
        };
        if response.clicked() {
            action.navigate = Some(cell.node.clone());
        }
    }
    if model.rows > rows || model.cols > cols {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "rows {}–{} of {}, columns {}–{} of {}",
                first_row + 1,
                first_row + rows,
                model.rows,
                first_col + 1,
                first_col + cols,
                model.cols
            ))
            .font(fonts::mono(tokens::mono::SMALL))
            .color(tokens::TEXT_3),
        );
    }
}

fn paint_cell(
    ui: &Ui,
    rect: Rect,
    cell: &TableCell,
    text: &str,
    selected: bool,
    lacking: bool,
    hovered: bool,
) {
    let painter = ui.painter();
    // Under the pointer a cell steps up one surface and takes the hover wash, whatever its
    // outline says.
    let fill = if selected {
        tokens::ACCENT_DIM
    } else if cell.header || hovered {
        tokens::SURFACE_3
    } else {
        tokens::SURFACE_2
    };
    painter.rect_filled(rect, tokens::RADIUS_SM, fill);
    if hovered && !selected {
        painter.rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    let stroke = if selected {
        Stroke::new(1.5, tokens::ACCENT)
    } else if lacking {
        Stroke::new(if hovered { 2.0 } else { 1.5 }, tokens::WARN)
    } else if hovered {
        Stroke::new(1.0, tokens::BORDER_STRONG)
    } else {
        Stroke::new(1.0, tokens::BORDER)
    };
    painter.rect_stroke(rect, tokens::RADIUS_SM, stroke, StrokeKind::Inside);
    let (font, ink) = if cell.header {
        (fonts::bold(tokens::text::SMALL), tokens::TEXT)
    } else {
        (fonts::ui(tokens::text::SMALL), tokens::TEXT_2)
    };
    let clip = painter.with_clip_rect(rect.shrink2(vec2(3.0, 0.0)));
    let galley = clip.layout_no_wrap(text.to_owned(), font, ink);
    clip.galley(
        pos2(rect.min.x + 6.0, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
}

/// The header cells read with cell `at`, nearest first.
fn headers(ui: &mut Ui, table: &TableData, at: usize, action: &mut Action) {
    let cell = &table.model.cells[at];
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(fonts::section_header("HEADERS OF THE CELL"));
    });
    ui.add_space(4.0);
    if cell.headers.is_empty() {
        let (text, warn) = if cell.header {
            ("none: a header cell", false)
        } else {
            ("none found", true)
        };
        PropRow::new("Headers", text).warn(warn).show(ui);
    }
    for header in &cell.headers {
        let Some(found) = table.index_of(&header.cell) else {
            continue;
        };
        let target = &table.model.cells[found];
        let text = table.text(found);
        let value = format!(
            "{} · row {}, col {}",
            if text.is_empty() { "empty cell" } else { text },
            target.row + 1,
            target.col + 1
        );
        let row = PropRow::new(rule_label(header.via), &value).link(true).show(ui);
        tooltip::text(row.value.clone(), rule_hint(header.via));
        if row.value.clicked() {
            action.navigate = Some(header.cell.clone());
        }
    }
    for id in &cell.missing_header_ids {
        PropRow::new("/Headers", &format!("{id}: no cell has this ID"))
            .warn(true)
            .show(ui);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_big_table_shows_the_part_around_the_selection() {
        assert_eq!(window(4, 10, Some(2)), 0);
        assert_eq!(window(40, 10, None), 0);
        assert_eq!(window(40, 10, Some(20)), 15);
        // Near the end the window stops at the last row.
        assert_eq!(window(40, 10, Some(39)), 30);
        assert_eq!(window(40, 10, Some(2)), 0);
    }

    #[test]
    fn sizes_and_spans_read_in_words() {
        assert_eq!(size_text(1, 1), "1 row × 1 column");
        assert_eq!(size_text(4, 3), "4 rows × 3 columns");
    }
}
