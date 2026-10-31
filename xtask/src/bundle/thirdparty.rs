//! `THIRD-PARTY.txt`: the licenses of every crate that ends up in the shipped binaries.
//!
//! The list comes from `cargo metadata`: the normal (non-dev, non-build) dependency closure of the
//! shipped binaries for each target triple, minus the workspace's own crates. Each crate's license
//! expression is listed, followed by the distinct license texts found in the crate sources.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::hash::sha256_hex;

/// Cargo packages whose dependency closure ships in the bundle.
pub const SHIPPED_PACKAGES: [&str; 2] = ["pdfxray-app", "pdfxray-cli"];

/// The parts of `cargo metadata --format-version 1` used here.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    resolve: Resolve,
    /// Cargo's target directory.
    pub target_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    repository: Option<String>,
    manifest_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Debug, Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Debug, Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKind>,
}

#[derive(Debug, Deserialize)]
struct DepKind {
    /// `null` for a normal dependency, `"dev"` or `"build"` otherwise.
    kind: Option<String>,
}

impl Metadata {
    /// Parses the JSON printed by `cargo metadata --format-version 1`.
    pub fn parse(json: &[u8]) -> Result<Self> {
        serde_json::from_slice(json).context("cannot parse `cargo metadata` output")
    }

    /// Third-party crates linked into [`SHIPPED_PACKAGES`], sorted by name then version.
    fn shipped_dependencies(&self) -> Vec<&Package> {
        let by_id: HashMap<&str, &Package> = self.packages.iter().map(|p| (p.id.as_str(), p)).collect();
        let nodes: HashMap<&str, &Node> = self.resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let workspace: BTreeSet<&str> = self.workspace_members.iter().map(String::as_str).collect();

        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<&str> = self
            .packages
            .iter()
            .filter(|p| SHIPPED_PACKAGES.contains(&p.name.as_str()) && workspace.contains(p.id.as_str()))
            .map(|p| p.id.as_str())
            .collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(node) = nodes.get(id) else { continue };
            stack.extend(
                node.deps
                    .iter()
                    .filter(|d| d.dep_kinds.iter().any(|k| k.kind.is_none()))
                    .map(|d| d.pkg.as_str()),
            );
        }
        let mut deps: Vec<&Package> = seen
            .into_iter()
            .filter(|id| !workspace.contains(id))
            .filter_map(|id| by_id.get(id).copied())
            .collect();
        deps.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
        deps
    }
}

/// One third-party crate in the notice.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entry {
    /// Crate name.
    pub name: String,
    /// Crate version.
    pub version: String,
    /// SPDX expression from `Cargo.toml`, or a placeholder.
    pub license: String,
    /// Repository URL, if the crate declares one.
    pub repository: Option<String>,
    /// License texts found in the crate source.
    pub texts: Vec<String>,
}

/// Entries for the union of the shipped closures of `metadata` documents (one per target triple).
pub fn collect(documents: &[Metadata]) -> Result<Vec<Entry>> {
    let mut entries: BTreeMap<(String, String), Entry> = BTreeMap::new();
    for doc in documents {
        for package in doc.shipped_dependencies() {
            let key = (package.name.clone(), package.version.clone());
            if entries.contains_key(&key) {
                continue;
            }
            let dir = package
                .manifest_path
                .parent()
                .with_context(|| format!("{} has no manifest directory", package.name))?;
            let entry = Entry {
                name: package.name.clone(),
                version: package.version.clone(),
                license: package.license.clone().unwrap_or_else(|| {
                    if package.license_file.is_some() {
                        "SEE LICENSE FILE".to_owned()
                    } else {
                        "UNSPECIFIED".to_owned()
                    }
                }),
                repository: package.repository.clone(),
                texts: license_texts(dir, package.license_file.as_deref()),
            };
            entries.insert(key, entry);
        }
    }
    Ok(entries.into_values().collect())
}

/// Reads `LICENSE*`, `COPYING*`, `NOTICE*`, `UNLICENSE*` files from the root of a crate, plus the
/// declared `license-file`. Unreadable files are skipped: the license expression is still listed.
fn license_texts(dir: &Path, license_file: Option<&str>) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let upper = name.to_ascii_uppercase();
            let is_license = ["LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE"]
                .iter()
                .any(|prefix| upper.starts_with(prefix));
            if is_license && entry.path().is_file() {
                names.insert(name);
            }
        }
    }
    if let Some(file) = license_file {
        names.insert(file.to_owned());
    }
    let mut texts = Vec::new();
    for name in names {
        if let Ok(text) = fs::read_to_string(dir.join(&name)) {
            let text = text.trim().to_owned();
            if !text.is_empty() {
                texts.push(text);
            }
        }
    }
    texts
}

