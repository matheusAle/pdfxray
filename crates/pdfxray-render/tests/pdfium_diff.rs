//! hayro against `PDFium` over the generated fixtures.
//!
//! Needs the `PDFium` library: run `cargo xtask pdfium fetch` first. Without it every test
//! prints a skip message and passes; `PDFXRAY_PDFIUM=required` turns a missing library into a
//! failure (CI sets it). `PDFXRAY_DIFF_DIR=<dir>` also writes the two renders and the diff of
//! every fixture as PNGs for looking at.

#![cfg(feature = "pdfium")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use pdfxray_core::{Document, EngineError};
use pdfxray_fixtures::{Fixture, PdfVersion, PdfWriter, Trailer};
use pdfxray_render::pdfium::{PdfiumError, PdfiumRenderer};
use pdfxray_render::{Diff, RenderError, RenderSpec, RenderedPage, diff, render_page};
use pretty_assertions::assert_eq;

/// Per-channel difference that still counts as the same pixel: anti-aliasing and gamma
/// differences between the two rasterisers stay below this on edges.
const TOLERANCE: u8 = 48;

/// The library, or `None` (after saying so) when it is not installed and not required.
#[expect(clippy::print_stderr, reason = "tells the developer why the test was skipped")]
fn renderer() -> Option<PdfiumRenderer> {
    match PdfiumRenderer::load(None) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            assert!(
                std::env::var("PDFXRAY_PDFIUM").as_deref() != Ok("required"),
                "PDFXRAY_PDFIUM=required but PDFium is unavailable: {error}"
            );
            eprintln!("SKIPPED: {error}");
            None
        }
    }
}

fn open(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn hayro(doc: &Document, page: u32) -> RenderedPage {
    render_page(doc, page, &RenderSpec::new(1.0)).unwrap()
}

/// Fixtures both engines can open: everything except the encrypted stub.
fn fixtures() -> Vec<&'static Fixture> {
    pdfxray_fixtures::all()
        .iter()
        .filter(|f| f.name != "encrypted-stub")
        .collect()
}

