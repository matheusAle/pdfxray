//! The PAINT ORDER section: where the operator the stepper stopped at sits in the painting of
//! the page. Shown above GEOMETRY while the stepper is on and the operator is selected.
//!
//! Step (`211 of 377`), what it paints over (`page background only` when nothing), the later
//! operators that cover it (links that move the stepper there), how much of it stays visible,
//! and the stack at that point, top first. Overlap is judged on bounding boxes, so the section
//! says it is an estimate.
//!
//! The stack lists three operators above the current one and three under it; the `+N more`
//! row of either part is a button that lists [`UNFOLD_BY`] more, and once all of it shows,
//! *Show fewer* folds it again. What is unfolded belongs to the operator: another operator
//! starts folded, and this one is as it was when the stepper comes back to it.

use egui::{Align2, CursorIcon, Id, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use pdfxray_core::capture::CapturedPage;
use pdfxray_core::numeric::{u32_from_f64_round, usize_from_u32};
use pdfxray_core::{NodeId, PaintStep};

use super::Action;
use super::structure::Cx;
use crate::components::prop_row::VALUE_X;
use crate::components::{Icon, PropRow, focus};
use crate::drawer::paint_order::summary;
use crate::theme::{fonts, tokens};

/// Title of the section.
pub const TITLE: &str = "Paint order";

/// Title of the stack list.
pub const STACK_TITLE: &str = "Stack at this point (top first)";

/// The note under the visibility: the figure comes from bounding boxes.
pub const ESTIMATE_NOTE: &str = "≈ bbox estimate";

/// What the paints-over row says when nothing was painted there before.
pub const BARE_PAGE: &str = "page background only";

/// The last stack row.
pub const PAGE_BACKGROUND: &str = "page background";

/// The row that folds an unfolded part of the stack again.
pub const SHOW_FEWER: &str = "Show fewer";

/// Operators listed in each part of the stack and of the covered-by rows before "+N more".
const SHOWN: usize = 3;

/// How many more operators a click on a `+N more` row of the stack lists.
pub const UNFOLD_BY: usize = 20;

const STACK_ROW_H: f32 = 22.0;

/// `211 of 377`.
pub fn step_text(step: &PaintStep) -> String {
    format!("{} of {}", step.step, step.total)
}

/// `82% · 18% under later ops`, or `100%` when nothing covers the operator.
pub fn visible_text(step: &PaintStep) -> String {
    // The fraction is between 0 and 1.
    let visible = u32_from_f64_round(step.visible_fraction * 100.0);
    if visible >= 100 {
        "100%".to_owned()
    } else {
        format!("{visible}% · {}% under later ops", 100 - visible)
    }
}

/// True when the section is for `cx`'s node: the stepper is on and it is the operator it stopped
/// at. The inspector shows nodes of A, so a comparison's stepper on B has no section.
fn applies(cx: &Cx<'_>) -> bool {
    let view = &cx.state.canvas.view;
    cx.state.stepped_side() == pdfxray_core::diff::Side::A
        && matches!(cx.id, NodeId::Op { page, ix } if view.stepper && *page == view.step.page && *ix == view.step.ix)
}

pub(super) fn show(ui: &mut Ui, cx: &Cx<'_>, action: &mut Action) {
    if !applies(cx) {
        return;
    }
    let NodeId::Op { page, ix } = cx.id else {
        return;
    };
    let (Some(step), Some(captured)) = (cx.doc.paint_step(*page, *ix), cx.doc.captured(*page)) else {
        return;
    };
    cx.section(ui, TITLE, |ui| {
        PropRow::new("Step", &step_text(&step)).show(ui);
        paints_over(ui, captured, &step);
        covered_by(ui, captured, &step, action);
        PropRow::new("Visible", &visible_text(&step))
            .warn(step.visible_fraction < 1.0)
            .show(ui);
        if step.approximate {
            note(ui, ESTIMATE_NOTE);
        }
        stack(ui, captured, &step, (*page, *ix), action);
    });
}

fn note(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(VALUE_X);
        ui.label(
            egui::RichText::new(text)
                .font(fonts::ui(tokens::text::SMALL))
                .color(tokens::TEXT_3),
        );
    });
}

/// The operator of `node`, when it is one on this page's capture.
fn op_of<'a>(captured: &'a CapturedPage, node: &NodeId) -> Option<&'a pdfxray_core::capture::OpRecord> {
    let NodeId::Op { ix, .. } = node else {
        return None;
    };
    captured.ops.get(usize_from_u32(*ix)).filter(|op| op.ix == *ix)
}

