//! Rulers along the top and left edge of the canvas.
//!
//! They measure display space: the origin is the top-left corner of the page as it is drawn (its
//! `CropBox` clipped to the `MediaBox`, turned by `/Rotate`), and values grow to the right and
//! downwards, like the `display` coordinates of the cursor in the status bar. The tick maths is
//! pure and tested separately from the painting.

use egui::{Align2, Pos2, Rect as UiRect, Stroke, Ui};
use pdfxray_core::numeric::f32_from_f64;

use super::geometry::PageGeometry;
use crate::theme::{fonts, tokens};
use crate::units::{Lengths, Unit};

/// Thickness of a ruler strip, in points.
pub const THICKNESS: f32 = 26.0;

/// Smallest distance between two labelled ticks, in points.
const MIN_LABEL_SPACING: f32 = 56.0;

/// Longest tick list that is ever generated.
const MAX_TICKS: usize = 2_000;

/// Hover text of the ruler corner: the origin convention.
pub const ORIGIN_HELP: &str = "Ruler origin: the top-left corner of the page as shown (its CropBox, \
turned by /Rotate). x grows to the right and y downwards, like the display coordinates in the \
status bar. Values are in the unit chosen in the status bar.";

/// [`ORIGIN_HELP`], plus a note when the page's `/UserUnit` scales its coordinates.
pub fn origin_help(user_unit: f64) -> String {
    if (user_unit - 1.0).abs() < f64::EPSILON {
        ORIGIN_HELP.to_owned()
    } else {
        format!(
            "{ORIGIN_HELP} This page has /UserUnit {}: lengths are the coordinates in the file times {0}.",
            pdfxray_core::num(user_unit)
        )
    }
}

/// The text in the ruler corner: the origin value and the unit, e.g. `0 pt`.
pub fn corner_text(unit: Unit) -> String {
    format!("0 {}", unit.label())
}

/// One mark on a ruler.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tick {
    /// Screen position along the ruler.
    pub(crate) pos: f32,
    /// The number written next to major ticks.
    pub(crate) label: Option<String>,
}

/// The smallest 1, 2 or 5 times a power of ten that is not below `raw`.
pub(crate) fn nice_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let base = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * base)
        .find(|step| *step >= raw * (1.0 - 1e-9))
        .unwrap_or(10.0 * base)
}

/// Ticks of a ruler whose page extent runs from screen position `start` (value `v_start`) to
/// `end` (value `v_end`). Values are in page units; `lengths` converts them for the labels.
/// Major ticks carry a label, minor ones sit halfway between.
pub(crate) fn ticks(start: f32, end: f32, v_start: f64, v_end: f64, lengths: &Lengths) -> Vec<Tick> {
    let (p0, p1) = (lengths.points(v_start), lengths.points(v_end));
    let per_point = lengths.unit.per_point();
    let span_points = p1 - p0;
    let length = f64::from(end - start).abs();
    if span_points == 0.0 || length < 1.0 {
        return Vec::new();
    }
    // Screen points per ruler point and the step, chosen in the displayed unit.
    let px_per_point = f64::from(end - start) / span_points;
    let step_unit = nice_step(f64::from(MIN_LABEL_SPACING) * per_point / px_per_point.abs());
    let step = step_unit / per_point;

    let (lo, hi) = (p0.min(p1), p0.max(p1));
    let first = (lo / (step / 2.0)).ceil();
    let last = (hi / (step / 2.0)).floor();
    let mut out = Vec::new();
    let mut k = first;
    while k <= last && out.len() < MAX_TICKS {
        let value = k * (step / 2.0);
        let major = (k % 2.0).abs() < f64::EPSILON;
        // Positions are screen points; f32 is plenty.
        let pos = start + f32_from_f64((value - p0) * px_per_point);
        out.push(Tick {
            pos,
            label: major.then(|| lengths.unit.number(value)),
        });
        k += 1.0;
    }
    out
}

/// The ruler strips around the canvas.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Strips {
    /// Along the top edge.
    pub(crate) top: UiRect,
    /// Along the left edge.
    pub(crate) left: UiRect,
}

/// Display-space values at both ends of the page image along one screen axis: from 0 at the
/// top-left corner to the page's width or height.
fn values(geo: &PageGeometry, horizontal: bool) -> (f64, f64) {
    let (start, end) = (geo.rect.left_top(), geo.rect.right_bottom());
    let (a, b) = (geo.screen_to_display(start), geo.screen_to_display(end));
    if horizontal { (a.x, b.x) } else { (a.y, b.y) }
}

