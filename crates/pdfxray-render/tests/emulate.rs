//! Viewer emulation: one page drawn with each viewer's pixel rounding, at one device pixel per
//! point so a point of the page is a pixel of the bitmap (y flipped: page y 30 is pixel row 70).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_possible_truncation)]

use pdfxray_core::Document;
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pdfxray_render::{Display, Emulation, RenderSpec, RenderedPage, Viewer, ViewerZoom, render_page};
use pretty_assertions::assert_eq;

/// A 100 × 100 pt page painting `content`, with Helvetica as `/F1` and a black 2 × 2 image as
/// `/Im1`.
fn page(content: &str) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> >>",
    );
    w.stream(4, "", content.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.stream(
        6,
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8",
        &[0, 0, 0, 0],
    );
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("emulate", w.into_output().unwrap().bytes).unwrap()
}

/// `doc`'s page at one pixel per point, rounded as `viewer` rounds.
fn draw(doc: &Document, viewer: Option<Viewer>) -> RenderedPage {
    let spec = RenderSpec {
        viewer,
        ..RenderSpec::new(1.0)
    };
    render_page(doc, 0, &spec).unwrap()
}

/// The grey level of pixel `(x, y)`: 0 black, 255 white.
fn grey(page: &RenderedPage, x: u32, y: u32) -> u8 {
    page.rgba[((y * page.width + x) * 4) as usize]
}

/// The grey levels of pixels `x0..x1` of row `y`.
fn row(page: &RenderedPage, y: u32, xs: std::ops::Range<u32>) -> Vec<u8> {
    xs.map(|x| grey(page, x, y)).collect()
}

/// The grey levels of pixels `y0..y1` of column `x`.
fn column(page: &RenderedPage, x: u32, ys: std::ops::Range<u32>) -> Vec<u8> {
    ys.map(|y| grey(page, x, y)).collect()
}

#[test]
fn acrobat_adjusts_a_thin_stroke_to_a_whole_pixel_and_pdfium_does_not() {
    // A 0.5 pt line whose centre is at pixel row 49.7: drawn at least a pixel wide, it covers
    // 49.2 .. 50.2 unless something moves it.
    let doc = page("0 G 0.5 w 10 50.3 m 90 50.3 l S");

    let own = draw(&doc, None);
    let chrome = draw(&doc, Some(Viewer::Chrome));
    let acrobat = draw(&doc, Some(Viewer::Acrobat));

    // Acrobat centres it on row 49: one black row, nothing below.
    assert_eq!(column(&acrobat, 50, 48..52), [255, 0, 255, 255]);
    // pdfxray and PDFium leave it across two rows, grey.
    let spread = column(&own, 50, 48..52);
    assert!(
        spread[1] > 0 && spread[1] < 128 && spread[2] > 128 && spread[2] < 255,
        "{spread:?}"
    );
    assert_eq!(column(&chrome, 50, 48..52), spread);
}

#[test]
fn pdfium_grows_a_rectangular_clip_out_to_whole_pixels() {
    // A black page clipped to x 20.4 .. 50.4, rows 70 .. 90.
    let doc = page("q 20.4 10 30 20 re W n 0 g 0 0 100 100 re f Q");

    let own = draw(&doc, None);
    let chrome = draw(&doc, Some(Viewer::Chrome));
    let acrobat = draw(&doc, Some(Viewer::Acrobat));

    // PDFium clips to 20 .. 51: both edge columns are solid.
    assert_eq!(row(&chrome, 80, 19..22), [255, 0, 0]);
    assert_eq!(row(&chrome, 80, 49..52), [0, 0, 255]);
    // pdfxray covers 60 % of column 20 and 40 % of column 50.
    let left = row(&own, 80, 19..22);
    let right = row(&own, 80, 49..52);
    assert!(
        left[0] == 255 && left[1] > 64 && left[1] < 160 && left[2] == 0,
        "{left:?}"
    );
    assert!(
        right[0] == 0 && right[1] > 96 && right[1] < 192 && right[2] == 255,
        "{right:?}"
    );
    // Smoke: Acrobat does not snap a clip half a pixel from the grid.
    assert_eq!(row(&acrobat, 80, 19..22), left);
}

