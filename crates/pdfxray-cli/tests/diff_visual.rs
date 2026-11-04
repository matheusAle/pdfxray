//! `pdfxray diff --visual`: the visual score of each page pair beside its residue, opt-in,
//! never deciding the exit status.

#![allow(clippy::unwrap_used, reason = "test helpers fail loudly on unexpected input")]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn pdfxray() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
}

/// Runs `pdfxray diff <a> <b> <args>`, checks the exit status and returns stdout.
fn diff(a: &Path, b: &Path, args: &[&str], status: i32) -> String {
    let out = pdfxray()
        .arg("diff")
        .arg(a)
        .arg(b)
        .args(args)
        .assert()
        .code(status)
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap()
}

fn json_of(s: &str) -> Value {
    serde_json::from_str(s).unwrap()
}

fn pair(name: &str) -> (PathBuf, PathBuf) {
    pdfxray_fixtures::pairs::pair(name)
}

/// The visual scores of the pages listed, by A's page.
fn scores(out: &Value) -> Vec<(Value, Value)> {
    out["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["a"].clone(), p["visual"].clone()))
        .collect()
}

#[test]
fn without_visual_nothing_is_drawn_and_no_score_is_reported() {
    let (a, b) = pair("artifact-page-number");

    let out = json_of(&diff(&a, &b, &["--scope", "ink"], 1));

    assert_eq!(out["visual"], Value::Null);
    assert!(scores(&out).iter().all(|(_, v)| v.is_null()));
    // Smoke: the pages are listed, with their residue, just without a score.
    assert_eq!(out["pages"].as_array().unwrap().len(), 3);
    assert_eq!(out["pages"][0]["residue"]["only_a"], 6);
}

#[test]
fn a_changed_page_number_shows_its_residue_and_its_score_side_by_side() {
    let (a, b) = pair("artifact-page-number");

    let out = json_of(&diff(&a, &b, &["--visual", "--scope", "ink"], 1));

    let first = &out["pages"][0];
    // Structurally: every glyph of the footer.
    assert_eq!(
        first["residue"],
        json!({ "a": 6, "b": 5, "only_a": 6, "only_b": 5 })
    );
    // Visually: a few hundredths of a percent of the page.
    let visual = &first["visual"];
    assert!(visual["differing_pixels"].as_u64().unwrap() > 50, "{visual}");
    assert!(visual["ratio"].as_f64().unwrap() < 0.001, "{visual}");
    assert_eq!(visual["exceeds"], true);
    assert_eq!(
        out["visual"],
        json!({ "scale": 2.0, "threshold": 0.1, "include_aa": false })
    );
    // Smoke: every page pair has both.
    assert!(
        scores(&out)
            .iter()
            .all(|(_, v)| v["differing_pixels"].as_u64() > Some(0))
    );
}

/// Drop-alt's A with its figures repainted from `0.25 g` to `0.75 g`, written next to the
/// tests' other output: one digit per figure, so the cross-reference table still holds.
fn recoloured() -> PathBuf {
    let (a, _) = pair("drop-alt");
    let mut bytes = std::fs::read(a).unwrap();
    let (from, to) = (b"0.25 g", b"0.75 g");
    let mut at = 0;
    while let Some(found) = bytes[at..].windows(from.len()).position(|w| w == from) {
        bytes[at + found..at + found + to.len()].copy_from_slice(to);
        at += found + to.len();
    }
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("drop-alt-recoloured.pdf");
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_colour_change_is_listed_by_its_score_with_no_residue_and_does_not_change_the_exit_status() {
    let (a, _) = pair("drop-alt");
    let b = recoloured();

    let plain = json_of(&diff(&a, &b, &[], 0));
    let out = json_of(&diff(&a, &b, &["--visual"], 0));

    // The pairing sees nothing: the documents are identical and no page is listed.
    assert_eq!(plain["identical"], true);
    assert_eq!(plain["pages"], json!([]));
    // The score sees the two pages with figures, and lists them for it, with zero residue.
    assert_eq!(out["identical"], true, "a score never decides the exit status");
    let listed: Vec<Value> = scores(&out).into_iter().map(|(page, _)| page).collect();
    assert_eq!(listed, [json!(0), json!(1)]);
    for page in out["pages"].as_array().unwrap() {
        assert_eq!(page["residue"]["only_a"], 0);
        assert_eq!(page["residue"]["only_b"], 0);
        assert!(
            page["visual"]["differing_pixels"].as_u64().unwrap() > 1000,
            "{page}"
        );
        assert_eq!(page["visual"]["ink_added"], 0, "the figures got lighter");
    }
    // Smoke: identical but for that, so no pair changed either.
    assert_eq!(out["summary"]["modified"], 0);
}

#[test]
fn limits_decide_exceeds_the_stricter_winning() {
    let (a, b) = pair("artifact-page-number");
    let run = |extra: &[&str]| {
        let mut args = vec!["--visual", "--scope", "ink", "--page", "0"];
        args.extend_from_slice(extra);
        json_of(&diff(&a, &b, &args, 1))["pages"][0]["visual"].clone()
    };

    // ~150 of 480 000 pixels differ.
    let loose = run(&["--max-diff-pixels", "1000", "--max-diff-ratio", "0.01"]);
    let count_strict = run(&["--max-diff-pixels", "10", "--max-diff-ratio", "0.01"]);
    let ratio_strict = run(&["--max-diff-pixels", "1000", "--max-diff-ratio", "0.00001"]);

    assert_eq!(loose["exceeds"], false);
    assert_eq!(count_strict["exceeds"], true);
    assert_eq!(ratio_strict["exceeds"], true);
    // Smoke: the limits change the verdict, never the score.
    assert_eq!(loose["differing_pixels"], count_strict["differing_pixels"]);
}

#[test]
fn including_anti_aliasing_counts_more_and_says_so() {
    let (a, b) = pair("move-paragraph-3pt");

    let ignored = json_of(&diff(&a, &b, &["--visual", "--page", "1"], 1));
    let included = json_of(&diff(&a, &b, &["--visual", "--include-aa", "--page", "1"], 1));

    let count = |out: &Value| out["pages"][0]["visual"]["differing_pixels"].as_u64().unwrap();
    let aa = ignored["pages"][0]["visual"]["anti_aliased"].as_u64().unwrap();
    assert_eq!(count(&included), count(&ignored) + aa);
    assert_eq!(included["visual"]["include_aa"], true);
    // Smoke: there was anti-aliasing to count.
    assert!(aa > 0);
}

#[test]
fn page_scores_only_the_page_pair_shown() {
    let (a, b) = pair("untagged-edits");

    let out = json_of(&diff(
        &a,
        &b,
        &["--visual", "--page", "1", "--include-unchanged"],
        1,
    ));

    let pages = scores(&out);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].0, 1);
    assert!(pages[0].1["differing_pixels"].as_u64().unwrap() > 0);
    // Smoke: B's inserted page is a page only one side has: listed without a score.
    let all = json_of(&diff(&a, &b, &["--visual", "--scope", "ink"], 1));
    let inserted = all["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["a"].is_null())
        .unwrap();
    assert_eq!(
        (inserted["b"].clone(), inserted["visual"].clone()),
        (json!(1), Value::Null)
    );
}

#[test]
fn a_page_that_cannot_be_drawn_says_why_instead_of_scoring() {
    let encrypted = pdfxray_fixtures::pdf_path("encrypted-stub");
    let other = pdfxray_fixtures::pdf_path("reading-order");

    let out = json_of(&diff(&encrypted, &other, &["--visual", "--include-unchanged"], 1));

    let refused = out["pages"][0]["visual"]["refused"].as_str().unwrap();
    assert!(refused.contains("side A cannot be drawn"), "{refused}");
    // Smoke: the structure-only comparison is still reported.
    assert_eq!(out["sides"]["a"]["structure_only"], true);
}

#[test]
fn visual_output_is_byte_identical_across_runs() {
    let (a, b) = pair("untagged-edits");

    let first = diff(&a, &b, &["--visual", "--include-unchanged"], 1);

    for _ in 0..2 {
        assert_eq!(diff(&a, &b, &["--visual", "--include-unchanged"], 1), first);
    }
    // Smoke: there were scores in it.
    assert!(first.contains("\"differing_pixels\""));
}

#[test]
fn visual_flags_need_visual_and_a_scope_with_ink() {
    let (a, b) = pair("drop-alt");
    let fails = |args: &[&str], message: &str| {
        pdfxray()
            .arg("diff")
            .arg(&a)
            .arg(&b)
            .args(args)
            .assert()
            .code(2)
            .stderr(contains(message.to_owned()));
    };

    fails(&["--threshold", "0.2"], "--threshold needs --visual");
    fails(&["--include-aa"], "--include-aa needs --visual");
    fails(&["--visual-scale", "1"], "--visual-scale needs --visual");
    fails(&["--visual", "--scope", "tags"], "--visual scores ink");
    fails(&["--visual", "--threshold", "2"], "not between 0 and 1");
    // Smoke: a valid request runs.
    diff(
        &a,
        &b,
        &["--visual", "--threshold", "0.2", "--visual-scale", "1"],
        1,
    );
}
