//! Where the page is on screen: the mapping between screen positions, bitmap pixels and page
//! space for one page on the canvas.

use egui::{Pos2, Rect as UiRect, Vec2};
use pdfxray_core::{Point, Rect};
use pdfxray_render::{PixelPoint, Transform};

use crate::view::{local_to_pixel, pixel_to_local};

/// The displayed page image and the transform behind it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageGeometry {
    /// Where the page image is on screen.
    pub(crate) rect: UiRect,
    /// Bitmap size in pixels.
    pub(crate) bitmap: (u32, u32),
    /// Page space to bitmap pixels.
    pub(crate) to_pixel: Transform,
}

impl PageGeometry {
    /// Size of the image on screen, in points.
    pub(crate) fn display(&self) -> Vec2 {
        self.rect.size()
    }

    /// The page-space point under the screen position `pos`.
    pub(crate) fn screen_to_page(&self, pos: Pos2) -> Point {
        let pixel = local_to_pixel(pos - self.rect.min, self.display(), self.bitmap);
        self.to_pixel.to_page(pixel)
    }

    /// The screen position of a page-space point.
    pub(crate) fn page_to_screen(&self, point: Point) -> Pos2 {
        let pixel = self.to_pixel.to_pixel(point);
        self.rect.min + pixel_to_local(pixel, self.display(), self.bitmap)
    }

    /// The screen rectangle covering a page-space rectangle.
    pub(crate) fn rect_to_screen(&self, rect: Rect) -> UiRect {
        let px = self.to_pixel.rect_to_pixel(rect);
        let a = pixel_to_local(PixelPoint::new(px.x0, px.y0), self.display(), self.bitmap);
        let b = pixel_to_local(PixelPoint::new(px.x1, px.y1), self.display(), self.bitmap);
        UiRect::from_two_pos(self.rect.min + a, self.rect.min + b)
    }

    /// The point under `pos` in display space: origin at the top-left corner of the drawn page,
    /// y down, in page units (after `/Rotate`, before any unit conversion).
    pub(crate) fn screen_to_display(&self, pos: Pos2) -> Point {
        let pixel = local_to_pixel(pos - self.rect.min, self.display(), self.bitmap);
        let scale = self.pixels_per_unit();
        Point::new(pixel.x / scale, pixel.y / scale)
    }

    /// Bitmap pixels per page unit.
    pub(crate) fn pixels_per_unit(&self) -> f64 {
        let [a, b, ..] = self.to_pixel.coeffs();
        a.hypot(b)
    }
}

/// A geometry for `page` of a generated fixture, rendered at `scale` and shown 1:1 with its
/// top-left corner at screen position (100, 50).
#[cfg(test)]
pub(crate) fn fixture_geometry(name: &str, page: u32, scale: f32) -> PageGeometry {
    use pdfxray_core::numeric::f32_from_u32;
    use pdfxray_render::{RenderSpec, render_page};

    let doc = crate::test_support::fixture(name);
    let rendered = render_page(&doc, page, &RenderSpec::new(scale)).unwrap();
    let size = egui::vec2(f32_from_u32(rendered.width), f32_from_u32(rendered.height));
    PageGeometry {
        rect: UiRect::from_min_size(egui::pos2(100.0, 50.0), size),
        bitmap: (rendered.width, rendered.height),
        to_pixel: rendered.page_to_pixel,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use egui::pos2;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn screen_and_page_positions_convert_both_ways() {
        // minimal-one-page is 200 x 100 pt with the origin at the bottom-left.
        let geo = fixture_geometry("minimal-one-page", 0, 2.0);

        assert_eq!(geo.screen_to_page(pos2(100.0, 50.0)), Point::new(0.0, 100.0));
        assert_eq!(geo.screen_to_page(pos2(140.0, 130.0)), Point::new(20.0, 60.0));
        assert_eq!(geo.page_to_screen(Point::new(20.0, 60.0)), pos2(140.0, 130.0));
        assert_eq!(geo.rect_to_screen(Rect::new(0.0, 0.0, 200.0, 100.0)), geo.rect);
    }

    #[test]
    fn display_space_has_its_origin_top_left_in_page_units() {
        let geo = fixture_geometry("minimal-one-page", 0, 2.0);

        assert_eq!(geo.pixels_per_unit(), 2.0);
        assert_eq!(geo.screen_to_display(pos2(140.0, 130.0)), Point::new(20.0, 40.0));
    }

    #[test]
    fn a_page_turned_clockwise_has_its_page_origin_top_left() {
        // page-boxes page 1: /Rotate 90, CropBox [10 10 390 290].
        let geo = fixture_geometry("page-boxes", 0, 1.0);

        assert_eq!(geo.rect.size(), egui::vec2(280.0, 380.0));
        assert_eq!(geo.screen_to_page(geo.rect.min), Point::new(10.0, 10.0));
        assert_eq!(geo.screen_to_page(geo.rect.max), Point::new(390.0, 290.0));
        assert_eq!(geo.screen_to_display(pos2(103.0, 55.0)), Point::new(3.0, 5.0));
    }
}
