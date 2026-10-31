//! `fixtures/corpus.toml`: the pinned list of fetched (tier 3) files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

/// Parsed and validated `corpus.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    /// One entry per file, in manifest order.
    #[serde(rename = "file", default)]
    pub files: Vec<CorpusFile>,
}

/// One `[[file]]` entry.
///
/// For archive entries (`archive_member` set), `sha256` and `size` describe the extracted member
/// and `archive_sha256` / `archive_size` describe the downloaded archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusFile {
    /// Unique, filesystem-safe id; also the cache subdirectory.
    pub id: String,
    /// Pinned download URL (commit-SHA or snapshot URL, never a moving branch).
    pub url: String,
    /// Path of the PDF inside the archive when `url` points to a zip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_member: Option<String>,
    /// SHA-256 of the downloaded archive (required with `archive_member`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_sha256: Option<String>,
    /// Size of the downloaded archive in bytes (required with `archive_member`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_size: Option<u64>,
    /// Lowercase hex SHA-256 of the cached PDF.
    pub sha256: String,
    /// Size of the cached PDF in bytes.
    pub size: u64,
    /// SPDX id, or `unknown – do not vendor`.
    pub license: String,
    /// Where the file comes from.
    pub source: String,
    /// What makes the file interesting.
    pub notes: String,
}

impl CorpusManifest {
    /// Reads and validates the manifest at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    /// Parses and validates manifest text.
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(text).context("parsing corpus manifest")?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serializes back to TOML.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(self).context("serializing corpus manifest")
    }

    /// Finds an entry by id.
    pub fn get(&self, id: &str) -> Option<&CorpusFile> {
        self.files.iter().find(|f| f.id == id)
    }

    fn validate(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for file in &self.files {
            file.validate().with_context(|| format!("entry `{}`", file.id))?;
            ensure!(seen.insert(file.id.as_str()), "duplicate id `{}`", file.id);
        }
        Ok(())
    }
}

impl CorpusFile {
    /// Path of the cached PDF: `<cache_root>/<id>/<file name>`.
    pub fn cache_path(&self, cache_root: &Path) -> PathBuf {
        cache_root.join(&self.id).join(self.file_name())
    }

    /// File name used inside the cache: the last segment of the member path or URL path.
    pub fn file_name(&self) -> String {
        let source = self
            .archive_member
            .as_deref()
            .unwrap_or_else(|| self.url.split(['?', '#']).next().unwrap_or(""));
        source.rsplit('/').next().unwrap_or("").to_owned()
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty()
                && self
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
                && !self.id.starts_with('.'),
            "id must be non-empty lowercase [a-z0-9._-] and not start with `.`"
        );
        ensure!(self.url.starts_with("https://"), "url must be https");
        ensure!(
            is_sha256_hex(&self.sha256),
            "sha256 must be 64 lowercase hex characters"
        );
        ensure!(self.size > 0, "size must be positive");
        ensure!(!self.license.trim().is_empty(), "license must not be empty");
        let name = self.file_name();
        ensure!(
            !name.is_empty() && name != "." && name != "..",
            "cannot derive a cache file name"
        );
        match (&self.archive_member, &self.archive_sha256, self.archive_size) {
            (None, None, None) => {}
            (Some(member), Some(sha), Some(size)) => {
                ensure!(
                    is_sha256_hex(sha),
                    "archive_sha256 must be 64 lowercase hex characters"
                );
                ensure!(size > 0, "archive_size must be positive");
                ensure!(
                    !member.is_empty() && !member.ends_with('/'),
                    "archive_member must name a file"
                );
            }
            _ => bail!("archive_member, archive_sha256 and archive_size must be set together"),
        }
        Ok(())
    }
}

/// True for 64 lowercase hex characters.
pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const SHA_B: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn sample() -> String {
        format!(
            r#"
[[file]]
id = "plain-one"
url = "https://example.org/a/one.pdf"
sha256 = "{SHA_A}"
size = 3
license = "unknown – do not vendor"
source = "example.org"
notes = "plain download"

[[file]]
id = "zip-member"
url = "https://example.org/suite.zip"
archive_member = "suite/two.pdf"
archive_sha256 = "{SHA_B}"
archive_size = 99
sha256 = "{SHA_A}"
size = 3
license = "CC-BY-4.0"
source = "example.org"
notes = "member of a zip"
"#
        )
    }

    #[test]
    fn parses_plain_and_archive_entries() {
        let manifest = CorpusManifest::parse(&sample()).expect("parse");
        let ids: Vec<_> = manifest.files.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["plain-one", "zip-member"]);

        let plain = manifest.get("plain-one").expect("plain");
        assert_eq!(plain.url, "https://example.org/a/one.pdf");
        assert_eq!(plain.sha256, SHA_A);
        assert_eq!(plain.size, 3);
        assert_eq!(plain.license, "unknown – do not vendor");
        assert_eq!(plain.archive_member, None);

        let zip = manifest.get("zip-member").expect("zip");
        assert_eq!(zip.archive_member.as_deref(), Some("suite/two.pdf"));
        assert_eq!(zip.archive_sha256.as_deref(), Some(SHA_B));
        assert_eq!(zip.archive_size, Some(99));
        // smoke: lookups of unknown ids fail
        assert!(manifest.get("nope").is_none());
    }

    #[test]
    fn round_trips_through_toml() {
        let manifest = CorpusManifest::parse(&sample()).expect("parse");
        let text = manifest.to_toml().expect("serialize");
        assert!(text.contains("[[file]]"));
        assert!(!text.contains("archive_member = \"suite/two.pdf\"\narchive_member"));
        let again = CorpusManifest::parse(&text).expect("reparse");
        assert_eq!(again, manifest);
        // smoke: both entries survived
        assert_eq!(again.files.len(), 2);
    }

    #[test]
    fn cache_path_uses_id_dir_and_last_segment() {
        let manifest = CorpusManifest::parse(&sample()).expect("parse");
        let root = Path::new("/cache");
        assert_eq!(
            manifest.get("plain-one").expect("e").cache_path(root),
            Path::new("/cache/plain-one/one.pdf")
        );
        assert_eq!(
            manifest.get("zip-member").expect("e").cache_path(root),
            Path::new("/cache/zip-member/two.pdf")
        );
    }

    fn error_of(text: &str) -> String {
        format!("{:#}", CorpusManifest::parse(text).expect_err("must be rejected"))
    }

    #[test]
    fn rejects_duplicate_ids() {
        let text = sample().replace("zip-member", "plain-one");
        assert!(error_of(&text).contains("duplicate id `plain-one`"));
    }

    #[test]
    fn rejects_bad_hashes_urls_and_ids() {
        assert!(error_of(&sample().replace(SHA_A, "XYZ")).contains("sha256 must be 64 lowercase hex"));
        assert!(
            error_of(&sample().replace("https://example.org/a", "http://example.org/a"))
                .contains("url must be https")
        );
        assert!(
            error_of(&sample().replace("plain-one", "../evil")).contains("id must be non-empty lowercase")
        );
    }

    #[test]
    fn rejects_partial_archive_fields_and_unknown_keys() {
        let partial = sample().replace("archive_size = 99\n", "");
        assert!(error_of(&partial).contains("must be set together"));
        let unknown = sample().replace("notes = \"plain download\"", "notes = \"x\"\nbranch = \"main\"");
        assert!(error_of(&unknown).contains("unknown field `branch`"));
    }
}