/// Draws both rulers for the page at `geo`.
pub(crate) fn paint(ui: &Ui, strips: Strips, geo: &PageGeometry, lengths: &Lengths) {
    let (fill, ink) = (tokens::SURFACE, tokens::TEXT_3);
    let stroke = Stroke::new(1.0, tokens::BORDER_STRONG);
    let font = fonts::mono(tokens::mono::TINY);

    let top = ui.painter_at(strips.top);
    top.rect_filled(strips.top, 0.0, fill);
    let (v0, v1) = values(geo, true);
    for tick in ticks(geo.rect.min.x, geo.rect.max.x, v0, v1, lengths) {
        if tick.pos < strips.top.min.x || tick.pos > strips.top.max.x {
            continue;
        }
        let length = if tick.label.is_some() {
            THICKNESS * 0.55
        } else {
            THICKNESS * 0.3
        };
        let bottom = strips.top.max.y;
        top.line_segment(
            [Pos2::new(tick.pos, bottom - length), Pos2::new(tick.pos, bottom)],
            stroke,
        );
        if let Some(label) = tick.label {
            top.text(
                Pos2::new(tick.pos + 2.0, strips.top.min.y + 1.0),
                Align2::LEFT_TOP,
                label,
                font.clone(),
                ink,
            );
        }
    }
    top.line_segment([strips.top.left_bottom(), strips.top.right_bottom()], stroke);

    let left = ui.painter_at(strips.left);
    left.rect_filled(strips.left, 0.0, fill);
    let (v0, v1) = values(geo, false);
    for tick in ticks(geo.rect.min.y, geo.rect.max.y, v0, v1, lengths) {
        if tick.pos < strips.left.min.y || tick.pos > strips.left.max.y {
            continue;
        }
        let length = if tick.label.is_some() {
            THICKNESS * 0.55
        } else {
            THICKNESS * 0.3
        };
        let right = strips.left.max.x;
        left.line_segment(
            [Pos2::new(right - length, tick.pos), Pos2::new(right, tick.pos)],
            stroke,
        );
        if let Some(label) = tick.label {
            left.text(
                Pos2::new(strips.left.min.x + 1.0, tick.pos + 1.0),
                Align2::LEFT_TOP,
                label,
                font.clone(),
                ink,
            );
        }
    }
    left.line_segment([strips.left.right_top(), strips.left.right_bottom()], stroke);
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn labels(ticks: &[Tick]) -> Vec<String> {
        ticks.iter().filter_map(|t| t.label.clone()).collect()
    }

    #[test]
    fn steps_are_one_two_or_five_times_a_power_of_ten() {
        assert_eq!(nice_step(0.7), 1.0);
        assert_eq!(nice_step(1.0), 1.0);
        assert_eq!(nice_step(1.1), 2.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(5.1), 10.0);
        assert_eq!(nice_step(56.0), 100.0);
        assert_eq!(nice_step(0.03), 0.05);
        assert_eq!(nice_step(0.0), 1.0);
        assert_eq!(nice_step(f64::NAN), 1.0);
    }

    #[test]
    fn a_200_pt_page_at_100_percent_is_labelled_every_100_points() {
        // Page image from screen x 30 to 230: 1 screen point per page point.
        let t = ticks(30.0, 230.0, 0.0, 200.0, &Lengths::new(Unit::Pt, 1.0));

        assert_eq!(labels(&t), ["0", "100", "200"]);
        assert_eq!(
            t.iter().map(|t| t.pos).collect::<Vec<_>>(),
            [30.0, 80.0, 130.0, 180.0, 230.0]
        );
        // Smoke: minor ticks have no label and sit between the major ones.
        assert_eq!(t.iter().filter(|t| t.label.is_none()).count(), 2);
    }

    #[test]
    fn labels_follow_the_unit() {
        // 200 pt over 400 screen points; millimetres: 70.56 mm in total, a label every 10 mm.
        let t = ticks(0.0, 400.0, 0.0, 200.0, &Lengths::new(Unit::Mm, 1.0));

        assert_eq!(labels(&t), ["0", "10", "20", "30", "40", "50", "60", "70"]);
        // Ticks alternate major/minor every 5 mm; 20 mm is 56.69 pt, i.e. 113.39 screen points.
        assert_eq!(t[4].label.as_deref(), Some("20"));
        assert!((t[4].pos - 113.385_83).abs() < 1e-3, "{}", t[4].pos);
    }

    #[test]
    fn upright_rulers_count_from_the_top_left_corner_and_y_counts_down() {
        // minimal-one-page: 200 x 100 pt, no CropBox of its own.
        let geo = super::super::geometry::fixture_geometry("minimal-one-page", 0, 1.0);

        // Left to right x grows from 0 to 200; top to bottom y grows from 0 to 100.
        assert_eq!(values(&geo, true), (0.0, 200.0));
        assert_eq!(values(&geo, false), (0.0, 100.0));
        // Smoke: the vertical ruler's labels read 0 at the top, downwards.
        let t = ticks(
            geo.rect.min.y,
            geo.rect.max.y,
            0.0,
            100.0,
            &Lengths::new(Unit::Pt, 1.0),
        );
        assert_eq!(
            t.first().map(|t| (t.pos, t.label.clone())),
            Some((geo.rect.min.y, Some("0".to_owned())))
        );
    }

    #[test]
    fn a_turned_page_is_measured_as_it_is_shown() {
        // page-boxes page 1: /Rotate 90, CropBox [10 10 390 290]: drawn 280 wide and 380 tall.
        let geo = super::super::geometry::fixture_geometry("page-boxes", 0, 1.0);

        // Both rulers start at the shown page's top-left corner, not at the CropBox's.
        assert_eq!(values(&geo, true), (0.0, 280.0));
        assert_eq!(values(&geo, false), (0.0, 380.0));
    }

    #[test]
    fn degenerate_rulers_have_no_ticks() {
        let lengths = Lengths::new(Unit::Pt, 1.0);

        assert_eq!(ticks(0.0, 0.0, 0.0, 10.0, &lengths), []);
        assert_eq!(ticks(0.0, 100.0, 5.0, 5.0, &lengths), []);
    }

    #[test]
    fn the_corner_names_the_origin_and_unit() {
        assert_eq!(corner_text(Unit::Mm), "0 mm");
    }

    #[test]
    fn the_help_mentions_a_user_unit_only_when_it_matters() {
        assert_eq!(origin_help(1.0), ORIGIN_HELP);
        assert_eq!(
            origin_help(2.0),
            format!(
                "{ORIGIN_HELP} This page has /UserUnit 2: lengths are the coordinates in the file times 2."
            )
        );
    }
}
