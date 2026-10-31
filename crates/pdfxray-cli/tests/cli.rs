//! End-to-end tests of the `pdfxray` binary.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::{contains, starts_with};
use pretty_assertions::assert_eq;

fn write_temp(name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

fn pdfxray() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
}

#[test]
fn version_flag_prints_crate_version() {
    pdfxray()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("pdfxray {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn info_emits_json_with_header_facts() {
    let path = write_temp("cli-info.pdf", b"%PDF-1.7\n%%EOF\n").unwrap();

    let out = pdfxray()
        .arg("info")
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&out).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "name": "cli-info.pdf",
            "byte_len": 15,
            "header_version": "1.7",
            "header_offset": 0,
            "page_count": 0,
            "object_count": 0,
            "revision_count": json["revision_count"],
            "encrypted": false,
            "engine_error": "invalid",
            "diagnostics": json["diagnostics"],
        })
    );
}

#[test]
fn info_pretty_snapshot_with_diagnostic() {
    let path = write_temp("cli-garbage.pdf", b"junk\n%PDF-2.0\n%%EOF\n").unwrap();

    let out = pdfxray()
        .args(["info", "--pretty"])
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    insta::assert_snapshot!(String::from_utf8(out).unwrap());
}

#[test]
fn info_on_non_pdf_fails_with_context() {
    let path = write_temp("cli-not.pdf", b"hello world").unwrap();

    pdfxray()
        .arg("info")
        .arg(&path)
        .assert()
        .failure()
        .stdout("")
        .stderr(starts_with("error: opening ").and(contains("not a PDF")));
}
