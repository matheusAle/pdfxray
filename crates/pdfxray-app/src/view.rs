//! Pure canvas math: zoom steps, fit-to-window, render scale and the mapping between
//! positions on the displayed page image and bitmap pixels.

use egui::Vec2;
use pdfxray_core::numeric::{f32_from_f64, f32_from_u32, u32_from_f32_trunc};
use pdfxray_render::PixelPoint;

/// Smallest zoom, 10 %.
pub const MIN_ZOOM: f32 = 0.1;

/// Largest zoom, 800 %.
pub const MAX_ZOOM: f32 = 8.0;

/// Factor of one zoom-in or zoom-out button press.
pub const ZOOM_STEP: f32 = 1.25;

/// Rendering resolution is rounded to multiples of `1 / SCALE_STEPS` so that continuous
/// zooming reuses cached bitmaps instead of rasterising at every new zoom value.
const SCALE_STEPS: f32 = 16.0;

/// How the canvas picks its zoom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZoomMode {
    /// The whole page always fits the canvas.
    #[default]
    Fit,
    /// The user chose the zoom.
    Manual,
}

/// `zoom` limited to [`MIN_ZOOM`]..=[`MAX_ZOOM`].
pub fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_nan() {
        return 1.0;
    }
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

/// One button step in.
pub fn zoom_in(zoom: f32) -> f32 {
    clamp_zoom(zoom * ZOOM_STEP)
}

/// One button step out.
pub fn zoom_out(zoom: f32) -> f32 {
    clamp_zoom(zoom / ZOOM_STEP)
}

/// The zoom at which a page of `page` points fits entirely into `available` points.
///
/// A degenerate page or no room at all gives 100 % and [`MIN_ZOOM`] respectively.
pub fn fit_zoom(page: Vec2, available: Vec2) -> f32 {
    if page.x <= 0.0 || page.y <= 0.0 {
        return 1.0;
    }
    clamp_zoom((available.x / page.x).min(available.y / page.y))
}

/// Cache key and render scale (bitmap pixels per page point) for showing a `page`-point page
/// at `zoom` on a display with `pixels_per_point`.
///
/// The scale is rounded to 1/16 and lowered when the bitmap would otherwise be larger than
/// `max_side` pixels; the image is then upscaled by the GPU.
pub fn render_scale(zoom: f32, pixels_per_point: f32, page: Vec2, max_side: u32) -> ScaleKey {
    let wanted = zoom * pixels_per_point;
    let longest = page.x.max(page.y).max(1.0);
    let steps = u32_from_f32_trunc((wanted * SCALE_STEPS).round().max(1.0));
    let cap = u32_from_f32_trunc(
        ((f32_from_u32(max_side) / longest) * SCALE_STEPS)
            .floor()
            .max(1.0),
    );
    ScaleKey(steps.min(cap))
}

/// A render scale in sixteenths of a pixel per point; the cache key of a bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScaleKey(u32);

impl ScaleKey {
    /// Pixels per point.
    pub fn scale(self) -> f32 {
        // At most a few thousand, exactly representable.
        f32_from_u32(self.0) / SCALE_STEPS
    }
}

/// Position `local` (points from the top-left corner of the displayed image of size
/// `display` points) as a continuous position in a `bitmap` of `(width, height)` pixels.
pub fn local_to_pixel(local: Vec2, display: Vec2, bitmap: (u32, u32)) -> PixelPoint {
    // Bitmap sides are at most `MAX_SIDE` = 16384, exactly representable.
    let (w, h) = (f32_from_u32(bitmap.0), f32_from_u32(bitmap.1));
    PixelPoint::new(
        f64::from(local.x / display.x * w),
        f64::from(local.y / display.y * h),
    )
}

/// Inverse of [`local_to_pixel`].
pub fn pixel_to_local(pixel: PixelPoint, display: Vec2, bitmap: (u32, u32)) -> Vec2 {
    let (w, h) = (f32_from_u32(bitmap.0), f32_from_u32(bitmap.1));
    // Display coordinates are small; f32 is what the painter uses anyway.
    let (x, y) = (f32_from_f64(pixel.x), f32_from_f64(pixel.y));
    Vec2::new(x / w * display.x, y / h * display.y)
}

