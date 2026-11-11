//! Viewer and display emulation: a page drawn on the device pixels of a given screen, rounded
//! the way a given Windows viewer rounds what lands between them.
//!
//! An [`Emulation`] is a [`Display`] (its size in device pixels and its DPI), the zoom the viewer
//! shows the page at ([`ViewerZoom`]), and optionally a [`Viewer`]. The display and zoom set the
//! scale, `zoom × DPI / 72` device pixels per point: what the viewer draws, not what pdfxray would
//! draw on this Mac. The viewer sets the rules that place geometry on that pixel grid, following
//! `docs/pdf-rendering-rounding-windows.md`:
//!
//! * **Chrome and Edge** draw with `PDFium`. Positions stay floats, with no snapping, overlap or
//!   stroke adjustment. A rectangular clip grows out to whole pixels (`GetOuterRect`). Each
//!   glyph's origin is rounded to the nearest pixel. An axis-aligned image covers a whole number
//!   of pixels, `ceil` of its size, starting at the floor or ceiling of its edge, whichever moves
//!   it less (`MatchFloatRange`).
//! * **Acrobat** draws with Adobe's AGM. A coordinate within 0.0001 px of a whole pixel snaps to
//!   it (`SnapToPureTolerance`). Fills reach 0.01 px past their edges (`PixelOverlap`). Strokes are
//!   adjusted to a whole number of pixels, at least one, centred so both edges land on pixel
//!   boundaries. A text run starts on the pixel grid and each glyph advance is rounded, unless
//!   it would round to zero.
//!
//! What is not emulated, because the document gives no values for it or it is the viewer's own
//! code: Acrobat's discrete resolution table and its feature thresholds, integer rasteriser
//! precision, font hinting, anti-aliasing (every viewer here is drawn with pdfxray's), GPU or GDI
//! compositing, and `PDFium`'s legacy truncating rectangles.

use std::fmt;

use hayro::PixelRules;
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape, Vec2};

/// A Windows PDF viewer whose pixel rounding can be emulated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Viewer {
    /// Google Chrome's built-in viewer.
    Chrome,
    /// Microsoft Edge's built-in viewer, Chromium's.
    Edge,
    /// Adobe Acrobat and Acrobat Reader.
    Acrobat,
}

/// The engine behind a [`Viewer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    /// Chromium's `PDFium`.
    Pdfium,
    /// Adobe's AGM and `CoolType`.
    Adobe,
}

impl Viewer {
    /// Every viewer, in the order a picker lists them.
    pub const ALL: [Self; 3] = [Self::Chrome, Self::Edge, Self::Acrobat];

    /// The viewer's name, as people call it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Acrobat => "Acrobat",
        }
    }

    /// The engine it draws pages with.
    pub const fn engine(self) -> Engine {
        match self {
            Self::Chrome | Self::Edge => Engine::Pdfium,
            Self::Acrobat => Engine::Adobe,
        }
    }

    /// The rules it places geometry on device pixels with.
    pub(crate) fn rules(self) -> &'static dyn PixelRules {
        match self.engine() {
            Engine::Pdfium => &PdfiumRules,
            Engine::Adobe => &AdobeRules,
        }
    }
}

impl Engine {
    /// The engine's name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pdfium => "PDFium",
            Self::Adobe => "Adobe AGM",
        }
    }
}

/// The density of a Windows display at 100 % scaling, in dots per inch.
pub const BASE_DPI: u32 = 96;

/// Points per inch: PDF user space.
const POINTS_PER_INCH: f64 = 72.0;

/// A screen: its size in device pixels and its density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Display {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// Dots per inch: 96 at Windows' 100 % scaling, 120 at 125 %, 144 at 150 %.
    pub dpi: u32,
}

impl Default for Display {
    /// A 1920 × 1080 screen at 100 %.
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            dpi: BASE_DPI,
        }
    }
}

impl Display {
    /// Windows' display scaling: the DPI over 96, 1.25 at 120 DPI.
    pub fn scale_factor(self) -> f64 {
        f64::from(self.dpi) / f64::from(BASE_DPI)
    }

    /// Device pixels per point at `zoom` (1 is 100 %).
    pub fn pixels_per_point(self, zoom: f64) -> f64 {
        zoom * f64::from(self.dpi) / POINTS_PER_INCH
    }
}

/// How the emulated viewer zooms the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerZoom {
    /// A zoom percentage: 100 shows a point as `DPI / 72` device pixels.
    Percent(u32),
    /// The page's width fills the screen's.
    FitWidth,
    /// The whole page fits the screen.
    FitPage,
}

