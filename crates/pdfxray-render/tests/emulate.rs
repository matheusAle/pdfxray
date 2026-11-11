//! Viewer emulation: one page drawn with each viewer's pixel rounding, at one device pixel per
//! point so a point of the page is a pixel of the bitmap (y flipped: page y 30 is pixel row 70).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_possible_truncation)]

use pdfxray_core::Document;
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pdfxray_render::{
    AcrobatPrefs, Display, Emulation, RenderSpec, RenderedPage, Viewer, ViewerZoom, render_page,
};
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
        acrobat: AcrobatPrefs::default(),
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

/// The colour of pixel `(x, y)`.
fn rgb(page: &RenderedPage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * page.width + x) * 4) as usize;
    [page.rgba[i], page.rgba[i + 1], page.rgba[i + 2]]
}

/// `doc`'s page at one pixel per point as Acrobat draws it with `acrobat` preferences.
fn acrobat_with(doc: &Document, acrobat: AcrobatPrefs) -> RenderedPage {
    let spec = RenderSpec {
        viewer: Some(Viewer::Acrobat),
        acrobat,
        ..RenderSpec::new(1.0)
    };
    render_page(doc, 0, &spec).unwrap()
}

#[test]
fn pdfium_draws_a_fill_without_area_as_a_faint_one_pixel_line() {
    // A black fill there and back along row 49 (page y 50.5): it encloses nothing.
    let doc = page("0 g 10 50.5 m 90 50.5 l h f");
    // One there and back across the page, its bounds not flat: the same, on a slant.
    let slant = page("0 g 10 10 m 90 90 l h f");

    let chrome = draw(&doc, Some(Viewer::Chrome));
    let own = draw(&doc, None);
    let acrobat = draw(&doc, Some(Viewer::Acrobat));

    // A quarter of black over white, one row.
    let faint = grey(&chrome, 50, 49);
    assert!((180..=200).contains(&faint), "{faint}");
    assert_eq!(column(&chrome, 50, 47..52), [255, 255, faint, 255, 255]);
    // pdfxray (as pdf.js does) and Acrobat draw it as a solid hairline.
    assert_eq!(grey(&own, 50, 49), 0);
    assert_eq!(grey(&acrobat, 50, 49), 0);
    let on_slant = draw(&slant, Some(Viewer::Chrome));
    let darkest = (0..100).map(|y| grey(&on_slant, 50, y)).min().unwrap();
    assert!((150..=230).contains(&darkest), "faint on a slant too: {darkest}");
}

#[test]
fn acrobat_enhances_thin_lines_to_black_and_without_line_weights_thins_every_stroke() {
    // A red 0.5 pt line centred on row 49.7, and a blue 6 pt one centred on row 80.
    let doc = page("1 0 0 RG 0.5 w 10 50.3 m 90 50.3 l S 0 0 1 RG 6 w 10 20 m 90 20 l S");

    let started = acrobat_with(&doc, AcrobatPrefs::default());
    let plain = acrobat_with(
        &doc,
        AcrobatPrefs {
            enhance_thin_lines: false,
            line_weights: true,
        },
    );
    let no_weights = acrobat_with(
        &doc,
        AcrobatPrefs {
            enhance_thin_lines: false,
            line_weights: false,
        },
    );

    // As Acrobat starts, the thin red line is a solid black row; the thick one keeps 6 px.
    assert_eq!(rgb(&started, 50, 49), [0, 0, 0]);
    assert_eq!(rgb(&started, 50, 50), [255, 255, 255]);
    let blue = (74..=86).filter(|&y| rgb(&started, 50, y) == [0, 0, 255]).count();
    assert_eq!(blue, 6);
    // Without Enhance thin lines, the thin line keeps its red.
    assert_eq!(rgb(&plain, 50, 49), [255, 0, 0]);
    // Without Line Weights, the 6 pt line is one blue row.
    let thin_blue = (74..=86)
        .filter(|&y| rgb(&no_weights, 50, y) == [0, 0, 255])
        .count();
    assert_eq!(thin_blue, 1);
    // Smoke: Chrome keeps both lines in their colours, the thin one a grey-ish red pixel wide.
    let chrome = draw(&doc, Some(Viewer::Chrome));
    assert_ne!(rgb(&chrome, 50, 49), [0, 0, 0]);
    let chrome_blue = (74..=86).filter(|&y| rgb(&chrome, 50, y) == [0, 0, 255]).count();
    assert_eq!(chrome_blue, 6);
}

