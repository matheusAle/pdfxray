//! What the canvas adds while the paint-order stepper is on.
//!
//! The bitmap is the page painted up to the current operator ([`pdfxray_render::ContentFilter::UpTo`]).
//! What is still to come is drawn as ghosts over it: one faint box per later operator that
//! paints, outlined in grey, with a label on the first one ("not painted yet · ops 212–377").
//! The renderer has no ghosting pass, so the later operators are marked by their boxes instead of
//! being drawn dimmed. The operator painting now gets the selection's accent box and its label
//! ("▶ op #211 · Do /Im1 · painting now"); the legend card names the three looks.

use egui::{Align2, Painter, Stroke, StrokeKind, Ui};
use pdfxray_core::capture::PaintKind;
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{NodeId, Rect};

use super::geometry::PageGeometry;
use super::marks::pill;
use crate::state::AppState;
use crate::theme::tokens;

/// Title of the legend card.
pub const LEGEND_TITLE: &str = "Paint order";

/// The legend's three entries: what is already painted, the operator painting now, what is not
/// painted yet.
pub const LEGEND_ENTRIES: [&str; 3] = ["painted", "current op", "not yet"];

/// Most ghosts drawn on one page.
const MAX_GHOSTS: usize = 4000;

/// Colour of a ghost's outline.
const GHOST: egui::Color32 = tokens::TEXT_3;

/// The operators that have not painted yet.
#[derive(Debug, Default)]
pub(super) struct Ghost {
    /// One box per later operator that paints.
    boxes: Vec<Rect>,
    /// The label and the box it hangs under: the first later operator that paints.
    label: Option<(Rect, String)>,
}

/// The ghosts for the stepper's position; none while the stepper is off.
pub(super) fn collect(state: &AppState) -> Ghost {
    let view = &state.canvas.view;
    if !view.stepper {
        return Ghost::default();
    }
    let Some(captured) = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.captured(state.canvas.page))
    else {
        return Ghost::default();
    };
    let mut ghost = Ghost::default();
    for op in captured.ops.iter().filter(|op| op.ix > view.step) {
        let Some(first) = op.first_item.and_then(|f| usize::try_from(f).ok()) else {
            continue;
        };
        let union = captured
            .items
            .iter()
            .skip(first)
            .take(usize_from_u32(op.item_count))
            .filter(|item| !matches!(item.kind, PaintKind::Clip))
            .map(|item| item.bbox)
            .reduce(|a, b| Rect {
                x0: a.x0.min(b.x0),
                y0: a.y0.min(b.y0),
                x1: a.x1.max(b.x1),
                y1: a.y1.max(b.y1),
            });
        let Some(union) = union else {
            continue;
        };
        if ghost.label.is_none() {
            let last = captured.ops.last().map_or(op.ix, |last| last.ix);
            ghost.label = Some((union, not_painted_yet(view.step + 1, last)));
        }
        if ghost.boxes.len() < MAX_GHOSTS {
            ghost.boxes.push(union);
        }
    }
    ghost
}

/// The label of the ghosts: `not painted yet · ops 212–377`.
pub fn not_painted_yet(from: u32, to: u32) -> String {
    format!("not painted yet · ops {from}–{to}")
}

/// The label of the operator painting now: `▶ op #211 · Do /Im1 · painting now`. `None` unless
/// the stepper is on and the selection is the operator it stopped at.
pub fn painting_now(state: &AppState) -> Option<String> {
    let view = &state.canvas.view;
    let Some(NodeId::Op { page, ix }) = state.selection.primary.as_ref() else {
        return None;
    };
    if !view.stepper || *page != state.canvas.page || *ix != view.step {
        return None;
    }
    let captured = state.document.as_ref()?.doc.captured(*page)?;
    let op = captured
        .ops
        .get(usize_from_u32(*ix))
        .filter(|op| op.ix == *ix && op.item_count > 0)?;
    let mut operands = op.operands.join(" ");
    if operands.chars().count() > 24 {
        operands = operands.chars().take(23).collect::<String>() + "…";
    }
    let what = format!("{} {operands}", op.operator);
    Some(format!("▶ op #{ix} · {} · painting now", what.trim_end()))
}

