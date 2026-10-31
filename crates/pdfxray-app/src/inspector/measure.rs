//! The inspector while two nodes are selected (the first, then a Shift-click on the second): the
//! space between them, how their edges line up and the print boxes of their page.

use egui::{Frame, Margin, RichText, ScrollArea, Stroke, Ui, vec2};
use pdfxray_core::NodeId;

use super::{Action, header, print};
use crate::center::canvas::measure::{Comparison, Gap, Horizontal, is_aligned};
use crate::components::crumb::{SEPARATOR_WIDTH, separator};
use crate::components::{Crumb, Icon, PropRow, section};
use crate::state::AppState;
use crate::state::path::crumb_label;
use crate::theme::{fonts, tokens};
use crate::units::{Lengths, Unit};

/// Title of the inspector with two nodes selected and a gap between them.
pub const TITLE: &str = "2 selected · gap";

/// Title when the two nodes cannot be compared.
pub const TITLE_NO_GAP: &str = "2 selected";

/// The line under the title.
pub const HINT: &str = "shift-click adds to the selection";

/// Title of the section with the gap.
pub const BETWEEN: &str = "Between A and B";

/// Title of the section about the edges.
pub const ALIGNMENT: &str = "Alignment";

/// Shown instead of the sections when the nodes cannot be compared.
pub const NOT_COMPARABLE: &str = "Both nodes need a box on the same page.";

/// How two left or right edges line up: `aligned · Δ 0 pt`, or the distance between them.
pub fn edge_text(delta: f64, lengths: &Lengths) -> String {
    if is_aligned(delta) {
        format!("aligned · Δ {}", lengths.format(0.0))
    } else {
        format!("Δ {}", lengths.signed(delta))
    }
}

/// The value of the Horizontal row.
pub fn horizontal_text(horizontal: Horizontal, lengths: &Lengths) -> String {
    match horizontal {
        Horizontal::Same => "A and B span the same width".to_owned(),
        Horizontal::BCoversA => "B fully overlaps A".to_owned(),
        Horizontal::ACoversB => "A fully overlaps B".to_owned(),
        Horizontal::Overlap(shared) => format!("overlap {}", lengths.format(shared)),
        Horizontal::Apart(gap) => format!("apart {}", lengths.format(gap)),
    }
}

/// The gap in each unit but the one of the status bar: the number and the unit name.
pub fn other_units(gap: &Gap, lengths: &Lengths) -> Vec<(String, &'static str)> {
    Unit::ALL
        .into_iter()
        .filter(|unit| *unit != lengths.unit)
        .map(|unit| {
            (
                Lengths::new(unit, lengths.user_unit).number(gap.distance),
                unit.label(),
            )
        })
        .collect()
}

pub(super) fn ui(ui: &mut Ui, state: &AppState, action: &mut Action) {
    let (Some(first), Some(second)) = (state.selection.primary.clone(), state.selection.secondary.clone())
    else {
        return;
    };
    let comparison = state.comparison();
    crumbs(ui, state, &first, &second, action);
    title(ui, comparison.is_some());
    let lengths = state.lengths();
    ScrollArea::vertical()
        .id_salt("inspector-compare")
        .auto_shrink(false)
        .show(ui, |ui| {
            let Some(comparison) = comparison else {
                super::note(ui, RichText::new(NOT_COMPARABLE).color(tokens::TEXT_3));
                return;
            };
            between(ui, &comparison, &lengths);
            alignment(ui, &comparison, &lengths);
            if let Some(info) = state
                .document
                .as_ref()
                .and_then(|loaded| loaded.doc.page(state.canvas.page))
            {
                print::show(ui, info, &lengths);
            }
        });
}

/// The path to the first node, then `+` and the second one.
fn crumbs(ui: &mut Ui, state: &AppState, first: &NodeId, second: &NodeId, action: &mut Action) {
    let Some(loaded) = &state.document else {
        return;
    };
    let doc = &loaded.doc;
    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
            ui.horizontal_wrapped(|ui| {
                let path = state.path_to(first);
                let last = path.len().saturating_sub(1);
                for (n, id) in path.iter().enumerate() {
                    if n > 0 {
                        separator(ui);
                    }
                    let name = crumb_label(doc, id);
                    let crumb = Crumb::new(&name, tokens::kind_color(id)).current(n == last);
                    if crumb.show(ui).clicked() {
                        action.navigate = Some(id.clone());
                    }
                }
                ui.add_space(SEPARATOR_WIDTH / 2.0);
                ui.label(RichText::new("+").color(tokens::TEXT_3));
                let name = crumb_label(doc, second);
                let crumb = Crumb::new(&name, tokens::PINK).current(true);
                if crumb.show(ui).clicked() {
                    action.navigate = Some(second.clone());
                }
            });
        });
}

