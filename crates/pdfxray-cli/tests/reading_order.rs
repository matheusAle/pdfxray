//! `reading-order` and `tag-boxes` against the generated fixtures.

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

#[test]
fn reading_order_defaults_to_the_structure_order_and_flags_the_backtrack() {
    let file = fixture("reading-order");

    let json = run_json(&["reading-order", file.to_str().unwrap(), "0"]);

    assert_eq!(json["page"], 0);
    assert_eq!(json["from"], "structure");
    assert_eq!(
        column(&json["steps"], "id"),
        [
            "struct:8:0",
            "struct:9:0",
            "struct:10:0",
            "struct:11:0",
            "struct:12:0"
        ]
    );
    assert_eq!(
        column(&json["steps"], "role"),
        ["Heading1", "P", "Caption", "Figure", "P"]
    );
    assert_eq!(
        column(&json["steps"], "class"),
        ["heading", "p", "caption", "figure", "p"]
    );
    assert_eq!(column(&json["steps"], "index"), [1, 2, 3, 4, 5]);
    assert_eq!(
        column(&json["steps"], "kind"),
        ["first", "forward", "forward", "backtrack", "forward"]
    );
    assert_eq!(
        column(&json["steps"], "backtrack"),
        [false, false, false, true, false]
    );
    assert_eq!(json["steps"][3]["preview"], "Grey square");
    assert_eq!(
        json["steps"][3]["rect"],
        json!({"x0": 40.0, "y0": 200.0, "x1": 140.0, "y1": 300.0})
    );
    // Smoke.
    assert_eq!(json["backtracks"], 1);
}

#[test]
fn reading_order_from_paint_lists_the_figure_before_its_caption() {
    let file = fixture("reading-order");

    let json = run_json(&["reading-order", file.to_str().unwrap(), "0", "--from", "paint"]);

    assert_eq!(json["from"], "paint");
    assert_eq!(
        column(&json["steps"], "role"),
        ["Heading1", "P", "Figure", "Caption", "P"]
    );
    assert_eq!(column(&json["steps"], "backtrack"), [false; 5]);
    assert_eq!(
        column(&json["steps"], "kind"),
        ["first", "forward", "forward", "forward", "forward"]
    );
    // Smoke.
    assert_eq!(json["backtracks"], 0);
}

#[test]
fn reading_order_names_the_column_break_of_a_two_column_page() {
    let file = fixture("reading-order-columns");

    let json = run_json(&["reading-order", file.to_str().unwrap(), "0"]);

    assert_eq!(
        column(&json["steps"], "kind"),
        [
            "first",
            "forward",
            "forward",
            "column_break",
            "forward",
            "forward"
        ]
    );
    assert_eq!(column(&json["steps"], "backtrack"), [false; 6]);
    assert_eq!(column(&json["steps"], "preview")[3], "Column two follows.");
    // Smoke.
    assert_eq!(json["backtracks"], 0);
}

#[test]
fn reading_order_names_the_detour_of_a_footnote() {
    let file = fixture("reading-order-footnote");

    let json = run_json(&["reading-order", file.to_str().unwrap(), "0"]);

    assert_eq!(
        column(&json["steps"], "kind"),
        ["first", "detour", "forward", "detour", "forward"]
    );
    assert_eq!(column(&json["steps"], "role"), ["P", "Lbl", "P", "P", "P"]);
    assert_eq!(column(&json["steps"], "backtrack"), [false; 5]);
    // Smoke.
    assert_eq!(json["backtracks"], 0);
}

#[test]
fn tag_boxes_lists_every_element_on_the_page_with_class_and_depth() {
    let file = fixture("reading-order");

    let json = run_json(&["tag-boxes", file.to_str().unwrap(), "0"]);

    assert_eq!(
        column(&json, "id"),
        [
            "struct:7:0",
            "struct:8:0",
            "struct:9:0",
            "struct:10:0",
            "struct:11:0",
            "struct:12:0"
        ]
    );
    assert_eq!(
        column(&json, "standard"),
        ["Document", "H1", "P", "Caption", "Figure", "P"]
    );
    assert_eq!(
        column(&json, "class"),
        ["group", "heading", "p", "caption", "figure", "p"]
    );
    assert_eq!(column(&json, "depth"), [0, 1, 1, 1, 1, 1]);
    assert_eq!(
        json[4]["rect"],
        json!({"x0": 40.0, "y0": 200.0, "x1": 140.0, "y1": 300.0})
    );
    // Smoke.
    assert_eq!(json.as_array().unwrap().len(), 6);
}

#[test]
fn an_untagged_page_has_no_steps() {
    let file = fixture("minimal-one-page");

    let json = run_json(&["reading-order", file.to_str().unwrap(), "0"]);

    assert_eq!(json["steps"], json!([]));
    assert_eq!(json["backtracks"], 0);
    // Smoke.
    assert_eq!(run_json(&["tag-boxes", file.to_str().unwrap(), "0"]), json!([]));
}

#[test]
fn a_page_out_of_range_is_an_error_for_both_commands() {
    let file = fixture("reading-order");

    for command in ["reading-order", "tag-boxes"] {
        Command::new(env!("CARGO_BIN_EXE_pdfxray"))
            .args([command, file.to_str().unwrap(), "9"])
            .assert()
            .failure()
            .stderr(contains("page 9 out of range (document has 1)"));
    }
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["reading-order", file.to_str().unwrap(), "0", "--from", "sideways"])
        .assert()
        .failure()
        .stderr(contains("invalid value 'sideways'"));
}
