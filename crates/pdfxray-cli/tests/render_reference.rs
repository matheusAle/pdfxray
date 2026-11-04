//! `pdfxray render --reference pdfium`: the page drawn by hayro compared with the `PDFium`
//! reference engine (what the deleted `compare` verb did, as `--mode mask`). Runs when
//! `cargo xtask pdfium fetch` has installed `PDFium`; `PDFXRAY_PDFIUM=required` turns a missing
//! library into a failure.

use std::path::PathBuf;

use assert_cmd::Command;
use pretty_assertions::assert_eq;

fn pdfium_installed() -> bool {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/pdfium");
    let found = std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some());
    assert!(
        found || std::env::var("PDFXRAY_PDFIUM").as_deref() != Ok("required"),
        "PDFXRAY_PDFIUM=required but PDFium is not installed; run `cargo xtask pdfium fetch`"
    );
    found
}

fn reference(out: &PathBuf, extra: &[&str]) -> serde_json::Value {
    let stdout = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("render")
        .arg(pdfxray_fixtures::pdf_path("images"))
        .args(["0", "--scale", "1", "--reference", "pdfium"])
        .args(extra)
        .arg("-o")
        .arg(out)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&stdout).unwrap_or_default()
}

#[test]
fn the_images_fixture_masks_to_no_difference_against_pdfium() {
    if !pdfium_installed() {
        return;
    }
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("reference-images.png");

    let json = reference(&out, &[]);

    assert_eq!(json["mode"], "mask");
    assert_eq!(json["differing_pixels"], 0);
    assert_eq!(json["tolerance"], 48);
    assert!(json["pdfium_version"].as_str().is_some_and(|v| !v.is_empty()));
    // Smoke: the mask was written at the page's size.
    let img = image::open(&out).map(|i| i.to_rgba8());
    assert!(img.is_ok_and(|i| (u64::from(i.width()), u64::from(i.height()))
        == (
            json["width"].as_u64().unwrap_or(0),
            json["height"].as_u64().unwrap_or(0)
        )));
}

#[test]
fn side_by_side_against_pdfium_puts_both_engines_in_one_bitmap() {
    if !pdfium_installed() {
        return;
    }
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("reference-side.png");

    let mask = reference(
        &PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("reference-mask.png"),
        &[],
    );
    let side = reference(&out, &["--mode", "side-by-side"]);

    assert_eq!(side["mode"], "side-by-side");
    assert!(side["width"].as_u64() > mask["width"].as_u64().map(|w| w * 2));
    // Smoke: nothing is compared pixel by pixel side by side.
    assert_eq!(side["differing_pixels"], serde_json::Value::Null);
}

#[test]
fn a_bad_library_path_fails_clearly() {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("render")
        .arg(pdfxray_fixtures::pdf_path("images"))
        .args([
            "0",
            "--reference",
            "pdfium",
            "-o",
            "/tmp/never.png",
            "--pdfium",
            "/nonexistent/libpdfium.dylib",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("loading PDFium"));
}

#[test]
fn the_compare_verb_is_gone() {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("compare")
        .arg(pdfxray_fixtures::pdf_path("images"))
        .args(["0", "-o", "/tmp/never.png"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unrecognized subcommand 'compare'"));
}
