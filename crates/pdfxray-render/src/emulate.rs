//! Viewer and display emulation: a page drawn on the device pixels of a given screen, rounded
//! the way a given Windows viewer rounds what lands between them.
//!
//! An [`Emulation`] is a [`Display`] (its size in device pixels and its DPI), the zoom the viewer
//! shows the page at ([`ViewerZoom`]), and optionally a [`Viewer`]. The display and zoom set the
//! scale, `zoom × DPI / 72` device pixels per point: what the viewer draws, not what pdfxray would
//! draw on this Mac. The viewer sets the rules that place geometry on that pixel grid, following
//! `docs/pdf-rendering-rounding-windows.md`:
//!
//! * **Chrome and Edge** draw with `PDFium`. Positions stay floats, with no snapping or overlap.
//!   A rectangular clip grows out to whole pixels (`GetOuterRect`). Each glyph's origin is
//!   rounded to the nearest pixel. An axis-aligned image covers a whole number of pixels, `ceil`
//!   of its size, starting at the floor or ceiling of its edge, whichever moves it less
//!   (`MatchFloatRange`). Thin lines: no stroke is narrower than one device pixel, a fill that
//!   encloses no area is a 1 pixel line at a quarter of its opacity, a segment that goes nowhere
//!   is nudged a pixel to the right, and a dash cycle under 0.1 px draws solid (a dash under a
//!   millionth becomes 0.1 px).
//! * **Acrobat** draws with Adobe's AGM. A coordinate within 0.0001 px of a whole pixel snaps to
//!   it (`SnapToPureTolerance`). Fills reach 0.01 px past their edges (`PixelOverlap`). Stroke
//!   adjustment puts a straight stroke's edges on the nearest pixel boundaries, at least a
//!   pixel apart, so a thin line is 1 px inside a pixel row and 2 px when it straddles two. A
//!   text run starts on the pixel grid and each glyph advance is rounded, unless it would round
//!   to zero. Its Page Display preferences ([`AcrobatPrefs`]): *Enhance thin lines* draws a
//!   stroke under 1 pt as a solid 1 px black line, and with *Line Weights* off every stroke is
//!   1 px.
//!
//! Paint order (the document's section 7). Both engines paint the content stream in its order,
//! as pdfxray does, then the annotations. `PDFium` paints the annotations in `/Annots` order;
//! Acrobat paints form fields (widget annotations) after every other annotation, which the
//! emulation does by drawing a copy of the page whose `/Annots` lists them last. Acrobat's GPU
//! path (*Use 2D graphics acceleration*, on as it starts) batches opaque, solid-colour shapes
//! and resolves their overlaps on a multisampled depth buffer instead of compositing them one
//! after the other: emulated by drawing the page [`GPU_SAMPLES`] times larger with those shapes
//! aliased (each sample covered or not) and averaging it down, the rules still placing geometry
//! on whole device pixels. Shapes that share an edge then leave no seam, where painting in order
//! lets the background show through the half-covered pixels between them. Translucent shapes,
//! blended ones, soft-masked ones, anything inside a transparency group, and text stay painted
//! in order, as on its GDI path (the preference off).
//!
//! Both hint upright text to the pixel grid (`crate::hinting`): Acrobat with the font's own
//! hints, as `CoolType` does, `PDFium` with the autohinter, as its `FreeType` does. A font skrifa
//! cannot read (a bare CFF program, a Type 1 font) and turned or squeezed text stay unhinted.
//!
//! What is not emulated, because the document gives no values for it, hayro does not have it, or
//! it is the viewer's own code: Acrobat's discrete resolution table and its feature thresholds
//! (dash conversion among them), integer rasteriser precision, `CoolType`'s own hinter and its
//! dynamic anti-aliasing (`ClearType`-like subpixel text among it), anti-aliasing and
//! blending arithmetic (every viewer here is drawn with pdfxray's), knockout groups, the GPU's
//! colour-managed shaders and the seams where its output meets GDI's, `PDFium`'s 64-level
//! nesting limit (hayro stops at 50), `PDFium`'s endpoint snapping under the `SA` flag (hayro does not read it), its
//! knockout of a path's semi-transparent stroke over its own fill, and its legacy truncating
//! rectangles.

