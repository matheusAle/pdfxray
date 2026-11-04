//! `pdfxray render` writes a PNG or JPEG of the page with the requested overlays and the View
//! menu's annotations.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> PathBuf {
    pdfxray_fixtures::pdf_path(name)
}

fn out_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name)
}

#[test]
fn render_writes_png_with_crop_size_times_scale() {
    let out = out_path("render-minimal.png");

    let stdout = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("render")
        .arg(fixture("minimal-one-page"))
        .args(["0", "--scale", "2", "--boxes", "-o"])
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(
        (json["width"].as_u64(), json["height"].as_u64()),
        (Some(400), Some(200))
    );
    let img = image::open(&out).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (400, 200));
    // Smoke: CropBox equals MediaBox here and is drawn last, so its colour is on the border.
    assert_eq!(img.get_pixel(0, 100).0, [40, 160, 40, 255]);
}

#[test]
fn highlight_without_geometry_fails() {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("render")
        .arg(fixture("minimal-one-page"))
        .args(["0", "--highlight", "obj:1:0", "-o"])
        .arg(out_path("render-bad.png"))
        .assert()
        .failure()
        .stderr(contains("`obj:1:0` has no geometry"));
}

#[expect(
    clippy::unwrap_used,
    reason = "test helper: an unreadable PNG must fail the test loudly"
)]
fn render_args(name: &str, extra: &[&str], out: &PathBuf) -> image::RgbaImage {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("render")
        .arg(fixture(name))
        .args(extra)
        .arg("-o")
        .arg(out)
        .assert()
        .success();
    image::open(out).unwrap().to_rgba8()
}

fn non_white(img: &image::RgbaImage) -> usize {
    img.pixels().filter(|p| p.0 != [255, 255, 255, 255]).count()
}

#[test]
fn hide_the_only_text_op_blanks_the_page_and_isolate_keeps_it() {
    let full = render_args("minimal-one-page", &["0"], &out_path("full.png"));
    let glyph_op = {
        let out = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
            .arg("hit")
            .arg(fixture("minimal-one-page"))
            .args(["0", "22", "52", "--layer", "content"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let hits: Vec<String> = serde_json::from_slice(&out).unwrap();
        hits.into_iter().find(|h| h.starts_with("op:")).unwrap()
    };

    let hidden = render_args(
        "minimal-one-page",
        &["0", "--hide", &glyph_op],
        &out_path("hidden.png"),
    );
    let isolated = render_args(
        "minimal-one-page",
        &["0", "--isolate", &glyph_op],
        &out_path("isolated.png"),
    );

    assert!(non_white(&full) > 0);
    assert_eq!(non_white(&hidden), 0);
    assert_eq!(isolated, full);
}

#[test]
fn xray_outlines_every_glyph() {
    let plain = render_args("minimal-one-page", &["0"], &out_path("plain.png"));
    let xray = render_args(
        "minimal-one-page",
        &["0", "--xray", "kind"],
        &out_path("xray.png"),
    );

    // Outlines are antialiased, so count blue-dominant pixels (glyph colour is [0, 90, 200]).
    let bluish = |img: &image::RgbaImage| {
        img.pixels()
            .filter(|p| i32::from(p.0[2]) > i32::from(p.0[0]) + 80 && p.0[2] > p.0[1])
            .count()
    };
    assert_eq!(bluish(&plain), 0);
    assert!(bluish(&xray) > 100, "only {} outline pixels", bluish(&xray));
}

#[test]
fn isolate_of_node_without_content_fails() {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("render")
        .arg(fixture("minimal-one-page"))
        .args(["0", "--isolate", "obj:1:0", "-o"])
        .arg(out_path("bad-isolate.png"))
        .assert()
        .failure()
        .stderr(contains("paints nothing"));
}

/// Pixels within 30 of `color` on every colour channel.
fn near(img: &image::RgbaImage, color: [u8; 3]) -> usize {
    img.pixels()
        .filter(|p| p.0.iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 30))
        .count()
}

/// The design's paragraph colour and the reading-order accent.
const PARAGRAPH: [u8; 3] = [0x5A, 0xA9, 0xFF];
const ACCENT: [u8; 3] = [0x3D, 0xD6, 0xF5];

#[test]
fn the_view_menu_flags_draw_tag_boxes_labels_and_the_reading_order() {
    let plain = render_args("reading-order", &["0"], &out_path("view-plain.png"));
    let boxes = render_args("reading-order", &["0", "--tags"], &out_path("view-tags.png"));
    let labelled = render_args(
        "reading-order",
        &["0", "--tags", "--labels"],
        &out_path("view-labels.png"),
    );
    let ordered = render_args(
        "reading-order",
        &["0", "--reading-order", "structure"],
        &out_path("view-order.png"),
    );
    let spoken = render_args(
        "reading-order",
        &["0", "--reading-order", "speech", "--speech-preset", "acrobat"],
        &out_path("view-speech.png"),
    );

    assert_eq!(near(&plain, PARAGRAPH), 0);
    assert!(near(&boxes, PARAGRAPH) > 100);
    // A chip is filled with its role colour, so labels add more of it.
    assert!(near(&labelled, PARAGRAPH) > near(&boxes, PARAGRAPH));
    assert!(near(&ordered, ACCENT) > 100);
    assert!(near(&spoken, ACCENT) > 100);
    // Smoke: without a View flag nothing of the annotation layer is drawn.
    assert_eq!(near(&plain, ACCENT), 0);
}

#[test]
fn nesting_and_artifacts_add_their_boxes() {
    let tags = render_args("artifacts", &["0", "--tags"], &out_path("view-art-off.png"));
    let artifacts = render_args(
        "artifacts",
        &["0", "--tags", "--artifacts"],
        &out_path("view-art-on.png"),
    );
    let nested = render_args(
        "tagged-simple",
        &["0", "--tags", "--nesting"],
        &out_path("view-nest.png"),
    );
    let flat = render_args("tagged-simple", &["0", "--tags"], &out_path("view-flat.png"));

    // The artifact class colour.
    assert!(near(&artifacts, [0x8A, 0x93, 0xA3]) > near(&tags, [0x8A, 0x93, 0xA3]));
    assert_ne!(nested, flat, "a container's box is drawn under --nesting");
    // Smoke: each render is the page's size.
    assert_eq!(artifacts.dimensions(), tags.dimensions());
}

#[test]
fn jpeg_output_follows_the_extension() {
    let out = out_path("render.jpeg");

    let img = render_args("minimal-one-page", &["0", "--scale", "1"], &out);

    assert_eq!(img.dimensions(), (200, 100));
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);
    // Smoke: a JPEG has no alpha; every pixel comes back opaque.
    assert!(img.pixels().all(|p| p.0[3] == 255));
}
