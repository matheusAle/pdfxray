//! `pdfxray compare` against the `PDFium` reference engine. Runs when `cargo xtask pdfium fetch`
//! has installed `PDFium`; `PDFXRAY_PDFIUM=required` turns a missing library into a failure.

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

#[test]
fn compare_images_fixture_reports_no_difference() {
    if !pdfium_installed() {
        return;
    }
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("compare-images.png");

    let stdout = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("compare")
        .arg(pdfxray_fixtures::pdf_path("images"))
        .args(["0", "-o"])
        .arg(&out)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&stdout).unwrap_or_default();

    assert_eq!(json["differing_pixels"], 0);
    assert!(json["pdfium_version"].as_str().is_some_and(|v| !v.is_empty()));
    assert!(out.exists());
}

#[test]
fn compare_with_bad_library_path_fails_clearly() {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("compare")
        .arg(pdfxray_fixtures::pdf_path("images"))
        .args([
            "0",
            "-o",
            "/tmp/never.png",
            "--pdfium",
            "/nonexistent/libpdfium.dylib",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("loading PDFium"));
}
