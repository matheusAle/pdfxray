//! Bitmap size, page <-> pixel transform and error handling over every generated fixture.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use pdfxray_core::{Document, EngineError, PageIx, Point, Rect};
use pdfxray_render::{
    MAX_SIDE, Overlay, PixelPoint, RenderError, RenderSpec, RenderedPage, page_dimensions, render_page,
    write_png,
};
use pretty_assertions::assert_eq;

fn open(name: &str) -> Document {
    let path = pdfxray_fixtures::pdf_path(name);
    Document::open(path).unwrap()
}

/// Fixtures the engine can render, i.e. everything except the encrypted stub.
fn renderable() -> Vec<(&'static str, Document)> {
    pdfxray_fixtures::all()
        .iter()
        .filter(|f| f.name != "encrypted-stub")
        .map(|f| (f.name, open(f.name)))
        .collect()
}

fn pages(doc: &Document) -> impl Iterator<Item = PageIx> + use<> {
    0..u32::try_from(doc.page_count()).unwrap()
}

/// Expected bitmap size: crop size times scale, truncated, width and height swapped for
/// quarter turns.
fn expected_size(doc: &Document, page: PageIx, scale: f32) -> (u32, u32) {
    let info = doc.page(page).unwrap();
    let (w, h) = (info.crop_box.width(), info.crop_box.height());
    let (w, h) = if matches!(info.rotate, 90 | 270) {
        (h, w)
    } else {
        (w, h)
    };
    let trunc = |v: f64| (v * f64::from(scale)).floor() as u32;
    (trunc(w), trunc(h))
}

fn render(doc: &Document, page: PageIx, scale: f32) -> RenderedPage {
    render_page(doc, page, &RenderSpec::new(scale)).unwrap()
}

#[test]
fn every_fixture_page_renders_at_crop_size_times_scale() {
    let mut rendered = 0;
    let mut expected_renders = 0;
    for (name, doc) in renderable() {
        expected_renders += doc.page_count() * 2;
        assert!(doc.page_count() > 0, "{name} has no pages");
        for page in pages(&doc) {
            for scale in [1.0, 2.0] {
                let out = render(&doc, page, scale);

                assert_eq!(
                    (out.width, out.height),
                    expected_size(&doc, page, scale),
                    "{name} page {page} at {scale}x"
                );
                assert_eq!(
                    out.rgba.len(),
                    out.width as usize * out.height as usize * 4,
                    "{name} page {page}"
                );
                rendered += 1;
            }
        }
    }
    // Smoke: every page of every renderable fixture was rendered at both scales, and the set
    // has not shrunk below the fixtures this crate was written against.
    assert_eq!(rendered, expected_renders);
    assert!(rendered >= (10 + 2) * 2, "{rendered}");
}

#[test]
fn rotated_page_swaps_width_and_height() {
    let doc = open("page-boxes");

    // Page 1: CropBox 380 x 280, /Rotate 90.
    assert_eq!(page_dimensions(&doc, 0), Ok((280.0, 380.0)));
    let out = render(&doc, 0, 2.0);
    assert_eq!((out.width, out.height), (560, 760));
    // Page 2: MediaBox 300 x 400 inherited, no rotation.
    assert_eq!(page_dimensions(&doc, 1), Ok((300.0, 400.0)));
}

