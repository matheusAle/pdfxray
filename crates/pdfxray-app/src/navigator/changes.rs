//! The Changes entry of a comparison: what changed, page pair by page pair. It is the primary
//! view of a comparison; the canvas follows the row selected here.
//!
//! Unchanged pairs and geometry within tolerance are hidden until the *Unchanged* chip shows
//! them; what changed outside the structure tree stays visible, one *Artifacts* row per page.
//! A pair matched on weak evidence carries a `low confidence` badge, so it does not read as a
//! certain one.

use egui::{RichText, Ui};
use pdfxray_core::NodeId;

use super::{filter_words, no_match_in, thousands};
use crate::components::{ToolButton, tooltip};
use crate::state::{AppState, ChangeRow};
use crate::theme::tokens;

/// Name of the header chip that shows the unchanged pairs too.
pub const UNCHANGED_LABEL: &str = "Unchanged";

/// Shown while the two documents are being paired.
pub const PAIRING_TEXT: &str = "Pairing the two documents…";

/// Shown when the two documents are the same.
pub const NO_CHANGES: &str = "No changes: every element pairs with an identical one.";

/// The header's count: `12 changes`.
pub(super) fn header_text(state: &AppState) -> Option<String> {
    let summary = state.diff.as_ref()?.pairing()?.summary();
    let n = summary.modified + summary.moved + summary.added + summary.removed;
    Some(format!(
        "{} {}",
        thousands(n),
        if n == 1 { "change" } else { "changes" }
    ))
}

/// The suppression chip at the right of the header.
pub(super) fn header_action(ui: &mut Ui, state: &mut AppState) {
    let Some(diff) = &mut state.diff else {
        return;
    };
    let on = diff.show_unchanged;
    let chip = ToolButton::new(UNCHANGED_LABEL).active(on).compact(true).show(ui);
    let chip = tooltip::text(
        chip,
        if on {
            "Unchanged pairs show. Click to list the changes only."
        } else {
            "Only changes show. Click to list the unchanged pairs too."
        },
    );
    if chip.clicked() {
        diff.set_show_unchanged(!on);
    }
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let words = filter_words(state);
    let filtered = !state.nav.filter.trim().is_empty();
    let Some(diff) = &mut state.diff else {
        return;
    };
    if let Some(why) = diff.pairing_error() {
        ui.label(RichText::new(format!("The documents could not be paired: {why}")).color(tokens::ERR));
        return;
    }
    if diff.is_pairing() {
        ui.label(RichText::new(PAIRING_TEXT).color(tokens::TEXT_3));
        return;
    }
    let notes = diff.pairing().map(|p| notes(p)).unwrap_or_default();
    for note in notes {
        ui.label(RichText::new(note).color(tokens::WARN));
    }
    // The most specific row holding the selection: the selected row itself, else its pair's
    // row, else its page pair's (content in no pair — all of an untagged page — or a pair the
    // list hides).
    let selected_row = state
        .selection
        .primary()
        .filter(|primary| diff.row(primary).is_some())
        .cloned()
        .or_else(|| {
            diff.selected_index()
                .and_then(|i| diff.row_id(ChangeRow::Pair(i)))
        })
        .or_else(|| diff.holding_page().and_then(|p| diff.row_id(ChangeRow::Page(p))));
    let Some(model) = diff.tree() else {
        return;
    };
    model.set_filter(&words);
    if model.is_empty() {
        ui.label(RichText::new(NO_CHANGES).color(tokens::TEXT_3));
        return;
    }
    if model.visible_len() == 0 {
        if filtered {
            no_match_in(ui, state, "changes");
        }
        return;
    }
    let output = model.ui(ui, selected_row.as_ref(), "changes");
    let picked = output.clicked.or(output.key_selected);
    if let Some(id) = picked {
        select_row(state, &id);
    }
    if let Some(id) = output.double_clicked {
        select_row(state, &id);
    }
}

/// Selects the row `id` of the Changes list.
fn select_row(state: &mut AppState, id: &NodeId) {
    if let Some(target) = state.diff.as_ref().and_then(|d| d.row(id)) {
        state.select_change(target);
    }
}

/// What limited the comparison, in words: a side compared on structure only, gaps too large to
/// align.
pub fn notes(pairing: &pdfxray_core::diff::Pairing) -> Vec<String> {
    use pdfxray_core::diff::{Note, Side};
    pairing
        .notes
        .iter()
        .map(|note| match note {
            Note::StructureOnly { side } => format!(
                "{} is encrypted: compared on structure only, without text, geometry or a visual diff.",
                if *side == Side::A { "A" } else { "B" }
            ),
            Note::UnboundedGaps { count } => format!(
                "{count} stretch(es) without any anchor were too large to align: their elements may \
                 show as removed and added."
            ),
        })
        .collect()
}
