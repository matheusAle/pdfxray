//! Pictures of single objects: an image, a font specimen and its glyphs, a Form `XObject`, the
//! colours of a colour space, a shading, a pattern or a page, rasterised by the engine that draws
//! the pages.
//!
//! hayro draws pages, not loose objects, so a preview is made of pages of its own.
//! [`render_preview`] appends an incremental update (the technique of [`crate::filtered_pdf`])
//! holding a new catalog and a page tree whose pages paint the object, which their resources name
//! `/PdfxrayObject`; then it renders those pages. The original bytes are an exact prefix of the
//! copy and are never changed.
//!
//! | object | the preview pages paint | [`Preview`] |
//! |---|---|---|
//! | image | `/PdfxrayObject Do` on a page of the image's pixel size, one pixel per sample (fewer past [`IMAGE_PREVIEW_SIDE`]) | [`Preview::Picture`] |
//! | Form `XObject` | `/PdfxrayObject Do` on its `/BBox` after `/Matrix`; a form without resources borrows those of a page that paints it | [`Preview::Picture`] |
//! | font | `Aa Bb Cc 0123` when the program holds every character, and each code the document shows, one page each | [`Preview::Font`]: ink masks |
//! | colour space | a square per swatch: `/PdfxrayObject cs <components> scn` | [`Preview::Swatches`]: the colour painted in each |
//! | shading | an axial shading along its axis, as a bar; another one over the area it covers | [`Preview::Picture`] |
//! | pattern | a rectangle filled with `/Pattern cs /PdfxrayObject scn` | [`Preview::Picture`] |
//!
//! A page is rendered as it is, by [`crate::render_page`].
//!
//! A font preview never substitutes. When the engine would draw a font with another one (its
//! program is missing, unreadable or of an unsupported kind) the preview fails with a
//! [`PreviewError`] that says so, instead of showing a look-alike. Its glyphs come back as ink
//! masks (how much of each pixel the glyph covers), so the app can draw them in its own text
//! colour on a dark canvas without redrawing them with another font.

use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hayro::RenderCache;
use hayro::hayro_interpret::{InterpreterSettings, InterpreterWarning};
use hayro::vello_cpu::color::palette::css::{TRANSPARENT, WHITE};
use pdfxray_core::numeric::{f32_from_f64, f32_from_u32, f64_from_usize, u32_from_f64_trunc};
use pdfxray_core::{
    CharCode, ColorSpaceVisual, Document, FontProgram, FontVisual, FormVisual, ImageVisual, ObjRef,
    PatternVisual, Point, Rect, ShadingVisual, Visual, VisualKind, num,
};
use pdfxray_raw::RawDocument;

use crate::rewrite::{merged_resources, page_resources};
use crate::update::{Update, stream_body};
use crate::{
    RenderError, RenderSpec, RenderedPage, RewriteError, page_dimensions, rasterise_cached, render_page,
};

/// Pixels along the longest side of a scaled picture (a form, a shading, a pattern, a page):
/// sharp at twice the size the app shows it.
pub const PREVIEW_SIDE: u32 = 480;

/// Most pixels along the longest side of an image preview. An image is drawn one pixel per
/// sample up to this size, and scaled down to it past it.
pub const IMAGE_PREVIEW_SIDE: u32 = 2048;

/// Pixels per em of the specimen mask of a font: its 56-point size at two pixels per point.
pub const SPECIMEN_EM: f32 = 112.0;

/// Pixels per em of the mask of one glyph: its 24-point size at two pixels per point.
pub const GLYPH_EM: f32 = 48.0;

/// The name the preview pages' resources give the object.
const NAME: &str = "PdfxrayObject";

/// The name of the `[/Pattern /DeviceGray]` space an uncoloured pattern is painted in.
const PATTERN_SPACE: &str = "PdfxrayPatternSpace";

/// Size the text of a font preview page is set at, in points; the page is rendered at the scale
/// that gives the em of its mask.
const FONT_SIZE: f64 = 24.0;

/// Room around the glyphs of a font preview page, in ems, so overhangs are not cut.
const MASK_MARGIN: f64 = 0.15;

/// Width assumed for a code the font's widths do not give, in thousandths of an em.
const UNKNOWN_WIDTH: f64 = 1000.0;

/// The narrowest a glyph page gets, in thousandths of an em: a code without advance is still seen.
const MIN_ADVANCE: f64 = 500.0;

/// Margin around the swatches, in points.
const MARGIN: f64 = 12.0;

