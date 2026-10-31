//! Measurement math: the measure tool's line and the comparison of two nodes' bounding boxes.
//!
//! All values are page-space user units (origin bottom-left, y up). [`Lengths`] turns them into
//! text in the unit the user chose.

use pdfxray_core::numeric::usize_from_f64_trunc;
use pdfxray_core::{PageIx, Point, Rect};

use crate::units::Lengths;

/// Edges closer than this (user units) count as aligned, boxes closer than this as touching.
const TOLERANCE: f64 = 1e-3;

/// The directions a line snaps to while Shift is held, every 45° counter-clockwise from the
/// x axis, as exact unit vectors so horizontal and vertical lines have no stray component.
const SNAP_DIRECTIONS: [(f64, f64); 8] = {
    let r = std::f64::consts::FRAC_1_SQRT_2;
    [
        (1.0, 0.0),
        (r, r),
        (0.0, 1.0),
        (-r, r),
        (-1.0, 0.0),
        (-r, -r),
        (0.0, -1.0),
        (r, -r),
    ]
};

/// `to` moved onto the nearest of the 45° directions from `from`: its projection on that
/// direction, so a nearly horizontal line becomes horizontal with the same Δx. Page space: the
/// directions stay horizontal, vertical and diagonal on a page with `/Rotate`.
pub fn snap_angle(from: Point, to: Point) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    if dx == 0.0 && dy == 0.0 {
        return to;
    }
    let eighths = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round();
    // `eighths` is in -4..=4; -4 and 4 are both "left".
    let (cos, sin) = SNAP_DIRECTIONS[usize_from_f64_trunc(eighths.rem_euclid(8.0))];
    let along = dx * cos + dy * sin;
    Point::new(from.x + along * cos, from.y + along * sin)
}

/// A line drawn with the measure tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// Page the line was drawn on.
    pub page: PageIx,
    /// Where it starts.
    pub from: Point,
    /// Where it ends; `None` while waiting for the second click.
    pub to: Option<Point>,
}

/// The vector from the start to the end of a [`Line`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    /// Horizontal part; positive to the right.
    pub dx: f64,
    /// Vertical part in page space; positive upwards.
    pub dy: f64,
    /// Straight-line length.
    pub distance: f64,
}

impl Offset {
    /// The offset from `from` to `to`.
    pub fn between(from: Point, to: Point) -> Self {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        Self {
            dx,
            dy,
            distance: dx.hypot(dy),
        }
    }

    /// Labelled rows for display: distance, then the two components.
    pub fn rows(&self, lengths: &Lengths) -> [(&'static str, String); 3] {
        [
            ("Distance", lengths.format(self.distance)),
            ("Δx", lengths.signed(self.dx)),
            ("Δy", lengths.signed(self.dy)),
        ]
    }
}

/// State of the measure tool.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeasureTool {
    /// Whether canvas clicks and drags measure instead of selecting.
    pub active: bool,
    line: Option<Line>,
}

impl MeasureTool {
    /// Turns the tool on or off; turning it off forgets the line.
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if !self.active {
            self.line = None;
        }
    }

    /// Forgets the current line.
    pub fn clear(&mut self) {
        self.line = None;
    }

    /// The current line, if any.
    pub const fn line(&self) -> Option<Line> {
        self.line
    }

    /// The finished measurement, if the line has both ends.
    pub fn offset(&self) -> Option<Offset> {
        let line = self.line?;
        Some(Offset::between(line.from, line.to?))
    }

    /// A plain click at `point`: the first click starts a line, the second one ends it, the
    /// third starts the next one. With `snap` (Shift held) the second click ends the line at
    /// the nearest 45° angle from the first.
    pub fn click(&mut self, page: PageIx, point: Point, snap: bool) {
        match &mut self.line {
            Some(line) if line.page == page && line.to.is_none() => {
                line.to = Some(Self::end(line.from, point, snap));
            }
            _ => {
                self.line = Some(Line {
                    page,
                    from: point,
                    to: None,
                });
            }
        }
    }

    /// A drag begins at `point`; the line follows [`Self::drag_to`].
    pub fn drag_start(&mut self, page: PageIx, point: Point) {
        self.line = Some(Line {
            page,
            from: point,
            to: Some(point),
        });
    }

    /// The drag reached `point`; with `snap` (Shift held) the line keeps to the nearest 45°
    /// angle from where the drag began.
    pub fn drag_to(&mut self, point: Point, snap: bool) {
        if let Some(line) = &mut self.line {
            line.to = Some(Self::end(line.from, point, snap));
        }
    }

    /// Where a line from `from` ends when the pointer is at `point`.
    pub fn end(from: Point, point: Point, snap: bool) -> Point {
        if snap { snap_angle(from, point) } else { point }
    }
}

