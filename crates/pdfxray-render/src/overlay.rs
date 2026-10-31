//! Overlay shapes painted on top of the rasterised page.

use pdfxray_core::Rect;
use pdfxray_core::numeric::{u8_from_f64_round, u32_from_f64_trunc, usize_from_u32};

use crate::transform::{PixelRect, Transform};

/// A shape drawn in page space over the page bitmap.
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    /// An axis-aligned rectangle.
    Rect {
        /// Rectangle in page space.
        rect: Rect,
        /// Outline colour, straight RGBA.
        stroke: [u8; 4],
        /// Interior colour, straight RGBA; `None` leaves the interior untouched.
        fill: Option<[u8; 4]>,
        /// Outline width in output pixels, centred on the rectangle edge. Zero draws no outline.
        width: f32,
    },
}

/// The bitmap overlays are painted into.
pub(crate) struct Canvas<'a> {
    pub(crate) rgba: &'a mut [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Overlay {
    /// Paints the shape into `canvas`, mapping page space with `to_pixel`.
    pub(crate) fn paint(&self, canvas: &mut Canvas<'_>, to_pixel: &Transform) {
        match self {
            Self::Rect {
                rect,
                stroke,
                fill,
                width,
            } => {
                let r = to_pixel.rect_to_pixel(*rect);
                if let Some(fill) = fill {
                    canvas.paint_rect(r, *fill);
                }
                let half = f64::from(width.max(0.0)) / 2.0;
                if half > 0.0 {
                    canvas.paint_ring(r, half, *stroke);
                }
            }
        }
    }
}

impl Canvas<'_> {
    /// Paints the band `r` grown by `half` minus `r` shrunk by `half`.
    fn paint_ring(&mut self, r: PixelRect, half: f64, color: [u8; 4]) {
        let outer = grow(r, half);
        let inner = grow(r, -half);
        for (x, y) in self.pixels_touching(outer) {
            self.blend(x, y, color, coverage(outer, x, y) - coverage(inner, x, y));
        }
    }

    fn paint_rect(&mut self, r: PixelRect, color: [u8; 4]) {
        for (x, y) in self.pixels_touching(r) {
            self.blend(x, y, color, coverage(r, x, y));
        }
    }

    /// Every pixel inside the bitmap that `r` touches.
    fn pixels_touching(&self, r: PixelRect) -> impl Iterator<Item = (u32, u32)> + use<> {
        let (x_lo, x_hi) = span(r.x0, r.x1, self.width);
        let (y_lo, y_hi) = span(r.y0, r.y1, self.height);
        (y_lo..y_hi).flat_map(move |y| (x_lo..x_hi).map(move |x| (x, y)))
    }

    /// Source-over blend of `color` (straight RGBA) at `cover` onto pixel `(x, y)`.
    fn blend(&mut self, x: u32, y: u32, color: [u8; 4], cover: f64) {
        let a = f64::from(color[3]) / 255.0 * cover.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let at = (usize_from_u32(y) * usize_from_u32(self.width) + usize_from_u32(x)) * 4;
        let Some(dst) = self.rgba.get_mut(at..at + 4) else {
            return;
        };
        let dst_a = f64::from(dst[3]) / 255.0;
        let out_a = a + dst_a * (1.0 - a);
        for (d, s) in dst.iter_mut().zip(color).take(3) {
            *d = u8_from_f64_round((f64::from(s) * a + f64::from(*d) * dst_a * (1.0 - a)) / out_a);
        }
        dst[3] = u8_from_f64_round(out_a * 255.0);
    }
}

/// `r` moved outwards by `by` on every side; a negative `by` can invert it.
fn grow(r: PixelRect, by: f64) -> PixelRect {
    PixelRect {
        x0: r.x0 - by,
        y0: r.y0 - by,
        x1: r.x1 + by,
        y1: r.y1 + by,
    }
}