/// Side of a colour swatch, in points.
const SWATCH: f64 = 20.0;

/// Space between two swatches, in points.
const SWATCH_GAP: f64 = 4.0;

/// Depth of the bar of an axial shading, as a share of its length.
const BAR_DEPTH: f64 = 0.125;

/// The shortest side of a preview page as a share of its longest, so a sliver still has pixels.
const MIN_ASPECT: f64 = 0.02;

/// What a preview is made of.
#[derive(Debug, Clone, PartialEq)]
pub enum Preview {
    /// A bitmap of the object: an image (on a transparent background, so its soft mask shows),
    /// or a form, a shading, a pattern or a page on a white page.
    Picture(RenderedPage),
    /// A font, drawn with its own program.
    Font(FontPreview),
    /// The colour the engine painted for each of [`ColorSpaceVisual::swatches`], in order, as
    /// sRGB.
    Swatches(Vec<[u8; 3]>),
}

/// The pictures of a font: its specimen and its glyphs, each drawn by the font's own program.
#[derive(Debug, Clone, PartialEq)]
pub struct FontPreview {
    /// [`pdfxray_core::SPECIMEN`], when the font has [`FontVisual::sample`] codes.
    pub specimen: Option<InkMask>,
    /// Each of [`FontVisual::glyphs`] on its own, in order. Every mask of one font has the same
    /// height and baseline, so they line up when their tops do.
    pub glyphs: Vec<(CharCode, InkMask)>,
}

/// Where a font's ink falls: one coverage byte per pixel, 0 where nothing is painted and 255
/// where the glyph covers the whole pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct InkMask {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixels per em: [`SPECIMEN_EM`] or [`GLYPH_EM`].
    pub em: f32,
    /// Pixels of room around the glyphs: the pen starts this far from the left edge, and the top
    /// of the font's glyph box (its ascent) is this far below the top edge.
    pub pad: f32,
    /// `width * height` coverage bytes, rows top to bottom.
    pub coverage: Vec<u8>,
}

impl InkMask {
    /// The ink of `page`, drawn in black and grey on white.
    fn of(page: &RenderedPage, em: f32) -> Self {
        let coverage = page
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let luma = (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000;
                u8::try_from(255_u32.saturating_sub(luma)).unwrap_or(u8::MAX)
            })
            .collect();
        let pad = f32_from_f64(MASK_MARGIN) * em;
        Self {
            width: page.width,
            height: page.height,
            em,
            pad,
            coverage,
        }
    }

    /// True when no pixel has ink.
    fn is_blank(&self) -> bool {
        self.coverage.iter().all(|c| *c == 0)
    }
}

