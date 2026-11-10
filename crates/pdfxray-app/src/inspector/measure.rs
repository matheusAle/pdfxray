//! The inspector while two nodes are selected (the primary and one more: a Shift-click, or a
//! marquee that caught two): the space between them, measured from the primary, how their edges
//! line up and the print boxes of their page.
//!
//! *Between A and B* gives the gap, or how deep the boxes overlap, then the gap or overlap along
//! each axis and the size of what they share. A length under one unit keeps four decimals, so a
//! hairline gap or overlap a fraction of a point wide is not rounded to nothing, and *On screen*
//! says how many pixels it is at the canvas's zoom, flagging one under a pixel.

use egui::{Frame, Margin, RichText, ScrollArea, Ui, vec2};
use pdfxray_core::NodeId;

use super::{Action, header, print};
use crate::center::canvas::measure::{Comparison, Horizontal, is_aligned};
use crate::components::crumb::{SEPARATOR_WIDTH, separator};
use crate::components::{Crumb, Icon, PropRow, section};
use crate::state::AppState;
use crate::state::path::crumb_label;
use crate::theme::{fonts, tokens};
use crate::units::Lengths;

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

/// Below this many points two edges along an axis count as touching.
const TOUCH: f64 = 1e-4;

/// The rows of the axes of *Between A and B*.
pub const ALONG_X: &str = "Along x";
/// See [`ALONG_X`].
pub const ALONG_Y: &str = "Along y";
/// The row of the area both boxes cover.
pub const OVERLAP_AREA: &str = "Overlap area";
/// The row of the measured length in screen pixels.
pub const ON_SCREEN: &str = "On screen";

/// How the boxes relate along one axis, from their `overlap` there (negative: apart):
/// `gap 0.0042 pt`, `overlap 3.25 pt` or `touching`.
pub fn axis_text(overlap: f64, lengths: &Lengths) -> String {
    if overlap > TOUCH {
        format!("overlap {}", lengths.precise(overlap))
    } else if overlap < -TOUCH {
        format!("gap {}", lengths.precise(-overlap))
    } else {
        "touching".to_owned()
    }
}

/// What the big number of *Between A and B* is: the gap, or the depth of the overlap, in user
/// units; and the word for it.
pub fn headline(comparison: &Comparison) -> (f64, &'static str) {
    let gap = comparison.gap();
    match comparison.overlap_depth() {
        Some(depth) => (depth, "overlap"),
        None => (gap.distance, "gap"),
    }
}

/// `length` points on the canvas at `zoom` (screen points per point): `0.42 px @136% · under a
/// pixel`, the last part only for a length that is there but under a pixel.
pub fn on_screen_text(points: f64, zoom: f64) -> String {
    let px = points * zoom;
    let mut text = format!("{} px @{:.0}%", trim_px(px), zoom * 100.0);
    if px > 0.0 && px < 1.0 {
        text.push_str(" · under a pixel");
    }
    text
}

/// Pixels with two decimals, trailing zeros dropped.
fn trim_px(px: f64) -> String {
    let text = format!("{px:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

pub(super) fn ui(ui: &mut Ui, state: &AppState, action: &mut Action) {
    let (Some(first), Some(second)) = (
        state.selection.primary().cloned(),
        state.selection.other().cloned(),
    ) else {
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
            between(ui, &comparison, &lengths, f64::from(state.canvas.zoom));
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

fn between(ui: &mut Ui, comparison: &Comparison, lengths: &Lengths, zoom: f64) {
    let gap = comparison.gap();
    let (length, word) = headline(comparison);
    section::show(ui, BETWEEN, BETWEEN, |ui| {
        Frame::new().inner_margin(Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(lengths.precise(length))
                        .font(fonts::mono_medium(22.0))
                        .color(tokens::TEXT),
                );
                ui.label(
                    RichText::new(word)
                        .font(fonts::ui(tokens::text::BODY))
                        .color(tokens::TEXT_3),
                );
            });
            ui.label(
                RichText::new(gap.describe())
                    .font(fonts::ui(tokens::text::BODY))
                    .color(tokens::TEXT_3),
            );
        });
        PropRow::new(ALONG_X, &axis_text(comparison.overlap_x(), lengths)).show(ui);
        PropRow::new(ALONG_Y, &axis_text(comparison.overlap_y(), lengths)).show(ui);
        if let Some(shared) = comparison.intersection() {
            let area = format!(
                "{} × {}",
                lengths.precise_number(shared.width()),
                lengths.precise(shared.height())
            );
            PropRow::new(OVERLAP_AREA, &area).show(ui);
        }
        let points = lengths.points(length);
        let px = points * zoom;
        PropRow::new(ON_SCREEN, &on_screen_text(points, zoom))
            .warn(px > 0.0 && px < 1.0)
            .show(ui);
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
    use crate::units::Unit;

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
    fn each_axis_says_gap_overlap_or_touching_to_a_fraction_of_a_point() {
        assert_eq!(axis_text(-0.0042, &pt()), "gap 0.0042 pt");
        assert_eq!(axis_text(3.25, &pt()), "overlap 3.25 pt");
        assert_eq!(axis_text(0.00004, &pt()), "touching");
        assert_eq!(axis_text(-12.5, &pt()), "gap 12.5 pt");
        // Smoke: other units keep their decimals for a length of a unit or more.
        assert_eq!(axis_text(-28.3465, &Lengths::new(Unit::Mm, 1.0)), "gap 10 mm");
    }

    #[test]
    fn the_big_number_is_the_gap_or_how_deep_the_boxes_overlap() {
        let apart = Comparison {
            first: Rect::new(0.0, 100.0, 50.0, 120.0),
            second: Rect::new(0.0, 0.0, 50.0, 99.6),
        };
        // 40 wide by 10 tall shared, the smaller of the two: 10 deep.
        let overlapping = Comparison {
            first: Rect::new(0.0, 0.0, 50.0, 20.0),
            second: Rect::new(10.0, 10.0, 60.0, 40.0),
        };

        let (gap, word) = headline(&apart);
        assert!((gap - 0.4).abs() < 1e-9 && word == "gap", "{gap} {word}");
        assert_eq!(headline(&overlapping), (10.0, "overlap"));
        assert_eq!(on_screen_text(0.4, 1.36), "0.54 px @136% · under a pixel");
        assert_eq!(on_screen_text(10.0, 1.0), "10 px @100%");
        // Smoke: nothing at all is not under a pixel, it is nothing.
        assert_eq!(on_screen_text(0.0, 2.0), "0 px @200%");
    }
}