use std::fmt;

use hayro::hayro_interpret::font::OutlineGlyph;
use hayro::{DeviceStroke, PixelRules};

use crate::hinting::{Hinter, Hinting};
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

    /// The rules it places geometry on device pixels with; `acrobat` are Acrobat's
    /// preferences, which the other viewers do not have.
    pub(crate) fn rules(self, acrobat: AcrobatPrefs) -> Rules {
        match self.engine() {
            Engine::Pdfium => Rules::Pdfium(PdfiumRules::new()),
            Engine::Adobe => Rules::Adobe(AdobeRules::new(acrobat)),
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

/// Acrobat's Page Display preferences that change how thin lines are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AcrobatPrefs {
    /// *Enhance thin lines*: a stroke under 1 pt (one user unit) is a solid 1 px black line.
    pub enhance_thin_lines: bool,
    /// *Line Weights* (Ctrl+5): strokes at their own width; off, every stroke is 1 px.
    pub line_weights: bool,
    /// *Use 2D graphics acceleration*: the GPU path, which batches opaque, solid-colour shapes
    /// and resolves their overlaps on a multisampled depth buffer, so shapes that share an edge
    /// leave no seam. Off, the GDI path paints every shape in order, as `PDFium` does.
    pub gpu: bool,
}

impl Default for AcrobatPrefs {
    /// All on, as Acrobat starts on a machine with a GPU.
    fn default() -> Self {
        Self {
            enhance_thin_lines: true,
            line_weights: true,
            gpu: true,
        }
    }
}

/// How many samples a side Acrobat's GPU path is emulated with: 3 × 3 per device pixel, near the
/// 8 of the multisampling it uses.
pub const GPU_SAMPLES: u32 = 3;

impl Viewer {
    /// The samples a side of each device pixel `viewer` draws opaque shapes with: more than one
    /// on Acrobat's GPU path, one otherwise.
    pub(crate) const fn samples(self, acrobat: AcrobatPrefs) -> u32 {
        match self.engine() {
            Engine::Adobe if acrobat.gpu => GPU_SAMPLES,
            Engine::Adobe | Engine::Pdfium => 1,
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
    /// Acrobat's preferences, for when the viewer is Acrobat.
    pub acrobat: AcrobatPrefs,
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

/// Whether `page` lists a widget annotation (a form field) before another annotation in its
/// `/Annots`: Acrobat then paints them in another order than the array's, form fields last.
pub(crate) fn widgets_out_of_place(page: &hayro::hayro_syntax::page::Page<'_>) -> bool {
    use hayro::hayro_syntax::object::dict::keys::{ANNOTS, SUBTYPE};
    use hayro::hayro_syntax::object::{Array, Dict, Name};
    let Some(annots) = page.raw().get::<Array<'_>>(ANNOTS) else {
        return false;
    };
    let widgets: Vec<bool> = annots
        .iter::<Dict<'_>>()
        .map(|annot| {
            annot
                .get::<Name<'_>>(SUBTYPE)
                .is_some_and(|s| s.as_ref() == b"Widget")
        })
        .collect();
    widgets.windows(2).any(|pair| pair[0] && !pair[1])
}

/// The rules of a viewer, as [`Viewer::rules`] gives them.
pub(crate) enum Rules {
    /// Chrome's and Edge's.
    Pdfium(PdfiumRules),
    /// Acrobat's.
    Adobe(AdobeRules),
}

impl Rules {
    fn get(&self) -> &dyn PixelRules {
        match self {
            Self::Pdfium(rules) => rules,
            Self::Adobe(rules) => rules,
        }
    }
}

impl PixelRules for Rules {
    fn fill(&self, path: &mut BezPath) {
        self.get().fill(path);
    }

    fn fill_overlap(&self) -> f64 {
        self.get().fill_overlap()
    }

    fn thin_fill_alpha(&self) -> Option<f32> {
        self.get().thin_fill_alpha()
    }

    fn stroke(&self, path: &mut BezPath, stroke: &mut DeviceStroke) {
        self.get().stroke(path, stroke);
    }

    fn clip(&self, path: &mut BezPath) {
        self.get().clip(path);
    }

    fn glyph_origins(&self, origins: &mut [Point]) {
        self.get().glyph_origins(origins);
    }

    fn image(&self, transform: Affine, width: u32, height: u32) -> Affine {
        self.get().image(transform, width, height)
    }

    fn glyph_outline(&self, glyph: &OutlineGlyph, device: Affine) -> Option<BezPath> {
        self.get().glyph_outline(glyph, device)
    }

    fn alias_opaque_paths(&self) -> bool {
        self.get().alias_opaque_paths()
    }
}

/// `inner`'s rules on a page drawn `factor` times larger than the device, so each device pixel
/// is `factor` × `factor` samples: geometry is handed to `inner` at device size, so it snaps and
/// rounds to whole device pixels, and comes back at sample size.
pub(crate) struct Supersampled<'a> {
    pub(crate) inner: &'a dyn PixelRules,
    pub(crate) factor: f64,
}

impl Supersampled<'_> {
    fn on_device(&self, path: &mut BezPath, adjust: impl FnOnce(&mut BezPath)) {
        *path = Affine::scale(1.0 / self.factor) * path.clone();
        adjust(path);
        *path = Affine::scale(self.factor) * path.clone();
    }
}

impl PixelRules for Supersampled<'_> {
    fn active(&self) -> bool {
        self.inner.active()
    }

    fn fill(&self, path: &mut BezPath) {
        self.on_device(path, |device| self.inner.fill(device));
    }

    fn fill_overlap(&self) -> f64 {
        self.inner.fill_overlap() * self.factor
    }

    fn thin_fill_alpha(&self) -> Option<f32> {
        self.inner.thin_fill_alpha()
    }

    fn stroke(&self, path: &mut BezPath, stroke: &mut DeviceStroke) {
        let k = self.factor;
        stroke.width /= k;
        stroke.dash_offset /= k;
        for dash in &mut stroke.dashes {
            *dash /= k;
        }
        self.on_device(path, |device| self.inner.stroke(device, stroke));
        stroke.width *= k;
        stroke.dash_offset *= k;
        for dash in &mut stroke.dashes {
            *dash *= k;
        }
    }

    fn alias_opaque_paths(&self) -> bool {
        self.inner.alias_opaque_paths()
    }

    fn clip(&self, path: &mut BezPath) {
        self.on_device(path, |device| self.inner.clip(device));
    }

    fn glyph_origins(&self, origins: &mut [Point]) {
        let k = self.factor;
        for origin in origins.iter_mut() {
            *origin = Point::new(origin.x / k, origin.y / k);
        }
        self.inner.glyph_origins(origins);
        for origin in origins.iter_mut() {
            *origin = Point::new(origin.x * k, origin.y * k);
        }
    }

    fn glyph_outline(&self, glyph: &OutlineGlyph, device: Affine) -> Option<BezPath> {
        self.inner
            .glyph_outline(glyph, Affine::scale(1.0 / self.factor) * device)
    }

    fn image(&self, transform: Affine, width: u32, height: u32) -> Affine {
        let k = self.factor;
        Affine::scale(k)
            * self
                .inner
                .image(Affine::scale(1.0 / k) * transform, width, height)
    }
}