/// Why an object could not be previewed.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum PreviewError {
    /// The font has no embedded program, so a viewer draws its text with a font of its own.
    #[error("no font program is embedded; a viewer draws this text with a font of its own")]
    NotEmbedded,
    /// The embedded font program cannot be read, so a viewer would substitute another font.
    #[error("the embedded font program cannot be read; a viewer would substitute another font")]
    Unreadable,
    /// The renderer does not support the font (a CID font with a non-Identity encoding).
    #[error("the renderer does not support this kind of font")]
    UnsupportedFont,
    /// The font program drew nothing for the codes shown.
    #[error("the font program draws nothing for these codes")]
    Blank,
    /// There is no code to show: the document shows no text with the font and its widths name
    /// no character.
    #[error("the document shows no text with this font and its widths name no character")]
    NoGlyphs,
    /// The image data cannot be decoded.
    #[error("the image data cannot be decoded")]
    ImageDecode,
    /// A mesh shading (or a pattern of one) without `/BBox`: its extent is only known by
    /// decoding the mesh.
    #[error("a mesh shading without /BBox covers an area that is only known by decoding the mesh")]
    NoArea,
    /// The preview page could not be built.
    #[error("cannot build the preview page")]
    Build(#[from] RewriteError),
    /// The preview page could not be rendered.
    #[error(transparent)]
    Render(#[from] RenderError),
}

/// Draws `visual` (see the module documentation).
pub fn render_preview(doc: &Document, visual: &Visual) -> Result<Preview, PreviewError> {
    let raw = RawDocument::parse(Arc::new(doc.bytes().to_vec()))
        .map_err(|_| RewriteError::Unreadable("the file has no PDF header"))?;
    if raw.encryption().is_some() {
        return Err(RewriteError::Unreadable("the file is encrypted").into());
    }
    let reference = reference(visual.obj);
    let kind = &visual.kind;
    let scene = match kind {
        VisualKind::Page(page) => {
            let (width, height) = page_dimensions(doc, *page)?;
            let spec = RenderSpec::new(fit(PREVIEW_SIDE, width, height));
            return Ok(Preview::Picture(render_page(doc, *page, &spec)?));
        }
        VisualKind::Font(font) => return font_preview(doc, &raw, font, &reference, kind),
        VisualKind::ColorSpace(space) => {
            let page = single(draw(doc, &raw, &[swatch_scene(space, &reference)], kind)?)?;
            return Ok(Preview::Swatches(swatch_colors(space, &page)));
        }
        VisualKind::Image(image) => image_scene(image, &reference),
        VisualKind::Form(form) => form_scene(doc, &raw, form, visual.obj),
        VisualKind::Shading(shading) => shading_scene(shading, &reference)?,
        VisualKind::Pattern(pattern) => pattern_scene(pattern, &reference)?,
    };
    Ok(Preview::Picture(single(draw(doc, &raw, &[scene], kind)?)?))
}

/// The one page of a one-page preview.
fn single(mut pages: Vec<RenderedPage>) -> Result<RenderedPage, PreviewError> {
    pages.pop().ok_or_else(|| RewriteError::Rejected.into())
}

/// Renders `scenes`, one page each, and checks what the engine reported while drawing `kind`.
fn draw(
    doc: &Document,
    raw: &RawDocument,
    scenes: &[Scene],
    kind: &VisualKind,
) -> Result<Vec<RenderedPage>, PreviewError> {
    let copy = preview_pdf(doc, raw, scenes)?;
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(copy)).map_err(|_| RewriteError::Rejected)?;
    let pages = pdf.pages();
    let watch = Watch::default();
    let settings = watch.settings(matches!(kind, VisualKind::Font(_)));
    let cache = RenderCache::new();
    // An image keeps its transparency (a soft mask, a stencil); anything else is on paper.
    let background = if matches!(kind, VisualKind::Image(_)) {
        TRANSPARENT
    } else {
        WHITE
    };
    let mut rendered = Vec::with_capacity(scenes.len());
    for (i, scene) in scenes.iter().enumerate() {
        let page = pages.get(i).ok_or(RewriteError::Rejected)?;
        let (width, height) = page.render_dimensions();
        let scale = match scene.fit {
            Fit::Longest(side) => fit(side, width, height),
            Fit::Scale(scale) => scale,
        };
        rendered.push(rasterise_cached(
            page,
            &RenderSpec::new(scale),
            &settings,
            &cache,
            background,
        )?);
    }
    watch.verdict(kind)?;
    Ok(rendered)
}

/// The scale that makes the longer side of a `width` x `height` point area `longest_side` pixels.
fn fit(longest_side: u32, width: f32, height: f32) -> f32 {
    f32_from_u32(longest_side) / width.max(height).max(f32::EPSILON)
}

/// `n g R`.
fn reference(r: ObjRef) -> String {
    format!("{} {} R", r.num, r.generation)
}

/// What one preview page holds.
#[derive(Debug, Clone, PartialEq)]
struct Scene {
    /// The page's `/MediaBox`.
    media_box: Rect,
    /// The page's `/Resources`, in PDF syntax.
    resources: String,
    /// The page's content.
    content: String,
    /// How large the page is rendered.
    fit: Fit,
}

/// How large a preview page is rendered.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fit {
    /// With its longest side this many pixels.
    Longest(u32),
    /// At this many pixels per point.
    Scale(f32),
}

/// The image on a page of its pixel size, so each sample is one pixel.
fn image_scene(image: &ImageVisual, reference: &str) -> Scene {
    let (w, h) = (f64::from(image.width), f64::from(image.height));
    Scene {
        media_box: Rect::new(0.0, 0.0, w, h),
        resources: format!("<< /XObject << /{NAME} {reference} >> >>"),
        content: format!("q {} 0 0 {} 0 0 cm /{NAME} Do Q", num(w), num(h)),
        fit: Fit::Longest(image.width.max(image.height).min(IMAGE_PREVIEW_SIDE)),
    }
}

fn form_scene(doc: &Document, raw: &RawDocument, form: &FormVisual, r: ObjRef) -> Scene {
    let borrowed = form.borrows_from.and_then(|page| {
        let info = doc.page(page)?;
        let object = raw.object(info.obj?)?;
        let page_dict = object.value.value.as_dict()?;
        Some(page_resources(raw, info.resources_from, page_dict))
    });
    let resources = match borrowed {
        Some(base) => merged_resources(raw, &base, &[(NAME.to_owned(), r)]),
        None => format!("<< /XObject << /{NAME} {} >> >>", reference(r)),
    };
    Scene {
        media_box: form.area,
        resources,
        content: format!("/{NAME} Do"),
        fit: Fit::Longest(PREVIEW_SIDE),
    }
}

