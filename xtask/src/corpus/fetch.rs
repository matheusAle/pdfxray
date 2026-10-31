//! Downloading corpus entries into the cache and extracting archive members.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use zip::ZipArchive;

use super::manifest::CorpusFile;
use crate::hash::{FileStatus, check_bytes, check_file};

/// Source of bytes for a URL. Production uses [`UreqDownloader`]; tests use a fake.
pub trait Downloader {
    /// Downloads `url`, failing if the body is larger than `max_bytes`.
    fn download(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>>;
}

/// HTTP downloader built on `ureq`.
pub struct UreqDownloader {
    agent: ureq::Agent,
}

impl UreqDownloader {
    /// Builds an agent with a global timeout.
    pub fn new() -> Result<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .build()
            .into();
        Ok(Self { agent })
    }
}

impl Downloader for UreqDownloader {
    fn download(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let mut response = self.agent.get(url).call().with_context(|| format!("GET {url}"))?;
        response
            .body_mut()
            .with_config()
            .limit(max_bytes)
            .read_to_vec()
            .with_context(|| format!("reading body of {url} (limit {max_bytes} bytes)"))
    }
}

/// Outcome of fetching one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// Already present and verified; nothing downloaded.
    Cached,
    /// Downloaded (or extracted) and verified.
    Fetched,
}

/// The cause of a failed fetch with its context chain (`anyhow`'s), shareable between entries.
pub type Cause = Arc<dyn Error + Send + Sync>;

/// Why one entry was not fetched.
#[derive(Debug, Clone, thiserror::Error)]
pub enum FetchError {
    /// Checking the cache, downloading, verifying or writing the entry failed.
    #[error(transparent)]
    Entry(Cause),
    /// The archive the entry is a member of could not be downloaded or verified.
    #[error("archive {url}")]
    Archive {
        /// Where the archive comes from.
        url: String,
        /// Why it failed.
        #[source]
        source: Cause,
    },
}

impl FetchError {
    fn entry(error: anyhow::Error) -> Self {
        Self::Entry(cause(error))
    }
}

fn cause(error: anyhow::Error) -> Cause {
    Arc::from(Box::<dyn Error + Send + Sync>::from(error))
}

/// `error` and its causes on one line, as `anyhow` prints them with `{:#}`.
pub fn describe(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// Result per entry id, in manifest order.
pub type FetchReport = Vec<(String, Result<FetchOutcome, FetchError>)>;

/// Fetches `entries` into `cache_root`. Entries already verified are skipped; entries sharing an
/// archive URL trigger a single download. Failures are collected, not fatal.
pub fn fetch_entries(entries: &[&CorpusFile], cache_root: &Path, downloader: &dyn Downloader) -> FetchReport {
    let mut results: BTreeMap<usize, Result<FetchOutcome, FetchError>> = BTreeMap::new();
    let mut archives: BTreeMap<&str, Vec<usize>> = BTreeMap::new();

    for (i, entry) in entries.iter().enumerate() {
        let target = entry.cache_path(cache_root);
        match check_file(&target, &entry.sha256, entry.size) {
            Ok(FileStatus::Ok) => {
                results.insert(i, Ok(FetchOutcome::Cached));
            }
            Ok(_) if entry.archive_member.is_some() => {
                archives.entry(entry.url.as_str()).or_default().push(i);
            }
            Ok(_) => {
                results.insert(
                    i,
                    fetch_plain(entry, &target, downloader).map_err(FetchError::entry),
                );
            }
            Err(err) => {
                results.insert(i, Err(FetchError::entry(err)));
            }
        }
    }

    for (url, indexes) in archives {
        let group: Vec<&CorpusFile> = indexes.iter().map(|&i| entries[i]).collect();
        match download_archive(group[0], downloader) {
            Ok(bytes) => {
                for (&i, entry) in indexes.iter().zip(&group) {
                    let target = entry.cache_path(cache_root);
                    results.insert(
                        i,
                        extract_member(entry, &bytes, &target).map_err(FetchError::entry),
                    );
                }
            }
            Err(err) => {
                let source = cause(err);
                for &i in &indexes {
                    let error = FetchError::Archive {
                        url: url.to_owned(),
                        source: Arc::clone(&source),
                    };
                    results.insert(i, Err(error));
                }
            }
        }
    }

    entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| results.remove(&i).map(|r| (e.id.clone(), r)))
        .collect()
}

fn fetch_plain(entry: &CorpusFile, target: &Path, downloader: &dyn Downloader) -> Result<FetchOutcome> {
    let bytes = downloader.download(&entry.url, entry.size.saturating_add(1))?;
    require_ok(&check_bytes(&bytes, &entry.sha256, entry.size), &entry.url)?;
    write_atomically(target, &bytes)?;
    Ok(FetchOutcome::Fetched)
}

