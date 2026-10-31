//! The Validation tab: the failed checks of the last run as a table, in the navigator's order.

use egui::{RichText, ScrollArea, Ui};
use pdfxray_core::numeric::{f32_from_usize, usize_from_u32};

use super::table::{self, Cell};
use crate::findings;
use crate::state::AppState;
use crate::theme::tokens;

/// Shown before a run.
pub const NO_FINDINGS: &str = "Run a validation to see its findings here.";

/// Shown when a run found nothing to fix.
pub const NO_FAILURES: &str = "No failed checks.";

const STATUS_W: f32 = 18.0;
const RULE_W: f32 = 96.0;
/// Least width of the `what failed` column; it takes what the others leave.
const WHAT_MIN_W: f32 = 300.0;
const NODE_W: f32 = 170.0;
const PAGE_W: f32 = 36.0;
/// Space before the first column, between columns and after the last.
const GAP: f32 = 8.0;

/// Width of the `what failed` column in a table `available` wide: what the other columns and
/// the gaps leave, never less than [`WHAT_MIN_W`].
fn what_width(available: f32) -> f32 {
    let fixed = STATUS_W + RULE_W + NODE_W + PAGE_W + 6.0 * GAP;
    (available - fixed).max(WHAT_MIN_W)
}

/// The line at the right end of the tab bar: `veraPDF 1.30.3 · PDF/UA-1 · ran 12 s ago · 0.8 s`.
pub fn header_note(state: &AppState) -> Option<String> {
    let run = state.document.as_ref()?.doc.validation()?;
    let mut parts = Vec::new();
    if let Some(version) = &run.verapdf_version {
        parts.push(format!("veraPDF {version}"));
    }
    match &state.validation.last {
        Some(last) => {
            parts.push(last.profile.label());
            parts.push(last.ago());
            parts.push(last.took_text());
        }
        None => parts.push(run.profile.clone()),
    }
    Some(parts.join(" · "))
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(loaded) = &state.document else {
        return;
    };
    let Some(run) = loaded.doc.validation() else {
        ui.label(RichText::new(NO_FINDINGS).color(tokens::TEXT_3));
        return;
    };
    if run.findings.is_empty() {
        ui.label(RichText::new(NO_FAILURES).color(tokens::TEXT_3));
        return;
    }
    let widths = [STATUS_W, RULE_W, what_width(ui.available_width()), NODE_W, PAGE_W];
    table::header(ui, &widths, &["", "rule", "what failed", "node", "page"]);
    let order = findings::order(run);
    let current = state.current_finding();
    let reveal = std::mem::take(&mut state.validation.reveal_in_table);
    let mut clicked = None;
    let mut scroll = ScrollArea::vertical()
        .id_salt("validation-table")
        .auto_shrink(false);
    if reveal && let Some(at) = order.iter().position(|index| Some(*index) == current) {
        // Keep the row in the middle of what is visible; the area clamps at both ends.
        let centre = (ui.available_height() - tokens::ROW_H) / 2.0;
        // A row number is far below f32's precision limit.
        let top = f32_from_usize(at).mul_add(tokens::ROW_H, -centre).max(0.0);
        scroll = scroll.vertical_scroll_offset(top);
    }
    scroll.show_rows(ui, tokens::ROW_H, order.len(), |ui, rows| {
        for row in rows {
            let index = order[row];
            let finding = &run.findings[usize_from_u32(index)];
            let node = findings::node_label(&loaded.doc, finding);
            let page =
                findings::page_of(&loaded.doc, finding).map_or_else(String::new, |p| (p + 1).to_string());
            let kind = if finding.target.is_some() {
                tokens::ACCENT
            } else {
                tokens::TEXT_3
            };
            let name = format!("{} {node}", findings::short_rule(&finding.rule));
            let cells = [
                Cell::Fail,
                Cell::mono_strong(findings::short_rule(&finding.rule), tokens::TEXT_2),
                Cell::ui(findings::title_of(finding), tokens::TEXT),
                Cell::mono(&node, kind),
                Cell::small(page, tokens::TEXT_3),
            ];
            let id = ui.make_persistent_id(("finding-row", index));
            if table::row(ui, id, &name, current == Some(index), &widths, &cells, None).clicked() {
                clicked = Some(index);
            }
        }
    });
    if let Some(index) = clicked {
        state.select_finding(index);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    // The widths are sums of exact halves and whole numbers.
    #[allow(clippy::float_cmp)]
    #[test]
    fn the_what_failed_column_takes_the_width_the_others_leave() {
        let fixed = STATUS_W + RULE_W + NODE_W + PAGE_W + 6.0 * GAP;

        assert_eq!(what_width(1000.0), 1000.0 - fixed);
        assert_eq!(what_width(1200.0) - what_width(1000.0), 200.0);
        // Smoke: a narrow drawer keeps the least it needs and the row is cut there.
        assert_eq!(what_width(400.0), WHAT_MIN_W);
    }
}