/// The specimen and each glyph of `font`, drawn with its program as ink masks.
fn font_preview(
    doc: &Document,
    raw: &RawDocument,
    font: &FontVisual,
    reference: &str,
    kind: &VisualKind,
) -> Result<Preview, PreviewError> {
    if font.program == FontProgram::Missing {
        return Err(PreviewError::NotEmbedded);
    }
    if font.sample.is_empty() && font.glyphs.is_empty() {
        return Err(PreviewError::NoGlyphs);
    }
    let specimen = (!font.sample.is_empty()).then(|| text_scene(font, &font.sample, reference, SPECIMEN_EM));
    let glyphs = font
        .glyphs
        .iter()
        .map(|code| text_scene(font, std::slice::from_ref(code), reference, GLYPH_EM));
    let scenes: Vec<Scene> = specimen.into_iter().chain(glyphs).collect();
    let mut pages = draw(doc, raw, &scenes, kind)?.into_iter();
    let specimen = if font.sample.is_empty() {
        None
    } else {
        pages.next().map(|page| InkMask::of(&page, SPECIMEN_EM))
    };
    let glyphs: Vec<(CharCode, InkMask)> = font
        .glyphs
        .iter()
        .cloned()
        .zip(pages.map(|page| InkMask::of(&page, GLYPH_EM)))
        .collect();
    if specimen
        .iter()
        .chain(glyphs.iter().map(|(_, mask)| mask))
        .all(InkMask::is_blank)
    {
        return Err(PreviewError::Blank);
    }
    Ok(Preview::Font(FontPreview { specimen, glyphs }))
}

/// `codes` in one line, on a page as tall as the font's glyph box and as wide as their advances,
/// with [`MASK_MARGIN`] around; rendered at `em` pixels per em.
fn text_scene(font: &FontVisual, codes: &[CharCode], reference: &str, em: f32) -> Scene {
    let ascent = font.facts.ascent_em.filter(|a| *a > 0.0).unwrap_or(0.8);
    let descent = font.facts.descent_em.filter(|d| *d < 0.0).unwrap_or(-0.2);
    let margin = MASK_MARGIN * FONT_SIZE;
    let advance: f64 = codes
        .iter()
        .map(|c| font.facts.width_units(c.value).unwrap_or(UNKNOWN_WIDTH))
        .sum();
    let width = advance.max(MIN_ADVANCE) / 1000.0 * FONT_SIZE + 2.0 * margin;
    let height = (ascent - descent) * FONT_SIZE + 2.0 * margin;
    let shown = codes.iter().fold(String::new(), |mut shown, code| {
        let _ = write!(shown, "<{}>", code.hex);
        shown
    });
    let scale = em / f32_from_f64(FONT_SIZE);
    Scene {
        media_box: Rect::new(0.0, 0.0, width, height),
        resources: format!("<< /Font << /{NAME} {reference} >> >>"),
        content: format!(
            "BT /{NAME} {} Tf 1 0 0 1 {} {} Tm [{shown}] TJ ET",
            num(FONT_SIZE),
            num(margin),
            num(margin - descent * FONT_SIZE)
        ),
        fit: Fit::Scale(scale),
    }
}

/// The page size of the swatches of `space`, in points.
fn swatch_page(space: &ColorSpaceVisual) -> (f64, f64) {
    let columns = space.columns.max(1);
    let rows = space.swatches.len().div_ceil(columns).max(1);
    let span = |n: usize| {
        let n = f64_from_usize(n);
        n * SWATCH + (n - 1.0) * SWATCH_GAP + 2.0 * MARGIN
    };
    (span(columns), span(rows))
}

/// The bottom-left corner of swatch `i` of `space`, in points.
fn swatch_corner(space: &ColorSpaceVisual, i: usize) -> (f64, f64) {
    let columns = space.columns.max(1);
    let (_, height) = swatch_page(space);
    let (column, row) = (f64_from_usize(i % columns), f64_from_usize(i / columns));
    (
        MARGIN + column * (SWATCH + SWATCH_GAP),
        height - MARGIN - SWATCH - row * (SWATCH + SWATCH_GAP),
    )
}