fn dump(name: &str, hayro: &RenderedPage, pdfium: &RenderedPage, d: &Diff) {
    let Some(dir) = std::env::var_os("PDFXRAY_DIFF_DIR").map(PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let save = |suffix: &str, rgba: &[u8], w: u32, h: u32| {
        image::save_buffer(
            dir.join(format!("{name}.{suffix}.png")),
            rgba,
            w,
            h,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    };
    save("hayro", &hayro.rgba, hayro.width, hayro.height);
    save("pdfium", &pdfium.rgba, pdfium.width, pdfium.height);
    save("diff", &d.image, d.width, d.height);
}

/// Fixtures `PDFium` cannot render at all, with why. The test asserts this list is exact, so a
/// `PDFium` that starts recovering them (or a fixture that stops being broken) shows up.
///
/// `broken-xref-offsets` has every cross-reference offset shifted; hayro rebuilds the table by
/// scanning the file, `PDFium` opens the document but finds no page.
const PDFIUM_REJECTS: &[&str] = &["broken-xref-offsets"];

/// Renders page 0 of `name` with both engines at scale 1 and diffs them.
fn compare(renderer: &PdfiumRenderer, name: &str) -> (RenderedPage, RenderedPage, Diff) {
    let doc = open(name);
    let (ours, theirs) = (hayro(&doc, 0), renderer.render_page(&doc, 0, 1.0).unwrap());
    let d = diff(&ours, &theirs, TOLERANCE);
    dump(name, &ours, &theirs, &d);
    (ours, theirs, d)
}

fn blank(like: &RenderedPage) -> RenderedPage {
    RenderedPage {
        rgba: vec![255; like.rgba.len()],
        ..like.clone()
    }
}

#[test]
fn every_fixture_renders_with_the_same_geometry_and_prints_its_diff() {
    let Some(renderer) = renderer() else { return };
    eprintln!(
        "PDFium {} from {}",
        renderer.version(),
        renderer.library().display()
    );
    eprintln!(
        "{:<26} {:>9} {:>10} {:>9} {:>5} {:>8}",
        "fixture", "size", "differing", "fraction", "max", "mean"
    );
    let mut compared = 0;
    let mut rejected = Vec::new();
    for fixture in fixtures() {
        let doc = open(fixture.name);
        let theirs = match renderer.render_page(&doc, 0, 1.0) {
            Ok(theirs) => theirs,
            Err(e @ PdfiumError::Render(_)) => {
                eprintln!("{:<26} PDFium rejects the page: {e}", fixture.name);
                rejected.push(fixture.name);
                continue;
            }
            Err(e) => panic!("{}: {e}", fixture.name),
        };
        let ours = hayro(&doc, 0);

        assert_eq!(
            (theirs.width, theirs.height),
            (ours.width, ours.height),
            "{}",
            fixture.name
        );
        assert_eq!(theirs.page_to_pixel, ours.page_to_pixel, "{}", fixture.name);
        assert_eq!(
            theirs.rgba.len(),
            (ours.width * ours.height * 4) as usize,
            "{}",
            fixture.name
        );
        assert!(
            theirs.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255),
            "{} has transparent pixels",
            fixture.name
        );

        let d = diff(&ours, &theirs, TOLERANCE);
        dump(fixture.name, &ours, &theirs, &d);
        eprintln!(
            "{:<26} {:>9} {:>10} {:>8.3}% {:>5} {:>8.3}",
            fixture.name,
            format!("{}x{}", d.width, d.height),
            d.differing_pixels,
            d.differing_fraction() * 100.0,
            d.max_channel_delta,
            d.mean_delta
        );
        assert_eq!((d.width, d.height), (ours.width, ours.height), "{}", fixture.name);
        compared += 1;
    }
    assert_eq!(rejected, PDFIUM_REJECTS);
    // smoke: every fixture but the encrypted stub was either compared or listed as rejected
    assert_eq!(compared + rejected.len(), pdfxray_fixtures::all().len() - 1);
    assert!(compared >= 20, "only {compared} fixtures compared");
}

/// Share of pixels that may differ (beyond [`TOLERANCE`]) per fixture, with the measurement it
/// was derived from (macOS arm64, `PDFium` 158.0.8125.0).
///
/// * `minimal-one-page`: one line of Helvetica. The engines hint and anti-alias glyphs
///   differently (measured 0.82%); 5% leaves room for the fallback font a Linux CI machine
///   substitutes for Helvetica.
/// * `graphics-state`: shapes, strokes, a shading, alpha and Multiply, no text (measured 0.002%).
/// * `images`: sampled and masked images (measured 0%).
const SIMPLE: &[(&str, f64)] = &[
    ("minimal-one-page", 0.05),
    ("graphics-state", 0.001),
    ("images", 0.0),
];

#[test]
fn simple_fixtures_agree_within_their_documented_thresholds() {
    let Some(renderer) = renderer() else { return };
    for &(name, limit) in SIMPLE {
        let (ours, theirs, d) = compare(&renderer, name);

        assert!(
            d.differing_fraction() <= limit,
            "{name}: {:.4}% of pixels differ, limit {:.4}%",
            d.differing_fraction() * 100.0,
            limit * 100.0
        );
        // The limit must not be vacuous: both pages carry ink, and the diff against a blank
        // page is far larger than the diff between the two engines.
        let ink = |page: &RenderedPage| diff(page, &blank(page), TOLERANCE).differing_pixels;
        assert!(
            ink(&ours) > 100 && ink(&theirs) > 100,
            "{name} rendered (nearly) blank"
        );
        assert!(
            ink(&theirs) > 2 * d.differing_pixels,
            "{name}: engines differ as much as PDFium differs from white paper"
        );
    }
}

/// A one-page 100x100 file whose content draws the transparency-group form `/Fm`.
fn group_document(group: &str, form_content: &str, page_content: &str) -> Document {
    const GS: &str = "/ExtGState << /Mul << /BM /Multiply >> >>";
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
         /Resources << /XObject << /Fm 5 0 R >> >> >>",
    );
    w.stream(4, "", page_content.as_bytes());
    w.stream(
        5,
        &format!("/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Group << /S /Transparency {group} >> /Resources << {GS} >>"),
        form_content.as_bytes(),
    );
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("group.pdf", w.into_output().unwrap().bytes).unwrap()
}

fn pixel(page: &RenderedPage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * page.width + x) * 4) as usize;
    [page.rgba[i], page.rgba[i + 1], page.rgba[i + 2]]
}

