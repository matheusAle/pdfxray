//! The visual diff: two pages in one bitmap, side by side, overlaid or as a mask of where their
//! pixels differ.
//!
//! Both sides are drawn at the same scale. For [`Mode::Overlay`] and [`Mode::Mask`] page B is
//! drawn **into A's pixel grid** ([`crate::PixelGrid`]), so a page a fraction of a point larger
//! does not change the bitmap's size and every pixel compares with the pixel it covers. A page
//! whose size or `/Rotate` really differs is refused with the reason ([`CompositeError::Geometry`])
//! unless scaling B onto A is asked for ([`Fit::ScaleBToA`]): scaling silently would be a repair.
//! The global offset a pairing reports is applied only when asked ([`Registration`]), and the
//! composite says that it was.
//!
//! The composite carries where each side's page space landed on it ([`Composite::a`],
//! [`Composite::b`]), so the annotation layer can be laid out over both sides at once, and the
//! app and an export draw the same picture.

use hayro::vello_cpu::Pixmap;
use pdfxray_core::numeric::{f32_from_f64, u32_from_f64_round, usize_from_u32};
use pdfxray_core::{Document, PageIx, Point, Rect};

use crate::annotate::{self, Annotation, Grid, Legend, Palette, Scene};
use crate::diff::{diff, luma};
use crate::transform::PixelRect;
use crate::{Overlay, PixelGrid, RenderError, RenderSpec, RenderedPage, Transform, overlay, render_page};

/// How the two sides are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A on the left, B on the right, each on its own grid, in one wide bitmap.
    SideBySide,
    /// A's ink in [`TINT_A`], B's in [`TINT_B`], multiplied: ink both share comes out dark, ink
    /// of one side only keeps that side's colour.
    Overlay,
    /// A in grey with every pixel that differs from B beyond the tolerance in red (see
    /// [`crate::diff`]).
    Mask,
}

impl Mode {
    /// True when B is drawn into A's grid, so the two must have the same geometry.
    pub const fn shares_grid(self) -> bool {
        !matches!(self, Self::SideBySide)
    }
}

/// What to do when the two pages differ in size or `/Rotate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Refuse, with the reason: the default.
    #[default]
    Refuse,
    /// Scale B uniformly so it fits A's page, from the top-left corner. Only on request, and
    /// the composite says by how much ([`Composite::b_scale`]).
    ScaleBToA,
}

/// The translation of B relative to A, points, removed before B is drawn: the offset a pairing
/// reports, applied only on request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Registration {
    /// Horizontal, points, page space.
    pub dx: f64,
    /// Vertical, points, page space (up).
    pub dy: f64,
}

/// Where one side's page landed on the composite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// Page space of that side onto the composite's pixels.
    pub to_pixel: Transform,
    /// The pixels its page covers.
    pub bounds: PixelRect,
}

/// The pixel comparison of the two sides, when they share a grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Delta {
    /// Pixels whose largest channel difference exceeds the tolerance.
    pub differing_pixels: u64,
    /// Share of the bitmap's pixels that differ, `0.0..=1.0`.
    pub differing_fraction: f64,
    /// Largest channel difference of any pixel.
    pub max_channel_delta: u8,
    /// Mean of every pixel's largest channel difference.
    pub mean_delta: f64,
    /// The tolerance it was measured with.
    pub tolerance: u8,
}

/// Two pages in one bitmap.
#[derive(Debug, Clone, PartialEq)]
pub struct Composite {
    /// How they are shown.
    pub mode: Mode,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes of opaque RGBA8, rows top to bottom.
    pub rgba: Vec<u8>,
    /// Where A landed.
    pub a: Placement,
    /// Where B landed.
    pub b: Placement,
    /// The offset removed from B; `None` when none was.
    pub registration: Option<Registration>,
    /// B's scale relative to A's when [`Fit::ScaleBToA`] scaled it; `None` when it was not.
    pub b_scale: Option<f64>,
    /// Where the pixels differ; `None` side by side, where no pixel is compared.
    pub delta: Option<Delta>,
}

/// Why two pages cannot be composited.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CompositeError {
    /// The pages differ in size or `/Rotate` and scaling was not asked for.
    #[error("{0}; not overlaid (scaling B onto A is a separate, explicit choice)")]
    Geometry(String),
    /// A side could not be drawn: no visual diff for it.
    #[error("side {side} cannot be drawn, so there is no visual diff: {error}")]
    Render {
        /// `A` or `B`.
        side: char,
        /// Why.
        #[source]
        error: RenderError,
    },
}

