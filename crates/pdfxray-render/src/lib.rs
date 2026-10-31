//! pdfxray page rendering.
//!
//! [`render_page`] rasterises one page with hayro and composites [`Overlay`]s on top. The
//! result carries the exact [`Transform`] between page space and pixels, so clicks on the
//! bitmap can be turned into page-space points for `Document::hit` and page-space
//! rectangles can be drawn on top of it.
//!
//! # Page space to pixel space
//!
//! hayro draws a page with `S * R * F * T`, applied right to left to a page-space point:
//!
//! 1. `T` translates the `CropBox` (clipped to the `MediaBox`) corner to the origin,
//! 2. `F` flips y (`y' = H - y`, `H` = unrotated crop height) so the origin is top-left,
//! 3. `R` rotates clockwise by `/Rotate` and moves the result back into the positive quadrant,
//! 4. `S` scales by [`RenderSpec::scale`].
//!
//! [`render_page`] asks hayro for `R * F * T` (`Page::initial_transform`) instead of
//! re-deriving it, so the transform cannot drift from what was drawn. The bitmap is
//! `trunc(W * scale)` x `trunc(H * scale)` pixels where `W x H` is the crop size with
//! width and height swapped for `/Rotate` 90 and 270.

mod diff;
mod filter;
mod overlay;
#[cfg(feature = "pdfium")]
pub mod pdfium;
mod preview;
mod rewrite;
mod transform;
mod update;
pub mod xray;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use hayro::hayro_interpret::{InterpreterSettings, TransformExt};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::color::{AlphaColor, Srgb};
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use kurbo::Affine;
use pdfxray_core::numeric::{u32_from_f32_trunc, usize_from_u32};
use pdfxray_core::{Document, EngineError, PageIx, Point};

pub use diff::{Diff, diff};
pub use filter::{ContentFilter, ops_for};
pub use overlay::Overlay;
pub use preview::{
    FontPreview, GLYPH_EM, IMAGE_PREVIEW_SIDE, InkMask, PREVIEW_SIDE, Preview, PreviewError, SPECIMEN_EM,
    render_preview,
};
pub use rewrite::filtered_pdf;
pub use transform::{PixelPoint, PixelRect, Transform};
pub use xray::{
    ColorBy, ROLE_CLASSES, ROLE_COLORS, RolePalette, XrayOptions, alpha_heatmap_overlays, clip_overlays,
    role_slot, xray_overlays,
};

use overlay::Canvas;

/// Largest bitmap side [`render_page`] produces, in pixels.
pub const MAX_SIDE: u32 = 16_384;

/// What to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSpec {
    /// Output pixels per PDF point. Must be finite and positive.
    pub scale: f32,
    /// Shapes composited over the page, in order.
    pub overlays: Vec<Overlay>,
    /// Which operators paint. Anything but [`ContentFilter::All`] renders a rewritten copy of
    /// the document, see [`filtered_pdf`]; the file itself is never modified.
    pub content: ContentFilter,
}

impl RenderSpec {
    /// A plain render at `scale` without overlays, every operator painting.
    pub const fn new(scale: f32) -> Self {
        Self {
            scale,
            overlays: Vec::new(),
            content: ContentFilter::All,
        }
    }
}

/// A rendered page bitmap.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPage {
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes of straight (non-premultiplied) RGBA8, rows top to bottom.
    pub rgba: Vec<u8>,
    /// Maps page-space points to pixels of this bitmap.
    pub page_to_pixel: Transform,
}

impl RenderedPage {
    /// Maps a pixel position of this bitmap back to page space.
    pub fn pixel_to_page(&self, p: PixelPoint) -> Point {
        self.page_to_pixel.to_page(p)
    }
}

/// Why a page could not be rendered.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    /// The rendering engine could not load the document.
    #[error("the document cannot be rendered ({})", engine_reason(*.0))]
    Engine(EngineError),
    /// The page index is past the last page.
    #[error("page {page} does not exist, the document has {count} pages")]
    NoSuchPage {
        /// Requested zero-based index.
        page: PageIx,
        /// Pages in the document.
        count: usize,
    },
    /// The scale is zero, negative, infinite or NaN.
    #[error("scale must be finite and positive")]
    InvalidScale,
    /// The bitmap would be empty or larger than [`MAX_SIDE`].
    #[error("a {width} x {height} pixel bitmap is outside 1..={MAX_SIDE} pixels per side")]
    BadSize {
        /// Requested width, saturated.
        width: u32,
        /// Requested height, saturated.
        height: u32,
    },
    /// The page content could not be filtered.
    #[error("cannot filter the page content")]
    Filter(#[from] RewriteError),
    /// Writing the PNG failed.
    #[error("cannot write {}: {message}", path.display())]
    Write {
        /// Destination file.
        path: PathBuf,
        /// What the encoder or the file system said.
        message: String,
    },
}

/// Why the content of a page could not be rewritten for a [`ContentFilter`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RewriteError {
    /// The page has no captured content (the engine could not load the document).
    #[error("the page content was not captured")]
    NotCaptured,
    /// The page tree could not be read, so the page object is unknown.
    #[error("the page object is unknown")]
    NoPageObject,
    /// The file cannot be extended by an update.
    #[error("{0}")]
    Unreadable(&'static str),
    /// The file is too large for the update's cross-reference stream (offsets above 4 GiB).
    #[error("the file is too large to extend")]
    TooLarge,
    /// The rewritten document could not be loaded by the engine.
    #[error("the engine rejects the rewritten document")]
    Rejected,
}

