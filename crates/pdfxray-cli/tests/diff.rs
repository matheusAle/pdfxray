//! End-to-end tests of `pdfxray diff`: output, scopes, exit statuses, determinism.

#![allow(clippy::unwrap_used, reason = "test helpers fail loudly on unexpected input")]

use std::path::PathBuf;

use assert_cmd::Command;
use pretty_assertions::assert_eq;
use serde_json::Value;

fn pdfxray() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
}

fn fixture(name: &str) -> PathBuf {
    pdfxray_fixtures::pdf_path(name)
}

/// Runs `pdfxray diff <a> <b> <args>`, checks the exit status and returns stdout.
fn diff(a: &str, b: &str, args: &[&str], status: i32) -> String {
    let out = pdfxray()
        .arg("diff")
        .arg(fixture(a))
        .arg(fixture(b))
        .args(args)
        .assert()
        .code(status)
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap()
}

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap()
}

/// Two tagged fixtures that differ: the same report, the second with a footnote added and the
/// figure taken out, so some elements pair, some move, and some are only on one side.
const A: &str = "reading-order";
const B: &str = "reading-order-footnote";

#[test]
fn a_file_against_itself_is_identical_and_exits_zero() {
    let out = json(&diff(A, A, &[], 0));
    assert_eq!(out["identical"], true);
    assert_eq!(out["pairs"], serde_json::json!([]));
    assert_eq!(out["pages"], serde_json::json!([]));
    let same = out["summary"]["same"].as_u64().unwrap();
    assert_eq!(same, out["sides"]["a"]["elements"].as_u64().unwrap());
    // Smoke: the fixture is tagged, so "identical" was decided over real pairs.
    assert!(same > 3, "{out}");
}

#[test]
fn different_files_exit_one_with_the_changes() {
    let text = diff(A, B, &["--pretty"], 1);
    let out = json(&text);
    assert_eq!(out["identical"], false);
    let pairs = out["pairs"].as_array().unwrap();
    assert_ne!(pairs.as_slice(), &[] as &[Value]);
    assert!(
        pairs.iter().all(|p| p["status"] != "same"),
        "unchanged pairs are hidden by default"
    );
    assert!(
        pairs
            .iter()
            .all(|p| p["kind"] == "element" && p["confidence"].is_object())
    );
    let statuses: Vec<&str> = pairs.iter().map(|p| p["status"].as_str().unwrap()).collect();
    assert!(
        ["moved", "added", "removed"].iter().all(|s| statuses.contains(s)),
        "{statuses:?}"
    );
    insta::assert_snapshot!(text);
}

#[test]
fn exit_zero_keeps_the_output_and_exits_zero() {
    let with = diff(A, B, &["--exit-zero"], 0);
    let without = diff(A, B, &[], 1);
    assert_eq!(with, without);
    assert_eq!(json(&with)["identical"], false);
}

#[test]
fn errors_exit_two_even_with_exit_zero() {
    pdfxray()
        .args(["diff", "/nonexistent/a.pdf"])
        .arg(fixture(A))
        .arg("--exit-zero")
        .assert()
        .code(2);
    let err = pdfxray()
        .arg("diff")
        .arg(fixture(A))
        .arg(fixture(B))
        .args(["--page", "99"])
        .assert()
        .code(2)
        .get_output()
        .stderr
        .clone();
    let err = String::from_utf8(err).unwrap();
    assert!(err.contains("page 99 out of range"), "{err}");
}

#[test]
fn include_unchanged_lists_every_pair() {
    let all = json(&diff(A, A, &["--include-unchanged"], 0));
    let pairs = all["pairs"].as_array().unwrap();
    assert_eq!(
        u64::try_from(pairs.len()).unwrap(),
        all["sides"]["a"]["elements"].as_u64().unwrap()
    );
    assert!(pairs.iter().all(|p| p["status"] == "same" && p["a"] == p["b"]));
    assert_ne!(all["pages"].as_array().unwrap().as_slice(), &[] as &[Value]);
}

#[test]
fn scopes_select_pairs_or_residue() {
    let ink = json(&diff(A, A, &["--scope", "ink", "--include-unchanged"], 0));
    assert_eq!(ink["scope"], "ink");
    assert!(
        ink.get("pairs").is_none() && ink.get("summary").is_none(),
        "{ink}"
    );
    let pages = ink["pages"].as_array().unwrap();
    assert!(!pages.is_empty() && pages.iter().all(|p| p["residue"].is_object()));

    let tags = json(&diff(A, A, &["--scope", "tags", "--include-unchanged"], 0));
    assert!(tags["pairs"].is_array());
    assert!(
        tags["pages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("residue").is_none())
    );

    // The default is `all`: pairs and residue.
    let all = json(&diff(A, A, &["--include-unchanged"], 0));
    assert_eq!(all["scope"], "all");
    assert!(all["pairs"].is_array() && all["pages"][0]["residue"].is_object());
}

#[test]
fn page_narrows_to_one_page_of_a() {
    let out = json(&diff(A, B, &["--page", "0", "--include-unchanged"], 1));
    assert_eq!(out["page"], 0);
    let pairs = out["pairs"].as_array().unwrap();
    assert_ne!(pairs.as_slice(), &[] as &[Value]);
    assert!(
        pairs
            .iter()
            .all(|p| p["page_a"] == 0 || (p.get("a").is_none() && p.get("page_b").is_some())),
        "{out}"
    );
    assert!(out["pages"].as_array().unwrap().iter().all(|p| p["a"] == 0));
}

#[test]
fn output_is_byte_identical_across_runs() {
    let first = diff(A, B, &["--include-unchanged"], 1);
    for _ in 0..3 {
        assert_eq!(diff(A, B, &["--include-unchanged"], 1), first);
    }
    assert!(first.len() > 100);
}

#[test]
fn an_encrypted_side_is_compared_on_structure_only_and_says_so() {
    let out = json(&diff("encrypted-stub", A, &[], 1));
    assert_eq!(out["sides"]["a"]["structure_only"], true);
    assert_eq!(out["sides"]["b"]["structure_only"], false);
    assert_eq!(
        out["notes"],
        serde_json::json!([{ "kind": "structure_only", "side": "a" }])
    );
}
