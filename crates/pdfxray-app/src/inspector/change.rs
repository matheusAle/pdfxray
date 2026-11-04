//! The inspector of a comparison: the selected pair as the pairing reports it, both sides.
//!
//! Shown while the selection is a row of the Changes list, a node of B, or an element of A that
//! is paired. Every fact is the pairing's: status, both elements' paths, the residual move, the
//! typed changes over the generic row changes, and how far the pair can be trusted. A pair on
//! weak evidence says so first.

use egui::{Frame, Grid, Margin, RichText, ScrollArea, Ui};
use pdfxray_core::diff::{Method, Pair, Rung, Side};
use pdfxray_core::{NodeId, UnicodeSource};

use crate::state::diff::{low_confidence, page_pair_label, status_color, status_word};
use crate::state::path::crumb_label;
use crate::state::{AppState, ChangeRow};
use crate::theme::{fonts, tokens};

/// Shown for a node of B that no pair holds.
pub const UNPAIRED: &str = "This node of B is not a structure element, so it is in no pair.";

/// Title of the warning of a pair on weak evidence.
pub const LOW_CONFIDENCE_TITLE: &str = "Low confidence";

/// True when the inspector shows the comparison's card instead of A's node.
pub fn wants(state: &AppState) -> bool {
    let Some(diff) = &state.diff else {
        return false;
    };
    let row = state.selection.primary().and_then(|id| diff.row(id)).is_some();
    row || state.selection.side() == Side::B || diff.selected().is_some()
}

/// Draws the card of the selected change.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
        ScrollArea::vertical()
            .id_salt("change-card")
            .show(ui, |ui| card(ui, state));
    });
}

fn card(ui: &mut Ui, state: &AppState) {
    let Some(diff) = &state.diff else {
        return;
    };
    let row = state.selection.primary().and_then(|id| diff.row(id));
    if let Some(pair) = diff.selected() {
        pair_card(ui, state, pair);
        return;
    }
    let Some(pairing) = diff.pairing() else {
        return;
    };
    match row {
        Some(ChangeRow::Page(page) | ChangeRow::Residue(page)) => {
            let Some(m) = pairing.pages.get(page) else {
                return;
            };
            title(ui, &page_pair_label(m.a, m.b));
            facts(
                ui,
                &[
                    ("Element pairs", m.pairs.to_string()),
                    ("Outside the tags, A", m.residue.a.to_string()),
                    ("Outside the tags, B", m.residue.b.to_string()),
                    ("Only in A", m.residue.only_a.to_string()),
                    ("Only in B", m.residue.only_b.to_string()),
                ],
            );
            if m.residue.only_a + m.residue.only_b > 0 {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Painted content outside the structure tree (artifacts, untagged content) \
                         differs. It is compared as a whole per page, not element by element.",
                    )
                    .color(tokens::TEXT_2),
                );
            }
        }
        Some(ChangeRow::Unplaced) => {
            title(ui, "Not on a page");
            ui.label(
                RichText::new(
                    "Elements with no box on any page: containers, or a side compared on structure only.",
                )
                .color(tokens::TEXT_2),
            );
        }
        Some(ChangeRow::Pair(_)) | None => {
            let Some(id) = state.selection.primary() else {
                return;
            };
            if let Some(doc) = state.doc_of(Side::B) {
                title(ui, &crumb_label(doc, id));
            }
            ui.label(RichText::new(UNPAIRED).color(tokens::TEXT_2));
        }
    }
}

fn title(ui: &mut Ui, text: &str) {
    ui.label(fonts::title(text, tokens::text::TITLE, tokens::TEXT));
    ui.add_space(6.0);
}

fn facts(ui: &mut Ui, rows: &[(&str, String)]) {
    Grid::new(ui.next_auto_id())
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (key, value) in rows {
                ui.label(RichText::new(*key).color(tokens::TEXT_3));
                ui.label(
                    RichText::new(value)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT),
                );
                ui.end_row();
            }
        });
}

/// The path of `id` on `side`, `Document › Sect › P`.
fn path(state: &AppState, side: Side, id: &NodeId) -> String {
    let Some(doc) = state.doc_of(side) else {
        return id.to_string();
    };
    state
        .path_on(side, id)
        .iter()
        .map(|node| crumb_label(doc, node))
        .collect::<Vec<_>>()
        .join(" › ")
}

