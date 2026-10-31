//! Tier 2 corpus: checks the vendored veraPDF tree against its `manifest.toml`.
//!
//! The tree must contain exactly the manifest's files (plus the license documents), every hash
//! must match, and nothing under an `Isartor` path may ever be vendored: its terms forbid
//! redistribution.

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::corpus::manifest::is_sha256_hex;
use crate::hash::{FileStatus, check_file};
use crate::paths;

/// Files that live next to the manifest and are not corpus entries.
const SIDECAR_FILES: [&str; 3] = ["manifest.toml", "LICENSE", "ATTRIBUTION.md"];
/// File names ignored when scanning for unlisted files.
const IGNORED_FILES: [&str; 1] = [".DS_Store"];
/// Path fragment that must never appear in the vendored tree (case-insensitive).
const FORBIDDEN_FRAGMENT: &str = "isartor";

/// True if `path` contains the forbidden `Isartor` fragment in any case.
pub fn is_forbidden_path(path: &str) -> bool {
    path.to_ascii_lowercase().contains(FORBIDDEN_FRAGMENT)
}

/// Parsed `fixtures/third_party/verapdf/manifest.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendoredManifest {
    /// One entry per vendored PDF.
    #[serde(rename = "file", default)]
    pub files: Vec<VendoredFile>,
}

/// Expected validator outcome of a test file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    /// The file conforms to the rule.
    Pass,
    /// The file violates the rule.
    Fail,
}

/// One vendored PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendoredFile {
    /// Path relative to the manifest directory (same as upstream).
    pub path: String,
    /// Lowercase hex SHA-256.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// Path inside the veraPDF-corpus repository.
    pub original_path: String,
    /// Standard the test targets.
    pub standard: String,
    /// Clause of the standard.
    pub clause: String,
    /// Rule or test id.
    pub rule: String,
    /// Expected result.
    pub expected: Expected,
    /// One-line description of what the file exercises.
    pub exercises: String,
}

impl VendoredManifest {
    /// Reads and validates the manifest at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    /// Parses and validates manifest text.
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(text).context("parsing vendored manifest")?;
        let mut seen = HashSet::new();
        for file in &manifest.files {
            file.validate()
                .with_context(|| format!("entry `{}`", file.path))?;
            ensure!(seen.insert(file.path.as_str()), "duplicate path `{}`", file.path);
        }
        Ok(manifest)
    }

    /// Serializes back to TOML.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self).context("serializing vendored manifest")
    }
}

impl VendoredFile {
    fn validate(&self) -> Result<()> {
        ensure!(is_safe_relative(&self.path), "path must be relative without `..`");
        ensure!(
            Path::new(&self.path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")),
            "path must end in .pdf"
        );
        ensure!(
            is_sha256_hex(&self.sha256),
            "sha256 must be 64 lowercase hex characters"
        );
        ensure!(!self.exercises.trim().is_empty(), "exercises must not be empty");
        Ok(())
    }
}

fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// A way the vendored tree deviates from its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Listed in the manifest but absent on disk.
    Missing(String),
    /// Present with the wrong size or hash.
    Mismatch(String, FileStatus),
    /// On disk but not listed in the manifest.
    Unlisted(String),
    /// A path (listed or on disk) contains `Isartor`.
    Forbidden(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(path) => write!(f, "{path}: listed in manifest.toml but missing on disk"),
            Self::Mismatch(path, status) => write!(f, "{path}: {status}"),
            Self::Unlisted(path) => write!(f, "{path}: on disk but not listed in manifest.toml"),
            Self::Forbidden(path) => {
                write!(
                    f,
                    "{path}: path contains `Isartor`; those files must never be vendored"
                )
            }
        }
    }
}

/// Result of checking a vendored tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendoredReport {
    /// Manifest entries examined.
    pub checked: usize,
    /// Everything found wrong, sorted by path.
    pub problems: Vec<Problem>,
}