/// A's ink colour in [`Mode::Overlay`].
pub const TINT_A: [u8; 4] = [0xC0, 0x26, 0xD3, 255];
/// B's ink colour in [`Mode::Overlay`].
pub const TINT_B: [u8; 4] = [0x08, 0x91, 0xB2, 255];
/// The space between the pages side by side, in pixels per unit of scale.
const GUTTER: f32 = 24.0;
/// The colour of that space and of the bitmap below the shorter page.
const BACKDROP: [u8; 4] = [0xD5, 0xD9, 0xDE, 255];
/// How far two page sizes may differ, points, and still be drawn into one grid unscaled.
pub const SIZE_TOLERANCE: f32 = 1.0;
/// Default mask tolerance for two files drawn by the same engine. Cross-engine comparison (see
/// [`crate::pdfium`]) needs a looser one, 48.
pub const SAME_ENGINE_TOLERANCE: u8 = 12;

/// What to composite and how.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Request {
    /// How the sides are shown.
    pub mode: Mode,
    /// Pixels per point of A (and of B unless it is scaled to A).
    pub scale: f32,
    /// What to do about differing page geometry.
    pub fit: Fit,
    /// The offset to remove from B, if any.
    pub registration: Option<Registration>,
    /// The per-channel difference that still counts as equal.
    pub tolerance: u8,
}

/// Draws page `a.1` of `a.0` and page `b.1` of `b.0` into one bitmap as `request` says.
pub fn composite(
    a: (&Document, PageIx),
    b: (&Document, PageIx),
    request: &Request,
) -> Result<Composite, CompositeError> {
    let side = |side: char| move |error| CompositeError::Render { side, error };
    let page_a = render_page(a.0, a.1, &RenderSpec::new(request.scale)).map_err(side('A'))?;
    if !request.mode.shares_grid() {
        let page_b = render_page(b.0, b.1, &RenderSpec::new(request.scale)).map_err(side('B'))?;
        return Ok(compose(
            request.mode,
            &page_a,
            &page_b,
            None,
            None,
            request.tolerance,
        ));
    }
    let b_scale = b_scale(a, b, request.fit).map_err(|e| match e {
        Refusal::Geometry(why) => CompositeError::Geometry(why),
        Refusal::Render(side_of, error) => CompositeError::Render { side: side_of, error },
    })?;
    let scale_b = b_scale.map_or(request.scale, |by| f32_from_f64(f64::from(request.scale) * by));
    let offset = match request.registration {
        Some(r) => {
            let to_pixel = crate::page_transform(b.0, b.1, scale_b).map_err(side('B'))?;
            let (o, moved) = (
                to_pixel.to_pixel(Point::new(0.0, 0.0)),
                to_pixel.to_pixel(Point::new(-r.dx, -r.dy)),
            );
            (moved.x - o.x, moved.y - o.y)
        }
        None => (0.0, 0.0),
    };
    let spec = RenderSpec {
        grid: Some(PixelGrid {
            offset,
            ..PixelGrid::of(&page_a)
        }),
        ..RenderSpec::new(scale_b)
    };
    let page_b = render_page(b.0, b.1, &spec).map_err(side('B'))?;
    Ok(compose(
        request.mode,
        &page_a,
        &page_b,
        request.registration,
        b_scale,
        request.tolerance,
    ))
}

enum Refusal {
    Geometry(String),
    Render(char, RenderError),
}

/// B's scale relative to A's: `None` when the pages have the same geometry; refused when they
/// do not and `fit` does not allow scaling.
fn b_scale(a: (&Document, PageIx), b: (&Document, PageIx), fit: Fit) -> Result<Option<f64>, Refusal> {
    let (wa, ha) = crate::page_dimensions(a.0, a.1).map_err(|e| Refusal::Render('A', e))?;
    let (wb, hb) = crate::page_dimensions(b.0, b.1).map_err(|e| Refusal::Render('B', e))?;
    let rotate = |doc: &Document, page| doc.page(page).map_or(0, |info| info.rotate);
    let (ra, rb) = (rotate(a.0, a.1), rotate(b.0, b.1));
    let mismatch = if ra != rb {
        Some(format!("/Rotate differs: A is {ra}, B is {rb}"))
    } else if (wa - wb).abs() > SIZE_TOLERANCE || (ha - hb).abs() > SIZE_TOLERANCE {
        Some(format!(
            "the pages differ in size: A is {wa:.1} x {ha:.1} pt, B is {wb:.1} x {hb:.1} pt"
        ))
    } else {
        None
    };
    match (mismatch, fit) {
        (None, _) => Ok(None),
        (Some(why), Fit::Refuse) => Err(Refusal::Geometry(why)),
        (Some(_), Fit::ScaleBToA) => Ok(Some(f64::from((wa / wb).min(ha / hb)))),
    }
}

