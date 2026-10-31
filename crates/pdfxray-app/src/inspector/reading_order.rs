//! "Page n · reading order": what the inspector shows for a selected page of a tagged file.
//!
//! The elements of the page in reading order, each with its number, a chip in its role colour and
//! a preview of its text (a click selects it). A step that goes back up the page is amber and the
//! `reading-order-backtrack` explanation follows the list. A second section, *vs paint order*,
//! says how far the structure order and the order the content is painted in agree.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Align2, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use pdfxray_core::{NodeId, OrderComparison, OrderStep, PageIx, ReadingOrder, ReadingOrderSource, explain};

use super::Action;
use crate::center::canvas::tag_overlay::chip_text;
use crate::components::{Icon, PropRow, section, tooltip};
use crate::modes::OrderSource;
use crate::roles::role_color;
use crate::state::{AppState, RailItem};
use crate::theme::{fonts, tokens};

/// Title of the list's section.
pub const TITLE: &str = "Reading order";

/// Title of the comparison's section.
pub const VS_PAINT_TITLE: &str = "Vs paint order";

/// The diagnostic the amber steps raise.
pub const BACKTRACK_CODE: &str = "reading-order-backtrack";

/// Shown when no structure element draws on the page.
pub const EMPTY: &str = "No structure element draws on this page.";

const ROW_H: f32 = tokens::ROW_H;
const BADGE: f32 = 8.0;

/// What the section needs about one page, read before the panel is drawn.
#[derive(Debug, Clone)]
pub(super) struct Data {
    source: OrderSource,
    /// The order shown: the structure tree's, or the paint order, as the View menu says.
    shown: Arc<ReadingOrder>,
    structure: Arc<ReadingOrder>,
    paint: Arc<ReadingOrder>,
    comparison: OrderComparison,
    /// What each element's chip says: the standard type its role resolves to (`H1` for a
    /// `Heading1`), else the role.
    chips: HashMap<NodeId, String>,
}

impl Data {
    /// The data for `selection` when it is a page of a file with a structure tree.
    pub(super) fn of(state: &mut AppState, selection: &NodeId) -> Option<Self> {
        let NodeId::Page(page) = selection else {
            return None;
        };
        if !state.document.as_ref()?.tagged {
            return None;
        }
        let source = state.canvas.view.order_from;
        let structure = state.reading_order(*page, ReadingOrderSource::Structure)?;
        let paint = state.reading_order(*page, ReadingOrderSource::Paint)?;
        let comparison = state.order_comparison(*page)?;
        let shown = match source {
            OrderSource::Structure => Arc::clone(&structure),
            OrderSource::Paint => Arc::clone(&paint),
            OrderSource::Speech => state.shown_order(*page, source)?,
        };
        let chips = state
            .tag_boxes(*page)
            .iter()
            .map(|tag| (tag.id.clone(), chip_text(tag).to_owned()))
            .collect();
        Some(Self {
            source,
            shown,
            structure,
            paint,
            comparison,
            chips,
        })
    }

    /// `from structure tree · 5 elements · 1 backtrack`.
    pub(super) fn summary(&self) -> String {
        summary_text(self.source, &self.shown)
    }
}

/// `from structure tree · 5 elements · 1 backtrack`; the backtracks are left out when there are
/// none.
pub fn summary_text(source: OrderSource, order: &ReadingOrder) -> String {
    let count = order.steps.len();
    let mut text = format!(
        "{} · {count} element{}",
        source.from_text(),
        if count == 1 { "" } else { "s" }
    );
    if order.backtracks > 0 {
        let n = order.backtracks;
        let _ = write!(text, " · {n} backtrack{}", if n == 1 { "" } else { "s" });
    }
    text
}

/// `4 of 5 elements`.
pub fn same_text(comparison: &OrderComparison) -> String {
    format!("{} of {} elements", comparison.same, comparison.total)
}