/// Two bounding boxes on one page, compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    /// The primary selection.
    pub first: Rect,
    /// The second selection.
    pub second: Rect,
}

/// How far two intervals overlap; negative when there is a gap between them.
fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    a1.min(b1) - a0.max(b0)
}

/// The part of the axis between two intervals that is either shared or empty between them.
fn span(a0: f64, a1: f64, b0: f64, b1: f64) -> (f64, f64) {
    if overlap(a0, a1, b0, b1) >= 0.0 {
        (a0.max(b0), a1.min(b1))
    } else if a1 < b0 {
        (a1, b0)
    } else {
        (b1, a0)
    }
}

impl Comparison {
    /// Horizontal overlap of the boxes; negative when they are apart.
    pub fn overlap_x(&self) -> f64 {
        overlap(self.first.x0, self.first.x1, self.second.x0, self.second.x1)
    }

    /// Vertical overlap of the boxes; negative when they are apart.
    pub fn overlap_y(&self) -> f64 {
        overlap(self.first.y0, self.first.y1, self.second.y0, self.second.y1)
    }

    /// Empty space between the boxes along x; zero when they overlap or touch.
    pub fn gap_x(&self) -> f64 {
        (-self.overlap_x()).max(0.0)
    }

    /// Empty space between the boxes along y; zero when they overlap or touch.
    pub fn gap_y(&self) -> f64 {
        (-self.overlap_y()).max(0.0)
    }

    /// The region both boxes cover, if they overlap in both directions.
    pub fn intersection(&self) -> Option<Rect> {
        (self.overlap_x() > TOLERANCE && self.overlap_y() > TOLERANCE).then(|| {
            Rect::new(
                self.first.x0.max(self.second.x0),
                self.first.y0.max(self.second.y0),
                self.first.x1.min(self.second.x1),
                self.first.y1.min(self.second.y1),
            )
        })
    }

    /// How far the second box's left edge is right of the first one's (negative: left of it).
    pub fn left_delta(&self) -> f64 {
        self.second.x0 - self.first.x0
    }

    /// How far the second box's right edge is right of the first one's.
    pub fn right_delta(&self) -> f64 {
        self.second.x1 - self.first.x1
    }

    /// Segments to draw for the gaps: one across each non-zero gap, in page space.
    pub fn gap_segments(&self) -> Vec<(Point, Point)> {
        let (a, b) = (&self.first, &self.second);
        let (x0, x1) = span(a.x0, a.x1, b.x0, b.x1);
        let (y0, y1) = span(a.y0, a.y1, b.y0, b.y1);
        let mut segments = Vec::new();
        if self.gap_x() > TOLERANCE {
            let y = f64::midpoint(y0, y1);
            segments.push((Point::new(x0, y), Point::new(x1, y)));
        }
        if self.gap_y() > TOLERANCE {
            let x = f64::midpoint(x0, x1);
            segments.push((Point::new(x, y0), Point::new(x, y1)));
        }
        segments
    }

    /// The space between the boxes: its kind and its length. Boxes apart in both directions
    /// have a diagonal gap, measured corner to corner.
    pub fn gap(&self) -> Gap {
        let (gx, gy) = (self.gap_x(), self.gap_y());
        let (a, b) = (&self.first, &self.second);
        let (kind, distance) = if gx > TOLERANCE && gy > TOLERANCE {
            (GapKind::Diagonal, gx.hypot(gy))
        } else if gy > TOLERANCE {
            (
                GapKind::Vertical {
                    a_above: a.y0 >= b.y1,
                },
                gy,
            )
        } else if gx > TOLERANCE {
            (GapKind::Horizontal { a_left: a.x1 <= b.x0 }, gx)
        } else if self.intersection().is_some() {
            (GapKind::Overlap, 0.0)
        } else {
            (GapKind::Touching, 0.0)
        };
        Gap { kind, distance }
    }

