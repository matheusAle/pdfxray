//! The path builder: the current path in user space, as path-construction operators extend it,
//! and the clip a `W`/`W*` makes of it when the path ends.
//!
//! The path is held as a `kurbo` path so curve bounds are exact; only page-space boxes leave
//! this module.

use kurbo::{Affine, BezPath, PathEl, Point as KPoint, Shape};

use super::geom::from_kurbo;
use crate::{Matrix, Rect};

/// The current path and the pending clip.
#[derive(Default)]
pub(super) struct PathBuilder {
    path: BezPath,
    /// Current point, for `v`.
    last: KPoint,
    /// Start of the current subpath, where `h` returns.
    start: KPoint,
    /// A `W`/`W*` waits for the path-painting operator that ends the path.
    clip_pending: bool,
}

impl PathBuilder {
    /// Applies a path-construction operator (`m l c v y h re`) with numeric operands `v`.
    /// Operators without enough operands, or that need a current point and have none, are
    /// ignored, as the engine ignores them.
    pub(super) fn construct(&mut self, operator: &str, v: &[f64]) {
        let pt = |i: usize| {
            KPoint::new(
                v.get(i).copied().unwrap_or(0.0),
                v.get(i + 1).copied().unwrap_or(0.0),
            )
        };
        let started = !self.path.elements().is_empty();
        match operator {
            "m" if v.len() >= 2 => {
                self.path.move_to(pt(0));
                self.last = pt(0);
                self.start = pt(0);
            }
            "l" if v.len() >= 2 && started => {
                self.path.line_to(pt(0));
                self.last = pt(0);
            }
            "c" if v.len() >= 6 && started => {
                self.path.curve_to(pt(0), pt(2), pt(4));
                self.last = pt(4);
            }
            "v" if v.len() >= 4 && started => {
                self.path.curve_to(self.last, pt(0), pt(2));
                self.last = pt(2);
            }
            "y" if v.len() >= 4 && started => {
                self.path.curve_to(pt(0), pt(2), pt(2));
                self.last = pt(2);
            }
            "h" => self.close_subpath(),
            "re" if v.len() >= 4 => {
                let (left, bottom, width, height) = (v[0], v[1], v[2], v[3]);
                self.path.move_to((left, bottom));
                self.path.line_to((left + width, bottom));
                self.path.line_to((left + width, bottom + height));
                self.path.line_to((left, bottom + height));
                self.path.close_path();
                self.last = KPoint::new(left, bottom);
                self.start = KPoint::new(left, bottom);
            }
            _ => {}
        }
    }

    /// Closes the current subpath unless it is empty or already closed (`h`, and the closing
    /// half of `s`, `b`, `b*`).
    pub(super) fn close_subpath(&mut self) {
        let elements = self.path.elements();
        if !elements.is_empty() && elements.last() != Some(&PathEl::ClosePath) {
            self.path.close_path();
            self.last = self.start;
        }
    }

    /// `W` / `W*`: the path clips once it ends.
    pub(super) fn clip(&mut self) {
        self.clip_pending = true;
    }

    /// Ends the path (a path-painting operator or `n`). Returns the page-space box of the clip
    /// it makes under `ctm`, when a `W`/`W*` was pending, the path is not empty and its box is
    /// finite.
    pub(super) fn end(&mut self, ctm: Matrix) -> Option<Rect> {
        let clips = std::mem::take(&mut self.clip_pending) && !self.path.elements().is_empty();
        let clip = if clips {
            // The path is discarded below, so it is moved to page space in place.
            self.path.apply_affine(Affine::new(ctm.to_array()));
            from_kurbo(self.path.bounding_box())
        } else {
            None
        };
        self.path.truncate(0);
        clip
    }

    /// Starts a Form `XObject`'s stream with no path and no pending clip.
    pub(super) fn reset(&mut self) {
        self.path.truncate(0);
        self.clip_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_clipping_rectangle_ends_as_its_page_space_box() {
        let mut path = PathBuilder::default();
        path.construct("re", &[10.0, 20.0, 30.0, 40.0]);
        path.clip();

        let clip = path.end(Matrix::from([2.0, 0.0, 0.0, 2.0, 5.0, 0.0]));

        assert_eq!(clip, Some(Rect::new(25.0, 40.0, 85.0, 120.0)));
        // smoke: the path is gone and the clip was consumed
        assert_eq!(path.end(Matrix::IDENTITY), None);
    }

    #[test]
    fn a_curve_clip_is_bounded_by_its_extrema_and_lines_need_a_current_point() {
        let mut path = PathBuilder::default();
        path.construct("l", &[50.0, 50.0]);
        path.construct("m", &[0.0, 0.0]);
        path.construct("c", &[0.0, 10.0, 10.0, 10.0, 10.0, 0.0]);
        path.construct("h", &[]);
        path.clip();

        let clip = path.end(Matrix::IDENTITY);

        assert_eq!(clip, Some(Rect::new(0.0, 0.0, 10.0, 7.5)));
        // smoke: without `W` a path makes no clip
        path.construct("re", &[0.0, 0.0, 1.0, 1.0]);
        assert_eq!(path.end(Matrix::IDENTITY), None);
    }

    #[test]
    fn reset_drops_the_path_and_a_pending_clip() {
        let mut path = PathBuilder::default();
        path.construct("re", &[0.0, 0.0, 5.0, 5.0]);
        path.clip();

        path.reset();
        path.construct("m", &[1.0, 1.0]);
        path.construct("l", &[2.0, 3.0]);

        assert_eq!(
            path.end(Matrix::IDENTITY),
            None,
            "the clip did not survive the reset"
        );
        // smoke: a new clip after the reset works
        path.construct("re", &[1.0, 1.0, 1.0, 1.0]);
        path.clip();
        assert_eq!(path.end(Matrix::IDENTITY), Some(Rect::new(1.0, 1.0, 2.0, 2.0)));
    }
}