#[test]
fn pdfium_puts_an_axis_aligned_image_on_whole_pixels() {
    // A black image over x 60.3 .. 90.2: PDFium makes it 30 px from 60, so column 90 is empty.
    let doc = page("q 29.9 0 0 10 60.3 20 cm /Im1 Do Q");

    let own = draw(&doc, None);
    let chrome = draw(&doc, Some(Viewer::Chrome));

    assert_eq!(row(&chrome, 75, 59..62), [255, 0, 0]);
    assert_eq!(row(&chrome, 75, 89..91), [0, 255]);
    // pdfxray draws column 60 (70 % covered) and column 90 (20 % covered) in part.
    let own_edges = [grey(&own, 60, 75), grey(&own, 90, 75)];
    assert_ne!(own_edges, [0, 255], "pdfxray leaves the image where it is");
    // Smoke: the image's rows are whole in both, 70 .. 80.
    assert_eq!(column(&chrome, 75, 69..81).first(), Some(&255));
    assert_eq!(column(&chrome, 75, 70..80), vec![0; 10]);
}

/// `I` drawn one glyph at a time at each of `xs`, baseline 30: what a viewer's rounded run
/// should look like.
fn glyphs_at(xs: &[u32]) -> RenderedPage {
    let shows = xs
        .iter()
        .map(|x| format!("BT /F1 20 Tf {x} 30 Td (I) Tj ET"))
        .collect::<Vec<_>>()
        .join(" ");
    draw(&page(&shows), None)
}

#[test]
fn chrome_rounds_each_glyph_and_acrobat_rounds_each_advance() {
    // Four `I`s from x 10.4, 30.3, 5.4 pt apart (Helvetica's I is 5.56 pt at 20 pt, less 0.16
    // of character spacing): at 10.4, 15.8, 21.2, 26.6.
    let doc = page("BT /F1 20 Tf -0.16 Tc 10.4 30.3 Td (IIII) Tj ET");

    let chrome = draw(&doc, Some(Viewer::Chrome));
    let edge = draw(&doc, Some(Viewer::Edge));
    let acrobat = draw(&doc, Some(Viewer::Acrobat));

    // PDFium rounds each origin: 10, 16, 21, 27.
    assert_eq!(chrome.rgba, glyphs_at(&[10, 16, 21, 27]).rgba);
    // Acrobat starts on the grid and adds the rounded advance, 5, each time: 10, 15, 20, 25.
    assert_eq!(acrobat.rgba, glyphs_at(&[10, 15, 20, 25]).rgba);
    // Edge is Chromium's viewer.
    assert_eq!(edge.rgba, chrome.rgba);
    // Smoke: pdfxray's own run is neither.
    let own = draw(&doc, None);
    assert_ne!(own.rgba, chrome.rgba);
    assert_ne!(own.rgba, acrobat.rgba);
}

#[test]
fn an_emulated_display_draws_the_page_at_its_device_pixels() {
    let doc = page("0 g 10 10 80 80 re f");
    let emulation = Emulation {
        viewer: Some(Viewer::Chrome),
        display: Display {
            width: 1920,
            height: 1080,
            dpi: 144,
        },
        zoom: ViewerZoom::Percent(125),
    };
    let scale = emulation.scale_for((100.0, 100.0));

    let drawn = render_page(
        &doc,
        0,
        &RenderSpec {
            viewer: emulation.viewer,
            ..RenderSpec::new(scale)
        },
    )
    .unwrap();

    // 125 % at 150 % scaling: 2.5 device pixels per point, a 250 px page.
    assert!((scale - 2.5).abs() < f32::EPSILON, "{scale}");
    assert_eq!((drawn.width, drawn.height), (250, 250));
    assert_eq!(grey(&drawn, 125, 125), 0);
    // Smoke: the margin is white.
    assert_eq!(grey(&drawn, 10, 10), 255);
}
