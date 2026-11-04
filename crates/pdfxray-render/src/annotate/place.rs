//! Where labels go: chips moved down clear of each other, badges beside their boxes, notes beside
//! their segments and inside the bounds.

use super::{BADGE_GAP, BADGE_RADIUS, Grid};
use crate::transform::{PixelPoint, PixelRect};

/// How many times a chip is moved down to clear the chips already there.
const MAX_NUDGES: usize = 8;

/// How far a note sits off its segment beyond what it needs to leave it clear, twice over.
const BESIDE: f32 = 11.0;

/// `rect` moved down a step at a time (its height and `gap`) until it covers none of `placed`, at
/// most eight times: the one rule by which every label on the canvas clears the others.
pub fn clear_of(placed: &[PixelRect], rect: PixelRect, gap: f64) -> PixelRect {
    let mut at = rect;
    for _ in 0..MAX_NUDGES {
        if !placed.iter().any(|other| other.intersects(&at)) {
            break;
        }
        at = at.translate(0.0, rect.height() + gap);
    }
    at
}

/// Where the badge of a box at `rect` goes: level with the middle of the box, just outside its
/// right edge, so it does not cover the last glyph. Where that is taken by one of `placed` (a chip
/// of the box beside it, another badge) it goes above the right end of the box, else below it;
/// with none free it stays beside the box.
pub(super) fn badge_spot(grid: &Grid, rect: PixelRect, placed: &[PixelRect]) -> PixelPoint {
    let (r, gap) = (grid.ud(BADGE_RADIUS), grid.ud(BADGE_GAP));
    let beside = PixelPoint::new(rect.x1 + r + gap, rect.center().y);
    let above = PixelPoint::new(rect.x1 - r, rect.y0 - r - gap);
    let below = PixelPoint::new(rect.x1 - r, rect.y1 + r + gap);
    [beside, above, below]
        .into_iter()
        .find(|at| {
            let disc = PixelRect::around(*at, r);
            !placed.iter().any(|other| other.intersects(&disc))
        })
        .unwrap_or(beside)
}

/// The middle of the line from `from` to `to`, moved to one side of it by as much as a box of
/// `size` needs to leave the line clear: a wide label by a steep line moves sideways by half its
/// width.
pub(super) fn clear_beside(grid: &Grid, from: PixelPoint, to: PixelPoint, size: (f64, f64)) -> PixelPoint {
    let mid = PixelPoint::new(f64::midpoint(from.x, to.x), f64::midpoint(from.y, to.y));
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = dx.hypot(dy);
    if !(length.is_finite() && length > 0.0) {
        return mid;
    }
    let normal = (-dy / length, dx / length);
    let reach = normal.0.abs() * size.0 / 2.0 + normal.1.abs() * size.1 / 2.0;
    let by = reach + grid.ud(BESIDE) / 2.0;
    PixelPoint::new(mid.x + normal.0 * by, mid.y + normal.1 * by)
}

/// `rect` moved the least it takes to lie inside `bounds` (as far as it fits).
pub(super) fn inside(bounds: PixelRect, rect: PixelRect) -> PixelRect {
    let dx = (bounds.x0 - rect.x0).max(0.0) + (bounds.x1 - rect.x1).min(0.0);
    let dy = (bounds.y0 - rect.y0).max(0.0) + (bounds.y1 - rect.y1).min(0.0);
    rect.translate(dx, dy)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use kurbo::Affine;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::transform::Transform;

    fn grid() -> Grid {
        Grid {
            to_out: Transform::new(Affine::IDENTITY).unwrap(),
            unit: 1.0,
            pixel: 1.0,
            bounds: PixelRect {
                x0: 0.0,
                y0: 0.0,
                x1: 600.0,
                y1: 400.0,
            },
        }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PixelRect {
        PixelRect { x0, y0, x1, y1 }
    }

    #[test]
    fn a_badge_sits_beside_its_box_unless_a_chip_is_there_and_then_goes_above_or_below() {
        let g = grid();
        let line = rect(100.0, 100.0, 200.0, 112.0);
        let r = f64::from(BADGE_RADIUS);
        let beside = PixelPoint::new(200.0 + r + f64::from(BADGE_GAP), 106.0);

        assert_eq!(badge_spot(&g, line, &[]), beside);

        // The chip of the box to the right takes the place beside the line.
        let chip = rect(205.0, 100.0, 221.0, 114.0);
        let above = badge_spot(&g, line, &[chip]);
        assert_eq!(above.y, 100.0 - r - f64::from(BADGE_GAP));
        assert!(!PixelRect::around(above, r).intersects(&chip));

        // With the room above taken as well, it goes below.
        let taken = [chip, PixelRect::around(above, r)];
        let below = badge_spot(&g, line, &taken);
        assert_eq!(below.y, 112.0 + r + f64::from(BADGE_GAP));

        // With none free it stays beside the box, where the badge always was.
        let all = [chip, PixelRect::around(above, r), PixelRect::around(below, r)];
        assert_eq!(badge_spot(&g, line, &all), beside);
    }

    #[test]
    fn a_chip_moves_down_clear_of_the_ones_there() {
        let at = rect(0.0, 0.0, 20.0, 10.0);

        assert_eq!(clear_of(&[], at, 1.0), at);
        assert_eq!(clear_of(&[at], at, 1.0), rect(0.0, 11.0, 20.0, 21.0));
        // Smoke: at most eight nudges, however crowded.
        let column: Vec<PixelRect> = (0..20).map(|n| at.translate(0.0, f64::from(n) * 11.0)).collect();
        assert_eq!(clear_of(&column, at, 1.0), at.translate(0.0, 88.0));
    }

    #[test]
    fn a_label_moves_off_a_steep_line_by_half_its_width_and_stays_inside_the_bounds() {
        let g = grid();
        let at = clear_beside(
            &g,
            PixelPoint::new(300.0, 100.0),
            PixelPoint::new(300.0, 300.0),
            (100.0, 20.0),
        );

        // Normal of a line going down is to the left: half the width plus half of BESIDE.
        assert_eq!(at, PixelPoint::new(300.0 - 50.0 - 5.5, 200.0));
        assert_eq!(
            inside(g.bounds, rect(-10.0, 390.0, 40.0, 410.0)),
            rect(0.0, 380.0, 50.0, 400.0)
        );
        // Smoke: a point line leaves the label on it.
        let p = PixelPoint::new(5.0, 5.0);
        assert_eq!(clear_beside(&g, p, p, (10.0, 10.0)), p);
    }
}
