//! `pdfxray validate` against a real veraPDF install. Runs when the install cached by the
//! pdfxray-verapdf tests exists; `PDFXRAY_VERAPDF=required` turns a missing install into a failure.

use std::path::PathBuf;

use assert_cmd::Command;
use pretty_assertions::assert_eq;
use serde_json::Value;

fn cached_launcher() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/verapdf-test-install");
    let version_dir = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .max()?;
    let launcher = version_dir.join("verapdf");
    launcher.exists().then_some(launcher)
}

#[expect(clippy::print_stderr, reason = "tells the developer why the test was skipped")]
fn launcher_or_skip() -> Option<PathBuf> {
    let found = cached_launcher();
    if found.is_none() {
        assert!(
            std::env::var("PDFXRAY_VERAPDF").as_deref() != Ok("required"),
            "PDFXRAY_VERAPDF=required but no cached veraPDF install; run the pdfxray-verapdf tests first"
        );
        eprintln!("SKIPPED: no cached veraPDF install");
    }
    found
}

#[test]
fn validate_maps_orphan_mcid_failure_to_marked_content() {
    let Some(launcher) = launcher_or_skip() else {
        return;
    };
    let file = pdfxray_fixtures::pdf_path("orphan-mcid");

    let out = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("validate")
        .arg(&file)
        .arg("--verapdf")
        .arg(&launcher)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&out).unwrap_or(Value::Null);

    assert_eq!(json["compliant"], Value::Bool(false));
    let targets: Vec<(&str, &str)> = json["findings"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|f| {
                    (
                        f["rule"].as_str().unwrap_or(""),
                        f["target"].as_str().unwrap_or(""),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        targets.contains(&("ISO 14289-1:2014 7.1-3", "mc:0:1")),
        "{targets:?}"
    );
    assert!(json["profile"].as_str().is_some_and(|p| p.contains("PDF/UA-1")));
}

#[test]
fn validate_without_verapdf_explains_how_to_get_it() {
    let file = pdfxray_fixtures::pdf_path("minimal-one-page");

    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg("validate")
        .arg(&file)
        .env("PATH", "/nonexistent")
        .env("PDFXRAY_DATA_DIR", env!("CARGO_TARGET_TMPDIR"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("veraPDF"));
}
