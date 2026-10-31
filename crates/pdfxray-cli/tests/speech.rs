//! `speech` and `table` against the generated fixtures.

#![allow(clippy::unwrap_used, reason = "test helpers fail loudly on unexpected input")]

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    pdfxray_fixtures::pdf_path(name)
}

fn run_json(args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap_or(Value::Null)
}

/// `field` of every element of the array `value`.
fn column<'a>(value: &'a Value, field: &str) -> Vec<&'a Value> {
    value.as_array().unwrap().iter().map(|v| &v[field]).collect()
}

// ---- speech -----------------------------------------------------------------------------------

#[test]
fn speech_defaults_to_the_structure_preset_and_lists_what_is_spoken() {
    let file = fixture("speech-alt-hides");

    let json = run_json(&["speech", file.to_str().unwrap()]);

    assert_eq!(json["preset"], "structure");
    assert_eq!(json["label"], "Structure (spec)");
    assert_eq!(json["approximation"], false);
    assert_eq!(
        column(&json["items"], "text"),
        [
            "Quarterly report",
            "Summary of results",
            "Revenue chart",
            "Cost chart",
            "Closing remarks follow."
        ]
    );
    assert_eq!(
        column(&json["items"], "role"),
        ["H1", "Sect", "Figure", "Figure", "P"]
    );
    assert_eq!(
        column(&json["items"], "node"),
        [
            "struct:8:0",
            "struct:9:0",
            "struct:12:0",
            "struct:13:0",
            "struct:14:0"
        ]
    );
    assert_eq!(
        column(&json["items"], "substituted_by"),
        [
            &Value::Null,
            &json!("alt"),
            &json!("alt"),
            &json!("alt"),
            &Value::Null
        ]
    );
    assert_eq!(
        json["items"][1]["painted"],
        "Revenue grew in every region. Costs stayed flat this year."
    );
    // Smoke: the spec preset has no rule differences to list.
    assert_eq!(json["rules"], json!([]));
}

#[test]
fn show_skipped_adds_the_items_that_are_not_spoken_with_their_reason() {
    let file = fixture("speech-alt-hides");

    let json = run_json(&["speech", file.to_str().unwrap(), "--show-skipped"]);

    assert_eq!(
        column(&json["items"], "skipped"),
        [
            &Value::Null,
            &Value::Null,
            &json!({"covered_by": "struct:9:0"}),
            &json!({"covered_by": "struct:9:0"}),
            &Value::Null,
            &Value::Null,
            &Value::Null
        ]
    );
    assert_eq!(column(&json["items"], "index"), [1, 2, 3, 4, 5, 6, 7]);
    // Smoke.
    assert_eq!(json["items"].as_array().unwrap().len(), 7);
}

#[test]
fn each_preset_reads_the_same_file_its_own_way() {
    let file = fixture("speech-alt-hides");
    let texts = |preset: &str| {
        let json = run_json(&["speech", file.to_str().unwrap(), "--preset", preset]);
        (
            json["label"].as_str().unwrap().to_owned(),
            json["approximation"].as_bool().unwrap(),
            column(&json["items"], "text")
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
        )
    };

    let acrobat = texts("acrobat");
    assert_eq!(acrobat.0, "Acrobat on Windows (NVDA, JAWS)");
    assert_eq!(acrobat.2, texts("structure").2);
    assert_eq!(
        texts("firefox"),
        (
            "Firefox (pdf.js)".to_owned(),
            true,
            vec![
                "Quarterly report".to_owned(),
                "Revenue grew in every region.".to_owned(),
                "Costs stayed flat this year.".to_owned(),
                "Revenue chart".to_owned(),
                String::new(),
                "Closing remarks follow.".to_owned()
            ]
        )
    );
    assert_eq!(
        texts("chromium"),
        (
            "Chrome and Edge (default)".to_owned(),
            true,
            vec![
                "Quarterly report".to_owned(),
                "Revenue grew in every region.".to_owned(),
                "Costs stayed flat this year.".to_owned(),
                "Closing remarks follow.".to_owned()
            ]
        )
    );
}

#[test]
fn the_rules_of_a_preset_come_with_their_evidence() {
    let file = fixture("speech-artifacts");

    let json = run_json(&["speech", file.to_str().unwrap(), "--preset", "chromium"]);

    assert_eq!(
        column(&json["rules"], "topic"),
        ["Order", "Artifacts", "Roles", "Untagged content"]
    );
    assert_eq!(
        column(&json["items"], "text"),
        [
            "Annual Report 2025",
            "Results",
            "Revenue grew in every region.",
            "Page 1 of 9"
        ]
    );
    // Smoke: the artifacts are spoken here.
    assert_eq!(column(&json["items"], "skipped"), [&Value::Null; 4]);
}