    /// How the horizontal extents of the boxes relate.
    pub fn horizontal(&self) -> Horizontal {
        let (a, b) = (&self.first, &self.second);
        let gap = self.gap_x();
        if gap > TOLERANCE {
            return Horizontal::Apart(gap);
        }
        let b_covers_a = b.x0 <= a.x0 + TOLERANCE && b.x1 >= a.x1 - TOLERANCE;
        let a_covers_b = a.x0 <= b.x0 + TOLERANCE && a.x1 >= b.x1 - TOLERANCE;
        match (b_covers_a, a_covers_b) {
            (true, true) => Horizontal::Same,
            (true, false) => Horizontal::BCoversA,
            (false, true) => Horizontal::ACoversB,
            (false, false) => Horizontal::Overlap(self.overlap_x().max(0.0)),
        }
    }
}

/// Whether two edges `delta` apart count as aligned.
pub fn is_aligned(delta: f64) -> bool {
    delta.abs() <= TOLERANCE
}

/// What lies between two boxes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GapKind {
    /// One above the other, with space between.
    Vertical {
        /// A is the upper one (greater y in page space).
        a_above: bool,
    },
    /// Side by side, with space between.
    Horizontal {
        /// A is the left one.
        a_left: bool,
    },
    /// Apart in both directions.
    Diagonal,
    /// They share an edge but no area.
    Touching,
    /// They share area.
    Overlap,
}

/// The gap between two boxes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    /// Which side faces which.
    pub kind: GapKind,
    /// Length in user units; 0 when the boxes touch or overlap.
    pub distance: f64,
}

impl Gap {
    /// One line saying which edges the gap runs between, e.g. `vertical gap, bottom of A → top
    /// of B`.
    pub const fn describe(&self) -> &'static str {
        match self.kind {
            GapKind::Vertical { a_above: true } => "vertical gap, bottom of A → top of B",
            GapKind::Vertical { a_above: false } => "vertical gap, top of A → bottom of B",
            GapKind::Horizontal { a_left: true } => "horizontal gap, right of A → left of B",
            GapKind::Horizontal { a_left: false } => "horizontal gap, left of A → right of B",
            GapKind::Diagonal => "diagonal gap, corner of A → corner of B",
            GapKind::Touching => "A and B touch, no gap",
            GapKind::Overlap => "A and B overlap, no gap",
        }
    }
}

/// How the horizontal extents of two boxes relate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Horizontal {
    /// Same left and right edges.
    Same,
    /// B spans all of A's width (and more).
    BCoversA,
    /// A spans all of B's width (and more).
    ACoversB,
    /// They share this much width, neither covers the other.
    Overlap(f64),
    /// They do not overlap; this far apart.
    Apart(f64),
}

/// The cursor over the canvas in both coordinate systems.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cursor {
    /// Page space: origin bottom-left, y up, before `/Rotate`.
    pub page: Point,
    /// Display space: origin top-left of the drawn page, y down, after `/Rotate`.
    pub display: Point,
}