#[cfg(test)]
mod tests {
    use egui::vec2;
    use pretty_assertions::assert_eq;

    use super::*;

    /// Zooms and scales are continuous values: equal within a millionth.
    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-6, "{actual} is not {expected}");
    }

    #[test]
    fn zoom_steps_are_clamped() {
        assert_close(zoom_in(1.0), 1.25);
        assert_close(zoom_out(1.25), 1.0);
        assert_close(zoom_in(MAX_ZOOM), MAX_ZOOM);
        assert_close(zoom_out(MIN_ZOOM), MIN_ZOOM);
        assert_close(clamp_zoom(f32::NAN), 1.0);
        // Smoke: in then out is the identity away from the limits.
        assert_close(zoom_out(zoom_in(2.0)), 2.0);
    }

    #[test]
    fn fit_uses_the_tighter_axis() {
        // 200 x 100 page in a 400 x 400 area: width limits, 200 %.
        assert_close(fit_zoom(vec2(200.0, 100.0), vec2(400.0, 400.0)), 2.0);
        // Same page in 1000 x 120: height limits, 120 %.
        assert_close(fit_zoom(vec2(200.0, 100.0), vec2(1000.0, 120.0)), 1.2);
        // Never beyond the limits.
        assert_close(fit_zoom(vec2(10.0, 10.0), vec2(1000.0, 1000.0)), MAX_ZOOM);
        assert_close(fit_zoom(vec2(1000.0, 1000.0), vec2(0.0, 0.0)), MIN_ZOOM);
        assert_close(fit_zoom(vec2(0.0, 100.0), vec2(300.0, 300.0)), 1.0);
    }

    #[test]
    fn render_scale_rounds_to_sixteenths_and_follows_the_display_density() {
        let page = vec2(200.0, 100.0);

        // The key counts sixteenths, so the rounding is checked on integers.
        assert_eq!(render_scale(1.0, 1.0, page, 8192), ScaleKey(16));
        assert_eq!(render_scale(1.0, 2.0, page, 8192), ScaleKey(32));
        // 1.51 rounds to 24/16 = 1.5; 1.56 rounds to 25/16 = 1.5625.
        assert_eq!(render_scale(1.51, 1.0, page, 8192), ScaleKey(24));
        assert_eq!(render_scale(1.56, 1.0, page, 8192), ScaleKey(25));
        assert_close(ScaleKey(25).scale(), 1.5625);
        // Nearby zoom values share one key, so one bitmap.
        assert_eq!(
            render_scale(1.51, 1.0, page, 8192),
            render_scale(1.52, 1.0, page, 8192)
        );
        assert_ne!(
            render_scale(1.0, 1.0, page, 8192),
            render_scale(1.25, 1.0, page, 8192)
        );
    }

    #[test]
    fn render_scale_respects_the_texture_limit() {
        // 800 pt side at zoom 8 and 2 ppp would be 12800 px; the limit is 2048 (2.56 -> 2.5).
        let key = render_scale(8.0, 2.0, vec2(800.0, 600.0), 2048);

        assert_eq!(key, ScaleKey(40));
        assert_close(key.scale(), 2.5);
        assert!(800.0 * key.scale() <= 2048.0);
    }

    #[test]
    fn local_and_pixel_positions_convert_both_ways() {
        let display = vec2(400.0, 200.0);
        let bitmap = (800, 400);

        let pixel = local_to_pixel(vec2(100.0, 50.0), display, bitmap);

        assert_eq!(pixel, PixelPoint::new(200.0, 100.0));
        assert_eq!(pixel_to_local(pixel, display, bitmap), vec2(100.0, 50.0));
        // Smoke: the display's far corner is the bitmap's far corner.
        assert_eq!(
            local_to_pixel(display, display, bitmap),
            PixelPoint::new(800.0, 400.0)
        );
    }
}