impl Default for ViewerZoom {
    fn default() -> Self {
        Self::Percent(100)
    }
}

impl fmt::Display for ViewerZoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Percent(p) => write!(f, "{p}%"),
            Self::FitWidth => f.write_str("fit width"),
            Self::FitPage => f.write_str("fit page"),
        }
    }
}

/// A display, the zoom a viewer shows the page at on it, and the viewer whose rounding is
/// emulated (`None`: pdfxray's own, on that display's pixels).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Emulation {
    /// The viewer, if any.
    pub viewer: Option<Viewer>,
    /// The screen.
    pub display: Display,
    /// The viewer's zoom.
    pub zoom: ViewerZoom,
}

impl Emulation {
    /// The viewer's zoom for a page of `page` points (width, height as drawn), 1 being 100 %.
    ///
    /// The fit zooms take the whole screen as the viewer's page area: viewers' own toolbars,
    /// scroll bars and margins are not modelled. Enter the size of the area the page gets to
    /// account for them.
    pub fn zoom_for(&self, (width, height): (f32, f32)) -> f64 {
        let at_100 = self.display.pixels_per_point(1.0);
        let across = f64::from(self.display.width) / (f64::from(width) * at_100);
        let down = f64::from(self.display.height) / (f64::from(height) * at_100);
        match self.zoom {
            ViewerZoom::Percent(p) => f64::from(p) / 100.0,
            ViewerZoom::FitWidth => across,
            ViewerZoom::FitPage => across.min(down),
        }
    }

    /// Device pixels per point for a page of `page` points: the scale to draw it at.
    pub fn scale_for(&self, page: (f32, f32)) -> f32 {
        // A few device pixels per point at most: well within f32.
        #[expect(clippy::cast_possible_truncation, reason = "a small positive scale")]
        let scale = self.display.pixels_per_point(self.zoom_for(page)) as f32;
        scale
    }
}

/// The tolerance under which Acrobat snaps a coordinate to a whole pixel
/// (`SnapToPureTolerance`).
pub const ADOBE_SNAP_TOLERANCE: f64 = 0.0001;

/// How far Acrobat lets a fill reach past its edge, in device pixels (`PixelOverlap`).
pub const ADOBE_PIXEL_OVERLAP: f64 = 0.01;

/// `PDFium`'s rounding.
struct PdfiumRules;

impl PixelRules for PdfiumRules {
    fn clip(&self, path: &mut BezPath) {
        if let Some(rect) = axis_rect(path) {
            *path = outer_rect(rect).to_path(0.1);
        }
    }

    fn glyph_origins(&self, origins: &mut [Point]) {
        for origin in origins {
            *origin = Point::new(origin.x.round(), origin.y.round());
        }
    }

    fn image(&self, transform: Affine, width: u32, height: u32) -> Affine {
        on_axes(transform, width, height, |(start, length)| {
            match_float_range(start, length)
        })
    }
}

/// Acrobat's rounding.
struct AdobeRules;

impl PixelRules for AdobeRules {
    fn fill(&self, path: &mut BezPath) {
        map_points(path, snap_to_device);
    }

    fn fill_overlap(&self) -> f64 {
        ADOBE_PIXEL_OVERLAP
    }

    fn stroke(&self, path: &mut BezPath, width: &mut f64) {
        *width = adjusted_width(*width);
        if is_polyline(path) {
            let on_grid = stroke_grid(*width);
            map_points(path, |p| Point::new(on_grid(p.x), on_grid(p.y)));
        } else {
            map_points(path, snap_to_device);
        }
    }

    fn clip(&self, path: &mut BezPath) {
        map_points(path, snap_to_device);
    }

    fn glyph_origins(&self, origins: &mut [Point]) {
        text_grid(origins);
    }

    fn image(&self, transform: Affine, width: u32, height: u32) -> Affine {
        on_axes(transform, width, height, |(start, length)| {
            let end = snap(start + length);
            let start = snap(start);
            (start, end - start)
        })
    }
}

/// `v` moved onto the whole pixel it is within [`ADOBE_SNAP_TOLERANCE`] of, if any.
pub fn snap(v: f64) -> f64 {
    let whole = v.round();
    if (v - whole).abs() < ADOBE_SNAP_TOLERANCE {
        whole
    } else {
        v
    }
}

fn snap_to_device(p: Point) -> Point {
    Point::new(snap(p.x), snap(p.y))
}

/// The smallest whole-pixel rectangle around `rect` (`PDFium`'s `GetOuterRect`).
pub fn outer_rect(rect: Rect) -> Rect {
    Rect::new(rect.x0.floor(), rect.y0.floor(), rect.x1.ceil(), rect.y1.ceil())
}