/// The opacity `PDFium` draws a fill that encloses no area with, as a 1 pixel line: a quarter
/// (`alpha >> 2`).
pub const PDFIUM_THIN_FILL_ALPHA: f32 = 0.25;

/// The shortest dash cycle `PDFium` draws dashed, in device pixels; a shorter one is solid.
pub const PDFIUM_MIN_DASH_CYCLE: f64 = 0.1;

/// `PDFium`'s rounding, and its glyphs autohinted.
pub(crate) struct PdfiumRules {
    hinter: Hinter,
}

impl PdfiumRules {
    fn new() -> Self {
        Self {
            hinter: Hinter::new(Hinting::Auto),
        }
    }
}

impl PixelRules for PdfiumRules {
    fn glyph_outline(&self, glyph: &OutlineGlyph, device: Affine) -> Option<BezPath> {
        self.hinter.outline(glyph, device)
    }

    fn thin_fill_alpha(&self) -> Option<f32> {
        Some(PDFIUM_THIN_FILL_ALPHA)
    }

    fn stroke(&self, path: &mut BezPath, stroke: &mut DeviceStroke) {
        stroke.width = stroke.width.max(1.0);
        nudge_degenerate_segments(path);
        collapse_dashes(stroke);
    }

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

/// Acrobat's rounding, with its preferences, and its glyphs hinted by their own hints.
pub(crate) struct AdobeRules {
    prefs: AcrobatPrefs,
    hinter: Hinter,
}

impl AdobeRules {
    fn new(prefs: AcrobatPrefs) -> Self {
        Self {
            prefs,
            hinter: Hinter::new(Hinting::FontHints),
        }
    }
}

impl PixelRules for AdobeRules {
    fn glyph_outline(&self, glyph: &OutlineGlyph, device: Affine) -> Option<BezPath> {
        self.hinter.outline(glyph, device)
    }

