//! The page-space to pixel-space mapping of a rendered page.

use kurbo::Affine;
use pdfxray_core::{Point, Rect};

/// A position in the rendered bitmap, in pixels, origin at the top-left corner, y down.
///
/// Pixel `(i, j)` covers `i..i+1` x `j..j+1`, so its centre is `(i + 0.5, j + 0.5)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelPoint {
    /// Horizontal coordinate, grows to the right.
    pub x: f64,
    /// Vertical coordinate, grows downwards.
    pub y: f64,
}

impl PixelPoint {
    /// Creates a pixel position.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in pixel space, normalised so `x0 <= x1` and `y0 <= y1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelRect {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
}

impl PixelRect {
    /// The rectangle with top-left corner `min` and size `(width, height)`.
    pub const fn from_min_size(min: PixelPoint, (width, height): (f64, f64)) -> Self {
        Self {
            x0: min.x,
            y0: min.y,
            x1: min.x + width,
            y1: min.y + height,
        }
    }

    /// The rectangle of size `(width, height)` centred on `center`.
    pub fn centred(center: PixelPoint, (width, height): (f64, f64)) -> Self {
        Self::from_min_size(
            PixelPoint::new(center.x - width / 2.0, center.y - height / 2.0),
            (width, height),
        )
    }

    /// The square around a disc of `radius` at `center`.
    pub fn around(center: PixelPoint, radius: f64) -> Self {
        Self::centred(center, (radius * 2.0, radius * 2.0))
    }

    /// Width.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// Height.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// The centre.
    pub fn center(&self) -> PixelPoint {
        PixelPoint::new(f64::midpoint(self.x0, self.x1), f64::midpoint(self.y0, self.y1))
    }

    /// True when the two share a point; touching edges count.
    pub fn intersects(&self, other: &Self) -> bool {
        self.x0 <= other.x1 && other.x0 <= self.x1 && self.y0 <= other.y1 && other.y0 <= self.y1
    }

    /// The same rectangle moved by `(dx, dy)`.
    #[must_use]
    pub fn translate(&self, dx: f64, dy: f64) -> Self {
        Self {
            x0: self.x0 + dx,
            y0: self.y0 + dy,
            x1: self.x1 + dx,
            y1: self.y1 + dy,
        }
    }
}

/// An invertible 2x3 affine map from page space (PDF default user space: points, origin
/// bottom-left, y up, before `/Rotate`) to pixel space of a [`RenderedPage`](crate::RenderedPage).
///
/// It is built from exactly the matrix the rasteriser used, so it already includes the
/// `CropBox` offset, the y flip, `/Rotate` and the render scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    forward: Affine,
    inverse: Affine,
}

impl Transform {
    /// Wraps `forward`, or returns `None` when it cannot be inverted.
    pub(crate) fn new(forward: Affine) -> Option<Self> {
        let det = forward.determinant();
        (det.is_finite() && det != 0.0).then(|| Self {
            forward,
            inverse: forward.inverse(),
        })
    }

    /// This map followed by a scale of `(sx, sy)` and a move by `(dx, dy)`: page space onto a
    /// bitmap shown scaled and placed on a screen. `None` when the result cannot be inverted.
    pub fn onto(&self, (sx, sy): (f64, f64), (dx, dy): (f64, f64)) -> Option<Self> {
        Self::new(Affine::translate((dx, dy)) * Affine::scale_non_uniform(sx, sy) * self.forward)
    }

    /// The matrix coefficients `[a, b, c, d, e, f]`: `x' = a x + c y + e`, `y' = b x + d y + f`.
    pub fn coeffs(&self) -> [f64; 6] {
        self.forward.as_coeffs()
    }

    /// Maps a page-space point to pixels.
    pub fn to_pixel(&self, p: Point) -> PixelPoint {
        let q = self.forward * kurbo::Point::new(p.x, p.y);
        PixelPoint::new(q.x, q.y)
    }

    /// Maps a pixel position back to page space.
    pub fn to_page(&self, p: PixelPoint) -> Point {
        let q = self.inverse * kurbo::Point::new(p.x, p.y);
        Point::new(q.x, q.y)
    }

    /// Maps a page-space rectangle to its bounding rectangle in pixels. Rotations are
    /// multiples of 90 degrees, so the result is exact.
    pub fn rect_to_pixel(&self, r: Rect) -> PixelRect {
        let a = self.to_pixel(Point::new(r.x0, r.y0));
        let b = self.to_pixel(Point::new(r.x1, r.y1));
        PixelRect {
            x0: a.x.min(b.x),
            y0: a.y.min(b.y),
            x1: a.x.max(b.x),
            y1: a.y.max(b.y),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// Y flip of a 100 pt tall page at scale 2: `x' = 2x`, `y' = 200 - 2y`.
    fn flip() -> Transform {
        Transform::new(Affine::new([2.0, 0.0, 0.0, -2.0, 0.0, 200.0])).unwrap()
    }

    #[test]
    fn maps_points_both_ways() {
        let t = flip();

        assert_eq!(t.to_pixel(Point::new(10.0, 20.0)), PixelPoint::new(20.0, 160.0));
        assert_eq!(t.to_page(PixelPoint::new(20.0, 160.0)), Point::new(10.0, 20.0));
        // Smoke: the origin of page space is the bottom-left pixel corner.
        assert_eq!(t.to_pixel(Point::new(0.0, 0.0)), PixelPoint::new(0.0, 200.0));
    }

    #[test]
    fn rect_is_normalised_after_flip() {
        let t = flip();

        assert_eq!(
            t.rect_to_pixel(Rect::new(10.0, 20.0, 30.0, 40.0)),
            PixelRect {
                x0: 20.0,
                y0: 120.0,
                x1: 60.0,
                y1: 160.0
            }
        );
    }

    #[test]
    fn singular_matrix_is_rejected() {
        assert_eq!(Transform::new(Affine::scale(0.0)), None);
        assert_eq!(Transform::new(Affine::scale(f64::NAN)), None);
    }
}