/// The whole-pixel span `PDFium` gives the range `start..start + length` (`MatchFloatRange`): it
/// is `ceil(length)` pixels long and starts at the floor or the ceiling of `start`, whichever
/// leaves its two ends nearer to the range's (the floor on a tie).
pub fn match_float_range(start: f64, length: f64) -> (f64, f64) {
    let end = start + length;
    let length = length.ceil();
    let error = |s: f64| (s - start).abs() + (s + length - end).abs();
    let (down, up) = (start.floor(), start.ceil());
    let start = if error(up) < error(down) { up } else { down };
    (start, length)
}

/// A stroke's width in device pixels after stroke adjustment: a whole number, at least one.
pub fn adjusted_width(width: f64) -> f64 {
    width.round().max(1.0)
}

/// Where stroke adjustment puts a coordinate for a stroke `width` whole pixels wide: on a pixel
/// centre when the width is odd, on a pixel edge when it is even, so both of the stroke's edges
/// land on pixel boundaries.
pub fn stroke_grid(width: f64) -> impl Fn(f64) -> f64 {
    let odd = width.rem_euclid(2.0) >= 1.0;
    move |c| if odd { c.floor() + 0.5 } else { c.round() }
}

/// Acrobat's text placement for one run: its first glyph on the pixel grid, and each glyph
/// after it one rounded advance further, the advance kept unrounded when it would round to zero.
/// The rounding error of each advance carries on to the glyphs after it.
pub fn text_grid(origins: &mut [Point]) {
    let Some(first) = origins.first().copied() else {
        return;
    };
    let mut previous = first;
    let mut placed = Point::new(first.x.round(), first.y.round());
    for origin in origins.iter_mut() {
        let advance = *origin - previous;
        previous = *origin;
        placed += Vec2::new(zero_guard(advance.x), zero_guard(advance.y));
        *origin = placed;
    }
}

/// `v` rounded, or `v` itself when it would round to zero.
fn zero_guard(v: f64) -> f64 {
    let rounded = v.round();
    if rounded == 0.0 { v } else { rounded }
}

/// Every point of `path`, control points included, moved by `f`.
fn map_points(path: &mut BezPath, f: impl Fn(Point) -> Point) {
    for el in path.elements_mut() {
        *el = match *el {
            PathEl::MoveTo(p) => PathEl::MoveTo(f(p)),
            PathEl::LineTo(p) => PathEl::LineTo(f(p)),
            PathEl::QuadTo(a, p) => PathEl::QuadTo(f(a), f(p)),
            PathEl::CurveTo(a, b, p) => PathEl::CurveTo(f(a), f(b), f(p)),
            PathEl::ClosePath => PathEl::ClosePath,
        };
    }
}

/// Whether `path` has straight segments only.
fn is_polyline(path: &BezPath) -> bool {
    path.elements()
        .iter()
        .all(|el| matches!(el, PathEl::MoveTo(_) | PathEl::LineTo(_) | PathEl::ClosePath))
}

/// The rectangle `path` is, if it is one subpath of straight segments on two x and two y
/// values.
fn axis_rect(path: &BezPath) -> Option<Rect> {
    let mut points = Vec::with_capacity(5);
    for (i, el) in path.elements().iter().enumerate() {
        match el {
            PathEl::MoveTo(p) if i == 0 => points.push(*p),
            PathEl::LineTo(p) if i > 0 => points.push(*p),
            PathEl::ClosePath => {}
            _ => return None,
        }
    }
    let bounds = path.bounding_box();
    let on_edge = |v: f64, a: f64, b: f64| (v - a).abs() < 1e-9 || (v - b).abs() < 1e-9;
    let corners = points
        .iter()
        .all(|p| on_edge(p.x, bounds.x0, bounds.x1) && on_edge(p.y, bounds.y0, bounds.y1));
    // Four corners, the first possibly repeated to close it; every side along an axis.
    let sides = points
        .windows(2)
        .all(|w| (w[0].x - w[1].x).abs() < 1e-9 || (w[0].y - w[1].y).abs() < 1e-9);
    (corners && sides && (4..=5).contains(&points.len()) && bounds.area() > 0.0).then_some(bounds)
}

