//! Box arithmetic shared by the capture collaborators, and the one conversion from `kurbo`
//! boxes (what the engine and path geometry produce) into core [`Rect`]s.

use crate::{Matrix, Point, Rect};

/// The smallest box holding both.
pub(super) fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// Intersection of two boxes; disjoint boxes give an empty box at the nearest corner.
pub(super) fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x0.max(b.x0);
    let y0 = a.y0.max(b.y0);
    Rect {
        x0,
        y0,
        x1: a.x1.min(b.x1).max(x0),
        y1: a.y1.min(b.y1).max(y0),
    }
}

/// `r` as a core box; `None` when a coordinate is not finite.
pub(super) fn from_kurbo(r: kurbo::Rect) -> Option<Rect> {
    [r.x0, r.y0, r.x1, r.y1]
        .iter()
        .all(|v| v.is_finite())
        .then(|| Rect::new(r.x0, r.y0, r.x1, r.y1))
}

/// Page-space bounding box of a Form `XObject`'s `/BBox` (as written: `[x0 y0 x1 y1]`, not
/// necessarily normalised) under `m`; `None` when it is not finite.
pub(super) fn transform_bbox(m: Matrix, [x0, y0, x1, y1]: [f64; 4]) -> Option<Rect> {
    let p00 = m.apply(Point::new(x0, y0));
    let p01 = m.apply(Point::new(x0, y1));
    let p10 = m.apply(Point::new(x1, y0));
    let p11 = m.apply(Point::new(x1, y1));
    let first = Rect::new(p00.x, p00.y, p01.x, p01.y);
    let second = Rect::new(p10.x, p10.y, p11.x, p11.y);
    let b = union(first, second);
    [b.x0, b.y0, b.x1, b.y1]
        .iter()
        .all(|v| v.is_finite())
        .then(|| Rect::new(b.x0, b.y0, b.x1, b.y1))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn union_and_intersect_follow_their_edges() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, -5.0, 20.0, 5.0);

        assert_eq!(union(a, b), Rect::new(0.0, -5.0, 20.0, 10.0));
        assert_eq!(intersect(a, b), Rect::new(5.0, 0.0, 10.0, 5.0));
        // smoke: disjoint boxes meet in an empty box at the nearest corner
        let far = Rect::new(30.0, 30.0, 40.0, 40.0);
        assert_eq!(intersect(a, far), Rect::new(30.0, 30.0, 30.0, 30.0));
    }

    #[test]
    fn a_form_box_lands_where_its_corners_land() {
        let m = Matrix::from([0.0, 1.0, -1.0, 0.0, 100.0, 0.0]);

        let quarter_turn = transform_bbox(m, [10.0, 20.0, 0.0, 0.0]);
        let infinite = transform_bbox(
            Matrix::from([f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0]),
            [0.0, 0.0, 1.0, 1.0],
        );

        assert_eq!(quarter_turn, Some(Rect::new(80.0, 0.0, 100.0, 10.0)));
        assert_eq!(infinite, None);
        // smoke: kurbo boxes convert only when finite
        assert_eq!(from_kurbo(kurbo::Rect::new(0.0, f64::NAN, 1.0, 1.0)), None);
    }
}