    fn alias_opaque_paths(&self) -> bool {
        self.prefs.gpu
    }

    fn fill(&self, path: &mut BezPath) {
        map_points(path, snap_to_device);
    }

    fn fill_overlap(&self) -> f64 {
        ADOBE_PIXEL_OVERLAP
    }

    fn stroke(&self, path: &mut BezPath, stroke: &mut DeviceStroke) {
        let thin = self.prefs.enhance_thin_lines && stroke.user_width < 1.0;
        if thin || !self.prefs.line_weights {
            // A hairline: stroke adjustment makes it the one pixel it falls in.
            stroke.width = 0.0;
            stroke.black |= thin;
        }
        stroke_adjust(path, stroke);
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

/// The pixel boundaries stroke adjustment puts the edges of a stroke `width` pixels wide,
/// centred on coordinate `c`: each edge on its nearest boundary, the centre where it is. A
/// stroke that would close up, narrower than a pixel there, covers the pixels it touches: one
/// inside a pixel, two across a boundary. A coordinate within the snapping tolerance of a
/// boundary is on it.
pub fn adjusted_edges(c: f64, width: f64) -> (f64, f64) {
    let half = width.max(0.0) / 2.0;
    let (low, high) = (snap(c - half), snap(c + half));
    if high.round() - low.round() >= 1.0 {
        return (low.round(), high.round());
    }
    let (low, high) = (low.floor(), high.ceil());
    if high > low { (low, high) } else { (low, low + 1.0) }
}

/// Acrobat's stroke adjustment of `path`, in device space. A straight segment along an axis
/// has its edges put on pixel boundaries ([`adjusted_edges`]): the stroke is as many whole
/// pixels wide as the widest of those spans, one at least, and each such coordinate is moved to
/// the middle of its span. So a thin line inside a pixel row is 1 px, and 2 px when it straddles
/// two rows: the line weight jumps as the zoom moves it. The other coordinates, and a path with curves,
/// keep their place but for the snapping tolerance; a curve's stroke is one pixel wide at least.
pub fn stroke_adjust(path: &mut BezPath, stroke: &mut DeviceStroke) {
    let width = stroke.width.max(0.0);
    if !is_polyline(path) {
        stroke.width = width.max(1.0);
        map_points(path, snap_to_device);
        return;
    }
    let (along_x, along_y) = axis_coordinates(path);
    let span = |c: f64| {
        let (low, high) = adjusted_edges(c, width);
        high - low
    };
    let points: Vec<Point> = path
        .elements()
        .iter()
        .filter_map(|el| match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect();
    let widest = points
        .iter()
        .zip(along_x.iter().zip(&along_y))
        .flat_map(|(p, (x, y))| [x.then(|| span(p.x)), y.then(|| span(p.y))])
        .flatten()
        .fold(None, |widest: Option<f64>, s| {
            Some(widest.map_or(s, |w| w.max(s)))
        });
    let Some(widest) = widest else {
        stroke.width = width.max(1.0);
        map_points(path, snap_to_device);
        return;
    };
    let adjusted = widest.max(1.0);
    let place = |c: f64, fixed: bool| {
        if fixed {
            adjusted_edges(c, width).0 + adjusted / 2.0
        } else {
            snap(c)
        }
    };
    let mut n = 0;
    for el in path.elements_mut() {
        if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
            *p = Point::new(place(p.x, along_x[n]), place(p.y, along_y[n]));
            n += 1;
        }
    }
    stroke.width = adjusted;
}

/// For each point of a polyline (its `MoveTo` and `LineTo` points, in order): whether a
/// vertical segment runs through it (its x is a stroke's centre across x), and whether a
/// horizontal one does (its y).
fn axis_coordinates(path: &BezPath) -> (Vec<bool>, Vec<bool>) {
    let (mut along_x, mut along_y) = (Vec::new(), Vec::new());
    let mut points: Vec<Point> = Vec::new();
    let mut start = 0;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                start = points.len();
                points.push(*p);
                along_x.push(false);
                along_y.push(false);
            }
            PathEl::LineTo(p) => {
                points.push(*p);
                along_x.push(false);
                along_y.push(false);
                let last = points.len() - 1;
                if last > 0 {
                    mark_axis(&points, (last - 1, last), &mut along_x, &mut along_y);
                }
            }
            PathEl::ClosePath => {
                if let Some(last) = points.len().checked_sub(1) {
                    mark_axis(&points, (last, start), &mut along_x, &mut along_y);
                }
            }
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
        }
    }
    (along_x, along_y)
}