/// Renders the notice for `entries`.
pub fn render(entries: &[Entry]) -> String {
    let mut out = String::new();
    out.push_str(
        "pdfxray third-party notices\n\
         ===========================\n\n\
         pdfxray itself is released under the MIT License (see LICENSE). The release binaries\n\
         contain the following third-party Rust crates. PDFium and veraPDF are not part of this\n\
         distribution; they are downloaded separately by the user and keep their own licenses.\n\n",
    );
    let _ = writeln!(out, "Crates ({})\n------", entries.len());
    for e in entries {
        let _ = write!(out, "{} {}  [{}]", e.name, e.version, e.license);
        if let Some(repo) = &e.repository {
            let _ = write!(out, "  {repo}");
        }
        out.push('\n');
    }

    // Distinct texts, in order of first appearance among the sorted crates.
    let mut texts: Vec<(&str, Vec<String>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for e in entries {
        for text in &e.texts {
            let slot = *index.entry(sha256_hex(text.as_bytes())).or_insert_with(|| {
                texts.push((text.as_str(), Vec::new()));
                texts.len() - 1
            });
            let users = &mut texts[slot].1;
            let label = format!("{} {}", e.name, e.version);
            if !users.contains(&label) {
                users.push(label);
            }
        }
    }
    let _ = writeln!(out, "\nLicense texts ({})\n------------", texts.len());
    for (text, users) in texts {
        let _ = write!(out, "\n=== Used by: {}\n\n{text}\n", users.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    /// app -> {dep-a (normal), dev-only (dev)}; dep-a -> {dep-b (normal), build-only (build)};
    /// cli -> dep-b, pdfxray-core (workspace); core -> dep-c. `unreachable` is in nobody's closure.
    fn sample(root: &Path) -> String {
        let pkg = |name: &str, version: &str, license: &str, extra: &str| {
            let manifest = root.join(format!("{name}-{version}")).join("Cargo.toml");
            format!(
                r#"{{"id":"{name} {version}","name":"{name}","version":"{version}",{license},"manifest_path":"{}"{extra}}}"#,
                manifest.display(),
            )
        };
        let packages = [
            pkg("pdfxray-app", "0.1.0", r#""license":"MIT""#, ""),
            pkg("pdfxray-cli", "0.1.0", r#""license":"MIT""#, ""),
            pkg("pdfxray-core", "0.1.0", r#""license":"MIT""#, ""),
            pkg(
                "dep-a",
                "1.0.0",
                r#""license":"MIT OR Apache-2.0""#,
                r#","repository":"https://example.com/a""#,
            ),
            pkg("dep-b", "2.1.0", r#""license":"Zlib""#, ""),
            pkg("dep-b", "1.0.0", r#""license":"Zlib""#, ""),
            pkg(
                "dep-c",
                "0.3.0",
                r#""license":null,"license_file":"LICENSE.txt""#,
                "",
            ),
            pkg("dev-only", "1.0.0", r#""license":"MIT""#, ""),
            pkg("build-only", "1.0.0", r#""license":"MIT""#, ""),
            pkg("unreachable", "1.0.0", r#""license":"MIT""#, ""),
        ]
        .join(",");
        let dep = |id: &str, kind: &str| format!(r#"{{"pkg":"{id}","dep_kinds":[{{"kind":{kind}}}]}}"#);
        let nodes = [
            format!(
                r#"{{"id":"pdfxray-app 0.1.0","deps":[{},{}]}}"#,
                dep("dep-a 1.0.0", "null"),
                dep("dev-only 1.0.0", r#""dev""#)
            ),
            format!(
                r#"{{"id":"pdfxray-cli 0.1.0","deps":[{},{}]}}"#,
                dep("dep-b 1.0.0", "null"),
                dep("pdfxray-core 0.1.0", "null")
            ),
            format!(
                r#"{{"id":"pdfxray-core 0.1.0","deps":[{}]}}"#,
                dep("dep-c 0.3.0", "null")
            ),
            format!(
                r#"{{"id":"dep-a 1.0.0","deps":[{},{}]}}"#,
                dep("dep-b 2.1.0", "null"),
                dep("build-only 1.0.0", r#""build""#)
            ),
            r#"{"id":"dep-b 2.1.0","deps":[]}"#.to_owned(),
            r#"{"id":"dep-b 1.0.0","deps":[]}"#.to_owned(),
            r#"{"id":"dep-c 0.3.0","deps":[]}"#.to_owned(),
        ]
        .join(",");
        format!(
            r#"{{"packages":[{packages}],"workspace_members":["pdfxray-app 0.1.0","pdfxray-cli 0.1.0","pdfxray-core 0.1.0"],"resolve":{{"nodes":[{nodes}]}},"target_directory":"/t"}}"#
        )
    }

    fn write_sources(tmp: &TempDir) {
        tmp.write("dep-a-1.0.0/LICENSE-MIT", b"MIT text for a\n");
        tmp.write("dep-a-1.0.0/LICENSE-APACHE", b"Apache text\n");
        tmp.write("dep-a-1.0.0/src/lib.rs", b"// not a license");
        tmp.write("dep-b-1.0.0/LICENSE", b"Apache text\n\n"); // same as dep-a's, modulo whitespace
        tmp.write("dep-c-0.3.0/LICENSE.txt", b"Custom terms");
    }

    #[test]
    fn closure_is_normal_dependencies_of_shipped_binaries_only() {
        let tmp = TempDir::new("tp-closure");
        let doc = Metadata::parse(sample(tmp.path()).as_bytes()).expect("parse");
        let names: Vec<_> = doc
            .shipped_dependencies()
            .iter()
            .map(|p| format!("{} {}", p.name, p.version))
            .collect();
        // workspace crates, dev/build-only and unreachable crates are out; both dep-b versions in
        assert_eq!(
            names,
            ["dep-a 1.0.0", "dep-b 1.0.0", "dep-b 2.1.0", "dep-c 0.3.0"]
        );
        assert_eq!(doc.target_directory, Path::new("/t"));
    }

    #[test]
    fn entries_carry_license_expressions_and_texts() {
        let tmp = TempDir::new("tp-entries");
        write_sources(&tmp);
        let doc = Metadata::parse(sample(tmp.path()).as_bytes()).expect("parse");
        let entries = collect(&[doc]).expect("collect");
        let summary: Vec<_> = entries
            .iter()
            .map(|e| {
                (
                    e.name.as_str(),
                    e.version.as_str(),
                    e.license.as_str(),
                    e.texts.len(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("dep-a", "1.0.0", "MIT OR Apache-2.0", 2),
                ("dep-b", "1.0.0", "Zlib", 1),
                ("dep-b", "2.1.0", "Zlib", 0),
                ("dep-c", "0.3.0", "SEE LICENSE FILE", 1),
            ]
        );
        assert_eq!(entries[0].repository.as_deref(), Some("https://example.com/a"));
        assert!(!entries[0].texts.iter().any(|t| t.contains("not a license")));
    }

    #[test]
    fn union_of_two_documents_lists_each_crate_once() {
        let tmp = TempDir::new("tp-union");
        let a = Metadata::parse(sample(tmp.path()).as_bytes()).expect("a");
        let b = Metadata::parse(sample(tmp.path()).as_bytes()).expect("b");
        assert_eq!(collect(&[a, b]).expect("collect").len(), 4);
    }

    #[test]
    fn rendered_notice_lists_crates_and_dedupes_texts() {
        let tmp = TempDir::new("tp-render");
        write_sources(&tmp);
        let doc = Metadata::parse(sample(tmp.path()).as_bytes()).expect("parse");
        let text = render(&collect(&[doc]).expect("collect"));

        assert!(text.contains("Crates (4)"), "{text}");
        assert!(text.contains("dep-a 1.0.0  [MIT OR Apache-2.0]  https://example.com/a\n"));
        assert!(text.contains("dep-b 2.1.0  [Zlib]\n"));
        assert!(text.contains("dep-c 0.3.0  [SEE LICENSE FILE]\n"));
        // 3 distinct texts: MIT-for-a, Apache (shared by a and b), custom terms
        assert!(text.contains("License texts (3)"), "{text}");
        assert_eq!(text.matches("Apache text").count(), 1);
        assert!(
            text.contains("=== Used by: dep-a 1.0.0, dep-b 1.0.0\n\nApache text\n"),
            "{text}"
        );
        assert!(text.contains("=== Used by: dep-c 0.3.0\n\nCustom terms\n"));
        // workspace and dev-only crates never appear
        for absent in ["pdfxray-core", "dev-only", "build-only", "unreachable"] {
            assert!(!text.contains(absent), "{absent} leaked into the notice");
        }
    }

    #[test]
    fn missing_license_metadata_is_flagged() {
        let tmp = TempDir::new("tp-missing");
        let json = sample(tmp.path()).replace(
            r#""license":null,"license_file":"LICENSE.txt""#,
            r#""license":null,"license_file":null"#,
        );
        let entries = collect(&[Metadata::parse(json.as_bytes()).expect("parse")]).expect("collect");
        assert_eq!(entries[3].license, "UNSPECIFIED");
    }
}