/// `#3 Caption (painted after Figure)`: the element the structure order has at the first position
/// where the two orders differ, and the one the paint order has there instead. `None` when they
/// agree.
pub fn differs_text(
    structure: &ReadingOrder,
    paint: &ReadingOrder,
    comparison: &OrderComparison,
) -> Option<String> {
    let (at, _) = comparison.first_difference.as_ref()?;
    let (here, instead) = (structure.steps.get(at - 1)?, paint.steps.get(at - 1)?);
    Some(format!("#{at} {} (painted after {})", here.role, instead.role))
}

/// The accessible name of a row of the list.
pub fn row_label(step: &OrderStep) -> String {
    let mut label = format!("Step {}: {}", step.index, step.role);
    if !step.preview.is_empty() {
        label.push_str(" · ");
        label.push_str(&step.preview);
    }
    if step.backtrack {
        label.push_str(" · goes back up");
    }
    label
}

/// The title and the line under it for a selected page while the reading order is on.
pub(super) fn heading(page: PageIx, data: &Data) -> (String, String) {
    (format!("Page {} · reading order", page + 1), data.summary())
}

/// Draws the two sections.
pub(super) fn show(ui: &mut Ui, state: &AppState, data: &Data, action: &mut Action) {
    let on = state.canvas.view.show.reading_order;
    let open = on || state.nav.rail == RailItem::Tags;
    section::show_with(ui, TITLE, TITLE, open, |ui| {
        if !on {
            super::note(
                ui,
                egui::RichText::new(data.summary())
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
        }
        if data.shown.steps.is_empty() {
            super::note(ui, egui::RichText::new(EMPTY).color(tokens::TEXT_3));
            return;
        }
        for step in &data.shown.steps {
            let chip = data
                .chips
                .get(&step.id)
                .map_or(step.role.as_str(), String::as_str);
            if row(ui, step, chip).clicked() {
                action.navigate = Some(step.id.clone());
            }
        }
        if data.shown.backtracks > 0 {
            ui.add_space(6.0);
            backtrack_card(ui);
        }
    });
    section::show_with(ui, VS_PAINT_TITLE, VS_PAINT_TITLE, open, |ui| {
        let c = &data.comparison;
        PropRow::new("Same order", &same_text(c))
            .good(c.same == c.total)
            .show(ui);
        if let Some(text) = differs_text(&data.structure, &data.paint, c) {
            PropRow::new("Differs at", &text).warn(true).show(ui);
        }
    });
}

/// One step: its number, a chip with the element's type in the role colour, and a preview.
fn row(ui: &mut Ui, step: &OrderStep, chip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let label = row_label(step);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    let amber = tokens::WARN;
    // A backtrack row keeps its amber tint and deepens it under the pointer.
    let fill = if step.backtrack {
        Some(amber.gamma_multiply(if response.hovered() { 0.2 } else { 0.12 }))
    } else if response.hovered() {
        Some(tokens::NEUTRAL_HOVER)
    } else {
        None
    };
    if let Some(fill) = fill {
        painter.rect_filled(rect, tokens::RADIUS_SM, fill);
    }
    let y = rect.center().y;
    let center = pos2(rect.min.x + 14.0, y);
    let badge = if step.backtrack { amber } else { tokens::ACCENT };
    painter.circle_filled(center, BADGE, badge);
    painter.text(
        center,
        Align2::CENTER_CENTER,
        step.index.to_string(),
        fonts::bold(tokens::text::SMALL),
        tokens::ON_ACCENT,
    );
    // The chip.
    let color = role_color(step.class);
    let galley = painter.layout_no_wrap(chip.to_owned(), fonts::bold(tokens::text::SMALL), tokens::BG);
    let chip_rect = egui::Rect::from_min_size(
        pos2(rect.min.x + 30.0, y - galley.size().y / 2.0 - 2.0),
        galley.size() + vec2(10.0, 4.0),
    );
    painter.rect_filled(chip_rect, tokens::RADIUS_SM, color);
    painter.rect_stroke(
        chip_rect,
        tokens::RADIUS_SM,
        Stroke::new(1.0, tokens::BG),
        StrokeKind::Outside,
    );
    painter.galley(chip_rect.min + vec2(5.0, 2.0), galley, tokens::BG);
    // The preview, cut to what is left of the row.
    let reserved = if step.backtrack { 24.0 } else { 6.0 };
    let from = chip_rect.max.x + 8.0;
    let mut job = LayoutJob::single_section(
        step.preview.clone(),
        TextFormat::simple(fonts::ui(tokens::text::BODY), tokens::TEXT_2),
    );
    job.wrap = TextWrapping {
        max_width: (rect.max.x - reserved - from).max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let text = painter.layout_job(job);
    let cut = text.elided;
    painter.galley(pos2(from, y - text.size().y / 2.0), text, tokens::TEXT_2);
    if step.backtrack {
        Icon::Warning.paint(
            ui,
            egui::Rect::from_center_size(pos2(rect.max.x - 12.0, y), vec2(12.0, 12.0)),
            12.0,
            amber,
        );
    }
    // A preview that was cut is read in full in the tooltip.
    if cut {
        return tooltip::text(response, label);
    }
    response
}

/// The plain-language card for `reading-order-backtrack`.
fn backtrack_card(ui: &mut Ui) {
    let color = tokens::WARN;
    egui::Frame::new()
        // Inset like the problem cards.
        .outer_margin(egui::Margin::symmetric(12, 0))
        .fill(color.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(tokens::RADIUS_LG)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(BACKTRACK_CODE)
                    .font(fonts::mono_medium(tokens::mono::BODY))
                    .color(color),
            );
            ui.add_space(2.0);
            let text = explain::diagnostic(BACKTRACK_CODE).unwrap_or(BACKTRACK_CODE);
            ui.add(egui::Label::new(egui::RichText::new(text).color(tokens::TEXT_2)).wrap());
        });
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{Rect, RoleClass, StepKind};
    use pretty_assertions::assert_eq;

    use super::*;

    fn step(index: usize, role: &str, preview: &str, kind: StepKind) -> OrderStep {
        OrderStep {
            index,
            id: NodeId::Document,
            role: role.to_owned(),
            class: RoleClass::Paragraph,
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            kind,
            backtrack: kind == StepKind::Backtrack,
            preview: preview.to_owned(),
        }
    }

    fn order(steps: Vec<OrderStep>) -> ReadingOrder {
        let backtracks = steps.iter().filter(|s| s.backtrack).count();
        ReadingOrder { steps, backtracks }
    }

    #[test]
    fn the_summary_counts_elements_and_backtracks() {
        let one = order(vec![step(1, "P", "", StepKind::First)]);
        let two = order(vec![
            step(1, "P", "", StepKind::First),
            step(2, "P", "", StepKind::Backtrack),
        ]);

        assert_eq!(
            summary_text(OrderSource::Structure, &one),
            "from structure tree · 1 element"
        );
        assert_eq!(
            summary_text(OrderSource::Paint, &two),
            "from paint order · 2 elements · 1 backtrack"
        );
    }

    #[test]
    fn the_comparison_names_the_elements_that_swapped() {
        let structure = order(vec![
            step(1, "H1", "", StepKind::First),
            step(2, "Caption", "", StepKind::Forward),
            step(3, "Figure", "", StepKind::Backtrack),
        ]);
        let paint = order(vec![
            step(1, "H1", "", StepKind::First),
            step(2, "Figure", "", StepKind::Forward),
            step(3, "Caption", "", StepKind::Forward),
        ]);
        let differ = OrderComparison {
            same: 1,
            total: 3,
            first_difference: Some((2, NodeId::Document)),
        };
        let same = OrderComparison {
            same: 3,
            total: 3,
            first_difference: None,
        };

        assert_eq!(same_text(&differ), "1 of 3 elements");
        assert_eq!(
            differs_text(&structure, &paint, &differ).as_deref(),
            Some("#2 Caption (painted after Figure)")
        );
        assert_eq!(differs_text(&structure, &structure, &same), None);
    }

    #[test]
    fn a_row_is_named_for_its_step_and_flags_a_backtrack() {
        assert_eq!(
            row_label(&step(3, "Caption", "Figure 1", StepKind::Forward)),
            "Step 3: Caption · Figure 1"
        );
        assert_eq!(
            row_label(&step(4, "Figure", "", StepKind::Backtrack)),
            "Step 4: Figure · goes back up"
        );
    }
}