/// Pixel index range `[lo, hi)` touched by the interval `[a, b]`, clamped to `0..=limit`.
fn span(a: f64, b: f64, limit: u32) -> (u32, u32) {
    let bound = |v: f64| u32_from_f64_trunc(v.clamp(0.0, f64::from(limit)));
    (bound(a.floor()), bound(b.ceil()))
}

/// Fraction of pixel `(x, y)` covered by `r`; inverted rectangles cover nothing.
fn coverage(r: PixelRect, x: u32, y: u32) -> f64 {
    if r.x1 <= r.x0 || r.y1 <= r.y0 {
        return 0.0;
    }
    let overlap = |lo: f64, hi: f64, cell: u32| {
        let cell = f64::from(cell);
        (hi.min(cell + 1.0) - lo.max(cell)).clamp(0.0, 1.0)
    };
    overlap(r.x0, r.x1, x) * overlap(r.y0, r.y1, y)
}

#[cfg(test)]
mod tests {
    use kurbo::Affine;
    use pretty_assertions::assert_eq;

    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    fn white(w: u32, h: u32) -> Vec<u8> {
        WHITE.repeat(usize_from_u32(w * h))
    }

    fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let at = usize_from_u32((y * w + x) * 4);
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    }

    fn paint(img: &mut [u8], w: u32, h: u32, overlay: &Overlay) {
        let identity = Transform::new(Affine::IDENTITY).unwrap();
        overlay.paint(
            &mut Canvas {
                rgba: img,
                width: w,
                height: h,
            },
            &identity,
        );
    }

    #[test]
    fn fill_covers_exactly_the_rectangle() {
        let mut img = white(6, 6);

        paint(
            &mut img,
            6,
            6,
            &Overlay::Rect {
                rect: Rect::new(1.0, 2.0, 3.0, 4.0),
                stroke: RED,
                fill: Some(RED),
                width: 0.0,
            },
        );

        let painted: Vec<(u32, u32)> = (0..6)
            .flat_map(|y| (0..6).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&img, 6, x, y) == RED)
            .collect();
        assert_eq!(painted, vec![(1, 2), (2, 2), (1, 3), (2, 3)]);
        // Smoke: everything else is still white.
        assert_eq!(pixel(&img, 6, 0, 0), WHITE);
    }

    #[test]
    fn stroke_is_centred_on_the_edge_and_leaves_the_interior() {
        let mut img = white(10, 10);

        paint(
            &mut img,
            10,
            10,
            &Overlay::Rect {
                rect: Rect::new(2.0, 2.0, 8.0, 8.0),
                stroke: RED,
                fill: None,
                width: 2.0,
            },
        );

        assert_eq!(
            [
                pixel(&img, 10, 1, 5),
                pixel(&img, 10, 2, 5),
                pixel(&img, 10, 5, 8),
                pixel(&img, 10, 5, 5),
                pixel(&img, 10, 0, 5),
            ],
            [RED, RED, RED, WHITE, WHITE]
        );
    }

    #[test]
    fn half_covered_pixel_blends_half() {
        let mut img = white(2, 1);

        paint(
            &mut img,
            2,
            1,
            &Overlay::Rect {
                rect: Rect::new(0.0, 0.0, 0.5, 1.0),
                stroke: RED,
                fill: Some([0, 0, 0, 255]),
                width: 0.0,
            },
        );

        assert_eq!(pixel(&img, 2, 0, 0), [128, 128, 128, 255]);
        assert_eq!(pixel(&img, 2, 1, 0), WHITE);
    }

    #[test]
    fn rectangles_outside_the_bitmap_are_ignored() {
        let mut img = white(3, 3);

        paint(
            &mut img,
            3,
            3,
            &Overlay::Rect {
                rect: Rect::new(-50.0, -50.0, -10.0, -10.0),
                stroke: RED,
                fill: Some(RED),
                width: 3.0,
            },
        );

        assert_eq!(img, white(3, 3));
    }
}
