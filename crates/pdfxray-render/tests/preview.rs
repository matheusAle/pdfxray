//! Object previews: each kind of object drawn on pages of its own by the page renderer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use pdfxray_core::{Document, ObjRef, Visual};
use pdfxray_fixtures::gallery::{
    GALLERY_BROKEN_FONT, GALLERY_EMBEDDED_FONT, GALLERY_FORM, GALLERY_INDEXED, GALLERY_MASK,
    GALLERY_MISSING_FONT, GALLERY_PAGE, GALLERY_PATTERN, GALLERY_SEPARATION, GALLERY_SHADING, object_gallery,
};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pdfxray_render::{
    FontPreview, GLYPH_EM, InkMask, PREVIEW_SIDE, Preview, PreviewError, RenderSpec, RenderedPage,
    SPECIMEN_EM, render_page, render_preview,
};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn gallery() -> Document {
    Document::from_bytes("gallery", object_gallery().unwrap()).unwrap()
}

fn visual(doc: &Document, n: u32) -> Visual {
    doc.visual(ObjRef::new(n, 0)).unwrap()
}

fn preview(doc: &Document, n: u32) -> Result<Preview, PreviewError> {
    render_preview(doc, &visual(doc, n))
}

/// The bitmap of a preview that is one.
fn picture(doc: &Document, n: u32) -> RenderedPage {
    match preview(doc, n).unwrap() {
        Preview::Picture(page) => page,
        other => panic!("{n} 0 obj is not a picture: {other:?}"),
    }
}

/// The masks of a font preview.
fn font(doc: &Document, n: u32) -> FontPreview {
    match preview(doc, n).unwrap() {
        Preview::Font(font) => font,
        other => panic!("{n} 0 obj is not a font: {other:?}"),
    }
}

/// How many pixels of `mask` have more than half ink.
fn inked(mask: &InkMask) -> usize {
    mask.coverage.iter().filter(|c| **c > 127).count()
}

/// The colour of the pixel at `(x, y)` as fractions of the width and height.
fn at(page: &RenderedPage, x: f32, y: f32) -> [u8; 3] {
    let px = ((page.width as f32 * x) as u32).min(page.width - 1);
    let py = ((page.height as f32 * y) as u32).min(page.height - 1);
    let i = ((py * page.width + px) * 4) as usize;
    [page.rgba[i], page.rgba[i + 1], page.rgba[i + 2]]
}

const RED: [u8; 3] = [0xFF, 0, 0];
const GREEN: [u8; 3] = [0, 0xFF, 0];
const BLUE: [u8; 3] = [0, 0, 0xFF];
const YELLOW: [u8; 3] = [0xFF, 0xFF, 0];

#[test]
fn an_image_preview_is_one_pixel_per_sample() {
    let doc = fixture("images");

    let page = picture(&doc, 5);

    assert_eq!((page.width, page.height), (2, 2));
    // The first row of samples is the top of the image, each sample exactly one pixel.
    let pixels: Vec<[u8; 4]> = page.rgba.as_chunks::<4>().0.to_vec();
    let opaque = |[r, g, b]: [u8; 3]| [r, g, b, 0xFF];
    assert_eq!(pixels, [RED, GREEN, BLUE, YELLOW].map(opaque));
    // Smoke: the samplers agree.
    assert_eq!(at(&page, 0.75, 0.75), YELLOW);
}

#[test]
fn a_form_without_resources_paints_with_those_of_its_page() {
    let doc = gallery();

    let page = picture(&doc, GALLERY_FORM);

    // The form's area is exactly where its image lands, so the image fills the preview.
    assert_eq!((page.width, page.height), (PREVIEW_SIDE, PREVIEW_SIDE));
    assert_eq!([at(&page, 0.25, 0.25), at(&page, 0.75, 0.75)], [RED, YELLOW]);
}

#[test]
fn an_embedded_font_draws_its_specimen_and_each_glyph_with_its_own_program() {
    let doc = gallery();

    let font = font(&doc, GALLERY_EMBEDDED_FONT);

    // `Aa Bb Cc 0123` in one line, wider than tall, at the specimen's em.
    let specimen = font.specimen.expect("a specimen");
    assert!(
        (specimen.em - SPECIMEN_EM).abs() < f32::EPSILON,
        "{}",
        specimen.em
    );
    assert!(
        specimen.width > 4 * specimen.height,
        "{} x {}",
        specimen.width,
        specimen.height
    );
    assert!(inked(&specimen) > 2_000, "{} inked pixels", inked(&specimen));
    // `Hi`, one mask each, of one height so their baselines line up.
    let codes: Vec<&str> = font.glyphs.iter().map(|(code, _)| code.hex.as_str()).collect();
    assert_eq!(codes, ["48", "69"]);
    let (h, i) = (&font.glyphs[0].1, &font.glyphs[1].1);
    assert_eq!((h.em, h.height), (GLYPH_EM, i.height));
    // An `H` has more ink than an `i`.
    assert!(
        inked(h) > inked(i) && inked(i) > 20,
        "H {} i {}",
        inked(h),
        inked(i)
    );
    // Smoke: a mask is one byte per pixel.
    assert_eq!(h.coverage.len(), (h.width * h.height) as usize);
}

