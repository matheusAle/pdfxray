//! End-to-end tests against a real veraPDF.
//!
//! They need Java and a download of the pinned veraPDF release (about 33 MB, cached in
//! `target/verapdf-test-install`), so they only run when `PDFXRAY_VERAPDF=required` is set;
//! otherwise every test prints why it was skipped and passes.

// Helper functions here are test code even though they are not `#[test]` functions themselves.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used, clippy::print_stderr)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use pdfxray_core::validation::{Finding, Locator, Report, Status};
use pdfxray_fixtures::Defect;
use pdfxray_verapdf::{
    Discovery, DiscoveryConfig, Profile, RunError, RunOptions, VeraPdf, discover, install,
};
use pretty_assertions::assert_eq;
use serde::Deserialize;

const PINNED_VERSION: &str = "1.30.3";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root exists")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("verapdf-it-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// `Some(veraPDF)` when the suite is enabled, installing it on first use.
fn vera() -> Option<&'static VeraPdf> {
    static VERA: OnceLock<Option<VeraPdf>> = OnceLock::new();
    VERA.get_or_init(|| {
        if std::env::var("PDFXRAY_VERAPDF").as_deref() != Ok("required") {
            return None;
        }
        // `target/tmp/..` is the cargo target directory.
        let target = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .expect("target dir")
            .to_path_buf();
        let dir = target.join("verapdf-test-install");
        let config = DiscoveryConfig {
            search_path: false,
            app_data_dir: Some(dir.clone()),
            ..DiscoveryConfig::default()
        };
        match discover(&config) {
            Discovery::Found(found) if found.version() == PINNED_VERSION => return Some(found),
            Discovery::JavaMissing => panic!("PDFXRAY_VERAPDF=required, but no Java 11+ runtime was found"),
            _ => {}
        }
        let mut last_percent = 0;
        let installed = install(&dir, |done, total| {
            if let Some(total) = total.filter(|t| *t > 0) {
                let percent = done * 100 / total;
                if percent / 25 > last_percent / 25 {
                    eprintln!("downloading veraPDF: {percent}%");
                }
                last_percent = percent;
            }
        })
        .unwrap_or_else(|e| panic!("installing veraPDF into {} failed: {e}", dir.display()));
        Some(installed)
    })
    .as_ref()
}

macro_rules! require_vera {
    () => {
        match vera() {
            Some(vera) => vera,
            None => {
                eprintln!("SKIPPED: set PDFXRAY_VERAPDF=required to run the veraPDF integration tests");
                return;
            }
        }
    };
}

fn options() -> RunOptions {
    RunOptions {
        timeout: Duration::from_secs(240),
        ..RunOptions::default()
    }
}

fn validate(vera: &VeraPdf, pdf: &Path, profile: &Profile) -> Report {
    vera.validate(pdf, profile, &options())
        .unwrap_or_else(|e| panic!("validating {} against {profile}: {e}", pdf.display()))
}

/// Writes the generated fixture `name` once to a scratch directory and returns its path.
fn generated(name: &str) -> PathBuf {
    static WRITTEN: Mutex<BTreeMap<String, PathBuf>> = Mutex::new(BTreeMap::new());
    let mut written = WRITTEN.lock().expect("fixture lock");
    written
        .entry(name.to_owned())
        .or_insert_with(|| {
            let built = pdfxray_fixtures::find(name)
                .unwrap_or_else(|| panic!("no fixture called {name}"))
                .build()
                .expect("fixture builds");
            let path = scratch(&format!("fixture-{name}")).join(format!("{name}.pdf"));
            std::fs::write(&path, built.pdf).expect("write fixture");
            path
        })
        .clone()
}

fn finding<'a>(report: &'a Report, clause: &str, test: u32) -> Vec<&'a Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.rule.clause == clause && f.rule.test_number == test)
        .collect()
}