fn engine_reason(error: EngineError) -> &'static str {
    match error {
        EngineError::Encrypted => "encrypted documents are not supported",
        EngineError::Invalid => "the file could not be parsed",
    }
}

/// Size of `page` in points as it is drawn: the `CropBox` clipped to the `MediaBox`, with width
/// and height swapped for `/Rotate` 90 and 270.
pub fn page_dimensions(doc: &Document, page: PageIx) -> Result<(f32, f32), RenderError> {
    Ok(engine_page(doc, page)?.render_dimensions())
}

/// The engine's `page` of `doc`. With the engine loaded, the document's pages are the engine's,
/// so [`Document::page_count`] is the engine's page count.
#[expect(
    clippy::disallowed_methods,
    reason = "rendering is the one use of the engine outside pdfxray-core (see clippy.toml)"
)]
fn engine_page(doc: &Document, page: PageIx) -> Result<&hayro::hayro_syntax::page::Page<'_>, RenderError> {
    doc.engine_page(page).ok_or_else(|| match doc.engine_error() {
        Some(error) => RenderError::Engine(error),
        None => RenderError::NoSuchPage {
            page,
            count: doc.page_count(),
        },
    })
}

/// Rasterises `page` (zero-based) on a white background and paints `spec.overlays` over it.
///
/// With a [`ContentFilter`] other than `All`, a rewritten copy of the document is rendered in
/// place of the original (see [`filtered_pdf`]); the page keeps its size and position.
pub fn render_page(doc: &Document, page: PageIx, spec: &RenderSpec) -> Result<RenderedPage, RenderError> {
    if !(spec.scale.is_finite() && spec.scale > 0.0) {
        return Err(RenderError::InvalidScale);
    }
    let original = engine_page(doc, page)?;
    if spec.content == ContentFilter::All {
        return rasterise(original, spec, &InterpreterSettings::default());
    }
    let copy = filtered_pdf(doc, page, &spec.content)?;
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(copy)).map_err(|_| RewriteError::Rejected)?;
    let engine_page = pdf
        .pages()
        .get(usize_from_u32(page))
        .ok_or(RewriteError::Rejected)?;
    rasterise(engine_page, spec, &InterpreterSettings::default())
}

/// Rasterises `engine_page` with `settings` (fonts, warnings) and paints `spec.overlays` over it.
fn rasterise(
    engine_page: &hayro::hayro_syntax::page::Page<'_>,
    spec: &RenderSpec,
    settings: &InterpreterSettings,
) -> Result<RenderedPage, RenderError> {
    rasterise_cached(engine_page, spec, settings, &RenderCache::new(), WHITE)
}

/// [`rasterise`] on `background`, with a cache shared by several pages of one document so the
/// resources they share (a font program) are read once.
fn rasterise_cached<'a>(
    engine_page: &'a hayro::hayro_syntax::page::Page<'a>,
    spec: &RenderSpec,
    settings: &InterpreterSettings,
    cache: &RenderCache<'a>,
    background: AlphaColor<Srgb>,
) -> Result<RenderedPage, RenderError> {
    let (_, _, page_to_pixel) = geometry(engine_page, spec.scale)?;

    let pixmap = hayro::render(
        engine_page,
        cache,
        settings,
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: spec.scale,
            y_scale: spec.scale,
            bg_color: background,
        },
    );
    let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    let mut rgba = pixmap.take_rgba8(ImageAlphaType::Alpha);

    let mut canvas = Canvas {
        rgba: &mut rgba,
        width,
        height,
    };
    for overlay in &spec.overlays {
        overlay.paint(&mut canvas, &page_to_pixel);
    }

    Ok(RenderedPage {
        width,
        height,
        rgba,
        page_to_pixel,
    })
}

/// Bitmap size and page-to-pixel transform of a page drawn at `scale`.
///
/// Every renderer (hayro here, the `PDFium` reference renderer) takes its geometry from this one function,
/// so their bitmaps line up pixel for pixel. The size is the same arithmetic as `hayro::render`.
fn geometry(
    engine_page: &hayro::hayro_syntax::page::Page<'_>,
    scale: f32,
) -> Result<(u32, u32, Transform), RenderError> {
    let (page_w, page_h) = engine_page.render_dimensions();
    // Truncated to whole pixels, saturating at the `u32` range.
    let (width, height) = (
        u32_from_f32_trunc(page_w * scale),
        u32_from_f32_trunc(page_h * scale),
    );
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
        return Err(RenderError::BadSize { width, height });
    }
    let forward = Affine::scale(f64::from(scale)) * engine_page.initial_transform(true).to_kurbo();
    let page_to_pixel = Transform::new(forward).ok_or(RenderError::BadSize { width, height })?;
    Ok((width, height, page_to_pixel))
}

/// [`geometry`] of `page` in `doc`, after validating `scale`.
#[cfg(feature = "pdfium")]
fn page_geometry(doc: &Document, page: PageIx, scale: f32) -> Result<(u32, u32, Transform), RenderError> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(RenderError::InvalidScale);
    }
    geometry(engine_page(doc, page)?, scale)
}

/// Writes `page` as an 8-bit RGBA PNG.
pub fn write_png(page: &RenderedPage, path: &Path) -> Result<(), RenderError> {
    image::save_buffer(
        path,
        &page.rgba,
        page.width,
        page.height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| RenderError::Write {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}