fn paints_over(ui: &mut Ui, captured: &CapturedPage, step: &PaintStep) {
    if step.paints_over.is_empty() {
        PropRow::new("Paints over", BARE_PAGE).show(ui);
        return;
    }
    let mut parts: Vec<String> = step
        .paints_over
        .iter()
        .filter_map(|node| op_of(captured, node))
        .take(SHOWN)
        .map(|op| summary(captured, op))
        .collect();
    if step.paints_over.len() > SHOWN {
        parts.push(format!("+{} more", step.paints_over.len() - SHOWN));
    }
    PropRow::new("Paints over", &parts.join(", ")).show(ui);
}

/// One link per operator that paints later in this one's box, nearest first.
fn covered_by(ui: &mut Ui, captured: &CapturedPage, step: &PaintStep, action: &mut Action) {
    if step.covered_by.is_empty() {
        PropRow::new("Covered later by", "nothing").show(ui);
        return;
    }
    for (n, node) in step.covered_by.iter().take(SHOWN).enumerate() {
        let Some(op) = op_of(captured, node) else {
            continue;
        };
        let text = format!("op {}", summary(captured, op));
        let key = if n == 0 { "Covered later by" } else { "" };
        if PropRow::new(key, &text).link(true).show(ui).followed {
            action.step_to = Some(op.ix);
        }
    }
    if step.covered_by.len() > SHOWN {
        let more = format!("+{} more", step.covered_by.len() - SHOWN);
        PropRow::new("", &more).show(ui);
    }
}

/// How a row of the stack relates to the current operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Place {
    Later,
    This,
    Under,
}

/// `+12 more later` or `+12 more under`: the row that unfolds the `hidden` operators of a part.
pub fn more_text(hidden: usize, place_word: &str) -> String {
    format!("+{hidden} more {place_word}")
}

/// Where the number of rows the `place` part of the stack of operator `at` lists is kept.
fn shown_id(at: (u32, u32), place: Place) -> Id {
    Id::unique(("paint-stack-shown", at, place))
}

/// The rows a part lists after a click on its fold row, from `shown` of `len`: [`UNFOLD_BY`]
/// more while some are hidden, else back to the three a folded part lists.
pub fn after_fold_click(shown: usize, len: usize) -> usize {
    if shown < len {
        (shown + UNFOLD_BY).min(len)
    } else {
        SHOWN
    }
}

fn stack(ui: &mut Ui, captured: &CapturedPage, step: &PaintStep, at: (u32, u32), action: &mut Action) {
    let current = at.1;
    let shown = |ui: &Ui, place| ui.data(|d| d.get_temp::<usize>(shown_id(at, place)).unwrap_or(SHOWN));
    let set_shown = |ui: &Ui, place, rows: usize| ui.data_mut(|d| d.insert_temp(shown_id(at, place), rows));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(STACK_TITLE)
                .font(fonts::ui(tokens::text::SMALL))
                .color(tokens::TEXT_3),
        );
    });
    // Top first: the last painter, then down to the operator, then what it paints over.
    let later_rows = shown(ui, Place::Later);
    let later = step.covered_by.iter().rev().take(later_rows);
    for node in later {
        if let Some(op) = op_of(captured, node)
            && stack_row(ui, &summary(captured, op), Place::Later)
        {
            action.step_to = Some(op.ix);
        }
    }
    if fold_row(ui, step.covered_by.len(), later_rows, "later") {
        set_shown(
            ui,
            Place::Later,
            after_fold_click(later_rows, step.covered_by.len()),
        );
    }
    if let Some(op) = captured.ops.get(usize_from_u32(current)) {
        stack_row(ui, &summary(captured, op), Place::This);
    }
    let under_rows = shown(ui, Place::Under);
    for node in step.paints_over.iter().take(under_rows) {
        if let Some(op) = op_of(captured, node)
            && stack_row(ui, &summary(captured, op), Place::Under)
        {
            action.step_to = Some(op.ix);
        }
    }
    if fold_row(ui, step.paints_over.len(), under_rows, "under") {
        set_shown(
            ui,
            Place::Under,
            after_fold_click(under_rows, step.paints_over.len()),
        );
    }
    dim_row(ui, PAGE_BACKGROUND);
}