fn download_archive(entry: &CorpusFile, downloader: &dyn Downloader) -> Result<Vec<u8>> {
    let (Some(sha), Some(size)) = (entry.archive_sha256.as_deref(), entry.archive_size) else {
        bail!("entry `{}` has no archive_sha256/archive_size", entry.id);
    };
    let bytes = downloader.download(&entry.url, size.saturating_add(1))?;
    require_ok(&check_bytes(&bytes, sha, size), &entry.url)?;
    Ok(bytes)
}

fn extract_member(entry: &CorpusFile, archive: &[u8], target: &Path) -> Result<FetchOutcome> {
    let member = entry
        .archive_member
        .as_deref()
        .context("entry has no archive_member")?;
    let mut zip = ZipArchive::new(Cursor::new(archive)).context("opening archive")?;
    let file = zip
        .by_name(member)
        .with_context(|| format!("member `{member}` not found in archive"))?;
    if file.size() > entry.size {
        bail!(
            "member `{member}` is {} bytes, manifest pins {}",
            file.size(),
            entry.size
        );
    }
    let mut bytes = Vec::new();
    // The target path comes from the manifest, never from the archive: no zip-slip.
    file.take(entry.size.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading `{member}`"))?;
    require_ok(&check_bytes(&bytes, &entry.sha256, entry.size), member)?;
    write_atomically(target, &bytes)?;
    Ok(FetchOutcome::Fetched)
}

pub(crate) fn require_ok(status: &FileStatus, what: &str) -> Result<()> {
    if status.is_ok() {
        Ok(())
    } else {
        bail!("{what}: {status}")
    }
}

/// Writes `bytes` to `target` via a `.part` file so a failure never leaves a truncated cache file.
fn write_atomically(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let part = target.with_extension("part");
    let mut file = fs::File::create(&part).with_context(|| format!("creating {}", part.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("writing {}", part.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing {}", part.display()))?;
    fs::rename(&part, target).with_context(|| format!("renaming into {}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;
    use crate::hash::sha256_hex;
    use crate::test_util::TempDir;

    /// The outcome of each entry, or its error as `corpus fetch` prints it.
    fn printed(report: &FetchReport) -> Vec<(String, Result<FetchOutcome, String>)> {
        report
            .iter()
            .map(|(id, result)| (id.clone(), result.as_ref().cloned().map_err(|e| describe(&e))))
            .collect()
    }

    #[derive(Default)]
    struct FakeDownloader {
        bodies: HashMap<String, Vec<u8>>,
        calls: RefCell<Vec<String>>,
    }

    impl Downloader for FakeDownloader {
        fn download(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
            self.calls.borrow_mut().push(url.to_owned());
            let body = self
                .bodies
                .get(url)
                .with_context(|| format!("no fake body for {url}"))?;
            if u64::try_from(body.len())? > max_bytes {
                bail!("body larger than limit {max_bytes}");
            }
            Ok(body.clone())
        }
    }

    fn plain_entry(id: &str, url: &str, body: &[u8]) -> CorpusFile {
        CorpusFile {
            id: id.to_owned(),
            url: url.to_owned(),
            archive_member: None,
            archive_sha256: None,
            archive_size: None,
            sha256: sha256_hex(body),
            size: body.len() as u64,
            license: "unknown – do not vendor".to_owned(),
            source: "test".to_owned(),
            notes: String::new(),
        }
    }

    fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, body) in members {
            writer.start_file(*name, options).expect("start");
            writer.write_all(body).expect("write");
        }
        writer.finish().expect("finish").into_inner()
    }

    fn member_entry(id: &str, url: &str, archive: &[u8], member: &str, body: &[u8]) -> CorpusFile {
        CorpusFile {
            archive_member: Some(member.to_owned()),
            archive_sha256: Some(sha256_hex(archive)),
            archive_size: Some(archive.len() as u64),
            ..plain_entry(id, url, body)
        }
    }

    #[test]
    fn fetches_a_plain_file_then_skips_it() {
        let dir = TempDir::new("fetch-plain");
        let entry = plain_entry("one", "https://x.test/one.pdf", b"%PDF-1.7 one");
        let mut fake = FakeDownloader::default();
        fake.bodies.insert(entry.url.clone(), b"%PDF-1.7 one".to_vec());

        let report = fetch_entries(&[&entry], dir.path(), &fake);
        assert_eq!(
            printed(&report),
            vec![("one".to_owned(), Ok(FetchOutcome::Fetched))]
        );
        assert_eq!(
            fs::read(dir.path().join("one/one.pdf")).expect("cached"),
            b"%PDF-1.7 one"
        );
        assert!(!dir.path().join("one/one.part").exists());

        let again = fetch_entries(&[&entry], dir.path(), &fake);
        assert_eq!(
            printed(&again),
            vec![("one".to_owned(), Ok(FetchOutcome::Cached))]
        );
        // smoke: the second run did not touch the network
        assert_eq!(fake.calls.borrow().len(), 1);
    }

    #[test]
    fn hash_mismatch_is_reported_and_nothing_is_cached() {
        let dir = TempDir::new("fetch-bad");
        let entry = plain_entry("bad", "https://x.test/bad.pdf", b"expected body");
        let mut fake = FakeDownloader::default();
        fake.bodies.insert(entry.url.clone(), b"tampered body".to_vec());

        let report = printed(&fetch_entries(&[&entry], dir.path(), &fake));
        let (id, result) = &report[0];
        assert_eq!(id, "bad");
        let message = result.as_ref().expect_err("must fail");
        assert!(message.contains("sha256 mismatch"), "{message}");
        assert!(!dir.path().join("bad").exists());
    }

    #[test]
    fn corrupt_cache_file_is_replaced() {
        let dir = TempDir::new("fetch-heal");
        let entry = plain_entry("heal", "https://x.test/heal.pdf", b"good");
        dir.write("heal/heal.pdf", b"junk");
        let mut fake = FakeDownloader::default();
        fake.bodies.insert(entry.url.clone(), b"good".to_vec());

        let report = fetch_entries(&[&entry], dir.path(), &fake);
        assert_eq!(
            printed(&report),
            vec![("heal".to_owned(), Ok(FetchOutcome::Fetched))]
        );
        assert_eq!(
            fs::read(dir.path().join("heal/heal.pdf")).expect("cached"),
            b"good"
        );
    }

    #[test]
    fn archive_is_downloaded_once_and_members_extracted() {
        let dir = TempDir::new("fetch-zip");
        let archive = zip_of(&[
            ("s/a.pdf", b"AAA"),
            ("s/b.pdf", b"BBBB"),
            ("__MACOSX/._a.pdf", b"junk"),
        ]);
        let url = "https://x.test/suite.zip";
        let a = member_entry("zip-a", url, &archive, "s/a.pdf", b"AAA");
        let b = member_entry("zip-b", url, &archive, "s/b.pdf", b"BBBB");
        let mut fake = FakeDownloader::default();
        fake.bodies.insert(url.to_owned(), archive);

        let report = fetch_entries(&[&a, &b], dir.path(), &fake);
        assert_eq!(
            printed(&report),
            vec![
                ("zip-a".to_owned(), Ok(FetchOutcome::Fetched)),
                ("zip-b".to_owned(), Ok(FetchOutcome::Fetched))
            ]
        );
        assert_eq!(fs::read(dir.path().join("zip-a/a.pdf")).expect("a"), b"AAA");
        assert_eq!(fs::read(dir.path().join("zip-b/b.pdf")).expect("b"), b"BBBB");
        // smoke: one download for two members
        assert_eq!(*fake.calls.borrow(), vec![url.to_owned()]);
    }

    #[test]
    fn missing_member_and_wrong_member_hash_are_failures() {
        let dir = TempDir::new("fetch-zip-bad");
        let archive = zip_of(&[("s/a.pdf", b"AAA")]);
        let url = "https://x.test/suite.zip";
        let absent = member_entry("absent", url, &archive, "s/zzz.pdf", b"ZZZ");
        let wrong = member_entry("wrong", url, &archive, "s/a.pdf", b"BBB");
        let mut fake = FakeDownloader::default();
        fake.bodies.insert(url.to_owned(), archive);

        let report = printed(&fetch_entries(&[&absent, &wrong], dir.path(), &fake));
        let messages: Vec<String> = report
            .iter()
            .map(|(_, r)| r.clone().expect_err("fails"))
            .collect();
        assert!(
            messages[0].contains("member `s/zzz.pdf` not found"),
            "{messages:?}"
        );
        assert!(messages[1].contains("sha256 mismatch"), "{messages:?}");
        assert!(!dir.path().join("wrong").exists());
    }

    #[test]
    fn archive_with_wrong_hash_fails_every_member() {
        let dir = TempDir::new("fetch-zip-hash");
        let archive = zip_of(&[("s/a.pdf", b"AAA")]);
        let url = "https://x.test/suite.zip";
        let a = member_entry("zip-a", url, &archive, "s/a.pdf", b"AAA");
        let mut fake = FakeDownloader::default();
        let mut other = archive.clone();
        other[0] ^= 0xFF;
        fake.bodies.insert(url.to_owned(), other);

        let report = fetch_entries(&[&a], dir.path(), &fake);
        let message = printed(&report)[0].1.clone().expect_err("fails");
        assert!(
            message
                .starts_with("archive https://x.test/suite.zip: https://x.test/suite.zip: sha256 mismatch"),
            "{message}"
        );
        // smoke: the archive failure keeps its cause as the error source
        let error = report[0].1.as_ref().expect_err("fails");
        assert!(
            error
                .source()
                .is_some_and(|cause| cause.to_string().contains("sha256 mismatch"))
        );
    }
}