/// Puts two rendered pages into one bitmap: side by side, or, for the modes that share a grid,
/// blended pixel by pixel (B is expected on A's grid; where it is not, what only one side covers
/// counts as differing). `registration` and `b_scale` are what was done to B, recorded.
pub fn compose(
    mode: Mode,
    a: &RenderedPage,
    b: &RenderedPage,
    registration: Option<Registration>,
    b_scale: Option<f64>,
    tolerance: u8,
) -> Composite {
    let bounds = |page: &RenderedPage, x: f64| PixelRect {
        x0: x,
        y0: 0.0,
        x1: x + f64::from(page.width),
        y1: f64::from(page.height),
    };
    let placement = |page: &RenderedPage, x: f64| Placement {
        to_pixel: page
            .page_to_pixel
            .onto((1.0, 1.0), (x, 0.0))
            .unwrap_or(page.page_to_pixel),
        bounds: bounds(page, x),
    };
    if mode == Mode::SideBySide {
        let gutter = side_gutter(a);
        let width = a.width + gutter + b.width;
        let height = a.height.max(b.height);
        let mut rgba = BACKDROP.repeat(usize_from_u32(width) * usize_from_u32(height));
        blit(&mut rgba, width, a, 0);
        blit(&mut rgba, width, b, a.width + gutter);
        let b_at = f64::from(a.width + gutter);
        return Composite {
            mode,
            width,
            height,
            rgba,
            a: placement(a, 0.0),
            b: placement(b, b_at),
            registration: None,
            b_scale: None,
            delta: None,
        };
    }
    let compared = diff(a, b, tolerance);
    let delta = Delta {
        differing_pixels: compared.differing_pixels,
        differing_fraction: compared.differing_fraction(),
        max_channel_delta: compared.max_channel_delta,
        mean_delta: compared.mean_delta,
        tolerance,
    };
    let rgba = match mode {
        Mode::Mask => compared.image,
        _ => overlay_pixels(a, b, compared.width, compared.height),
    };
    Composite {
        mode,
        width: compared.width,
        height: compared.height,
        rgba,
        a: placement(a, 0.0),
        b: placement(b, 0.0),
        registration,
        b_scale,
        delta: Some(delta),
    }
}

/// The gutter between two pages side by side: [`GUTTER`] pixels per unit of the scale they were
/// drawn at, read off A's transform.
fn side_gutter(a: &RenderedPage) -> u32 {
    let [m11, m12, ..] = a.page_to_pixel.coeffs();
    u32_from_f64_round((f64::from(GUTTER) * m11.hypot(m12)).clamp(1.0, 4096.0))
}

/// Copies `page` into the `width`-pixel-wide `rgba` with its left edge at column `x`.
fn blit(rgba: &mut [u8], width: u32, page: &RenderedPage, x: u32) {
    let stride = usize_from_u32(width) * 4;
    let row = usize_from_u32(page.width) * 4;
    for (y, line) in page.rgba.chunks_exact(row).enumerate() {
        let at = y * stride + usize_from_u32(x) * 4;
        if let Some(target) = rgba.get_mut(at..at + row) {
            target.copy_from_slice(line);
        }
    }
}

/// A's ink in [`TINT_A`] times B's in [`TINT_B`] over a `width` x `height` grid; a pixel only
/// one side has is white on the other.
fn overlay_pixels(a: &RenderedPage, b: &RenderedPage, width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(usize_from_u32(width) * usize_from_u32(height) * 4);
    for y in 0..height {
        for x in 0..width {
            let (ta, tb) = (tint(pixel(a, x, y), TINT_A), tint(pixel(b, x, y), TINT_B));
            out.extend_from_slice(&[mul(ta[0], tb[0]), mul(ta[1], tb[1]), mul(ta[2], tb[2]), 255]);
        }
    }
    out
}

/// The pixel at `(x, y)` of `page`; white outside it.
fn pixel(page: &RenderedPage, x: u32, y: u32) -> [u8; 4] {
    if x >= page.width || y >= page.height {
        return [255; 4];
    }
    let at = (usize_from_u32(y) * usize_from_u32(page.width) + usize_from_u32(x)) * 4;
    page.rgba
        .get(at..at + 4)
        .and_then(|p| <[u8; 4]>::try_from(p).ok())
        .unwrap_or([255; 4])
}

