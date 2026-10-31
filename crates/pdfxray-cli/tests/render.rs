//! `pdfxray render` writes a PNG of the page with requested overlays.

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