/// `transform` (an image's `width` × `height` sample grid to device space) with each axis's
/// device span `(start, length)` replaced by `place`'s, when the image is axis-aligned
/// (upright, flipped or turned a quarter). Any other image keeps its transform.
fn on_axes(transform: Affine, width: u32, height: u32, place: impl Fn((f64, f64)) -> (f64, f64)) -> Affine {
    // x' = sx·u + kx·v + tx and y' = ky·u + sy·v + ty for sample (u, v).
    let [sx, ky, kx, sy, tx, ty] = transform.as_coeffs();
    let (columns, rows) = (f64::from(width), f64::from(height));
    if columns <= 0.0 || rows <= 0.0 {
        return transform;
    }
    let tiny = 1e-9 * sx.abs().max(ky.abs()).max(kx.abs()).max(sy.abs());
    // One device axis, along which `samples` samples step `step` pixels each from `offset`: the
    // new step and offset once `place` has moved its span.
    let axis = |step: f64, samples: f64, offset: f64| -> (f64, f64) {
        let (start, length) = place((offset + step.min(0.0) * samples, step.abs() * samples));
        let step = step.signum() * length / samples;
        (step, if step < 0.0 { start + length } else { start })
    };
    if ky.abs() <= tiny && kx.abs() <= tiny {
        let (sx, tx) = axis(sx, columns, tx);
        let (sy, ty) = axis(sy, rows, ty);
        Affine::new([sx, 0.0, 0.0, sy, tx, ty])
    } else if sx.abs() <= tiny && sy.abs() <= tiny {
        let (kx, tx) = axis(kx, rows, tx);
        let (ky, ty) = axis(ky, columns, ty);
        Affine::new([0.0, ky, kx, 0.0, tx, ty])
    } else {
        transform
    }
}

