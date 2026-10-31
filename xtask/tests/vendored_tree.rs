//! Runs the `vendored verify` logic against the real committed `fixtures/third_party/verapdf` tree.
//! Offline: reads local files only.

// Integration-test helpers sit outside `#[test]` fns, where clippy.toml's allowance does not reach.
#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use xtask::corpus::manifest::CorpusManifest;
use xtask::vendored::{Expected, VendoredManifest, is_forbidden_path, verify_tree};
use xtask::{paths, vendored};

fn verapdf_dir() -> std::path::PathBuf {
    paths::vendored_verapdf_dir(&paths::workspace_root().expect("workspace root"))
}

#[test]
fn real_tree_matches_its_manifest() {
    let report = verify_tree(&verapdf_dir()).expect("verify real tree");
    assert_eq!(
        report.problems,
        vec![],
        "vendored tree deviates from manifest.toml"
    );
    // smoke: we really checked a non-trivial sample
    assert!(report.checked >= 25, "only {} files checked", report.checked);
}

#[test]
fn manifest_covers_standards_and_both_outcomes_without_isartor() {
    let manifest = VendoredManifest::load(&verapdf_dir().join("manifest.toml")).expect("manifest");

    let standards: BTreeSet<&str> = manifest.files.iter().map(|f| f.standard.as_str()).collect();
    assert_eq!(
        standards,
        BTreeSet::from([
            "ISO 32000-1",
            "ISO 32000-2",
            "PDF/UA-1 (ISO 14289-1)",
            "PDF/UA-2 (ISO 14289-2)"
        ])
    );
    assert!(manifest.files.iter().any(|f| f.expected == Expected::Pass));
    assert!(manifest.files.iter().any(|f| f.expected == Expected::Fail));
    assert!(
        manifest
            .files
            .iter()
            .all(|f| !is_forbidden_path(&f.path) && !is_forbidden_path(&f.original_path))
    );

    // The pass/fail flag must agree with the veraPDF file-name convention.
    for file in &manifest.files {
        let marker = match file.expected {
            Expected::Pass => "-pass-",
            Expected::Fail => "-fail-",
        };
        assert!(
            file.path.contains(marker),
            "{} is not a `{marker}` file",
            file.path
        );
    }
    // smoke: total size stays inside the repo budget of 8 MB
    let total: u64 = manifest.files.iter().map(|f| f.size).sum();
    assert!(total > 0 && total <= 8 * 1024 * 1024, "total {total} bytes");
}

#[test]
fn license_and_attribution_ship_with_the_tree() {
    let dir = verapdf_dir();
    let license = std::fs::read_to_string(dir.join("LICENSE")).expect("LICENSE");
    assert!(license.starts_with("Attribution 4.0 International"));
    assert!(license.contains("Creative Commons Attribution 4.0 International Public License"));

    let attribution = std::fs::read_to_string(dir.join("ATTRIBUTION.md")).expect("ATTRIBUTION.md");
    for needle in [
        "veraPDF test corpus",
        "veraPDF Consortium",
        "https://github.com/veraPDF/veraPDF-corpus",
        "CC BY 4.0",
        "Files are unmodified; selected subset",
    ] {
        assert!(attribution.contains(needle), "ATTRIBUTION.md lacks `{needle}`");
    }
}

#[test]
fn verify_cmd_succeeds_on_the_real_tree() {
    let root = paths::workspace_root().expect("workspace root");
    let mut out = Vec::new();
    vendored::verify_cmd(&root, &mut out).expect("verify_cmd");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.ends_with(" files checked, 0 problems\n"), "{text}");
}

#[test]
fn committed_corpus_manifest_is_well_formed_and_pinned() {
    let root = paths::workspace_root().expect("workspace root");
    let manifest = CorpusManifest::load(&paths::corpus_manifest(&root)).expect("corpus.toml");

    let first = manifest.files.first().expect("entries");
    assert_eq!(first.id, "pdfua-ref-01-magazine-danish");
    assert!(manifest.files.iter().any(|f| f.id.starts_with("pdfjs-")));
    assert!(manifest.files.iter().any(|f| f.id.starts_with("pdfbox-")));
    // No moving branches: GitHub URLs carry a 40-hex commit SHA, others a Wayback timestamp.
    for file in &manifest.files {
        let pinned = file.url.contains("/web/2")
            || file
                .url
                .split('/')
                .any(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(pinned, "{} is not pinned: {}", file.id, file.url);
    }
}
