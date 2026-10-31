//! `tree`, `node`, `breadcrumb` and `hit` against generated fixtures.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    pdfxray_fixtures::pdf_path(name)
}

fn run_json(args: &[&str], file: &PathBuf) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .arg(args[0])
        .arg(file)
        .args(&args[1..])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).unwrap_or(Value::Null)
}

#[test]
fn tree_pages_lists_page_with_boxes() {
    let json = run_json(&["tree"], &fixture("minimal-one-page"));

    assert_eq!(
        json,
        json!([{
            "id": "page:0",
            "label": "Page 1",
            "detail": "200 × 100 pt",
            "children": [
                {"id": "box:0:media", "label": "MediaBox", "detail": "[0 0 200 100]"},
                {"id": "box:0:crop", "label": "CropBox", "detail": "[0 0 200 100] (default = MediaBox)"},
                {"id": "box:0:bleed", "label": "BleedBox", "detail": "[0 0 200 100] (default = CropBox)"},
                {"id": "box:0:trim", "label": "TrimBox", "detail": "[0 0 200 100] (default = CropBox)"},
                {"id": "box:0:art", "label": "ArtBox", "detail": "[0 0 200 100] (default = CropBox)"},
            ],
        }])
    );
}

#[test]
fn node_returns_grouped_props_with_links() {
    let json = run_json(&["node", "page:0"], &fixture("minimal-one-page"));

    assert_eq!(json["title"], "Page 1");
    assert_eq!(json["kind"], "page");
    assert_eq!(
        json["groups"],
        json!([
            {
                "name": "Geometry",
                "rows": [
                    {"key": "MediaBox", "value": "[0 0 200 100]", "link": "box:0:media"},
                    {"key": "CropBox", "value": "[0 0 200 100] (default = MediaBox)", "link": "box:0:crop"},
                    {"key": "BleedBox", "value": "[0 0 200 100] (default = CropBox)", "link": "box:0:bleed"},
                    {"key": "TrimBox", "value": "[0 0 200 100] (default = CropBox)", "link": "box:0:trim"},
                    {"key": "ArtBox", "value": "[0 0 200 100] (default = CropBox)", "link": "box:0:art"},
                    {"key": "Rotate", "value": "0°"},
                    {"key": "UserUnit", "value": "1"},
                    {"key": "Display size", "value": "200 × 100 pt"},
                ],
            },
            {
                "name": "Objects",
                "rows": [
                    {"key": "Page object", "value": "3 0 R", "link": "obj:3:0"},
                    {"key": "Resources from", "value": "3 0 R", "link": "obj:3:0"},
                ],
            },
        ])
    );
}

#[test]
fn breadcrumb_is_root_to_leaf() {
    let json = run_json(&["breadcrumb", "box:0:crop"], &fixture("minimal-one-page"));

    assert_eq!(json, json!(["doc", "page:0", "box:0:crop"]));
}

#[test]
fn hit_respects_layer_filter() {
    let file = fixture("minimal-one-page");

    let all = run_json(&["hit", "0", "10", "10"], &file);
    let tags = run_json(&["hit", "0", "10", "10", "--layer", "tags"], &file);
    let outside = run_json(&["hit", "0", "-5", "10"], &file);

    assert_eq!(all, json!(["box:0:media", "box:0:crop", "page:0"]));
    assert_eq!(tags, json!(["page:0"]));
    assert_eq!(outside, json!(["page:0"]));
}

#[test]
fn unknown_node_and_bad_page_fail_clearly() {
    let file = fixture("minimal-one-page");

    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["node"])
        .arg(&file)
        .arg("page:7")
        .assert()
        .failure()
        .stderr(contains("no node `page:7`"));
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["hit"])
        .arg(&file)
        .args(["3", "1", "1"])
        .assert()
        .failure()
        .stderr(contains("page 3 out of range (document has 1)"));
    Command::new(env!("CARGO_BIN_EXE_pdfxray"))
        .args(["node"])
        .arg(&file)
        .arg("bogus")
        .assert()
        .failure()
        .stderr(contains("invalid node id `bogus`"));
}