#[test]
fn installation_is_the_pinned_release() {
    let vera = require_vera!();
    assert_eq!(vera.version(), PINNED_VERSION);
    let profiles = vera
        .profiles_dir()
        .expect("the installer layout has a profiles directory");
    let mut names: Vec<String> = std::fs::read_dir(profiles)
        .expect("profiles dir readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "ISO-32000-1-Tagged.xml",
            "ISO-32000-2-Tagged.xml",
            "WCAG-2-2-Machine.xml"
        ]
    );
}

#[test]
fn tagged_simple_fails_exactly_three_ua1_checks() {
    let vera = require_vera!();
    let report = validate(vera, &generated("tagged-simple"), &Profile::PdfUa1);
    assert_eq!(
        (
            report.profile.as_str(),
            report.compliant,
            report.checks_failed,
            report.verapdf_version.as_deref()
        ),
        ("PDF/UA-1 validation profile", false, 3, Some(PINNED_VERSION))
    );
    let mut got: Vec<(String, Locator, String)> = report
        .findings
        .iter()
        .map(|f| {
            (
                f.rule.to_string(),
                f.locator.clone(),
                f.error_message.clone().unwrap_or_default(),
            )
        })
        .collect();
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        got,
        vec![
            (
                "ISO 14289-1:2014 7.1-10".to_owned(),
                Locator::Document,
                "ViewerPreferences dictionary is not present in the document Catalog, or DisplayDocTitle key is set to false or is not present in the ViewerPreferences dictionary (ViewerPreferences = null, DisplayDocTitle = null)".to_owned()
            ),
            (
                "ISO 14289-1:2014 7.1-8".to_owned(),
                Locator::Document,
                "The document catalog dictionary doesn't contain metadata key or metadata stream dictionary does not contain either entry Type with value /Metadata or entry Subtype with value /XML".to_owned()
            ),
            (
                "ISO 14289-1:2014 7.21.4.1-1".to_owned(),
                Locator::Font { page: Some(0), name: "Helvetica".to_owned() },
                "The font program is not embedded".to_owned()
            ),
        ]
    );
    assert!(
        report.raw_xml.contains("<validationReport"),
        "the raw report is kept"
    );
}