#[test]
fn artifacts_are_listed_only_on_request() {
    let file = fixture("speech-artifacts");

    let spoken = run_json(&["speech", file.to_str().unwrap()]);
    let all = run_json(&["speech", file.to_str().unwrap(), "--show-skipped"]);

    assert_eq!(
        column(&spoken["items"], "text"),
        ["Results", "Revenue grew in every region."]
    );
    assert_eq!(
        column(&all["items"], "skipped"),
        [&json!("artifact"), &Value::Null, &Value::Null, &json!("artifact")]
    );
    assert_eq!(column(&all["items"], "node")[0], "artifact:0:0");
}

#[test]
fn table_cells_carry_their_header_cells_and_the_rule_that_found_them() {
    let file = fixture("table-scope-pdf2");

    let json = run_json(&["speech", file.to_str().unwrap()]);

    let q1 = &json["items"][4];
    assert_eq!(q1["text"], "1.2");
    assert_eq!(
        q1["cell"],
        json!({
            "table": "struct:9:0",
            "row": 1,
            "col": 1,
            "row_span": 1,
            "col_span": 1,
            "header": false,
            "headers": [
                {"cell": "struct:14:0", "via": "scope"},
                {"cell": "struct:16:0", "via": "scope"}
            ]
        })
    );
    // Smoke: nine cells.
    assert_eq!(
        column(&json["items"], "cell")
            .iter()
            .filter(|c| !c.is_null())
            .count(),
        9
    );
}

#[test]
fn notes_languages_and_links_are_flagged() {
    let note = run_json(&["speech", fixture("speech-footnote").to_str().unwrap()]);
    assert_eq!(
        note["items"][1]["note"],
        json!({"role": "reference", "other": "struct:11:0", "via": "label_match", "items_apart": 1})
    );

    let lang = run_json(&["speech", fixture("speech-lang").to_str().unwrap()]);
    assert_eq!(
        lang["items"][1]["lang_change"],
        json!({"from": "en-US", "to": "de-DE"})
    );

    let link = run_json(&["speech", fixture("speech-link-objr").to_str().unwrap()]);
    assert_eq!(
        link["items"][1]["link"],
        json!({"subtype": "Link", "target": {"uri": "https://example.com/report"}})
    );
    assert_eq!(link["items"][1]["node"], "obj:14:0");
}

#[test]
fn an_unknown_preset_is_an_error() {
    let file = fixture("speech-lang");

    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["speech", file.to_str().unwrap(), "--preset", "voiceover"])
        .assert()
        .failure()
        .stderr(contains("invalid value 'voiceover'"));
}

// ---- table ------------------------------------------------------------------------------------

#[test]
fn table_prints_the_grid_and_the_header_associations() {
    let file = fixture("table-headers-ids");

    let json = run_json(&["table", file.to_str().unwrap(), "struct:8:0"]);

    assert_eq!(json["table"], "struct:8:0");
    assert_eq!(json["rules"], "pdf17");
    assert_eq!((json["rows"].as_u64(), json["cols"].as_u64()), (Some(4), Some(3)));
    assert_eq!(json["slots"], json!([0, 1, 1, 0, 2, 3, 4, 5, 6, 7, 8, 9]));
    let last = &json["cells"][9];
    assert_eq!(last["node"], "struct:22:0");
    assert_eq!(
        last["headers"],
        json!([
            {"cell": "struct:20:0", "via": "headers"},
            {"cell": "struct:16:0", "via": "headers"},
            {"cell": "struct:14:0", "via": "headers"}
        ])
    );
    assert_eq!(last["missing_header_ids"], json!(["h-missing"]));
    assert_eq!(json["cells"][0]["row_span"], 2);
    assert_eq!(json["cells"][1]["col_span"], 2);
    // Smoke.
    assert_eq!(json["irregular"], json!([]));
}

#[test]
fn table_names_the_pdf_2_rules_for_a_namespaced_pdf_2_file() {
    let file = fixture("table-scope-pdf2");

    let json = run_json(&["table", file.to_str().unwrap(), "struct:9:0"]);

    assert_eq!(json["rules"], "pdf2");
    assert_eq!(column(&json["cells"], "scope")[..3], [&json!("column"); 3]);
}

#[test]
fn table_of_something_that_is_not_a_table_is_an_error() {
    let file = fixture("table-simple-pdf17");

    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["table", file.to_str().unwrap(), "struct:12:0"])
        .assert()
        .failure()
        .stderr(contains("`struct:12:0` is not a Table element of"));
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["table", file.to_str().unwrap(), "page:0"])
        .assert()
        .failure()
        .stderr(contains("is not a Table element"));
}