/// `JetBrains` Mono (OFL), a TrueType font with hinting instructions.
const MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

/// A 100 × 100 pt page painting `content` with `JetBrains` Mono embedded as `/F2`.
fn mono_page(content: &str) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
         /Resources << /Font << /F2 5 0 R >> >> >>",
    );
    w.stream(4, "", content.as_bytes());
    let widths = vec!["600"; 95].join(" ");
    w.object(
        5,
        format!(
            "<< /Type /Font /Subtype /TrueType /BaseFont /JetBrainsMono-Regular /FirstChar 32 \
             /LastChar 126 /Widths [{widths}] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>"
        ),
    );
    w.object(
        6,
        "<< /Type /FontDescriptor /FontName /JetBrainsMono-Regular /Flags 33 \
         /FontBBox [-1000 -300 1000 1020] /ItalicAngle 0 /Ascent 1020 /Descent -300 \
         /CapHeight 730 /StemV 80 /FontFile2 7 0 R >>",
    );
    w.stream(7, &format!("/Length1 {}", MONO.len()), MONO);
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("mono", w.into_output().unwrap().bytes).unwrap()
}

/// `doc`'s page at 96 DPI and 100 % (4/3 px a point), as `viewer` draws it.
fn at_96_dpi(doc: &Document, viewer: Option<Viewer>) -> RenderedPage {
    let spec = RenderSpec {
        viewer,
        ..RenderSpec::new(96.0 / 72.0)
    };
    render_page(doc, 0, &spec).unwrap()
}

/// Of the pixels text inks, the share only partly covered (mid greys): the blur of its edges.
fn blur(page: &RenderedPage) -> f64 {
    let greys: Vec<u8> = page.rgba.chunks(4).map(|px| px[0]).collect();
    let inked = greys.iter().filter(|g| **g < 250).count();
    let partial = greys.iter().filter(|g| (64..192).contains(*g)).count();
    #[allow(clippy::cast_precision_loss)]
    let share = partial as f64 / inked.max(1) as f64;
    share
}

#[test]
fn acrobat_and_chrome_hint_upright_text_so_its_stems_land_on_whole_pixels() {
    // 9 pt text at 96 DPI: 12 px an em, drawn from a fraction of a pixel.
    let doc = mono_page("BT /F2 9 Tf 10.3 50.4 Td (Hamburgefonstiv 0123) Tj ET");

    let own = at_96_dpi(&doc, None);
    let acrobat = at_96_dpi(&doc, Some(Viewer::Acrobat));
    let chrome = at_96_dpi(&doc, Some(Viewer::Chrome));

    // Hinted, fewer of the text's pixels are half covered: its edges are sharper.
    let (own_blur, acrobat_blur, chrome_blur) = (blur(&own), blur(&acrobat), blur(&chrome));
    assert!(
        acrobat_blur < own_blur * 0.9,
        "Acrobat {acrobat_blur:.3} vs pdfxray {own_blur:.3}"
    );
    assert!(
        chrome_blur < own_blur * 0.9,
        "Chrome {chrome_blur:.3} vs pdfxray {own_blur:.3}"
    );
    // Acrobat's font hints and Chrome's autohinter fit the glyphs differently.
    assert_ne!(acrobat.rgba, chrome.rgba);
    // Smoke: turned text is not hinted, so it is as blurry as pdfxray's own.
    let turned = mono_page("BT /F2 9 Tf 0.9 0.42 -0.42 0.9 20 40 Tm (Hamburgefonstiv) Tj ET");
    let (own_turned, acrobat_turned) = (
        blur(&at_96_dpi(&turned, None)),
        blur(&at_96_dpi(&turned, Some(Viewer::Acrobat))),
    );
    assert!(
        (acrobat_turned - own_turned).abs() < 0.05,
        "{acrobat_turned:.3} vs {own_turned:.3}"
    );
}