#[test]
fn orphan_mcid_is_located_at_the_marked_content() {
    let vera = require_vera!();
    let built = pdfxray_fixtures::find("orphan-mcid")
        .expect("fixture")
        .build()
        .expect("builds");
    let orphans: Vec<(u32, i32)> = built
        .expected
        .defects
        .iter()
        .filter_map(|d| match d {
            Defect::OrphanMcid { page, mcid } => Some((*page, *mcid)),
            _ => None,
        })
        .collect();
    assert_eq!(orphans, vec![(0, 1)], "the fixture declares one orphan MCID");

    let report = validate(vera, &generated("orphan-mcid"), &Profile::PdfUa1);
    assert_eq!((report.compliant, report.checks_failed), (false, 4));
    let located = finding(&report, "7.1", 3);
    assert_eq!(
        located.len(),
        1,
        "exactly one content-not-tagged finding: {:?}",
        report.findings
    );
    assert_eq!(located[0].locator, Locator::MarkedContent { page: 0, mcid: 1 });
    assert_eq!(
        located[0].error_message.as_deref(),
        Some("Content is neither marked as Artifact nor tagged as real content")
    );
    assert_eq!(located[0].status, Status::Failed);
    // The orphan MCID reported by veraPDF is the one the fixture declares.
    assert_eq!(
        orphans,
        report
            .findings
            .iter()
            .filter_map(|f| match f.locator {
                Locator::MarkedContent { page, mcid } => Some((page, mcid)),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn builtin_profiles_all_run() {
    let vera = require_vera!();
    let pdf = generated("tagged-simple");
    let got: Vec<(String, bool, bool)> = Profile::builtin()
        .iter()
        .map(|profile| {
            let report = validate(vera, &pdf, profile);
            (profile.label(), report.compliant, report.findings.is_empty())
        })
        .collect();
    // The deliberately minimal file violates every accessibility and archival profile; the two
    // "Tagged PDF" profiles only check structure-tree relations, which it satisfies.
    let expected: Vec<(String, bool, bool)> = Profile::builtin()
        .iter()
        .map(|p| {
            let structure_only = matches!(p, Profile::Iso32000_1Tagged | Profile::Iso32000_2Tagged);
            (p.label(), structure_only, structure_only)
        })
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn wcag_machine_profile_carries_severity_tags() {
    let vera = require_vera!();
    let report = validate(vera, &generated("orphan-mcid"), &Profile::Wcag22Machine);
    assert_eq!(report.profile, "WCAG 2.2 machine validation profile");
    let orphan = &finding(&report, "7.1", 3)[0];
    assert_eq!(
        orphan.tags,
        vec!["critical".to_owned(), "machine".to_owned(), "artifact".to_owned()]
    );
    assert_eq!(orphan.locator, Locator::MarkedContent { page: 0, mcid: 1 });
}

#[test]
fn custom_profile_files_are_accepted() {
    let vera = require_vera!();
    let wcag = vera
        .profiles_dir()
        .expect("profiles")
        .join("WCAG-2-2-Machine.xml");
    let report = validate(vera, &generated("tagged-simple"), &Profile::Custom(wcag));
    assert_eq!(report.profile, "WCAG 2.2 machine validation profile");
    let err = vera
        .validate(
            &generated("tagged-simple"),
            &Profile::Custom(PathBuf::from("/definitely/not/a/profile.xml")),
            &options(),
        )
        .expect_err("missing profile file");
    assert!(matches!(err, RunError::ProfileUnavailable { .. }), "got {err:?}");
}

#[test]
fn capped_failure_listing_keeps_the_count() {
    let vera = require_vera!();
    let opts = RunOptions {
        max_failures_displayed: Some(1),
        ..options()
    };
    let report = vera
        .validate(&generated("orphan-mcid"), &Profile::PdfA1b, &opts)
        .expect("validates");
    // Rules 6.2.3.3 (three failed checks), 6.3.4 and 6.7.2 are listed once each.
    let mut clauses: Vec<&str> = report.findings.iter().map(|f| f.rule.clause.as_str()).collect();
    clauses.sort_unstable();
    assert_eq!(
        (report.checks_failed, clauses),
        (5, vec!["6.2.3.3", "6.3.4", "6.7.2"])
    );
}

#[test]
fn damaged_and_encrypted_inputs_map_to_typed_errors() {
    let vera = require_vera!();
    let dir = scratch("damaged");
    let garbage = dir.join("garbage.pdf");
    std::fs::write(&garbage, b"garbage").expect("write");
    let err = vera
        .validate(&garbage, &Profile::PdfUa1, &options())
        .expect_err("not a PDF");
    assert!(
        matches!(&err, RunError::DamagedPdf(m) if m.contains("Couldn't parse stream")),
        "got {err:?}"
    );

    let err = vera
        .validate(&generated("encrypted-stub"), &Profile::PdfUa1, &options())
        .expect_err("encrypted");
    assert!(
        matches!(&err, RunError::Encrypted(m) if m.contains("encrypted")),
        "got {err:?}"
    );

    let err = vera
        .validate(&dir.join("absent.pdf"), &Profile::PdfUa1, &options())
        .expect_err("absent");
    assert!(matches!(err, RunError::InputNotFound { .. }), "got {err:?}");
}

#[test]
fn a_tiny_timeout_kills_the_child() {
    let vera = require_vera!();
    let opts = RunOptions {
        timeout: Duration::from_millis(5),
        ..options()
    };
    let started = std::time::Instant::now();
    let err = vera
        .validate(&generated("tagged-simple"), &Profile::PdfUa1, &opts)
        .expect_err("times out");
    assert!(
        matches!(err, RunError::Timeout(d) if d == Duration::from_millis(5)),
        "got {err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(20));
}

// ---- corpus ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Manifest {
    file: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    path: String,
    standard: String,
    clause: String,
    rule: String,
    expected: String,
}

fn corpus(standard_prefix: &str) -> Vec<Entry> {
    let text = std::fs::read_to_string(workspace_root().join("fixtures/third_party/verapdf/manifest.toml"))
        .expect("manifest readable");
    let manifest: Manifest = toml::from_str(&text).expect("manifest parses");
    manifest
        .file
        .into_iter()
        .filter(|e| e.standard.starts_with(standard_prefix))
        .collect()
}

/// Validates every vendored file of `standard` and returns `(file name, failing rules)` in
/// manifest order. Verdict and failing clause are checked against the manifest on the way.
///
/// The manifest's `rule` is the corpus' own numbering; veraPDF 1.30.3 numbers some rules
/// differently (e.g. the corpus' `7.1-4` is veraPDF's `7.1-7`), so only the clause is compared
/// there and the exact rules are pinned in the tests.
fn run_corpus(vera: &VeraPdf, standard: &str, profile: &Profile) -> Vec<(String, Vec<String>)> {
    let root = workspace_root().join("fixtures/third_party/verapdf");
    let mut results = Vec::new();
    for entry in corpus(standard) {
        let report = validate(vera, &root.join(&entry.path), profile);
        let failing: Vec<String> = report
            .findings
            .iter()
            .map(|f| format!("{}-{}", f.rule.clause, f.rule.test_number))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for finding in &report.findings {
            assert_ne!(
                finding.locator,
                Locator::Unknown,
                "{}: {}",
                entry.path,
                finding.context
            );
        }
        assert_eq!(
            report.compliant,
            entry.expected == "pass",
            "{}: {failing:?}",
            entry.path
        );
        if entry.expected == "fail" {
            assert!(
                report.findings.iter().any(|f| f.rule.clause == entry.clause),
                "{}: expected a failure in clause {} but got {failing:?}",
                entry.path,
                entry.clause
            );
        } else {
            assert_eq!(failing, Vec::<String>::new(), "{}", entry.path);
        }
        let name = Path::new(&entry.path)
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        results.push((name, failing));
    }
    results
}

fn expected_results(table: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    table
        .iter()
        .map(|(name, rules)| {
            (
                (*name).to_owned(),
                rules.iter().map(|r| (*r).to_owned()).collect(),
            )
        })
        .collect()
}

#[test]
fn pdf_ua1_corpus_matches_the_manifest() {
    let vera = require_vera!();
    assert_eq!(
        run_corpus(vera, "PDF/UA-1", &Profile::PdfUa1),
        expected_results(&[
            ("7.1-t03-fail-b.pdf", &["7.1-3"]),
            ("7.1-t03-pass-b.pdf", &[]),
            ("7.1-t07-fail-a.pdf", &["7.1-7"]),
            ("7.1-t07-pass-a.pdf", &[]),
            ("7.3-t01-fail-a.pdf", &["7.3-1"]),
            ("7.3-t01-pass-a.pdf", &[]),
            ("7.4.2-t01-fail-a.pdf", &["7.4.2-1"]),
            ("7.4.2-t01-pass-a.pdf", &[]),
            ("7.5-t01-fail-a.pdf", &["7.5-1"]),
            ("7.5-t01-pass-a.pdf", &[]),
            ("7.9-t01-fail-a.pdf", &["7.9-1"]),
            ("7.9-t01-pass-a.pdf", &[]),
            ("7.18.1-t02-fail-a.pdf", &["7.18.1-2"]),
            ("7.18.1-t02-pass-a.pdf", &[]),
            ("7.18.5-t01-fail-a.pdf", &["7.18.5-1"]),
            ("7.18.5-t01-pass-a.pdf", &[]),
            ("7.20-t02-fail-a.pdf", &["7.20-2"]),
            ("7.20-t02-pass-a.pdf", &[]),
            ("7.2-t29-fail-f.pdf", &["7.2-2", "7.2-29"]),
            ("7.2-t29-pass-a.pdf", &[]),
        ])
    );
}

#[test]
fn pdf_ua2_corpus_matches_the_manifest() {
    let vera = require_vera!();
    assert_eq!(
        run_corpus(vera, "PDF/UA-2", &Profile::PdfUa2),
        expected_results(&[
            ("8.2.2-t01-fail-b.pdf", &["8.2.2-1"]),
            ("8.2.2-t01-pass-b.pdf", &[]),
            ("8.2.4-t01-fail-b.pdf", &["8.2.4-1"]),
            ("8.2.4-t01-pass-b.pdf", &[]),
            ("8.2.5.12-t01-fail-a.pdf", &["8.2.5.12-1"]),
            ("8.2.5.12-t01-pass-a.pdf", &[]),
            ("8.2.5.20-t02-fail-b.pdf", &["8.2.5.20-2"]),
            ("8.2.5.20-t02-pass-b.pdf", &[]),
            ("8.2.5.25-t01-fail-a.pdf", &["8.2.5.25-1"]),
            ("8.2.5.26-t03-fail-a.pdf", &["8.2.5.26-3"]),
            ("8.2.5.26-t05-fail-a.pdf", &["8.2.5.26-5"]),
            ("8.2.5.26-t05-pass-a.pdf", &[]),
            ("8.2.5.28.2-t01-fail-a.pdf", &["8.2.5.28.2-1"]),
            ("8.2.5.28.2-t01-pass-a.pdf", &[]),
        ])
    );
}

/// The ISO 32000 corpus files exercise rules of the ISO 32000 validation profiles that veraPDF
/// 1.30.3 does not ship; the "Tagged PDF" profiles cover only structure-tree relations. So the
/// files fail the accessibility flavours, and the Span-kid file also trips the Tagged PDF one.
#[test]
fn iso_32000_corpus_files_fail_accessibility_profiles() {
    let vera = require_vera!();
    let root = workspace_root().join("fixtures/third_party/verapdf");
    let mut verdicts = Vec::new();
    for (standard, profile) in [("ISO 32000-1", Profile::PdfUa1), ("ISO 32000-2", Profile::PdfUa2)] {
        for entry in corpus(standard) {
            let report = validate(vera, &root.join(&entry.path), &profile);
            for f in &report.findings {
                assert_ne!(f.locator, Locator::Unknown, "{}: {}", entry.path, f.context);
            }
            verdicts.push((entry.rule, report.compliant));
        }
    }
    assert_eq!(
        verdicts,
        vec![
            ("6-8-2-2-t01".to_owned(), false),
            ("6-8-3-3-t01".to_owned(), false),
            ("6-8-3-3-t01".to_owned(), false),
            ("6-1-3-t04".to_owned(), false),
        ]
    );

    let span_kid = root.join("ISO 32000-1/veraPDF test suite 6-8-3-3-t01-fail-b.pdf");
    let report = validate(vera, &span_kid, &Profile::Iso32000_1Tagged);
    let got: Vec<(String, Locator, Vec<String>)> = report
        .findings
        .iter()
        .map(|f| (f.rule.to_string(), f.locator.clone(), f.error_arguments.clone()))
        .collect();
    assert_eq!(
        (report.profile.as_str(), report.compliant, got),
        (
            "Tagged pdf profile for PDF 1.7 specification",
            false,
            vec![(
                "ISO 19005-2:2011 Annex_L-16".to_owned(),
                Locator::Object {
                    num: 6,
                    generation: 0
                },
                vec![]
            )]
        )
    );
}
