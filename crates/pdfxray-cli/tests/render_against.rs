//! `pdfxray render a.pdf --against b.pdf`: the visual diff of two files, side by side, overlaid
//! or as a mask, in status colours with the role on each chip.

#![allow(clippy::unwrap_used, reason = "test helpers fail loudly on unexpected input")]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::Value;

fn pdfxray() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
}

fn out_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name)
}

/// `render <pair>.a.pdf <page> --against <pair>.b.pdf <extra> -o <out>`: its JSON and the image.
fn against(pair: &str, page: &str, extra: &[&str], out: &str) -> (Value, image::RgbaImage) {
    let (a, b) = pdfxray_fixtures::pairs::pair(pair);
    let out = out_path(out);
    let stdout = pdfxray()
        .arg("render")
        .arg(a)
        .args([page, "--scale", "1", "--against"])
        .arg(b)
        .args(extra)
        .arg("-o")
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    (
        serde_json::from_slice(&stdout).unwrap(),
        image::open(&out).unwrap().to_rgba8(),
    )
}

/// Pixels within 40 of `color` on every channel.
fn near(img: &image::RgbaImage, color: [u8; 4]) -> usize {
    img.pixels()
        .filter(|p| p.0.iter().zip(color).take(3).all(|(a, b)| a.abs_diff(b) <= 40))
        .count()
}

/// [`near`] over the top `rows` rows only: the pages, above the legend's band.
fn near_above(img: &image::RgbaImage, rows: u32, color: [u8; 4]) -> usize {
    let pages = image::imageops::crop_imm(img, 0, 0, img.width(), rows).to_image();
    near(&pages, color)
}

const MOVED: [u8; 4] = [0x8B, 0x5C, 0xF6, 255];
const MODIFIED: [u8; 4] = [0xF5, 0x9E, 0x0B, 255];
const ADDED: [u8; 4] = [0x22, 0xC5, 0x5E, 255];
const REMOVED: [u8; 4] = [0xEF, 0x44, 0x44, 255];

#[test]
fn side_by_side_is_the_default_and_puts_both_pages_in_one_wide_bitmap() {
    let (json, img) = against("renumber-objects", "0", &[], "against-side.png");

    assert_eq!(json["mode"], "side-by-side");
    assert_eq!(json["against"]["page"], 0);
    let (w, h) = (json["width"].as_u64().unwrap(), json["height"].as_u64().unwrap());
    assert_eq!((u64::from(img.width()), u64::from(img.height())), (w, h));
    // Two 300 pt pages and a 24-pixel gutter, at scale 1.
    assert_eq!(w, 300 + 24 + 300);
    assert_eq!(json["summary"], serde_json::json!({ "same": 6 }));
    // Smoke: two pages of one size are on one grid even side by side, so they are scored, and
    // renumbered objects draw the same ink.
    assert_eq!(json["differing_pixels"], 0);
    assert_eq!(json["visual"]["score"]["anti_aliased"], 0);
}

#[test]
fn an_overlay_of_identical_ink_differs_nowhere() {
    let (json, img) = against(
        "rewrite-content-same-ink",
        "0",
        &["--mode", "overlay"],
        "against-same.png",
    );

    assert_eq!(json["mode"], "overlay");
    assert_eq!(json["differing_pixels"], 0);
    // Two files by one engine: compared perceptually, not by a channel tolerance.
    assert_eq!(json["tolerance"], Value::Null);
    assert_eq!(
        json["visual"]["settings"],
        serde_json::json!({ "scale": 1.0, "threshold": 0.1, "include_aa": false })
    );
    // Smoke: nothing changed, so no status colour is drawn on the page (400 pt tall at scale
    // 1); the legend below it still shows the swatches.
    assert_eq!(near_above(&img, 400, MOVED), 0);
    assert!(near(&img, MOVED) > 0);
}

#[test]
fn a_moved_paragraph_is_outlined_in_the_moved_colour_and_named_by_its_role() {
    let (json, img) = against(
        "move-paragraph-3pt",
        "1",
        &["--mode", "overlay", "--labels"],
        "against-moved.png",
    );
    let (_, unlabelled) = against(
        "move-paragraph-3pt",
        "1",
        &["--mode", "overlay"],
        "against-moved-plain.png",
    );

    assert_eq!(json["summary"]["moved"], 1);
    assert!(json["differing_pixels"].as_u64().unwrap() > 0);
    assert!(near(&img, MOVED) > 100, "{} moved pixels", near(&img, MOVED));
    // The chip is filled with the status colour, so the labelled render has more of it.
    assert!(near(&img, MOVED) > near(&unlabelled, MOVED));
    // Smoke: nothing was modified, so the modified colour is only the legend's swatch.
    assert!(near(&img, MODIFIED) < 200, "{}", near(&img, MODIFIED));
}