fn pair_card(ui: &mut Ui, state: &AppState, pair: &Pair) {
    ui.horizontal(|ui| {
        ui.label(fonts::title(&pair.role, tokens::text::TITLE, tokens::TEXT));
        ui.label(
            RichText::new(status_word(pair.status))
                .font(fonts::mono(tokens::mono::SMALL))
                .color(status_color(pair.status)),
        );
    });
    if !pair.text.is_empty() {
        ui.label(RichText::new(&pair.text).color(tokens::TEXT_2));
    }
    ui.add_space(6.0);
    if low_confidence(pair) {
        ui.label(
            RichText::new(LOW_CONFIDENCE_TITLE)
                .font(fonts::semibold(tokens::text::BODY))
                .color(tokens::WARN),
        );
        ui.label(RichText::new(confidence_why(pair)).color(tokens::WARN));
        ui.add_space(6.0);
    }
    let mut rows: Vec<(&str, String)> = Vec::new();
    if let Some(a) = &pair.a {
        rows.push(("A", path(state, Side::A, a)));
    }
    if let Some(b) = &pair.b {
        rows.push(("B", path(state, Side::B, b)));
    }
    let page = |p: Option<u32>| p.map_or_else(|| "–".to_owned(), |p| (p + 1).to_string());
    rows.push(("Page", format!("{} → {}", page(pair.page_a), page(pair.page_b))));
    if let Some((dx, dy)) = pair.delta {
        rows.push(("Residual", format!("{dx:+.1}, {dy:+.1} pt (offset removed)")));
    }
    if pair.reordered {
        rows.push(("Order", "changed place in the reading order".to_owned()));
    }
    rows.push(("Matched on", matched_on(pair)));
    facts(ui, &rows);
    if !pair.changed.is_empty() {
        ui.add_space(10.0);
        ui.label(RichText::new("Changed").font(fonts::semibold(tokens::text::BODY)));
        changes(
            ui,
            pair.changed
                .iter()
                .map(|c| (format!("{:?}", c.field), c.a.as_deref(), c.b.as_deref())),
        );
    }
    if !pair.rows.is_empty() {
        ui.add_space(10.0);
        ui.label(RichText::new("Every row that changed").font(fonts::semibold(tokens::text::BODY)));
        changes(
            ui,
            pair.rows
                .iter()
                .map(|r| (format!("{} › {}", r.group, r.key), r.a.as_deref(), r.b.as_deref())),
        );
    }
}

/// A table of `(what, A, B)`; a value a side does not set reads `none`.
fn changes<'a>(ui: &mut Ui, rows: impl Iterator<Item = (String, Option<&'a str>, Option<&'a str>)>) {
    Grid::new(ui.next_auto_id())
        .num_columns(3)
        .spacing([10.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for (what, a, b) in rows {
                ui.label(RichText::new(what).color(tokens::TEXT_3));
                ui.label(RichText::new(a.unwrap_or("none")).font(fonts::mono(tokens::mono::SMALL)));
                ui.label(RichText::new(b.unwrap_or("none")).font(fonts::mono(tokens::mono::SMALL)));
                ui.end_row();
            }
        });
}

/// The rung and method a pair was matched on, in words.
fn matched_on(pair: &Pair) -> String {
    let c = &pair.confidence;
    let rung = match c.rung {
        Some(Rung::RoleText) => "role and text",
        Some(Rung::Text) => "text",
        Some(Rung::RoleBox) => "role and box",
        Some(Rung::Geometry) => "box alone",
        None => return "one side only".to_owned(),
    };
    let method = match c.method {
        Some(Method::Anchor) => "anchored",
        Some(Method::Bounded) => "aligned inside a gap without anchors",
        Some(Method::Moved) => "found out of order",
        Some(Method::Children) => "by its children",
        None => "",
    };
    let unicode = [c.unicode_a, c.unicode_b]
        .iter()
        .any(|u| matches!(u, Some(UnicodeSource::Encoding | UnicodeSource::None)));
    let mut text = format!("{rung}, {method}");
    if unicode {
        text.push_str(", text without /ToUnicode");
    }
    text
}

/// Why a pair is low confidence.
fn confidence_why(pair: &Pair) -> String {
    let c = &pair.confidence;
    let mut why = Vec::new();
    if c.method == Some(Method::Bounded) {
        why.push("matched inside a stretch with no unique anchor");
    }
    if c.rung == Some(Rung::Geometry) {
        why.push("matched on its box alone");
    }
    if c.degraded_a {
        why.push("A's page capture is degraded");
    }
    if c.degraded_b {
        why.push("B's page capture is degraded");
    }
    why.join("; ")
}