#[test]
fn a_font_that_cannot_be_drawn_from_the_file_is_not_faked() {
    let doc = gallery();

    let missing = preview(&doc, GALLERY_MISSING_FONT);
    let broken = preview(&doc, GALLERY_BROKEN_FONT);

    assert_eq!(missing, Err(PreviewError::NotEmbedded));
    assert_eq!(broken, Err(PreviewError::Unreadable));
    // Smoke: the page itself still renders (with a stand-in font, as viewers do).
    assert!(render_page(&doc, 0, &RenderSpec::new(1.0)).is_ok());
}

#[test]
fn a_type3_font_draws_its_glyph_procedures() {
    let doc = fixture("type3-font");

    let font = font(&doc, 5);

    // No specimen (the procedures may not hold its characters); `a` is a filled square, `b` a
    // stroked triangle.
    assert_eq!(font.specimen, None);
    let ink: Vec<(&str, bool)> = font
        .glyphs
        .iter()
        .map(|(code, mask)| (code.hex.as_str(), inked(mask) > 50))
        .collect();
    assert_eq!(ink, [("61", true), ("62", true)]);
}

#[test]
fn colour_space_swatches_are_the_colours_the_engine_paints() {
    let doc = gallery();

    let swatches = preview(&doc, GALLERY_INDEXED).unwrap();

    // `[/Indexed /DeviceRGB 1 <FF0000 0000FF>]`.
    assert_eq!(swatches, Preview::Swatches(vec![RED, BLUE]));
    // Smoke: a separation ramp gets one colour per tint, paper first.
    let Preview::Swatches(ramp) = preview(&doc, GALLERY_SEPARATION).unwrap() else {
        panic!("not swatches");
    };
    assert_eq!((ramp.len(), ramp[0]), (9, [0xFF; 3]));
}

#[test]
fn an_image_mask_is_drawn_on_a_transparent_background() {
    let doc = gallery();

    let page = picture(&doc, GALLERY_MASK);

    // `0x0F` with the default `/Decode [0 1]`: the four 0 samples paint the (black) fill colour,
    // the four 1 samples leave the background clear.
    assert_eq!((page.width, page.height), (8, 1));
    let alpha: Vec<u8> = page.rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    assert_eq!(alpha, [0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
    // Smoke: the painted samples are black.
    assert_eq!(page.rgba[0..3], [0, 0, 0]);
}

/// A one-page file whose object 4 is `shading`.
fn with_shading(shading: &str) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Shading << /Sh1 4 0 R >> >> >>",
    );
    w.object(4, shading);
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("shading", w.into_output().unwrap().bytes).unwrap()
}

#[test]
fn an_axial_shading_is_a_bar_along_its_axis() {
    let horizontal = gallery();
    // The same red-to-blue shading, its axis running down from (50, 100) to (50, 0).
    let vertical = with_shading(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [50 100 50 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
    );

    let bars = [picture(&horizontal, GALLERY_SHADING), picture(&vertical, 4)];

    for bar in &bars {
        // As long as the preview side, an eighth of that deep.
        assert_eq!((bar.width, bar.height), (PREVIEW_SIDE, PREVIEW_SIDE / 8));
        let (left, right) = (at(bar, 0.01, 0.5), at(bar, 0.99, 0.5));
        assert!(left[0] > 240 && left[2] < 15, "left {left:?}");
        assert!(right[2] > 240 && right[0] < 15, "right {right:?}");
    }
    // Smoke: the middle is half way.
    let middle = at(&bars[1], 0.5, 0.5);
    assert!((100..156).contains(&middle[0]), "middle {middle:?}");
}

#[test]
fn a_radial_shading_is_drawn_over_the_area_it_covers() {
    let doc = with_shading(
        "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 50 0 50 50 50] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >>",
    );

    let page = picture(&doc, 4);

    // The circles' box, square: red in the centre, blue at the rim.
    assert_eq!((page.width, page.height), (PREVIEW_SIDE, PREVIEW_SIDE));
    let centre = at(&page, 0.5, 0.5);
    assert!(centre[0] > 230 && centre[2] < 25, "centre {centre:?}");
    let rim = at(&page, 0.5, 0.01);
    assert!(rim[2] > 230 && rim[0] < 25, "rim {rim:?}");
}

#[test]
fn a_tiling_pattern_fills_a_few_tiles() {
    let doc = gallery();

    let page = picture(&doc, GALLERY_PATTERN);

    // Tiles of 10 points every 20 over 60 x 60 points: the first tile is at the bottom left.
    assert_eq!(
        [
            at(&page, 5.0 / 60.0, 55.0 / 60.0),
            at(&page, 15.0 / 60.0, 55.0 / 60.0)
        ],
        [RED, [0xFF; 3]]
    );
}

#[test]
fn a_page_preview_is_the_page_rendered_to_fit() {
    let doc = gallery();

    let page = picture(&doc, GALLERY_PAGE);

    let scale = PREVIEW_SIDE as f32 / 200.0;
    let direct = render_page(&doc, 0, &RenderSpec::new(scale)).unwrap();
    assert_eq!((page.width, page.height), (PREVIEW_SIDE, PREVIEW_SIDE));
    assert!(page.rgba == direct.rgba, "the preview differs from the page");
}