/// Checks `dir` (containing `manifest.toml`) against its manifest.
pub fn verify_tree(dir: &Path) -> Result<VendoredReport> {
    let manifest = VendoredManifest::load(&dir.join("manifest.toml"))?;
    let mut problems = Vec::new();
    let mut listed = BTreeSet::new();

    for file in &manifest.files {
        listed.insert(file.path.clone());
        if is_forbidden_path(&file.path) || is_forbidden_path(&file.original_path) {
            problems.push(Problem::Forbidden(file.path.clone()));
            continue;
        }
        match check_file(&dir.join(&file.path), &file.sha256, file.size)? {
            FileStatus::Ok => {}
            FileStatus::Missing => problems.push(Problem::Missing(file.path.clone())),
            other => problems.push(Problem::Mismatch(file.path.clone(), other)),
        }
    }

    let mut on_disk = Vec::new();
    walk(dir, "", &mut on_disk)?;
    for rel in on_disk {
        if is_forbidden_path(&rel) {
            problems.push(Problem::Forbidden(rel));
        } else if !listed.contains(&rel) && !is_sidecar(&rel) {
            problems.push(Problem::Unlisted(rel));
        }
    }

    problems.sort_by(|a, b| problem_path(a).cmp(problem_path(b)));
    problems.dedup();
    Ok(VendoredReport {
        checked: manifest.files.len(),
        problems,
    })
}

fn problem_path(problem: &Problem) -> &str {
    match problem {
        Problem::Missing(p) | Problem::Mismatch(p, _) | Problem::Unlisted(p) | Problem::Forbidden(p) => p,
    }
}

fn is_sidecar(rel: &str) -> bool {
    SIDECAR_FILES.contains(&rel)
        || rel
            .rsplit('/')
            .next()
            .is_some_and(|name| IGNORED_FILES.contains(&name))
}

