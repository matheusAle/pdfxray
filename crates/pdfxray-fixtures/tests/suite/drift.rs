//! Committed output must equal regenerated output, and generation must be deterministic.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use pdfxray_fixtures::{Expected, all, find, fixtures_dir, generate, pdf_path};
use pretty_assertions::assert_eq;

use crate::common::latin1;

/// Removes its directory on drop, even when the test fails.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("pdfxray-fixtures-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn catalog_lists_the_agreed_fixtures_in_order() {
    let names: Vec<&str> = all().iter().map(|f| f.name).collect();

    assert_eq!(
        names,
        [
            "minimal-one-page",
            "tagged-simple",
            "artifacts",
            "page-boxes",
            "rolemap-custom",
            "broken-xref-offsets",
            "incremental-update",
            "xref-stream-objstm",
            "form-xobject-nesting",
            "orphan-mcid",
            "encrypted-stub",
            "text-state",
            "graphics-state",
            "images",
            "type3-font",
            "struct-rich",
            "pdf2-namespaces",
            "annotations",
            "untagged-content",
            "box-matrix",
            "resources-inheritance",
            "startxref-broken",
            "hybrid-xref",
            "long-doc",
            "reading-order",
            "reading-order-columns",
            "reading-order-footnote",
            "speech-alt-hides",
            "speech-artifacts",
            "speech-footnote",
            "table-simple-pdf17",
            "table-scope-pdf2",
            "table-headers-ids",
            "speech-lang",
            "speech-link-objr",
            "rolemap-two-hops",
        ]
    );
    assert_eq!(names.len(), 36);
}

#[test]
fn catalog_names_are_unique_kebab_case_and_described() {
    let mut seen = BTreeSet::new();
    for fixture in all() {
        assert!(seen.insert(fixture.name), "duplicate name {}", fixture.name);
        assert!(
            fixture
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{} is not kebab-case",
            fixture.name
        );
        assert!(
            fixture.description.ends_with('.'),
            "{} needs a sentence description",
            fixture.name
        );
        assert_eq!(find(fixture.name).map(|f| f.name), Some(fixture.name));
    }
    assert_eq!(find("no-such-fixture").map(|f| f.name), None);
}

#[test]
fn fixtures_dir_points_at_the_workspace_fixtures() {
    let dir = fixtures_dir();

    assert!(dir.ends_with("fixtures/generated"), "{}", dir.display());
    assert!(dir.is_dir(), "{} is missing; run the gen binary", dir.display());
}

#[test]
fn pdf_path_names_the_committed_pdf_of_a_fixture() {
    let path = pdf_path("minimal-one-page");

    assert_eq!(path, fixtures_dir().join("minimal-one-page.pdf"));
    assert_eq!(
        path.file_name().and_then(|n| n.to_str()),
        find("minimal-one-page")
            .map(pdfxray_fixtures::Fixture::pdf_file_name)
            .as_deref()
    );
    // Smoke: the file is there.
    assert!(path.is_file(), "{}", path.display());
}

#[test]
fn committed_pdfs_equal_regenerated_bytes() {
    for fixture in all() {
        let path = fixtures_dir().join(fixture.pdf_file_name());
        let committed = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let built = fixture.build().unwrap();

        assert_eq!(latin1(&committed), latin1(&built.pdf), "{} drifted", fixture.name);
        assert!(committed.starts_with(b"%PDF-"));
    }
}

#[test]
fn committed_expected_json_equals_regenerated_json() {
    for fixture in all() {
        let path = fixtures_dir().join(fixture.expected_file_name());
        let committed = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let json = fixture.build().unwrap().expected_json().unwrap();

        assert_eq!(committed, json, "{} facts drifted", fixture.name);
        assert!(committed.ends_with("}\n"));
    }
}

#[test]
fn committed_json_deserializes_to_the_built_facts() {
    for fixture in all() {
        let path = fixtures_dir().join(fixture.expected_file_name());
        let committed = fs::read_to_string(path).unwrap();
        let parsed: Expected = serde_json::from_str(&committed).unwrap();

        assert_eq!(parsed, fixture.build().unwrap().expected, "{}", fixture.name);
    }
}

#[test]
fn generated_dir_holds_exactly_the_catalog_files() {
    let expected: BTreeSet<String> = all()
        .iter()
        .flat_map(|f| [f.pdf_file_name(), f.expected_file_name()])
        .collect();
    let actual: BTreeSet<String> = fs::read_dir(fixtures_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();

    assert_eq!(actual, expected, "stale or missing files in fixtures/generated");
    assert_eq!(actual.len(), 72);
}

#[test]
fn building_twice_yields_identical_bytes_and_json() {
    for fixture in all() {
        let first = fixture.build().unwrap();
        let second = fixture.build().unwrap();

        assert_eq!(latin1(&first.pdf), latin1(&second.pdf), "{} bytes", fixture.name);
        assert_eq!(first.expected, second.expected, "{} facts", fixture.name);
        assert_eq!(first.expected_json().unwrap(), second.expected_json().unwrap());
        assert!(first.pdf.starts_with(b"%PDF-"));
    }
}

#[test]
fn generate_writes_what_build_returns() {
    let dir = TempDir::new("generate");

    let written = generate(&dir.0).unwrap();

    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    let want: Vec<String> = all()
        .iter()
        .flat_map(|f| [f.pdf_file_name(), f.expected_file_name()])
        .collect();
    assert_eq!(names, want);
    for fixture in all() {
        let built = fixture.build().unwrap();
        let pdf = fs::read(dir.0.join(fixture.pdf_file_name())).unwrap();
        let json = fs::read_to_string(dir.0.join(fixture.expected_file_name())).unwrap();
        assert_eq!(latin1(&pdf), latin1(&built.pdf));
        assert_eq!(json, built.expected_json().unwrap());
    }
}

#[test]
fn expected_json_has_stable_key_order() {
    let json = find("minimal-one-page")
        .unwrap()
        .build()
        .unwrap()
        .expected_json()
        .unwrap();

    let keys: Vec<&str> = [
        "header_version",
        "revision_count",
        "revisions",
        "page_count",
        "pages",
        "catalog",
    ]
    .into_iter()
    .collect();
    let positions: Vec<usize> = keys
        .iter()
        .map(|k| json.find(&format!("\"{k}\"")).unwrap())
        .collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted);
    assert!(json.starts_with("{\n  \"header_version\": \"1.4\""));
}