/// A square per swatch, at one pixel per point: the colour of each is read back from its centre.
fn swatch_scene(space: &ColorSpaceVisual, reference: &str) -> Scene {
    let (width, height) = swatch_page(space);
    let mut content = String::new();
    for (i, swatch) in space.swatches.iter().enumerate() {
        let (x, y) = swatch_corner(space, i);
        let components: Vec<String> = swatch.iter().map(|v| num(*v)).collect();
        let _ = writeln!(
            content,
            "/{NAME} cs {} scn {} {} {} {} re f",
            components.join(" "),
            num(x),
            num(y),
            num(SWATCH),
            num(SWATCH)
        );
    }
    let space_value = if space.icc_profile {
        format!("[/ICCBased {reference}]")
    } else {
        reference.to_owned()
    };
    Scene {
        media_box: Rect::new(0.0, 0.0, width, height),
        resources: format!("<< /ColorSpace << /{NAME} {space_value} >> >>"),
        content,
        fit: Fit::Scale(1.0),
    }
}

/// The colour at the centre of each swatch of `page`.
fn swatch_colors(space: &ColorSpaceVisual, page: &RenderedPage) -> Vec<[u8; 3]> {
    (0..space.swatches.len())
        .map(|i| {
            let (left, bottom) = swatch_corner(space, i);
            let centre = page
                .page_to_pixel
                .to_pixel(Point::new(left + SWATCH / 2.0, bottom + SWATCH / 2.0));
            let (px, py) = (
                u32_from_f64_trunc(centre.x.max(0.0)).min(page.width.saturating_sub(1)),
                u32_from_f64_trunc(centre.y.max(0.0)).min(page.height.saturating_sub(1)),
            );
            let at = usize::try_from((py * page.width + px) * 4).unwrap_or(0);
            match page.rgba.get(at..at + 3) {
                Some(&[red, green, blue]) => [red, green, blue],
                _ => [0xFF; 3],
            }
        })
        .collect()
}

/// An axial shading along its axis, as a bar; another shading over the area it covers.
fn shading_scene(shading: &ShadingVisual, reference: &str) -> Result<Scene, PreviewError> {
    if let Some((start, end)) = shading.axis
        && let Some(bar) = bar_scene(start, end, reference)
    {
        return Ok(bar);
    }
    Ok(Scene {
        media_box: shading.area.ok_or(PreviewError::NoArea)?,
        resources: format!("<< /Shading << /{NAME} {reference} >> >>"),
        content: format!("/{NAME} sh"),
        fit: Fit::Longest(PREVIEW_SIDE),
    })
}

/// A bar as long as the axis from `start` to `end` and [`BAR_DEPTH`] of that deep: the shading
/// is turned so the axis runs left to right through the middle of the bar.
fn bar_scene(start: Point, end: Point, reference: &str) -> Option<Scene> {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let length = dx.hypot(dy);
    if !(length.is_finite() && length > 0.0) {
        return None;
    }
    let (cos, sin) = (dx / length, dy / length);
    let depth = length * BAR_DEPTH;
    // Shading space to the bar: `start` lands on (0, depth / 2), the axis along x.
    let e = -(cos * start.x + sin * start.y);
    let f = depth / 2.0 - (cos * start.y - sin * start.x);
    let matrix = [cos, -sin, sin, cos, e, f].map(num).join(" ");
    Some(Scene {
        media_box: Rect::new(0.0, 0.0, length, depth),
        resources: format!("<< /Shading << /{NAME} {reference} >> >>"),
        content: format!("q {matrix} cm /{NAME} sh Q"),
        fit: Fit::Longest(PREVIEW_SIDE),
    })
}

fn pattern_scene(pattern: &PatternVisual, reference: &str) -> Result<Scene, PreviewError> {
    let area = pattern.area.ok_or(PreviewError::NoArea)?;
    let fill = format!(
        "{} {} {} {} re f",
        num(area.x0),
        num(area.y0),
        num(area.width()),
        num(area.height())
    );
    let (spaces, content) = if pattern.uncoloured {
        (
            format!(" /ColorSpace << /{PATTERN_SPACE} [/Pattern /DeviceGray] >>"),
            format!("/{PATTERN_SPACE} cs 0.2 /{NAME} scn {fill}"),
        )
    } else {
        (String::new(), format!("/Pattern cs /{NAME} scn {fill}"))
    };
    Ok(Scene {
        media_box: area,
        resources: format!("<< /Pattern << /{NAME} {reference} >>{spaces} >>"),
        content,
        fit: Fit::Longest(PREVIEW_SIDE),
    })
}

