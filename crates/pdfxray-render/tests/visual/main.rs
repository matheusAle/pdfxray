//! Visual regression: page 1 of every fixture, plus pixel-exact placement checks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod compare;

use compare::{Tolerance, assert_snapshot, snapshot_dir};
use pdfxray_core::{Document, NodeId, Point, Rect};
use pdfxray_render::{
    ContentFilter, Overlay, PixelPoint, RenderSpec, RenderedPage, XrayOptions, ops_for, render_page,
    xray_overlays,
};
use pretty_assertions::assert_eq;

fn open(name: &str) -> Document {
    let path = pdfxray_fixtures::pdf_path(name);
    Document::open(path).unwrap()
}

fn pixel(page: &RenderedPage, p: PixelPoint) -> [u8; 4] {
    // Pixel centres are at +0.5, so truncating the continuous position picks the pixel it is in.
    let (x, y) = (p.x as usize, p.y as usize);
    let at = (y * page.width as usize + x) * 4;
    page.rgba[at..at + 4].try_into().unwrap()
}

/// The `CropBox` outline in magenta, 2 px wide.
fn crop_overlay(doc: &Document, page: u32) -> Overlay {
    Overlay::Rect {
        rect: doc.page(page).unwrap().crop_box,
        stroke: [255, 0, 255, 255],
        fill: None,
        width: 2.0,
    }
}

/// Fixtures with a committed baseline. A fixed list: fixtures added to the corpus later do
/// not break this test, they are added here together with their baseline.
const BASELINES: [&str; 10] = [
    "artifacts",
    "broken-xref-offsets",
    "form-xobject-nesting",
    "incremental-update",
    "minimal-one-page",
    "orphan-mcid",
    "page-boxes",
    "rolemap-custom",
    "tagged-simple",
    "xref-stream-objstm",
];

#[test]
fn page_one_of_the_baseline_fixtures_matches_its_baseline() {
    for name in BASELINES {
        let doc = open(name);
        let spec = RenderSpec {
            scale: 1.0,
            overlays: vec![crop_overlay(&doc, 0)],
            ..RenderSpec::new(1.0)
        };

        let out = render_page(&doc, 0, &spec).unwrap();

        assert_snapshot(&snapshot_dir(), &format!("{name}.page1"), &out, Tolerance::STRICT);
    }
    // Smoke: every listed fixture exists and has a baseline file.
    let on_disk = BASELINES
        .iter()
        .filter(|name| snapshot_dir().join(format!("{name}.page1.png")).is_file())
        .count();
    assert_eq!(on_disk, BASELINES.len());
}

/// Page-space points that sit on the centre line of a 1 pt stroke of `page-boxes`, with the
/// colour that stroke has (see the generator in `pdfxray-fixtures`).
const STROKES: [(u32, Point, [u8; 4]); 6] = [
    // Page 1 is rotated 90 degrees; its rectangles are green, blue and magenta.
    (0, Point::new(20.0, 150.0), [0, 153, 0, 255]),
    (0, Point::new(30.0, 150.0), [0, 0, 255, 255]),
    (0, Point::new(40.0, 150.0), [255, 0, 255, 255]),
    // Page 2 inherits a 300 x 400 MediaBox; its frame is black.
    (1, Point::new(10.0, 200.0), [0, 0, 0, 255]),
    // Page 3: black frame and a red square that sticks out of the MediaBox.
    (2, Point::new(10.0, 100.0), [0, 0, 0, 255]),
    (2, Point::new(150.0, 180.0), [255, 0, 0, 255]),
];

#[test]
fn page_boxes_strokes_land_where_the_transform_predicts() {
    let doc = open("page-boxes");

    for (page, at, colour) in STROKES {
        // At 4x a 1 pt stroke is four whole pixels wide, so its centre pixel is pure colour.
        let out = render_page(&doc, page, &RenderSpec::new(4.0)).unwrap();

        assert_eq!(
            pixel(&out, out.page_to_pixel.to_pixel(at)),
            colour,
            "page {page} at {at:?}"
        );
    }
    // Smoke: the rotated page maps (20, 150) to x = (150 - 10) * 4 and y = (20 - 10) * 4.
    let out = render_page(&doc, 0, &RenderSpec::new(4.0)).unwrap();
    assert_eq!(
        out.page_to_pixel.to_pixel(Point::new(20.0, 150.0)),
        PixelPoint::new(560.0, 40.0)
    );
}