/// How dark `p` is, laid on in `color`: white stays white, black becomes `color`.
fn tint(p: [u8; 4], color: [u8; 4]) -> [u8; 3] {
    let dark = 255 - u32::from(luma(p));
    let channel = |c: u8| {
        let lift = dark * (255 - u32::from(c));
        u8::try_from(255 - (lift + 127) / 255).unwrap_or(0)
    };
    [channel(color[0]), channel(color[1]), channel(color[2])]
}

/// `a * b / 255`, rounded: the multiply blend of one channel.
fn mul(a: u8, b: u8) -> u8 {
    u8::try_from((u32::from(a) * u32::from(b) + 127) / 255).unwrap_or(u8::MAX)
}

/// Lays `scenes` out on `grid`, a composite's grid in A's page space onto any output (the
/// composite's own pixels, or the screen it is shown on), with `legend` in a band of its own
/// below `grid.bounds`. Returns the shapes and the height of that band in output units: what
/// [`Composite::annotate`] paints, and what the app draws over the same composite, so the two
/// are one picture.
pub fn layout(grid: &Grid, palette: &Palette, scenes: &[Scene], legend: &Legend) -> (Vec<Annotation>, f64) {
    let mut shapes = annotate::layout(grid, palette, scenes, None);
    let (_, height) = annotate::legend_extent(grid, legend);
    if height > 0.0 {
        let below = Grid {
            bounds: PixelRect {
                y0: grid.bounds.y1,
                y1: grid.bounds.y1 + height,
                ..grid.bounds
            },
            ..*grid
        };
        shapes.extend(annotate::layout(&below, palette, &[], Some(legend)));
    }
    (shapes, height)
}

impl Composite {
    /// The annotation grid of the whole composite in A's page space: B's rects go through
    /// [`Composite::b_in_a`] first, so both sides are laid out together and their chips clear
    /// each other. `unit` pixels per annotation unit.
    pub fn grid(&self, unit: f32) -> Grid {
        Grid::bitmap(self.a.to_pixel, self.width, self.height, unit)
    }

    /// A page-space rect of B expressed in A's page space, through where both landed on the
    /// composite: what a mark of B is laid out at on [`Composite::grid`].
    pub fn b_in_a(&self, rect: Rect) -> Rect {
        let px = self.b.to_pixel.rect_to_pixel(rect);
        let (p, q) = (
            self.a.to_pixel.to_page(crate::PixelPoint::new(px.x0, px.y0)),
            self.a.to_pixel.to_page(crate::PixelPoint::new(px.x1, px.y1)),
        );
        Rect::new(p.x.min(q.x), p.y.min(q.y), p.x.max(q.x), p.y.max(q.y))
    }

    /// Adds `height` pixels of backdrop below the pages: room for a legend of its own, clear of
    /// what the pages show (see [`layout`]). The pages stay where they are.
    fn extend_below(&mut self, height: f64) {
        let rows = u32_from_f64_round(height.max(0.0).ceil());
        self.rgba
            .extend(BACKDROP.repeat(usize_from_u32(self.width) * usize_from_u32(rows)));
        self.height += rows;
    }

    /// Lays `scenes` (in A's page space, see [`Composite::b_in_a`]) out over the whole
    /// composite, `unit` pixels per annotation unit, puts `legend` in a band of its own below
    /// the pages, paints all of it and returns the shapes with what they stand for: what the
    /// export draws. The app draws the same [`layout`] on its own grid over the same composite.
    pub fn annotate(
        &mut self,
        unit: f32,
        palette: &Palette,
        scenes: &[Scene],
        legend: &Legend,
    ) -> Vec<Annotation> {
        let (shapes, band) = layout(&self.grid(unit), palette, scenes, legend);
        if band > 0.0 {
            self.extend_below(band);
        }
        self.paint(&annotate::overlays(&shapes));
        shapes
    }