impl Cursor {
    /// Status bar text, e.g. `x 100  y 50 pt · display 100, 50`. Pages with a `/UserUnit`
    /// other than 1 say so, because their lengths are not the coordinates in the file.
    pub fn text(&self, lengths: &Lengths) -> String {
        let unit = lengths.unit.label();
        let mut text = format!(
            "x {}  y {} {unit} · display {}, {}",
            lengths.number(self.page.x),
            lengths.number(self.page.y),
            lengths.number(self.display.x),
            lengths.number(self.display.y),
        );
        if (lengths.user_unit - 1.0).abs() >= f64::EPSILON {
            text = format!("{text} · UserUnit {}", pdfxray_core::num(lengths.user_unit));
        }
        text
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::units::Unit;

    const PT: Lengths = Lengths::new(Unit::Pt, 1.0);

    fn pair(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> Comparison {
        Comparison {
            first: Rect::new(a.0, a.1, a.2, a.3),
            second: Rect::new(b.0, b.1, b.2, b.3),
        }
    }

    #[test]
    fn offset_is_the_vector_between_the_points() {
        let offset = Offset::between(Point::new(10.0, 20.0), Point::new(40.0, -20.0));

        assert_eq!(
            offset,
            Offset {
                dx: 30.0,
                dy: -40.0,
                distance: 50.0
            }
        );
        assert_eq!(
            offset.rows(&PT),
            [
                ("Distance", "50 pt".to_owned()),
                ("Δx", "+30 pt".to_owned()),
                ("Δy", "-40 pt".to_owned()),
            ]
        );
    }

    #[test]
    fn clicks_build_lines_in_pairs() {
        let mut tool = MeasureTool {
            active: true,
            ..MeasureTool::default()
        };

        tool.click(0, Point::new(1.0, 1.0), false);
        assert_eq!(tool.offset(), None);
        tool.click(0, Point::new(4.0, 5.0), false);
        assert_eq!(tool.offset().map(|o| o.distance), Some(5.0));
        // The third click starts over.
        tool.click(0, Point::new(9.0, 9.0), false);
        assert_eq!(
            tool.line(),
            Some(Line {
                page: 0,
                from: Point::new(9.0, 9.0),
                to: None
            })
        );
        // A click on another page never finishes a line of the first page.
        tool.click(1, Point::new(2.0, 2.0), false);
        assert_eq!(tool.line().map(|l| (l.page, l.to)), Some((1, None)));
    }

    #[test]
    fn shift_snaps_the_end_to_the_nearest_45_degrees_keeping_its_reach_along_it() {
        let from = Point::new(10.0, 10.0);

        // About 8° up: horizontal, with the same Δx.
        assert_eq!(snap_angle(from, Point::new(70.0, 18.0)), Point::new(70.0, 10.0));
        // Nearly straight down: vertical, with the same Δy.
        assert_eq!(snap_angle(from, Point::new(12.0, -50.0)), Point::new(10.0, -50.0));
        // Nearly left, just below the axis: still left (the ±180° seam).
        assert_eq!(snap_angle(from, Point::new(-50.0, 8.0)), Point::new(-50.0, 10.0));
        // About 41°: the diagonal, projected: (30 + 26) / 2 = 28 along each axis.
        let diagonal = snap_angle(from, Point::new(40.0, 36.0));
        assert!(
            (diagonal.x - 38.0).abs() < 1e-9 && (diagonal.y - 38.0).abs() < 1e-9,
            "{diagonal:?}"
        );
        // Smoke: no line, nothing to snap.
        assert_eq!(snap_angle(from, from), from);
    }

    #[test]
    fn snapping_moves_the_end_of_a_line_never_its_anchor() {
        let mut tool = MeasureTool::default();
        tool.toggle();

        // The first click is the anchor, Shift or not.
        tool.click(0, Point::new(0.0, 0.0), true);
        assert_eq!(tool.line().map(|l| l.from), Some(Point::new(0.0, 0.0)));
        tool.click(0, Point::new(30.0, 4.0), true);
        assert_eq!(tool.offset().map(|o| (o.dx, o.dy)), Some((30.0, 0.0)));

        // A drag snaps while Shift is held and lets go when it is released.
        tool.drag_start(0, Point::new(5.0, 5.0));
        tool.drag_to(Point::new(6.0, 40.0), true);
        assert_eq!(tool.line().and_then(|l| l.to), Some(Point::new(5.0, 40.0)));
        tool.drag_to(Point::new(6.0, 40.0), false);
        assert_eq!(tool.line().and_then(|l| l.to), Some(Point::new(6.0, 40.0)));
    }

    #[test]
    fn dragging_follows_the_pointer_and_turning_off_clears() {
        let mut tool = MeasureTool::default();
        tool.toggle();

        tool.drag_start(2, Point::new(0.0, 0.0));
        tool.drag_to(Point::new(3.0, 4.0), false);

        assert_eq!(
            tool.offset().map(|o| (o.dx, o.dy, o.distance)),
            Some((3.0, 4.0, 5.0))
        );
        tool.toggle();
        assert!(!tool.active);
        assert_eq!(tool.line(), None);
    }

    #[test]
    fn boxes_side_by_side_have_a_horizontal_gap_only() {
        // a: 0..10 x 0..10, b: 15..25 x 2..12.
        let c = pair((0.0, 0.0, 10.0, 10.0), (15.0, 2.0, 25.0, 12.0));

        assert_eq!((c.gap_x(), c.gap_y(), c.intersection()), (5.0, 0.0, None));
        assert_eq!(
            c.gap(),
            Gap {
                kind: GapKind::Horizontal { a_left: true },
                distance: 5.0
            }
        );
        assert_eq!(c.gap().describe(), "horizontal gap, right of A → left of B");
        assert_eq!((c.left_delta(), c.right_delta()), (15.0, 15.0));
        assert_eq!(c.horizontal(), Horizontal::Apart(5.0));
        // The gap segment runs across the gap at the middle of the shared height (2..10).
        assert_eq!(c.gap_segments(), [(Point::new(10.0, 6.0), Point::new(15.0, 6.0))]);
        // Swapping the boxes swaps the side the description names.
        let swapped = pair((15.0, 2.0, 25.0, 12.0), (0.0, 0.0, 10.0, 10.0));
        assert_eq!(swapped.gap().describe(), "horizontal gap, left of A → right of B");
    }

    #[test]
    fn a_box_below_another_has_a_vertical_gap() {
        // a is the upper box (y 50..60), b the lower one (y 10..30): 20 apart.
        let c = pair((0.0, 50.0, 40.0, 60.0), (0.0, 10.0, 40.0, 30.0));

        assert_eq!(
            c.gap(),
            Gap {
                kind: GapKind::Vertical { a_above: true },
                distance: 20.0
            }
        );
        assert_eq!(c.gap().describe(), "vertical gap, bottom of A → top of B");
        assert_eq!(c.horizontal(), Horizontal::Same);
        assert!(is_aligned(c.left_delta()) && is_aligned(c.right_delta()));
        assert_eq!(
            pair((0.0, 10.0, 40.0, 30.0), (0.0, 50.0, 40.0, 60.0))
                .gap()
                .describe(),
            "vertical gap, top of A → bottom of B"
        );
    }

    #[test]
    fn overlapping_boxes_report_their_intersection_and_how_their_widths_relate() {
        // b sits inside a, sharing the left and bottom edges.
        let c = pair((0.0, 0.0, 100.0, 50.0), (0.0, 0.0, 40.0, 20.0));

        assert_eq!(c.intersection(), Some(Rect::new(0.0, 0.0, 40.0, 20.0)));
        assert_eq!(
            c.gap(),
            Gap {
                kind: GapKind::Overlap,
                distance: 0.0
            }
        );
        assert_eq!((c.left_delta(), c.right_delta()), (0.0, -60.0));
        assert_eq!(c.horizontal(), Horizontal::ACoversB);
        assert_eq!(c.gap_segments(), []);
        // The other way round, B covers A; a partial overlap names the shared width.
        assert_eq!(
            pair((0.0, 0.0, 40.0, 20.0), (0.0, 0.0, 100.0, 50.0)).horizontal(),
            Horizontal::BCoversA
        );
        assert_eq!(
            pair((0.0, 0.0, 40.0, 20.0), (30.0, 0.0, 100.0, 50.0)).horizontal(),
            Horizontal::Overlap(10.0)
        );
    }

    #[test]
    fn diagonal_boxes_have_both_gaps_and_touching_boxes_have_none() {
        let diagonal = pair((0.0, 0.0, 10.0, 10.0), (20.0, 30.0, 30.0, 40.0));
        let touching = pair((0.0, 0.0, 10.0, 10.0), (10.0, 0.0, 20.0, 10.0));

        assert_eq!((diagonal.gap_x(), diagonal.gap_y()), (10.0, 20.0));
        assert_eq!(diagonal.gap().kind, GapKind::Diagonal);
        assert!((diagonal.gap().distance - 500.0_f64.sqrt()).abs() < 1e-9);
        assert_eq!(
            diagonal.gap_segments(),
            [
                (Point::new(10.0, 20.0), Point::new(20.0, 20.0)),
                (Point::new(15.0, 10.0), Point::new(15.0, 30.0)),
            ]
        );
        assert_eq!(touching.gap().kind, GapKind::Touching);
        assert_eq!(touching.gap().describe(), "A and B touch, no gap");
        assert_eq!(touching.intersection(), None);
        assert_eq!(touching.gap_segments(), []);
    }

    #[test]
    fn the_cursor_shows_both_spaces_in_the_unit() {
        let cursor = Cursor {
            page: Point::new(72.0, 36.0),
            display: Point::new(72.0, 64.0),
        };

        assert_eq!(
            cursor.text(&Lengths::new(Unit::In, 1.0)),
            "x 1  y 0.5 in · display 1, 0.889"
        );
        // A /UserUnit of 2 doubles every length and is mentioned.
        assert_eq!(
            cursor.text(&Lengths::new(Unit::Pt, 2.0)),
            "x 144  y 72 pt · display 144, 128 · UserUnit 2"
        );
    }
}