#[test]
fn overlay_lands_where_the_transform_predicts() {
    let doc = open("page-boxes");
    let magenta = [255, 0, 255, 255];

    for page in 0..3 {
        // A 1 pt square at (60, 100): empty space on every page of the fixture.
        let spec = RenderSpec {
            scale: 4.0,
            overlays: vec![Overlay::Rect {
                rect: Rect::new(60.0, 100.0, 61.0, 101.0),
                stroke: magenta,
                fill: Some(magenta),
                width: 0.0,
            }],
            ..RenderSpec::new(4.0)
        };

        let out = render_page(&doc, page, &spec).unwrap();

        let centre = out.page_to_pixel.to_pixel(Point::new(60.5, 100.5));
        let beside = out.page_to_pixel.to_pixel(Point::new(80.5, 100.5));
        assert_eq!(pixel(&out, centre), magenta, "page {page} square");
        assert_eq!(
            pixel(&out, beside),
            [255, 255, 255, 255],
            "page {page} beside the square"
        );
        // And the pixel maps back into the square.
        let back = out.pixel_to_page(PixelPoint::new(centre.x.floor() + 0.5, centre.y.floor() + 0.5));
        assert!(
            Rect::new(60.0, 100.0, 61.0, 101.0).contains(back),
            "page {page}: {back:?}"
        );
    }
}

#[test]
fn mismatch_fails_and_writes_new_and_diff_images() {
    let doc = open("minimal-one-page");
    let plain = render_page(&doc, 0, &RenderSpec::new(1.0)).unwrap();
    let marked = render_page(
        &doc,
        0,
        &RenderSpec {
            scale: 1.0,
            overlays: vec![crop_overlay(&doc, 0)],
            ..RenderSpec::new(1.0)
        },
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("pdfxray-render-snap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    pdfxray_render::write_png(&plain, &dir.join("sample.png")).unwrap();

    let outcome = std::panic::catch_unwind(|| {
        assert_snapshot(&dir, "sample", &marked, Tolerance::STRICT);
    });

    let message = *outcome.unwrap_err().downcast::<String>().unwrap();
    assert!(message.contains("snapshot `sample` differs in"), "{message}");
    let new = image::open(dir.join("sample.new.png")).unwrap().to_rgba8();
    let diff = image::open(dir.join("sample.diff.png")).unwrap().to_rgba8();
    assert_eq!(new.as_raw(), &marked.rgba);
    // The top-left pixel lies on the overlay stroke, so the diff marks it red.
    assert_eq!(diff.get_pixel(0, 0).0, [255, 0, 0, 255]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The debug render modes, each with a committed baseline: x-ray of every painted item of
/// `graphics-state`, isolation of one glyph run of `text-state`, and the stepper halfway through
/// `graphics-state`. A fixed list, like [`BASELINES`].
const MODE_BASELINES: [(&str, &str); 3] = [
    ("xray-graphics-state", "graphics-state"),
    ("isolate-glyph-run-text-state", "text-state"),
    ("stepper-mid-graphics-state", "graphics-state"),
];

fn mode_spec(name: &str, doc: &Document) -> RenderSpec {
    match name {
        "xray-graphics-state" => RenderSpec {
            overlays: xray_overlays(
                doc,
                0,
                XrayOptions {
                    include_invisible: true,
                    ..XrayOptions::default()
                },
            ),
            ..RenderSpec::new(1.0)
        },
        // "Spaced out words": operator 11, set in Times at 18 pt with spacing and scaling.
        "isolate-glyph-run-text-state" => RenderSpec {
            content: ContentFilter::Only(ops_for(
                doc,
                &NodeId::Glyph {
                    page: 0,
                    op: 11,
                    glyph: 3,
                },
            )),
            ..RenderSpec::new(1.0)
        },
        // After the rotated squares, before the clipped shading (operator 58).
        "stepper-mid-graphics-state" => RenderSpec {
            content: ContentFilter::UpTo(43),
            ..RenderSpec::new(1.0)
        },
        other => panic!("no spec for {other}"),
    }
}

#[test]
fn debug_render_modes_match_their_baselines() {
    for (name, fixture) in MODE_BASELINES {
        let doc = open(fixture);

        let out = render_page(&doc, 0, &mode_spec(name, &doc)).unwrap();

        assert_snapshot(&snapshot_dir(), name, &out, Tolerance::STRICT);
    }
    // Smoke: every listed mode has a baseline file.
    let on_disk = MODE_BASELINES
        .iter()
        .filter(|(name, _)| snapshot_dir().join(format!("{name}.png")).is_file())
        .count();
    assert_eq!(on_disk, MODE_BASELINES.len());
}