#[test]
fn page_point_round_trips_through_pixels() {
    for (name, doc) in renderable() {
        for page in pages(&doc) {
            let crop: Rect = doc.page(page).unwrap().crop_box;
            for scale in [1.0, 2.0] {
                let out = render(&doc, page, scale);
                let probes = [
                    Point::new(crop.x0, crop.y0),
                    Point::new(crop.x1, crop.y1),
                    Point::new(crop.x0, crop.y1),
                    Point::new(crop.x0.midpoint(crop.x1), crop.y0.midpoint(crop.y1)),
                    Point::new(crop.x0 + 3.25, crop.y0 + 7.5),
                ];
                for p in probes {
                    let back = out.pixel_to_page(out.page_to_pixel.to_pixel(p));

                    assert!(
                        (back.x - p.x).abs() < 1e-6 && (back.y - p.y).abs() < 1e-6,
                        "{name} page {page} {scale}x: {p:?} came back as {back:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn crop_box_corners_land_on_the_bitmap_corners() {
    let doc = open("page-boxes");
    let crop = doc.page(0).unwrap().crop_box;
    let out = render(&doc, 0, 1.0);

    // /Rotate 90 turns the page clockwise: the bottom-left corner moves to the top-left.
    let corners = [
        (Point::new(crop.x0, crop.y0), PixelPoint::new(0.0, 0.0)),
        (
            Point::new(crop.x0, crop.y1),
            PixelPoint::new(f64::from(out.width), 0.0),
        ),
        (
            Point::new(crop.x1, crop.y1),
            PixelPoint::new(f64::from(out.width), f64::from(out.height)),
        ),
        (
            Point::new(crop.x1, crop.y0),
            PixelPoint::new(0.0, f64::from(out.height)),
        ),
    ];
    for (page_point, pixel) in corners {
        assert_eq!(out.page_to_pixel.to_pixel(page_point), pixel);
    }
    // Smoke: scale 1 and no translation left over, only the rotation and crop offset.
    assert_eq!(out.page_to_pixel.coeffs(), [0.0, 1.0, 1.0, 0.0, -10.0, -10.0]);
}

#[test]
fn unrotated_page_flips_y_and_offsets_by_the_crop_box() {
    let doc = open("minimal-one-page");
    let out = render(&doc, 0, 2.0);

    assert_eq!(out.page_to_pixel.coeffs(), [2.0, 0.0, 0.0, -2.0, 0.0, 200.0]);
    assert_eq!(
        out.page_to_pixel.to_pixel(Point::new(0.0, 0.0)),
        PixelPoint::new(0.0, 200.0)
    );
}

#[test]
fn overlays_do_not_change_the_bitmap_size() {
    let doc = open("minimal-one-page");
    let plain = render(&doc, 0, 1.0);
    let spec = RenderSpec {
        scale: 1.0,
        overlays: vec![Overlay::Rect {
            rect: Rect::new(10.0, 10.0, 50.0, 50.0),
            stroke: [255, 0, 0, 255],
            fill: Some([0, 0, 255, 128]),
            width: 2.0,
        }],
        ..RenderSpec::new(1.0)
    };

    let marked = render_page(&doc, 0, &spec).unwrap();

    assert_eq!((marked.width, marked.height), (plain.width, plain.height));
    assert_eq!(marked.page_to_pixel, plain.page_to_pixel);
    assert!(marked.rgba != plain.rgba, "the overlay must change pixels");
}

#[test]
fn encrypted_document_is_refused() {
    let doc = open("encrypted-stub");

    assert_eq!(
        render_page(&doc, 0, &RenderSpec::new(1.0)),
        Err(RenderError::Engine(EngineError::Encrypted))
    );
    assert_eq!(
        RenderError::Engine(EngineError::Encrypted).to_string(),
        "the document cannot be rendered (encrypted documents are not supported)"
    );
}

#[test]
fn bad_requests_are_reported() {
    let doc = open("minimal-one-page");
    let attempt = |page, scale| render_page(&doc, page, &RenderSpec::new(scale)).map(|_| ());

    assert_eq!(
        attempt(1, 1.0),
        Err(RenderError::NoSuchPage { page: 1, count: 1 })
    );
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(attempt(0, scale), Err(RenderError::InvalidScale), "scale {scale}");
    }
    // 200 pt * 0.001 truncates to an empty bitmap.
    assert_eq!(
        attempt(0, 0.001),
        Err(RenderError::BadSize { width: 0, height: 0 })
    );
    let too_big = MAX_SIDE as f32;
    assert_eq!(
        attempt(0, too_big),
        Err(RenderError::BadSize {
            width: 200 * MAX_SIDE,
            height: 100 * MAX_SIDE
        })
    );
}

#[test]
fn png_round_trips_the_pixels() {
    let doc = open("page-boxes");
    let out = render(&doc, 1, 1.0);
    let path = std::env::temp_dir().join(format!("pdfxray-render-{}.png", std::process::id()));

    write_png(&out, &path).unwrap();
    let decoded = image::open(&path).unwrap().to_rgba8();
    std::fs::remove_file(&path).unwrap();

    assert_eq!((decoded.width(), decoded.height()), (out.width, out.height));
    assert_eq!(decoded.into_raw(), out.rgba);
}

#[test]
fn png_write_failure_names_the_path() {
    let doc = open("minimal-one-page");
    let out = render(&doc, 0, 1.0);
    let path = std::env::temp_dir()
        .join("pdfxray-render-missing-dir")
        .join("x.png");

    let err = write_png(&out, &path).unwrap_err();

    assert!(
        matches!(&err, RenderError::Write { path: p, .. } if *p == path),
        "{err:?}"
    );
    assert!(err.to_string().starts_with("cannot write "), "{err}");
}