/// The row after a part of `len` operators that lists `shown`: `+N more …` while some are
/// hidden, *Show fewer* once all show, nothing when the part fits. A button; true when clicked.
fn fold_row(ui: &mut Ui, len: usize, shown: usize, place_word: &str) -> bool {
    if len <= SHOWN {
        return false;
    }
    let (text, icon) = if shown < len {
        (more_text(len - shown, place_word), Icon::ChevronDown)
    } else {
        (SHOW_FEWER.to_owned(), Icon::ChevronUp)
    };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), STACK_ROW_H), Sense::hover());
    let inner = rect.shrink2(vec2(8.0, 1.0));
    let response = ui
        .interact(
            inner,
            ui.make_persistent_id(("paint-stack-fold", place_word)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, &text));
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(inner, tokens::RADIUS_SM, tokens::SURFACE_2);
    }
    focus::row_ring(ui, &response, inner, tokens::RADIUS_SM);
    painter.text(
        pos2(inner.min.x + 22.0, inner.center().y),
        Align2::LEFT_CENTER,
        &text,
        fonts::mono(tokens::mono::BODY),
        tokens::TEXT_2,
    );
    let slot = egui::Rect::from_center_size(pos2(inner.max.x - 12.0, inner.center().y), vec2(10.0, 10.0));
    icon.paint(ui, slot, 10.0, tokens::TEXT_2);
    response.clicked()
}

fn dim_row(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), STACK_ROW_H), Sense::hover());
    let inner = rect.shrink2(vec2(8.0, 1.0));
    ui.painter()
        .rect_filled(inner, tokens::RADIUS_SM, tokens::SURFACE_2);
    ui.painter()
        .circle_filled(pos2(inner.min.x + 10.0, inner.center().y), 3.0, tokens::TEXT_3);
    ui.painter().text(
        pos2(inner.min.x + 22.0, inner.center().y),
        Align2::LEFT_CENTER,
        text,
        fonts::mono(tokens::mono::BODY),
        tokens::TEXT_3,
    );
}

/// One row of the stack; true when clicked. The current operator's row has the accent outline.
fn stack_row(ui: &mut Ui, text: &str, place: Place) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), STACK_ROW_H), Sense::hover());
    let inner = rect.shrink2(vec2(8.0, 1.0));
    let response = ui.interact(
        inner,
        ui.make_persistent_id(("paint-stack", text, place)),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, place == Place::This, text));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let (dot, tag) = match place {
        Place::Later => (tokens::ERR, "later"),
        Place::This => (tokens::ACCENT, "this op"),
        Place::Under => (tokens::TEXT_3, "under"),
    };
    let painter = ui.painter();
    if place == Place::This {
        painter.rect(
            inner,
            tokens::RADIUS_SM,
            tokens::ACCENT_DIM,
            Stroke::new(1.0, tokens::ACCENT),
            StrokeKind::Inside,
        );
    } else {
        let fill = if response.hovered() {
            tokens::SURFACE_3
        } else {
            tokens::SURFACE_2
        };
        painter.rect_filled(inner, tokens::RADIUS_SM, fill);
    }
    focus::ring(ui, &response, inner, tokens::RADIUS_SM);
    painter.circle_filled(pos2(inner.min.x + 10.0, inner.center().y), 3.0, dot);
    painter.text(
        pos2(inner.min.x + 22.0, inner.center().y),
        Align2::LEFT_CENTER,
        text,
        fonts::mono(tokens::mono::BODY),
        tokens::TEXT,
    );
    painter.text(
        pos2(inner.max.x - 8.0, inner.center().y),
        Align2::RIGHT_CENTER,
        tag,
        fonts::mono(tokens::mono::SMALL),
        tokens::TEXT_3,
    );
    response.clicked()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn step(visible: f64) -> PaintStep {
        PaintStep {
            step: 211,
            total: 377,
            paints_over: Vec::new(),
            covered_by: Vec::new(),
            visible_fraction: visible,
            approximate: true,
        }
    }

    #[test]
    fn a_fold_row_unfolds_twenty_more_at_a_time_then_folds_back() {
        assert_eq!(after_fold_click(3, 50), 23);
        assert_eq!(after_fold_click(23, 50), 43);
        assert_eq!(after_fold_click(43, 50), 50);
        // All shown: the row is Show fewer, and folds back to three.
        assert_eq!(after_fold_click(50, 50), 3);
        // Smoke: a short part unfolds to its length, not past it.
        assert_eq!(after_fold_click(3, 5), 5);
        assert_eq!(more_text(47, "under"), "+47 more under");
    }

    #[test]
    fn the_step_and_the_visible_share_read_in_plain_words() {
        assert_eq!(step_text(&step(1.0)), "211 of 377");
        assert_eq!(visible_text(&step(1.0)), "100%");
        assert_eq!(visible_text(&step(0.82)), "82% · 18% under later ops");
        assert_eq!(visible_text(&step(0.0)), "0% · 100% under later ops");
    }
}
