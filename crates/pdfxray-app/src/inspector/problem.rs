//! The selection's problem, first: a card above everything else with what is wrong in a line and
//! a Details button that opens the finding.
//!
//! A failed validation check on the node is a problem, and so are a figure without alternative
//! text and painted content that is neither tagged nor an artifact. Several problems stack, the
//! worst on top.

use egui::{Frame, Margin, RichText, Stroke, Ui};
use pdfxray_core::{NodeId, Severity};

use crate::components::{Icon, ToolButton};
use crate::findings::{short_rule, title_of};
use crate::state::{AppState, RailItem};
use crate::theme::{fonts, tokens};

/// Name of the button of a card that has a finding behind it.
pub const DETAILS_LABEL: &str = "Details";

/// What the card of a figure without alternative text says.
pub const NO_ALT: &str = "No alternative text";

/// What the card of untagged content says.
pub const UNTAGGED: &str = "Not tagged and not an artifact";

/// Most cards shown at once.
const MAX_CARDS: usize = 3;

/// One problem of the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// How serious it is: the colour of the card.
    pub severity: Severity,
    /// What is wrong, in a few words.
    pub title: String,
    /// Why it matters, in a line.
    pub detail: String,
    /// The validation finding behind it, for the Details button.
    pub finding: Option<u32>,
}

/// The first sentence of `text`.
fn first_sentence(text: &str) -> &str {
    text.split_once(". ")
        .map_or(text.trim_end_matches('.'), |(first, _)| first)
}

/// The problems of `id`, worst first.
pub fn of(state: &mut AppState, id: &NodeId) -> Vec<Problem> {
    let mut found: Vec<Problem> = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.validation())
        .map(|run| {
            run.findings_for(id)
                .map(|(index, finding)| Problem {
                    severity: Severity::Error,
                    title: title_of(finding),
                    detail: format!(
                        "{} · rule {}",
                        finding.explain().map_or_else(
                            || finding.description.clone(),
                            |text| first_sentence(text.why).to_owned()
                        ),
                        short_rule(&finding.rule)
                    ),
                    finding: Some(index),
                })
                .collect()
        })
        .unwrap_or_default();
    if found.is_empty() {
        found.extend(own_problem(state, id));
    }
    found.truncate(MAX_CARDS);
    found
}

/// A problem the app finds in the node itself, for when no validation has run.
fn own_problem(state: &mut AppState, id: &NodeId) -> Option<Problem> {
    match id {
        NodeId::StructElem(_) if state.tag_index().is_some_and(|tags| tags.lacks_alt(id)) => Some(Problem {
            severity: Severity::Warning,
            title: NO_ALT.to_owned(),
            detail: "Screen readers skip this figure".to_owned(),
            finding: None,
        }),
        NodeId::Op { page, ix } => {
            let (page, ix) = (*page, *ix);
            let untagged = state
                .page_stats(page)
                .is_some_and(|stats| stats.coverage.untagged_ops.contains(&ix));
            untagged.then(|| Problem {
                severity: Severity::Error,
                title: UNTAGGED.to_owned(),
                detail: "Assistive technology cannot reach this content".to_owned(),
                finding: None,
            })
        }
        _ => None,
    }
}

/// The colour of a card.
fn tint(severity: Severity) -> egui::Color32 {
    match severity {
        Severity::Info => tokens::ACCENT,
        Severity::Warning => tokens::WARN,
        Severity::Error => tokens::ERR,
    }
}

/// Draws the cards; returns the finding whose Details button was pressed.
pub fn show(ui: &mut Ui, problems: &[Problem]) -> Option<u32> {
    let mut details = None;
    for problem in problems {
        let color = tint(problem.severity);
        Frame::new()
            .outer_margin(Margin::symmetric(12, 4))
            .fill(color.gamma_multiply(0.08))
            .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
            .corner_radius(tokens::RADIUS_LG)
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    Icon::Warning.show(ui, 14.0, color);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let button_w = if problem.finding.is_some() { 64.0 } else { 0.0 };
                        ui.set_max_width((ui.available_width() - button_w).max(80.0));
                        ui.add(
                            egui::Label::new(
                                RichText::new(&problem.title)
                                    .font(fonts::semibold(tokens::text::BODY))
                                    .color(tokens::TEXT),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&problem.detail)
                                    .font(fonts::ui(tokens::text::SMALL))
                                    .color(tokens::TEXT_2),
                            )
                            .wrap(),
                        );
                    });
                    if let Some(finding) = problem.finding {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ToolButton::new(DETAILS_LABEL).secondary().show(ui).clicked() {
                                details = Some(finding);
                            }
                        });
                    }
                });
            });
    }
    details
}

/// Opens the finding: the Validation entry with the finding selected.
pub fn open(state: &mut AppState, finding: u32) {
    state.set_rail(RailItem::Validation);
    state.select_finding(finding);
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_card_says_the_first_sentence_of_why() {
        assert_eq!(
            first_sentence("Screen readers skip this figure. A blind reader hears nothing."),
            "Screen readers skip this figure"
        );
        assert_eq!(first_sentence("One sentence."), "One sentence");
        // Smoke: nothing to cut.
        assert_eq!(first_sentence("plain"), "plain");
    }
}