#[test]
fn registration_is_opt_in_labelled_and_removes_the_global_offset() {
    let (raw, _) = against("global-shift-2pt", "0", &["--mode", "mask"], "against-shift.png");
    let (registered, _) = against(
        "global-shift-2pt",
        "0",
        &["--mode", "mask", "--register"],
        "against-shift-registered.png",
    );

    assert!(raw["differing_pixels"].as_u64().unwrap() > 1000);
    assert_eq!(raw["registration"], Value::Null);
    assert_eq!(registered["differing_pixels"], 0);
    assert_eq!(
        (
            registered["registration"]["dx"].as_f64(),
            registered["registration"]["dy"].as_f64()
        ),
        (Some(2.0), Some(-2.0))
    );
    assert_eq!(
        registered["warnings"],
        serde_json::json!(["B registered by -2.0, +2.0 pt"])
    );
    // Smoke: the image is the size of one page, the mask of two on one grid.
    assert_eq!(registered["width"], raw["width"]);
}

#[test]
fn a_mask_colours_ink_b_adds_and_removes_apart_and_its_legend_states_the_score() {
    let (json, img) = against("move-paragraph-3pt", "1", &["--mode", "mask"], "against-mask.png");

    let score = &json["visual"]["score"];
    let (added, removed) = (
        score["ink_added"].as_u64().unwrap(),
        score["ink_removed"].as_u64().unwrap(),
    );
    assert!(added > 100 && removed > 100, "{score}");
    assert_eq!(json["differing_pixels"].as_u64(), Some(added + removed));
    // Both colours are on the page (400 pt tall at scale 1), not only in the legend.
    assert!(near_above(&img, 400, ADDED) > 100);
    assert!(near_above(&img, 400, REMOVED) > 100);
    // Smoke: the legend band below the page is taller for its score rows than the plain
    // overlay's.
    let (overlay, _) = against(
        "move-paragraph-3pt",
        "1",
        &["--mode", "overlay"],
        "against-mask-overlay.png",
    );
    assert!(json["height"].as_u64() > overlay["height"].as_u64());
}

#[test]
fn a_page_size_mismatch_is_refused_with_its_reason_and_scaled_only_on_request() {
    let (a, b) = (
        pdfxray_fixtures::pdf_path("minimal-one-page"),
        pdfxray_fixtures::pdf_path("tagged-simple"),
    );
    let run = |extra: &[&str]| {
        let mut cmd = pdfxray();
        cmd.arg("render")
            .arg(&a)
            .args(["0", "--scale", "1", "--against-page", "0", "--against"])
            .arg(&b)
            .args(["--mode", "overlay"])
            .args(extra)
            .arg("-o")
            .arg(out_path("against-mismatch.png"));
        cmd
    };

    run(&[]).assert().failure().stderr(contains(
        "the pages differ in size: A is 200.0 x 100.0 pt, B is 200.0 x 200.0 pt",
    ));
    let stdout = run(&["--scale-b-to-a"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&stdout).unwrap();

    assert_eq!(json["b_scale"], 0.5);
    assert_eq!(json["warnings"], serde_json::json!(["B scaled to A by 0.500"]));
    // Smoke: scaled onto A, the composite is A's page plus the legend band.
    assert_eq!(json["width"], 200);
}

#[test]
fn a_retagged_heading_reads_as_modified_on_both_sides_and_jpeg_is_written() {
    let (json, img) = against(
        "change-heading-level",
        "1",
        &["--labels", "--reading-order", "structure"],
        "against-heading.jpg",
    );

    assert_eq!(json["summary"]["modified"], 1);
    assert_eq!(json["output"], out_path("against-heading.jpg").to_str().unwrap());
    let jpeg = std::fs::read(out_path("against-heading.jpg")).unwrap();
    assert_eq!(&jpeg[..3], &[0xFF, 0xD8, 0xFF], "a JPEG starts with SOI");
    assert!(near(&img, MODIFIED) > 100);
    // Smoke: the reading-order badges are drawn in the accent colour.
    assert!(near(&img, [0x3D, 0xD6, 0xF5, 255]) > 100);
}

#[test]
fn what_a_comparison_cannot_show_is_refused_not_guessed() {
    let (a, b) = pdfxray_fixtures::pairs::pair("artifact-page-number");
    let fails = |extra: &[&str], message: &str| {
        pdfxray()
            .arg("render")
            .arg(&a)
            .arg("0")
            .args(extra)
            .arg("-o")
            .arg(out_path("against-refused.png"))
            .assert()
            .failure()
            .stderr(contains(message.to_owned()));
    };

    fails(
        &["--against", b.to_str().unwrap(), "--artifacts"],
        "artifacts are not paired yet",
    );
    fails(
        &["--against", b.to_str().unwrap(), "--xray", "kind"],
        "cannot be used with",
    );
    fails(&["--mode", "overlay"], "--mode needs --against");
    fails(&["--tolerance", "4"], "--tolerance needs --reference");
    fails(
        &["--against", b.to_str().unwrap(), "--tolerance", "4"],
        "two files are compared with --threshold",
    );
    fails(&["--threshold", "0.2"], "--threshold needs --against");
    // Smoke: an output name that is neither PNG nor JPEG is refused.
    pdfxray()
        .arg("render")
        .arg(&a)
        .args(["0", "-o"])
        .arg(out_path("never.svg"))
        .assert()
        .failure()
        .stderr(contains("must end in .png, .jpg or .jpeg"));
}