/// Marks the segment from point `a` to point `b` of `points`: its x when it is vertical, its y
/// when it is horizontal.
fn mark_axis(points: &[Point], (a, b): (usize, usize), along_x: &mut [bool], along_y: &mut [bool]) {
    let (p, q) = (points[a], points[b]);
    if (p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() >= 1e-9 {
        along_x[a] = true;
        along_x[b] = true;
    }
    if (p.y - q.y).abs() < 1e-9 && (p.x - q.x).abs() >= 1e-9 {
        along_y[a] = true;
        along_y[b] = true;
    }
}

/// `PDFium`'s nudge of a segment that goes nowhere (a `LineTo` the point it starts from): its
/// end moves one pixel to the right, so the line is not lost.
pub fn nudge_degenerate_segments(path: &mut BezPath) {
    let mut previous: Option<Point> = None;
    for el in path.elements_mut() {
        match el {
            PathEl::LineTo(p) => {
                if previous == Some(*p) {
                    p.x += 1.0;
                }
                previous = Some(*p);
            }
            PathEl::MoveTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => previous = Some(*p),
            PathEl::ClosePath => {}
        }
    }
}

/// `PDFium`'s dashes: a cycle shorter than [`PDFIUM_MIN_DASH_CYCLE`] is drawn solid, and a dash
/// or gap under a millionth becomes 0.1.
pub fn collapse_dashes(stroke: &mut DeviceStroke) {
    if stroke.dashes.is_empty() {
        return;
    }
    if stroke.dashes.iter().sum::<f64>() < PDFIUM_MIN_DASH_CYCLE {
        stroke.dashes.clear();
        stroke.dash_offset = 0.0;
        return;
    }
    for dash in &mut stroke.dashes {
        if *dash < 1e-6 {
            *dash = 0.1;
        }
    }
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
            acrobat: AcrobatPrefs::default(),
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
        PdfiumRules::new().clip(&mut clip);
        assert_eq!(clip.bounding_box(), Rect::new(10.0, 3.0, 21.0, 9.0));
        // Smoke: a clip that is not a rectangle is left as it is.
        let mut triangle = BezPath::new();
        triangle.move_to((0.5, 0.5));
        triangle.line_to((9.5, 0.5));
        triangle.line_to((0.5, 9.5));
        triangle.close_path();
        let before = triangle.clone();
        PdfiumRules::new().clip(&mut triangle);
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

        let placed = PdfiumRules::new().image(drawn, 4, 2);

        // x: 3.3 .. 13.8 becomes 11 px from 3; y: 20.6 .. 25.85 becomes 6 px from 20.
        assert_eq!(placed.as_coeffs(), [2.75, 0.0, 0.0, -3.0, 3.0, 26.0]);
        let corner = placed * Point::new(0.0, 2.0);
        assert_eq!(corner, Point::new(3.0, 20.0));
        // Smoke: a rotated image keeps its transform.
        let turned = Affine::rotate(0.3).then_translate(Vec2::new(5.0, 5.0));
        assert_eq!(PdfiumRules::new().image(turned, 4, 2), turned);
    }

    /// A solid stroke `width` px wide, `user_width` units in user space.
    fn device_stroke(width: f64, user_width: f64) -> DeviceStroke {
        DeviceStroke {
            width,
            user_width,
            dashes: Vec::new(),
            dash_offset: 0.0,
            black: false,
        }
    }

    /// A horizontal line from x 10.2 to 30.9 at `y`.
    fn horizontal(y: f64) -> BezPath {
        let mut line = BezPath::new();
        line.move_to((10.2, y));
        line.line_to((30.9, y));
        line
    }

    /// The y of `line`'s points.
    fn ys(line: &BezPath) -> Vec<f64> {
        line.elements()
            .iter()
            .filter_map(|el| match el {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(p.y),
                _ => None,
            })
            .collect()
    }

    /// Acrobat with *Enhance thin lines* off and *Line Weights* on: stroke adjustment alone.
    fn adjust_only() -> AdobeRules {
        AdobeRules::new(AcrobatPrefs {
            enhance_thin_lines: false,
            line_weights: true,
            ..AcrobatPrefs::default()
        })
    }

    /// `stroke` adjusted by `rules` on a horizontal line at `y`: its width, its y and whether it
    /// turned black.
    fn adjusted(rules: &AdobeRules, y: f64, mut stroke: DeviceStroke) -> (f64, f64, bool) {
        let mut line = horizontal(y);
        rules.stroke(&mut line, &mut stroke);
        (stroke.width, ys(&line)[0], stroke.black)
    }

    #[test]
    fn stroke_adjustment_makes_a_thin_line_one_pixel_inside_a_row_and_two_across_rows() {
        // 0.6 px centred in row 4: its edges round to 4 and 5, one pixel.
        assert_eq!(
            adjusted(&adjust_only(), 4.5, device_stroke(0.6, 1.5)),
            (1.0, 4.5, false)
        );
        // The same line on the boundary of rows 4 and 5 would close up: it covers both.
        assert_eq!(
            adjusted(&adjust_only(), 5.0, device_stroke(0.6, 1.5)),
            (2.0, 5.0, false)
        );
        // 1.33 px (1 pt at 96 DPI): 1 px in a row, 2 px across two.
        assert_eq!(adjusted(&adjust_only(), 4.5, device_stroke(1.33, 1.0)).0, 1.0);
        assert_eq!(adjusted(&adjust_only(), 5.0, device_stroke(1.33, 1.0)).0, 2.0);
        // A hairline (width 0) is the pixel it falls in.
        assert_eq!(
            adjusted(&adjust_only(), 4.7, device_stroke(0.0, 0.0)),
            (1.0, 4.5, false)
        );
        // x runs along the line: its ends keep their place.
        let mut line = horizontal(4.7);
        adjust_only().stroke(&mut line, &mut device_stroke(0.6, 1.5));
        assert_eq!(line.elements()[1], PathEl::LineTo(Point::new(30.9, 4.5)));
        // Smoke: a curve keeps its shape, only coordinates within the tolerance snap, and its
        // stroke is a pixel wide at least.
        let mut curve = BezPath::new();
        curve.move_to((0.3, 0.3));
        curve.curve_to((4.3, 0.3), (8.3, 4.3), (8.00002, 8.3));
        let mut stroke = device_stroke(0.4, 0.3);
        adjust_only().stroke(&mut curve, &mut stroke);
        assert_eq!(
            curve.elements()[1],
            PathEl::CurveTo((4.3, 0.3).into(), (8.3, 4.3).into(), (8.0, 8.3).into())
        );
        assert_eq!(stroke.width, 1.0);
    }

    #[test]
    fn acrobat_enhances_thin_lines_and_without_line_weights_draws_every_stroke_one_pixel() {
        let enhanced = AdobeRules::new(AcrobatPrefs::default());
        // 0.9 pt is under one user unit: a solid black pixel row, even on a row boundary.
        assert_eq!(
            adjusted(&enhanced, 5.0, device_stroke(1.2, 0.9)),
            (1.0, 5.5, true)
        );
        // 1 pt is not thin: adjusted as it is, in its own colour.
        assert_eq!(
            adjusted(&enhanced, 5.0, device_stroke(1.33, 1.0)),
            (2.0, 5.0, false)
        );
        // Line Weights off: a 4 px stroke is one pixel too, in its colour.
        let no_weights = AdobeRules::new(AcrobatPrefs {
            enhance_thin_lines: false,
            line_weights: false,
            ..AcrobatPrefs::default()
        });
        assert_eq!(
            adjusted(&no_weights, 4.5, device_stroke(4.0, 3.0)),
            (1.0, 4.5, false)
        );
        // Smoke: with both as Acrobat starts, a 4 px stroke keeps 4 px.
        assert_eq!(adjusted(&enhanced, 4.5, device_stroke(4.0, 3.0)).0, 4.0);
    }

    #[test]
    fn pdfium_clamps_strokes_to_a_pixel_nudges_dots_and_collapses_tiny_dashes() {
        let mut line = horizontal(4.7);
        let mut stroke = device_stroke(0.13, 0.1);

        PdfiumRules::new().stroke(&mut line, &mut stroke);

        // A 0.1 pt line at 96 DPI is a full pixel wide, where it is: no snapping.
        assert_eq!(stroke.width, 1.0);
        assert_eq!(ys(&line), [4.7, 4.7]);
        // A segment that goes nowhere ends a pixel to the right.
        let mut dot = BezPath::new();
        dot.move_to((3.0, 3.0));
        dot.line_to((3.0, 3.0));
        nudge_degenerate_segments(&mut dot);
        assert_eq!(dot.elements()[1], PathEl::LineTo(Point::new(4.0, 3.0)));
        // A dash cycle under 0.1 px is solid; a vanishing dash becomes 0.1 px.
        let mut fine = DeviceStroke {
            dashes: vec![0.04, 0.04],
            dash_offset: 0.02,
            ..device_stroke(1.0, 1.0)
        };
        collapse_dashes(&mut fine);
        assert_eq!((fine.dashes.len(), fine.dash_offset), (0, 0.0));
        let mut dotted = DeviceStroke {
            dashes: vec![0.0, 3.0],
            ..device_stroke(1.0, 1.0)
        };
        collapse_dashes(&mut dotted);
        assert_eq!(dotted.dashes, [0.1, 3.0]);
        // Smoke: a stroke wider than a pixel keeps its width, and zero-area fills are a
        // quarter-opacity line.
        let mut wide = device_stroke(2.6, 2.0);
        PdfiumRules::new().stroke(&mut horizontal(4.7), &mut wide);
        assert_eq!(wide.width, 2.6);
        assert_eq!(PdfiumRules::new().thin_fill_alpha(), Some(0.25));
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
        PdfiumRules::new().glyph_origins(&mut pdfium);
        assert_eq!(pdfium.map(|p| p.x), [10.0, 18.0, 25.0, 33.0]);
    }

    #[test]
    fn each_viewer_names_its_engine() {
        assert_eq!(
            Viewer::ALL.map(|v| (v.name(), v.engine().name())),
            [("Chrome", "PDFium"), ("Edge", "PDFium"), ("Acrobat", "Adobe AGM")]
        );
        // Smoke: Acrobat's fills overlap; PDFium's do not.
        let prefs = AcrobatPrefs::default();
        assert_eq!(Viewer::Acrobat.rules(prefs).fill_overlap(), ADOBE_PIXEL_OVERLAP);
        assert_eq!(Viewer::Chrome.rules(prefs).fill_overlap(), 0.0);
        assert_eq!(Viewer::Acrobat.rules(prefs).thin_fill_alpha(), None);
    }
}