fn title(ui: &mut Ui, comparable: bool) {
    header::block(
        ui,
        |ui| {
            Icon::Measure.show(ui, 16.0, tokens::ACCENT);
        },
        if comparable { TITLE } else { TITLE_NO_GAP },
        HINT,
        None,
    );
}

fn between(ui: &mut Ui, comparison: &Comparison, lengths: &Lengths) {
    let gap = comparison.gap();
    section::show(ui, BETWEEN, BETWEEN, |ui| {
        Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.label(
                RichText::new(lengths.format(gap.distance))
                    .font(fonts::mono_medium(22.0))
                    .color(tokens::TEXT),
            );
            ui.label(
                RichText::new(gap.describe())
                    .font(fonts::ui(tokens::text::BODY))
                    .color(tokens::TEXT_3),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let tiles = other_units(&gap, lengths);
                let width = (ui.available_width() - 6.0 * 2.0) / 3.0;
                for (number, unit) in tiles {
                    tile(ui, width, &number, unit);
                }
            });
        });
    });
}

fn tile(ui: &mut Ui, width: f32, number: &str, unit: &str) {
    Frame::new()
        .fill(tokens::SURFACE_2)
        .stroke(Stroke::new(1.0, tokens::BORDER))
        .corner_radius(tokens::RADIUS_MD)
        .inner_margin(Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(width - 16.0 - 2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(number)
                    .font(fonts::mono(tokens::mono::LARGE))
                    .color(tokens::TEXT),
            );
            ui.label(
                RichText::new(unit)
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
        });
}

fn alignment(ui: &mut Ui, comparison: &Comparison, lengths: &Lengths) {
    section::show(ui, ALIGNMENT, ALIGNMENT, |ui| {
        let (left, right) = (comparison.left_delta(), comparison.right_delta());
        PropRow::new("Left edges", &edge_text(left, lengths))
            .good(is_aligned(left))
            .show(ui);
        PropRow::new("Right edges", &edge_text(right, lengths))
            .good(is_aligned(right))
            .show(ui);
        PropRow::new("Horizontal", &horizontal_text(comparison.horizontal(), lengths)).show(ui);
    });
}

#[cfg(test)]
mod tests {
    use pdfxray_core::Rect;
    use pretty_assertions::assert_eq;

    use super::*;

    fn pt() -> Lengths {
        Lengths::new(Unit::Pt, 1.0)
    }

    #[test]
    fn edges_are_aligned_or_say_how_far_apart_they_are() {
        assert_eq!(edge_text(0.0, &pt()), "aligned · Δ 0 pt");
        assert_eq!(edge_text(0.0004, &pt()), "aligned · Δ 0 pt");
        assert_eq!(edge_text(3.5, &pt()), "Δ +3.5 pt");
        assert_eq!(edge_text(-2.0, &Lengths::new(Unit::Mm, 1.0)), "Δ -0.71 mm");
    }

    #[test]
    fn the_horizontal_row_says_which_box_covers_the_other() {
        assert_eq!(horizontal_text(Horizontal::BCoversA, &pt()), "B fully overlaps A");
        assert_eq!(horizontal_text(Horizontal::ACoversB, &pt()), "A fully overlaps B");
        assert_eq!(
            horizontal_text(Horizontal::Same, &pt()),
            "A and B span the same width"
        );
        assert_eq!(horizontal_text(Horizontal::Overlap(12.0), &pt()), "overlap 12 pt");
        assert_eq!(horizontal_text(Horizontal::Apart(5.0), &pt()), "apart 5 pt");
    }

    #[test]
    fn the_gap_is_shown_in_the_three_other_units() {
        let comparison = Comparison {
            first: Rect::new(0.0, 100.0, 50.0, 120.0),
            second: Rect::new(0.0, 0.0, 50.0, 60.0),
        };
        let gap = comparison.gap();

        assert!((gap.distance - 40.0).abs() < 1e-9);
        assert_eq!(
            other_units(&gap, &pt()),
            [
                ("14.11".to_owned(), "mm"),
                ("0.556".to_owned(), "in"),
                ("53.33".to_owned(), "px")
            ]
        );
        assert_eq!(
            other_units(&gap, &Lengths::new(Unit::In, 1.0)),
            [
                ("40".to_owned(), "pt"),
                ("14.11".to_owned(), "mm"),
                ("53.33".to_owned(), "px")
            ]
        );
    }
}