/// Collects every non-directory path under `dir` as `/`-separated relative paths. Directories
/// are included only when their path is forbidden, so an empty `Isartor` directory is still
/// caught. Symlinks are reported as files and never followed.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .collect::<std::io::Result<_>>()
        .with_context(|| format!("listing {}", dir.display()))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            bail!("non-UTF-8 file name under {}", dir.display());
        };
        let rel = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        let kind = entry.file_type().with_context(|| format!("stat {rel}"))?;
        if kind.is_dir() {
            if is_forbidden_path(&rel) {
                out.push(rel.clone());
            }
            walk(&entry.path(), &rel, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

/// `vendored verify`: prints one line per problem; non-zero exit if there are any.
pub fn verify_cmd(root: &Path, out: &mut dyn Write) -> Result<()> {
    let report = verify_tree(&paths::vendored_verapdf_dir(root))?;
    for problem in &report.problems {
        writeln!(out, "PROBLEM  {problem}")?;
    }
    writeln!(
        out,
        "{} files checked, {} problems",
        report.checked,
        report.problems.len()
    )?;
    if !report.problems.is_empty() {
        bail!("vendored veraPDF tree has {} problems", report.problems.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::sha256_hex;
    use crate::test_util::TempDir;

    fn entry(path: &str, body: &[u8]) -> String {
        format!(
            "[[file]]\npath = \"{path}\"\nsha256 = \"{}\"\nsize = {}\noriginal_path = \"{path}\"\n\
             standard = \"PDF/UA-1\"\nclause = \"7.1\"\nrule = \"7.1-3\"\nexpected = \"pass\"\n\
             exercises = \"test file\"\n\n",
            sha256_hex(body),
            body.len()
        )
    }

    fn tree(files: &[(&str, &[u8])]) -> TempDir {
        let dir = TempDir::new("vendored");
        let manifest: String = files.iter().map(|(p, b)| entry(p, b)).collect();
        dir.write("manifest.toml", manifest.as_bytes());
        for (p, b) in files {
            dir.write(p, b);
        }
        dir
    }

    #[test]
    fn isartor_guard_matches_any_case_in_any_segment() {
        assert!(is_forbidden_path("Isartor test files/x.pdf"));
        assert!(is_forbidden_path("PDF_A-1b/ISARTOR/x.pdf"));
        assert!(is_forbidden_path("deep/dir/my-isartor-copy.pdf"));
        assert!(!is_forbidden_path("PDF_UA-1/7.1 General/7.1-t01-pass-a.pdf"));
        // smoke: empty path is fine
        assert!(!is_forbidden_path(""));
    }

    #[test]
    fn manifest_parses_and_round_trips() {
        let text = entry("PDF_UA-1/a.pdf", b"aaa") + &entry("PDF_UA-2/b.pdf", b"bb");
        let manifest = VendoredManifest::parse(&text).expect("parse");
        assert_eq!(manifest.files[0].path, "PDF_UA-1/a.pdf");
        assert_eq!(manifest.files[0].sha256, sha256_hex(b"aaa"));
        assert_eq!(manifest.files[0].size, 3);
        assert_eq!(manifest.files[0].expected, Expected::Pass);
        assert_eq!(manifest.files[1].standard, "PDF/UA-1");
        let again = VendoredManifest::parse(&manifest.to_toml().expect("serialize")).expect("reparse");
        assert_eq!(again, manifest);
        assert_eq!(again.files.len(), 2);
    }

    #[test]
    fn manifest_rejects_unsafe_and_duplicate_paths() {
        let escape = format!(
            "{:#}",
            VendoredManifest::parse(&entry("../x.pdf", b"a")).expect_err("escape")
        );
        assert!(escape.contains("path must be relative without `..`"), "{escape}");
        let absolute = format!(
            "{:#}",
            VendoredManifest::parse(&entry("/etc/x.pdf", b"a")).expect_err("abs")
        );
        assert!(absolute.contains("path must be relative"), "{absolute}");
        let twice = entry("a.pdf", b"a") + &entry("a.pdf", b"a");
        let dup = format!("{:#}", VendoredManifest::parse(&twice).expect_err("dup"));
        assert!(dup.contains("duplicate path `a.pdf`"), "{dup}");
    }

    #[test]
    fn clean_tree_has_no_problems() {
        let dir = tree(&[("PDF_UA-1/a.pdf", b"aaa"), ("PDF_UA-2/sub dir/b.pdf", b"bb")]);
        dir.write("LICENSE", b"license text");
        dir.write("ATTRIBUTION.md", b"attribution");
        let report = verify_tree(dir.path()).expect("verify");
        assert_eq!(
            report,
            VendoredReport {
                checked: 2,
                problems: vec![]
            }
        );
        assert_eq!(report.checked, 2);
    }

    #[test]
    fn missing_mismatched_and_unlisted_files_are_reported() {
        let dir = tree(&[("a.pdf", b"aaa"), ("b.pdf", b"bbb"), ("c.pdf", b"ccc")]);
        fs::remove_file(dir.path().join("a.pdf")).expect("remove");
        dir.write("b.pdf", b"BBB");
        dir.write("extra/stray.pdf", b"stray");

        let report = verify_tree(dir.path()).expect("verify");
        assert_eq!(
            report.problems,
            vec![
                Problem::Missing("a.pdf".to_owned()),
                Problem::Mismatch(
                    "b.pdf".to_owned(),
                    FileStatus::HashMismatch {
                        expected: sha256_hex(b"bbb"),
                        actual: sha256_hex(b"BBB")
                    }
                ),
                Problem::Unlisted("extra/stray.pdf".to_owned()),
            ]
        );
        // smoke: the untouched file is not mentioned
        assert!(report.problems.iter().all(|p| problem_path(p) != "c.pdf"));
    }

    #[test]
    fn isartor_files_on_disk_and_in_manifest_are_flagged() {
        let dir = tree(&[("ok.pdf", b"ok"), ("Isartor test files/x.pdf", b"x")]);
        dir.write("sneaky/ISARTOR-copy.pdf", b"y");
        let report = verify_tree(dir.path()).expect("verify");
        assert_eq!(
            report.problems,
            vec![
                Problem::Forbidden("Isartor test files".to_owned()),
                Problem::Forbidden("Isartor test files/x.pdf".to_owned()),
                Problem::Forbidden("sneaky/ISARTOR-copy.pdf".to_owned()),
            ]
        );
        assert!(report.problems[0].to_string().contains("must never be vendored"));
    }

    #[test]
    fn empty_isartor_directory_is_flagged() {
        let dir = tree(&[("ok.pdf", b"ok")]);
        fs::create_dir_all(dir.path().join("Isartor test files")).expect("mkdir");
        let report = verify_tree(dir.path()).expect("verify");
        assert_eq!(
            report.problems,
            vec![Problem::Forbidden("Isartor test files".to_owned())]
        );
        assert_eq!(report.checked, 1);
    }

    #[test]
    fn verify_cmd_prints_problems_and_fails() {
        let dir = TempDir::new("vendored-cmd");
        let root = dir.path();
        let verapdf = paths::vendored_verapdf_dir(root);
        fs::create_dir_all(&verapdf).expect("mkdir");
        fs::write(verapdf.join("manifest.toml"), entry("a.pdf", b"aaa")).expect("manifest");

        let mut out = Vec::new();
        let err = verify_cmd(root, &mut out).expect_err("a.pdf is missing");
        assert_eq!(err.to_string(), "vendored veraPDF tree has 1 problems");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "PROBLEM  a.pdf: listed in manifest.toml but missing on disk\n1 files checked, 1 problems\n"
        );

        fs::write(verapdf.join("a.pdf"), b"aaa").expect("file");
        let mut out = Vec::new();
        verify_cmd(root, &mut out).expect("now clean");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "1 files checked, 0 problems\n"
        );
    }
}