/// `rect` grown around its centre so the shorter side is at least [`MIN_ASPECT`] of the longer.
fn at_least_a_sliver(rect: Rect) -> Rect {
    let floor = rect.width().max(rect.height()) * MIN_ASPECT;
    let (cx, cy) = (f64::midpoint(rect.x0, rect.x1), f64::midpoint(rect.y0, rect.y1));
    let (half_w, half_h) = (rect.width().max(floor) / 2.0, rect.height().max(floor) / 2.0);
    Rect::new(cx - half_w, cy - half_h, cx + half_w, cy + half_h)
}

/// The bytes of `doc` followed by an update whose catalog holds only the pages of `scenes`.
fn preview_pdf(doc: &Document, raw: &RawDocument, scenes: &[Scene]) -> Result<Vec<u8>, RewriteError> {
    let mut update = Update::new(raw);
    let catalog = update.alloc();
    let pages = update.alloc();
    let mut kids = Vec::with_capacity(scenes.len());
    for scene in scenes {
        let page = update.alloc();
        let contents = update.alloc();
        let b = at_least_a_sliver(scene.media_box);
        update.put(contents, stream_body("", scene.content.as_bytes()));
        update.put(
            page,
            format!(
                "<< /Type /Page /Parent {} /MediaBox [{} {} {} {}] /Resources {} /Contents {} >>",
                reference(pages),
                num(b.x0),
                num(b.y0),
                num(b.x1),
                num(b.y1),
                scene.resources,
                reference(contents)
            )
            .into_bytes(),
        );
        kids.push(reference(page));
    }
    update.put(
        pages,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            kids.len()
        )
        .into_bytes(),
    );
    update.put(
        catalog,
        format!("<< /Type /Catalog /Pages {} >>", reference(pages)).into_bytes(),
    );
    update.set_root(catalog);
    update.finish(raw, doc.bytes())
}

/// What the engine reported while drawing a preview.
#[derive(Default)]
struct Watch {
    /// The engine asked for a font of its own: the file's program was missing or unreadable.
    substituted: Arc<AtomicBool>,
    unsupported_font: Arc<AtomicBool>,
    image_failed: Arc<AtomicBool>,
}

impl Watch {
    /// The engine's settings, reporting to this watch. With `strict_fonts` no font is
    /// substituted: a text the file cannot draw itself stays undrawn.
    fn settings(&self, strict_fonts: bool) -> InterpreterSettings {
        let mut settings = InterpreterSettings::default();
        if strict_fonts {
            let substituted = Arc::clone(&self.substituted);
            settings.font_resolver = Arc::new(move |_| {
                substituted.store(true, Ordering::Relaxed);
                None
            });
        }
        let (unsupported, failed) = (Arc::clone(&self.unsupported_font), Arc::clone(&self.image_failed));
        settings.warning_sink = Arc::new(move |warning| match warning {
            InterpreterWarning::UnsupportedFont => unsupported.store(true, Ordering::Relaxed),
            InterpreterWarning::ImageDecodeFailure => failed.store(true, Ordering::Relaxed),
            _ => {}
        });
        settings
    }

    /// The error the reports amount to for a preview of `kind`.
    fn verdict(&self, kind: &VisualKind) -> Result<(), PreviewError> {
        match kind {
            VisualKind::Font(_) if self.unsupported_font.load(Ordering::Relaxed) => {
                Err(PreviewError::UnsupportedFont)
            }
            VisualKind::Font(_) if self.substituted.load(Ordering::Relaxed) => Err(PreviewError::Unreadable),
            VisualKind::Image(_) if self.image_failed.load(Ordering::Relaxed) => {
                Err(PreviewError::ImageDecode)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn references_print_like_the_file_writes_them() {
        assert_eq!(reference(ObjRef::new(12, 3)), "12 3 R");
        // Smoke: generation zero.
        assert_eq!(reference(ObjRef::new(5, 0)), "5 0 R");
    }

    #[test]
    fn a_sliver_grows_to_a_fiftieth_of_its_length() {
        let grown = at_least_a_sliver(Rect::new(0.0, 0.0, 100.0, 0.0));

        assert_eq!(grown, Rect::new(0.0, -1.0, 100.0, 1.0));
        // Smoke: an area with room is kept as it is.
        assert_eq!(
            at_least_a_sliver(Rect::new(1.0, 2.0, 3.0, 4.0)),
            Rect::new(1.0, 2.0, 3.0, 4.0)
        );
    }
}