/// hayro does not implement blending against the backdrop of a non-isolated group (its README
/// lists "blending and isolation"), `PDFium` does. Cyan multiplied onto a yellow backdrop is
/// green per the PDF specification; hayro leaves it cyan.
#[test]
fn hayro_gap_non_isolated_group_blend_is_visible_in_the_diff() {
    let Some(renderer) = renderer() else { return };
    let page = "1 1 0 rg 0 0 100 100 re f /Fm Do";
    let form = "/Mul gs 0 1 1 rg 20 20 60 60 re f";

    let gap = group_document("/I false", form, page);
    let (ours, theirs) = (hayro(&gap, 0), renderer.render_page(&gap, 0, 1.0).unwrap());
    let d = diff(&ours, &theirs, TOLERANCE);

    assert_eq!(
        pixel(&theirs, 50, 50),
        [0, 255, 0],
        "PDFium multiplies against the backdrop"
    );
    assert_eq!(
        pixel(&ours, 50, 50),
        [0, 255, 255],
        "hayro paints the group as if isolated"
    );
    // exactly the 60 x 60 square differs, and the diff marks it red
    assert_eq!(d.differing_pixels, 3600);
    assert_eq!(d.max_channel_delta, 255);
    let at = |x: usize, y: usize| &d.image[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4];
    assert_eq!(at(50, 50), [255, 0, 0, 255]);
    // outside the square nothing differs: the diff shows hayro's yellow as gray (Rec. 601 luma 225)
    assert_eq!(at(10, 10), [225, 225, 225, 255]);

    // control: the same drawing in an isolated group blends against nothing in both engines
    let control = group_document("/I true", form, page);
    let (ours, theirs) = (
        hayro(&control, 0),
        renderer.render_page(&control, 0, 1.0).unwrap(),
    );
    assert_eq!(diff(&ours, &theirs, TOLERANCE).differing_pixels, 0);
    assert_eq!(pixel(&theirs, 50, 50), [0, 255, 255]);
}

#[test]
fn renders_scale_with_the_page_and_reject_bad_input() {
    let Some(renderer) = renderer() else { return };
    let doc = open("minimal-one-page");

    let double = renderer.render_page(&doc, 0, 2.0).unwrap();
    assert_eq!((double.width, double.height), (400, 200));
    let hayro_double = render_page(&doc, 0, &RenderSpec::new(2.0)).unwrap();
    assert_eq!(double.page_to_pixel, hayro_double.page_to_pixel);
    assert_eq!(
        (hayro_double.width, hayro_double.height),
        (double.width, double.height)
    );

    assert_eq!(
        renderer.render_page(&doc, 0, 0.0).unwrap_err(),
        PdfiumError::Geometry(RenderError::InvalidScale)
    );
    assert_eq!(
        renderer.render_page(&doc, 0, f32::NAN).unwrap_err(),
        PdfiumError::Geometry(RenderError::InvalidScale)
    );
    assert_eq!(
        renderer.render_page(&doc, 7, 1.0).unwrap_err(),
        PdfiumError::Geometry(RenderError::NoSuchPage { page: 7, count: 1 })
    );
    let encrypted = open("encrypted-stub");
    assert_eq!(
        renderer.render_page(&encrypted, 0, 1.0).unwrap_err(),
        PdfiumError::Geometry(RenderError::Engine(EngineError::Encrypted))
    );
}

#[test]
fn loader_reports_version_and_checks_explicit_paths() {
    let Some(renderer) = renderer() else { return };

    let parts: Vec<&str> = renderer.version().split('.').collect();
    assert_eq!(parts.len(), 4, "{}", renderer.version());
    assert!(
        parts.iter().all(|p| p.parse::<u32>().is_ok()),
        "{}",
        renderer.version()
    );
    // the library bound first is the one a repeated load hands back
    let again = PdfiumRenderer::load(Some(renderer.library())).unwrap();
    assert_eq!(again.library(), renderer.library());
    assert_eq!(again.version(), renderer.version());
    // an explicit path is always checked, even when a library is already bound
    let missing = PathBuf::from("/definitely/not/pdfium");
    assert_eq!(
        PdfiumRenderer::load(Some(&missing)).unwrap_err(),
        PdfiumError::NotFound {
            searched: vec![missing]
        }
    );
    let other = std::env::temp_dir().join(format!("pdfxray-other-lib-{}", std::process::id()));
    std::fs::write(&other, b"not the library").unwrap();
    let error = PdfiumRenderer::load(Some(&other)).unwrap_err();
    std::fs::remove_file(&other).unwrap();
    assert_eq!(
        error,
        PdfiumError::AlreadyLoaded {
            loaded: renderer.library().to_path_buf(),
            requested: other
        }
    );
}