    /// Paints `overlays` (in the composite's pixels) over the bitmap.
    pub fn paint(&mut self, overlays: &[Overlay]) {
        let (Ok(w), Ok(h)) = (u16::try_from(self.width), u16::try_from(self.height)) else {
            return;
        };
        let mut pixmap = Pixmap::new(w, h);
        // Opaque, so straight and premultiplied RGBA are the same bytes.
        pixmap.data_as_u8_slice_mut().copy_from_slice(&self.rgba);
        overlay::paint(&mut pixmap, overlays);
        self.rgba = pixmap.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha);
    }

    /// The composite as a page bitmap in A's page space, for writing it out.
    pub fn into_page(self) -> RenderedPage {
        RenderedPage {
            width: self.width,
            height: self.height,
            rgba: self.rgba,
            page_to_pixel: self.a.to_pixel,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use kurbo::Affine;
    use pretty_assertions::assert_eq;

    use super::*;

    const WHITE: [u8; 4] = [255; 4];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    fn page(width: u32, height: u32, fill: [u8; 4]) -> RenderedPage {
        RenderedPage {
            width,
            height,
            rgba: fill.repeat(usize_from_u32(width * height)),
            page_to_pixel: Transform::new(Affine::new([2.0, 0.0, 0.0, -2.0, 0.0, f64::from(height)]))
                .unwrap(),
        }
    }

    fn at(c: &Composite, x: u32, y: u32) -> [u8; 4] {
        let i = usize_from_u32((y * c.width + x) * 4);
        c.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn side_by_side_puts_b_right_of_a_past_a_gutter_and_places_its_page_space_there() {
        let c = compose(
            Mode::SideBySide,
            &page(4, 3, BLACK),
            &page(5, 6, WHITE),
            None,
            None,
            0,
        );

        // Scale 2: a 48-pixel gutter.
        assert_eq!((c.width, c.height), (4 + 48 + 5, 6));
        assert_eq!(
            [at(&c, 0, 0), at(&c, 10, 0), at(&c, 52, 5)],
            [BLACK, BACKDROP, WHITE]
        );
        assert_eq!(at(&c, 0, 4), BACKDROP, "below the shorter page");
        assert_eq!(c.b.bounds.x0, 52.0);
        assert_eq!(c.b.to_pixel.to_pixel(Point::new(0.0, 0.0)).x, 52.0);
        // Smoke: nothing is compared side by side.
        assert_eq!(c.delta, None);
    }

    #[test]
    fn overlay_keeps_each_sides_colour_where_only_it_has_ink_and_darkens_shared_ink() {
        let mut a = page(3, 1, WHITE);
        let mut b = page(3, 1, WHITE);
        a.rgba[0..4].copy_from_slice(&BLACK);
        a.rgba[8..12].copy_from_slice(&BLACK);
        b.rgba[4..8].copy_from_slice(&BLACK);
        b.rgba[8..12].copy_from_slice(&BLACK);

        let c = compose(Mode::Overlay, &a, &b, None, None, 0);

        let shared = [
            mul(TINT_A[0], TINT_B[0]),
            mul(TINT_A[1], TINT_B[1]),
            mul(TINT_A[2], TINT_B[2]),
            255,
        ];
        assert_eq!(
            [at(&c, 0, 0), at(&c, 1, 0), at(&c, 2, 0)],
            [TINT_A, TINT_B, shared]
        );
        assert_eq!(c.delta.map(|d| d.differing_pixels), Some(2));
        // Smoke: white on both sides stays white.
        assert_eq!(tint(WHITE, TINT_A), [255, 255, 255]);
    }

    #[test]
    fn mask_is_the_pixel_diff_of_the_two_sides() {
        let a = page(2, 1, WHITE);
        let mut b = a.clone();
        b.rgba[4..8].copy_from_slice(&BLACK);

        let c = compose(Mode::Mask, &a, &b, None, None, 12);

        assert_eq!([at(&c, 0, 0), at(&c, 1, 0)], [WHITE, [255, 0, 0, 255]]);
        let delta = c.delta.unwrap();
        assert_eq!((delta.differing_pixels, delta.tolerance), (1, 12));
        // Smoke: the fraction is of the whole bitmap.
        assert!((delta.differing_fraction - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_rect_of_b_lands_in_a_page_space_where_b_drew_it() {
        let c = compose(
            Mode::SideBySide,
            &page(4, 4, WHITE),
            &page(4, 4, WHITE),
            None,
            None,
            0,
        );

        let r = c.b_in_a(Rect::new(0.0, 0.0, 1.0, 1.0));

        // B starts 4 + 48 pixels right of A at 2 px per point: 26 pt.
        assert_eq!(r, Rect::new(26.0, 0.0, 27.0, 1.0));
        // Smoke: back onto the composite it is where B's own transform puts it.
        assert_eq!(
            c.a.to_pixel.rect_to_pixel(r),
            c.b.to_pixel.rect_to_pixel(Rect::new(0.0, 0.0, 1.0, 1.0))
        );
    }
}