/// Draws the ghosts and the label of the first one.
pub(super) fn draw(painter: &Painter, geo: &PageGeometry, ghost: &Ghost) {
    for rect in &ghost.boxes {
        let screen = geo.rect_to_screen(*rect);
        if screen.width() < 0.5 && screen.height() < 0.5 {
            continue;
        }
        painter.rect(
            screen,
            0.0,
            GHOST.gamma_multiply(0.07),
            Stroke::new(1.0, GHOST.gamma_multiply(0.8)),
            StrokeKind::Outside,
        );
    }
    if let Some((rect, text)) = &ghost.label {
        let screen = geo.rect_to_screen(*rect);
        pill(
            painter,
            screen.left_bottom() + egui::vec2(0.0, 4.0),
            Align2::LEFT_TOP,
            text,
            tokens::SURFACE_3,
            tokens::TEXT_2,
        );
    }
}

/// The legend card's content: painted, current op, not yet.
pub(super) fn legend(ui: &mut Ui) {
    super::cards::title(ui, LEGEND_TITLE);
    let looks = [
        (LEGEND_ENTRIES[0], tokens::TEXT_2.gamma_multiply(0.8), None),
        (LEGEND_ENTRIES[1], tokens::ACCENT, None),
        (LEGEND_ENTRIES[2], GHOST.gamma_multiply(0.14), Some(GHOST)),
    ];
    for (name, fill, outline) in looks {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(rect.shrink(1.0), 2.0, fill);
            if let Some(color) = outline {
                ui.painter()
                    .rect_stroke(rect.shrink(1.0), 2.0, Stroke::new(1.0, color), StrokeKind::Inside);
            }
            ui.label(name);
        });
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::install_fixture;

    fn stepping(fixture: &str, step: u32) -> AppState {
        let mut state = AppState::default();
        install_fixture(&mut state, fixture);
        state.set_stepper(true);
        state.step_to(step);
        state
    }

    #[test]
    fn the_ghosts_are_the_later_operators_that_paint_and_the_first_is_labelled() {
        let state = stepping("graphics-state", 49);

        let ghost = collect(&state);

        let doc = &state.document.as_ref().unwrap().doc;
        let painting_later = (50..=79).filter(|ix| !doc.is_state_only(0, *ix)).count();
        assert_eq!(ghost.boxes.len(), painting_later);
        // Op 53 strokes the square after the orange fill at 49; nothing paints in between.
        let (rect, text) = ghost.label.unwrap();
        assert_eq!(text, "not painted yet · ops 50–79");
        assert_eq!(Some((0, rect)), doc.bbox(&NodeId::Op { page: 0, ix: 53 }));
    }

    #[test]
    fn nothing_is_ghosted_at_the_last_operator_or_with_the_stepper_off() {
        let mut state = stepping("graphics-state", 79);
        assert_eq!(collect(&state).boxes.len(), 0);
        assert_eq!(collect(&state).label, None);

        state.step_to(10);
        state.set_stepper(false);
        assert_eq!(collect(&state).boxes.len(), 0);
    }

    #[test]
    fn the_operator_painting_now_names_itself() {
        let mut state = stepping("graphics-state", 49);
        assert_eq!(
            painting_now(&state).as_deref(),
            Some("▶ op #49 · f · painting now")
        );

        // Operands are part of the name.
        state.step_to(59);
        assert_eq!(
            painting_now(&state).as_deref(),
            Some("▶ op #59 · sh /Sh1 · painting now")
        );

        // An operator that paints nothing is not painting now.
        state.step_to(2);
        assert_eq!(painting_now(&state), None);
        state.step_to(49);

        // Another selection is not the operator painting now.
        state.select(NodeId::Op { page: 0, ix: 3 });
        assert_eq!(painting_now(&state), None);
        state.step_to(5);
        state.set_stepper(false);
        assert_eq!(painting_now(&state), None);
    }
}