// Rounding lands on whole pixels and exact binary fractions: equality is what is tested.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_display_and_zoom_set_device_pixels_per_point() {
        let display = Display {
            width: 1920,
            height: 1080,
            dpi: 120,
        };
        let letter = (612.0, 792.0);
        let at = |zoom| Emulation {
            viewer: None,
            display,
            zoom,
        };

        assert_eq!(display.scale_factor(), 1.25);
        // 100 % at 125 %: a point is 120 / 72 device pixels.
        assert_eq!(at(ViewerZoom::Percent(100)).scale_for(letter), 120.0 / 72.0);
        assert_eq!(at(ViewerZoom::Percent(200)).scale_for(letter), 240.0 / 72.0);
        // Fit width: 612 pt across 1920 px; fit page: 792 pt down 1080 px is tighter.
        assert!((f64::from(at(ViewerZoom::FitWidth).scale_for(letter)) - 1920.0 / 612.0).abs() < 1e-5);
        assert!((f64::from(at(ViewerZoom::FitPage).scale_for(letter)) - 1080.0 / 792.0).abs() < 1e-5);
        // Smoke: the default is a 1920 × 1080 screen at 96 DPI and 100 %.
        assert_eq!(Emulation::default().scale_for(letter), 96.0 / 72.0);
        assert_eq!(ViewerZoom::FitWidth.to_string(), "fit width");
    }

    #[test]
    fn acrobat_snaps_only_within_a_ten_thousandth_of_a_pixel() {
        assert_eq!(snap(10.00005), 10.0);
        assert_eq!(snap(9.99995), 10.0);
        assert_eq!(snap(10.0002), 10.0002);
        // Smoke: half a pixel stays.
        assert_eq!(snap(10.5), 10.5);
    }

    #[test]
    fn pdfium_grows_a_rectangle_out_to_whole_pixels() {
        assert_eq!(
            outer_rect(Rect::new(10.2, 3.7, 20.1, 8.0)),
            Rect::new(10.0, 3.0, 21.0, 8.0)
        );
        let mut clip = Rect::new(10.2, 3.7, 20.1, 8.5).to_path(0.1);
        PdfiumRules.clip(&mut clip);
        assert_eq!(clip.bounding_box(), Rect::new(10.0, 3.0, 21.0, 9.0));
        // Smoke: a clip that is not a rectangle is left as it is.
        let mut triangle = BezPath::new();
        triangle.move_to((0.5, 0.5));
        triangle.line_to((9.5, 0.5));
        triangle.line_to((0.5, 9.5));
        triangle.close_path();
        let before = triangle.clone();
        PdfiumRules.clip(&mut triangle);
        assert_eq!(triangle, before);
    }

    #[test]
    fn match_float_range_keeps_the_span_and_moves_the_start_least() {
        // 10.3 .. 15.6: 6 px long (ceil 5.3); from 10 it ends at 16, from 11 at 17.
        assert_eq!(match_float_range(10.3, 5.3), (10.0, 6.0));
        // 10.8 .. 13.0: 3 px; from 11 the start moves 0.2 and the end 1.0, from 10 0.8 and 0.2.
        assert_eq!(match_float_range(10.8, 2.2), (10.0, 3.0));
        // 10.9 .. 14.8: 4 px; from 11 it ends at 15 (0.1 + 0.2), from 10 at 14 (0.9 + 0.8).
        assert_eq!(match_float_range(10.9, 3.9), (11.0, 4.0));
        // Smoke: a range already on whole pixels stays.
        assert_eq!(match_float_range(4.0, 3.0), (4.0, 3.0));
    }

    #[test]
    fn pdfium_puts_an_axis_aligned_image_on_whole_pixels_and_keeps_its_orientation() {
        // A 4 × 2 image drawn upside down (as a PDF image is), 10.5 × 5.25 px from (3.3, 20.6).
        let drawn = Affine::new([2.625, 0.0, 0.0, -2.625, 3.3, 25.85]);

        let placed = PdfiumRules.image(drawn, 4, 2);

        // x: 3.3 .. 13.8 becomes 11 px from 3; y: 20.6 .. 25.85 becomes 6 px from 20.
        assert_eq!(placed.as_coeffs(), [2.75, 0.0, 0.0, -3.0, 3.0, 26.0]);
        let corner = placed * Point::new(0.0, 2.0);
        assert_eq!(corner, Point::new(3.0, 20.0));
        // Smoke: a rotated image keeps its transform.
        let turned = Affine::rotate(0.3).then_translate(Vec2::new(5.0, 5.0));
        assert_eq!(PdfiumRules.image(turned, 4, 2), turned);
    }

    #[test]
    fn stroke_adjustment_rounds_the_width_and_centres_the_edges_on_pixel_boundaries() {
        assert_eq!(adjusted_width(0.4), 1.0);
        assert_eq!(adjusted_width(1.4), 1.0);
        assert_eq!(adjusted_width(1.6), 2.0);
        let mut line = BezPath::new();
        line.move_to((10.2, 4.7));
        line.line_to((30.9, 4.7));
        let mut width = 0.67;

        AdobeRules.stroke(&mut line, &mut width);

        // One pixel wide: its centre on the pixel centre, 4.5, so it covers 4..5 exactly.
        assert_eq!(width, 1.0);
        assert_eq!(
            line.elements(),
            [
                PathEl::MoveTo(Point::new(10.5, 4.5)),
                PathEl::LineTo(Point::new(30.5, 4.5))
            ]
        );
        // Two pixels wide: centred on a pixel edge.
        assert_eq!(stroke_grid(2.0)(4.7), 5.0);
        // Smoke: a curve keeps its shape; only coordinates within the tolerance snap.
        let mut curve = BezPath::new();
        curve.move_to((0.3, 0.3));
        curve.curve_to((4.3, 0.3), (8.3, 4.3), (8.00002, 8.3));
        let mut width = 1.0;
        AdobeRules.stroke(&mut curve, &mut width);
        assert_eq!(
            curve.elements()[1],
            PathEl::CurveTo((4.3, 0.3).into(), (8.3, 4.3).into(), (8.0, 8.3).into())
        );
    }

    #[test]
    fn acrobat_rounds_each_advance_so_the_error_carries_along_the_run() {
        // Advances of 7.4 px from 10.3: rounded to 7 each, so the run falls behind 0.4 px a glyph.
        let mut origins = [10.3, 17.7, 25.1, 32.5].map(|x| Point::new(x, 50.6));

        text_grid(&mut origins);

        assert_eq!(origins.map(|p| p.x), [10.0, 17.0, 24.0, 31.0]);
        assert_eq!(origins.map(|p| p.y), [51.0; 4]);
        // An advance under half a pixel is kept as it is rather than stacking the glyphs.
        let mut tight = [Point::new(0.0, 0.0), Point::new(0.3, 0.0)];
        text_grid(&mut tight);
        assert_eq!(tight[1].x, 0.3);
        // Smoke: PDFium rounds each origin on its own, so it never falls behind.
        let mut pdfium = [10.3, 17.7, 25.1, 32.5].map(|x| Point::new(x, 50.6));
        PdfiumRules.glyph_origins(&mut pdfium);
        assert_eq!(pdfium.map(|p| p.x), [10.0, 18.0, 25.0, 33.0]);
    }

    #[test]
    fn each_viewer_names_its_engine() {
        assert_eq!(
            Viewer::ALL.map(|v| (v.name(), v.engine().name())),
            [("Chrome", "PDFium"), ("Edge", "PDFium"), ("Acrobat", "Adobe AGM")]
        );
        // Smoke: Acrobat's fills overlap; PDFium's do not.
        assert_eq!(Viewer::Acrobat.rules().fill_overlap(), ADOBE_PIXEL_OVERLAP);
        assert_eq!(Viewer::Chrome.rules().fill_overlap(), 0.0);
    }
}
