//! Rows of the drawer's tables: fixed-width columns of text and icons on one clickable line.

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Color32, CursorIcon, FontId, Id, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    pos2, vec2,
};

use crate::components::{Icon, focus};
use crate::findings;
use crate::theme::{fonts, tokens};

/// Space before the first column and between columns.
const PAD: f32 = 8.0;

/// What one column of a row shows.
#[derive(Debug, Clone)]
pub(super) enum Cell {
    /// Text; cut to the column with a `…`.
    Text {
        text: String,
        color: Color32,
        font: FontId,
    },
    /// An icon centred in the column.
    Icon(Icon, Color32),
    /// The mark of a failed check, centred in the column.
    Fail,
}

impl Cell {
    /// Mono text of the small size.
    pub(super) fn small(text: impl Into<String>, color: Color32) -> Self {
        Self::Text {
            text: text.into(),
            color,
            font: fonts::mono(tokens::mono::SMALL),
        }
    }

    /// Text in the UI font.
    pub(super) fn ui(text: impl Into<String>, color: Color32) -> Self {
        Self::Text {
            text: text.into(),
            color,
            font: fonts::ui(tokens::text::BODY),
        }
    }

    /// Mono text of the body size.
    pub(super) fn mono(text: impl Into<String>, color: Color32) -> Self {
        Self::Text {
            text: text.into(),
            color,
            font: fonts::mono(tokens::mono::BODY),
        }
    }
}

impl Cell {
    /// Mono text of the body size in semibold: operator names and rule numbers.
    pub(super) fn mono_strong(text: impl Into<String>, color: Color32) -> Self {
        Self::Text {
            text: text.into(),
            color,
            font: fonts::mono_semibold(tokens::mono::BODY),
        }
    }
}

/// The look every clickable row of the drawer shares with the tree's: a faint wash on hover, the
/// accent on the selected row, the focus ring under the keyboard, and the pointing hand.
/// Returns `response` with the cursor set.
pub(super) fn row_background(ui: &Ui, rect: Rect, response: Response, selected: bool) -> Response {
    let inset = rect.shrink2(vec2(2.0, 0.5));
    if selected {
        ui.painter().rect(
            inset,
            tokens::RADIUS_SM,
            tokens::ACCENT_DIM,
            Stroke::new(1.0, tokens::ACCENT),
            StrokeKind::Inside,
        );
    } else if response.hovered() {
        ui.painter()
            .rect_filled(inset, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    focus::ring(ui, &response, inset, tokens::RADIUS_SM);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Draws one row: `widths` are the column widths, the last column takes what is left. The row
/// is a button named `name`; `selected` highlights it. Text that does not fit its column ends in
/// a `…`; the pointer resting on the row spells it out in full, after `note` when there is one.
pub(super) fn row(
    ui: &mut Ui,
    id: Id,
    name: &str,
    selected: bool,
    widths: &[f32],
    cells: &[Cell],
    note: Option<String>,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, name));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let response = row_background(ui, rect, response, selected);
    let mut elided = Vec::new();
    let mut x = rect.min.x + PAD;
    for (n, cell) in cells.iter().enumerate() {
        let width = widths.get(n).copied().unwrap_or(rect.max.x - PAD - x);
        let column = Rect::from_min_max(pos2(x, rect.min.y), pos2((x + width).min(rect.max.x), rect.max.y));
        if let Some(full) = paint(ui, column, cell) {
            elided.push(full);
        }
        x += width + PAD;
    }
    let tip = match (note, elided.is_empty()) {
        (Some(note), true) => note,
        (Some(note), false) => format!("{note}\n\n{}", elided.join("\n")),
        (None, false) => elided.join("\n"),
        (None, true) => return response,
    };
    response.on_hover_text(tip)
}

/// Paints one cell; the full text when it did not fit and was cut.
fn paint<'a>(ui: &Ui, column: Rect, cell: &'a Cell) -> Option<&'a str> {
    match cell {
        Cell::Text { text, color, font } => {
            let mut job = LayoutJob::single_section(text.clone(), TextFormat::simple(font.clone(), *color));
            job.wrap = TextWrapping {
                max_width: column.width(),
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some('…'),
            };
            let galley = ui.painter().layout_job(job);
            let at: Pos2 = pos2(column.min.x, column.center().y - galley.size().y / 2.0);
            let elided = galley.elided;
            ui.painter().galley(at, galley, *color);
            elided.then_some(text.as_str())
        }
        Cell::Icon(icon, color) => {
            icon.paint(ui, column, 14.0, *color);
            None
        }
        Cell::Fail => {
            findings::fail_mark(
                ui.painter(),
                Rect::from_center_size(column.center(), vec2(13.0, 13.0)),
                tokens::ERR,
            );
            None
        }
    }
}

/// A header row of dim, small column titles.
pub(super) fn header(ui: &mut Ui, widths: &[f32], titles: &[&str]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let mut x = rect.min.x + PAD;
    for (n, title) in titles.iter().enumerate() {
        let width = widths.get(n).copied().unwrap_or(rect.max.x - PAD - x);
        let column = Rect::from_min_max(pos2(x, rect.min.y), pos2((x + width).min(rect.max.x), rect.max.y));
        let _ = paint(ui, column, &Cell::small(*title, tokens::TEXT_3));
        x += width + PAD;
    }
}

#[cfg(test)]
mod tests {
    use egui::{Context, RawInput, Shape};
    use pretty_assertions::assert_eq;

    use super::*;

    /// Paints `cell` into a column `width` wide; what it returned and the widths of the text
    /// galleys it drew.
    fn painted(cell: &Cell, width: f32) -> (Option<String>, Vec<f32>) {
        let ctx = Context::default();
        crate::theme::install(&ctx);
        // The fonts of the theme are there from the second frame.
        ctx.run_ui(RawInput::default(), |_| {}).textures_delta.clear();
        let mut elided = None;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let column = Rect::from_min_size(pos2(10.0, 0.0), vec2(width, tokens::ROW_H));
            elided = paint(ui, column, cell).map(str::to_owned);
        });
        output.textures_delta.clear();
        let widths = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Text(text) => Some(text.galley.size().x),
                _ => None,
            })
            .collect();
        (elided, widths)
    }

    #[test]
    fn text_wider_than_its_column_ends_in_an_ellipsis_and_hands_back_the_full_text() {
        let long = "CTM [0.5 0 0 0.5 123.45 678.9] and a good deal more";
        let (elided, widths) = painted(&Cell::mono(long, tokens::TEXT), 80.0);

        assert_eq!(elided.as_deref(), Some(long));
        assert_eq!(widths.len(), 1);
        assert!(widths[0] <= 80.0, "{widths:?}");
        // Smoke: text that fits is whole, and says nothing.
        let (elided, widths) = painted(&Cell::mono("cm", tokens::TEXT), 80.0);
        assert_eq!(elided, None);
        assert_eq!(widths.len(), 1);
        assert!(widths[0] < 40.0, "{widths:?}");
    }
}
