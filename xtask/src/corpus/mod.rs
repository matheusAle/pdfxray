//! Tier 3 corpus: `corpus list | fetch | verify`.

pub mod fetch;
pub mod manifest;

use std::io::Write;
use std::path::Path;

use anyhow::{Result, bail};

use self::fetch::{Downloader, FetchOutcome, describe, fetch_entries};
use self::manifest::{CorpusFile, CorpusManifest};
use crate::hash::{FileStatus, check_file};
use crate::paths;

/// Entries selected by an optional `--id`.
fn select<'a>(manifest: &'a CorpusManifest, id: Option<&str>) -> Result<Vec<&'a CorpusFile>> {
    let Some(id) = id else {
        return Ok(manifest.files.iter().collect());
    };
    if let Some(entry) = manifest.get(id) {
        Ok(vec![entry])
    } else {
        let known: Vec<&str> = manifest.files.iter().map(|f| f.id.as_str()).collect();
        bail!("no corpus entry with id `{id}`; known ids: {}", known.join(", "))
    }
}

/// `corpus list`: one line per entry with its cache state.
pub fn list(root: &Path, out: &mut dyn Write) -> Result<()> {
    let manifest = CorpusManifest::load(&paths::corpus_manifest(root))?;
    let cache = paths::cache_dir(root);
    for entry in &manifest.files {
        let state = match check_file(&entry.cache_path(&cache), &entry.sha256, entry.size)? {
            FileStatus::Ok => "cached",
            FileStatus::Missing => "not fetched",
            _ => "INVALID",
        };
        writeln!(
            out,
            "{:<44} {:>9} B  {:<24} {state}",
            entry.id, entry.size, entry.license
        )?;
    }
    writeln!(out, "{} entries", manifest.files.len())?;
    Ok(())
}

/// `corpus fetch [--id ID]`.
pub fn fetch_cmd(
    root: &Path,
    id: Option<&str>,
    downloader: &dyn Downloader,
    out: &mut dyn Write,
) -> Result<()> {
    let manifest = CorpusManifest::load(&paths::corpus_manifest(root))?;
    let entries = select(&manifest, id)?;
    let report = fetch_entries(&entries, &paths::cache_dir(root), downloader);
    let mut failed = 0usize;
    for (id, result) in &report {
        match result {
            Ok(FetchOutcome::Cached) => writeln!(out, "cached   {id}")?,
            Ok(FetchOutcome::Fetched) => writeln!(out, "fetched  {id}")?,
            Err(error) => {
                failed += 1;
                writeln!(out, "FAILED   {id}: {}", describe(error))?;
            }
        }
    }
    writeln!(out, "{} entries, {failed} failed", report.len())?;
    if failed > 0 {
        bail!("{failed} corpus entries failed to fetch");
    }
    Ok(())
}

/// `corpus verify`: non-zero exit on any missing file or mismatch.
pub fn verify_cmd(root: &Path, out: &mut dyn Write) -> Result<()> {
    let manifest = CorpusManifest::load(&paths::corpus_manifest(root))?;
    let problems = verify_into(&manifest, &paths::cache_dir(root), out)?;
    if problems > 0 {
        bail!(
            "{problems} corpus files are missing or do not match the manifest; run `cargo xtask corpus fetch`"
        );
    }
    Ok(())
}

/// Writes one status line per entry and returns the number of problems.
pub fn verify_into(manifest: &CorpusManifest, cache: &Path, out: &mut dyn Write) -> Result<usize> {
    let mut problems = 0usize;
    for entry in &manifest.files {
        let path = entry.cache_path(cache);
        let status = check_file(&path, &entry.sha256, entry.size)?;
        if status.is_ok() {
            writeln!(out, "ok       {}", entry.id)?;
        } else {
            problems += 1;
            writeln!(out, "PROBLEM  {}: {status} ({})", entry.id, path.display())?;
        }
    }
    writeln!(out, "{} entries, {problems} problems", manifest.files.len())?;
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::sha256_hex;
    use crate::test_util::TempDir;

    fn manifest_for(bodies: &[(&str, &[u8])]) -> CorpusManifest {
        CorpusManifest {
            files: bodies
                .iter()
                .map(|(id, body)| CorpusFile {
                    id: (*id).to_owned(),
                    url: format!("https://x.test/{id}.pdf"),
                    archive_member: None,
                    archive_sha256: None,
                    archive_size: None,
                    sha256: sha256_hex(body),
                    size: body.len() as u64,
                    license: "unknown – do not vendor".to_owned(),
                    source: "test".to_owned(),
                    notes: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn verify_reports_each_file_and_counts_problems() {
        let dir = TempDir::new("corpus-verify");
        let manifest = manifest_for(&[
            ("good", b"good body"),
            ("gone", b"never fetched"),
            ("bad", b"original"),
        ]);
        dir.write("good/good.pdf", b"good body");
        dir.write("bad/bad.pdf", b"tampered");

        let mut out = Vec::new();
        let problems = verify_into(&manifest, dir.path(), &mut out).expect("verify");
        let text = String::from_utf8(out).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(problems, 2);
        assert_eq!(lines[0], "ok       good");
        assert!(lines[1].starts_with("PROBLEM  gone: missing ("), "{}", lines[1]);
        assert!(
            lines[2].starts_with("PROBLEM  bad: sha256 mismatch: expected "),
            "{}",
            lines[2]
        );
        assert_eq!(lines[3], "3 entries, 2 problems");
    }

    #[test]
    fn verify_of_a_clean_cache_has_no_problems() {
        let dir = TempDir::new("corpus-verify-ok");
        let manifest = manifest_for(&[("a", b"aaa"), ("b", b"bbbb")]);
        dir.write("a/a.pdf", b"aaa");
        dir.write("b/b.pdf", b"bbbb");
        let mut out = Vec::new();
        assert_eq!(verify_into(&manifest, dir.path(), &mut out).expect("verify"), 0);
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            "ok       a\nok       b\n2 entries, 0 problems\n"
        );
    }

    #[test]
    fn select_by_id_and_unknown_id() {
        let manifest = manifest_for(&[("a", b"aaa"), ("b", b"bbbb")]);
        let picked = select(&manifest, Some("b")).expect("known id");
        assert_eq!(picked.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["b"]);
        assert_eq!(select(&manifest, None).expect("all").len(), 2);
        let err = select(&manifest, Some("zzz")).expect_err("unknown id");
        assert_eq!(err.to_string(), "no corpus entry with id `zzz`; known ids: a, b");
    }
}
