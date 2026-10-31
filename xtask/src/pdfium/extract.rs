//! Reading a `PDFium` `.tgz`: an allow-list of members, written to paths built here.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use tar::{Archive, EntryType};

use super::manifest::Platform;
use crate::hash::check_bytes;

/// Largest accepted size of a member other than the library (license texts, `VERSION`).
const MAX_AUX_BYTES: u64 = 1024 * 1024;

/// What an archive member is for, decided from its name alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Member {
    /// The dynamic library; must match the pinned hash and size.
    Library,
    /// `VERSION`, `LICENSE` or a file directly inside `licenses/`.
    Aux,
}

/// Normalised relative path of an archive member, or `None` when it could escape the install
/// directory (absolute, `..`, a Windows prefix) or is empty.
fn clean(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn classify(path: &Path, platform: &Platform) -> Option<Member> {
    if path == Path::new(platform.library) {
        return Some(Member::Library);
    }
    let parts: Vec<&std::ffi::OsStr> = path.iter().collect();
    match parts.as_slice() {
        [name] if *name == "VERSION" || *name == "LICENSE" => Some(Member::Aux),
        [dir, _file] if *dir == "licenses" => Some(Member::Aux),
        _ => None,
    }
}

/// Parses the `VERSION` file of a pdfium-binaries archive into `MAJOR.MINOR.BUILD.PATCH`.
pub fn parse_version(text: &str) -> Option<String> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().parse::<u64>().ok()).flatten()
        })
    };
    Some(format!(
        "{}.{}.{}.{}",
        value("MAJOR")?,
        value("MINOR")?,
        value("BUILD")?,
        value("PATCH")?
    ))
}

/// Writes the wanted members of the verified gzip tarball `archive` into the empty directory
/// `into`.
///
/// Only regular files are written. Links, devices, directories and every member outside the
/// allow-list are skipped, and a member whose path could leave `into` is ignored. The library
/// must be present, have exactly the pinned size and hash.
pub fn extract(archive: &[u8], platform: &Platform, into: &Path) -> Result<()> {
    let mut tar = Archive::new(GzDecoder::new(archive));
    let mut found_library = false;
    for entry in tar.entries().context("reading the archive")? {
        let mut entry = entry.context("reading an archive member")?;
        if !matches!(
            entry.header().entry_type(),
            EntryType::Regular | EntryType::Continuous
        ) {
            continue;
        }
        let raw = entry
            .path()
            .context("archive member has an unreadable name")?
            .into_owned();
        let Some(path) = clean(&raw) else { continue };
        let Some(kind) = classify(&path, platform) else {
            continue;
        };

        let (limit, what) = match kind {
            Member::Library => (platform.library_size, "library"),
            Member::Aux => (MAX_AUX_BYTES, "member"),
        };
        let declared = entry.size();
        if declared > limit || (kind == Member::Library && declared != limit) {
            bail!(
                "{what} `{}` is {declared} bytes, expected {}{limit}",
                path.display(),
                if kind == Member::Library { "" } else { "at most " }
            );
        }
        let mut bytes = Vec::with_capacity(usize::try_from(declared).unwrap_or(0));
        entry
            .by_ref()
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading `{}`", path.display()))?;
        if kind == Member::Library {
            let status = check_bytes(&bytes, platform.library_sha256, platform.library_size);
            if !status.is_ok() {
                bail!("library `{}`: {status}", path.display());
            }
            found_library = true;
        }

        let target = into.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&target, &bytes).with_context(|| format!("writing {}", target.display()))?;
    }
    if !found_library {
        bail!("the archive does not contain `{}`", platform.library);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;

    use super::*;
    use crate::hash::sha256_hex;
    use crate::test_util::TempDir;

    /// A gzip tarball with the given regular files. Names are written verbatim, so `..` and
    /// absolute names can be tested.
    pub(crate) fn tgz(members: &[(&str, &[u8])]) -> Vec<u8> {
        tgz_with(
            members
                .iter()
                .map(|(n, b)| (*n, EntryType::Regular, *b))
                .collect::<Vec<_>>()
                .as_slice(),
        )
    }

    pub(crate) fn tgz_with(members: &[(&str, EntryType, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, kind, body) in members {
            let mut header = tar::Header::new_gnu();
            let raw = header.as_old_mut();
            raw.name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(*kind);
            header.set_size(if *kind == EntryType::Regular {
                body.len() as u64
            } else {
                0
            });
            header.set_mode(0o644);
            if *kind == EntryType::Symlink {
                header.set_link_name("/etc/passwd").expect("link name");
            }
            header.set_cksum();
            let data: &[u8] = if *kind == EntryType::Regular { body } else { &[] };
            builder.append(&header, data).expect("append");
        }
        let tar_bytes = builder.into_inner().expect("tar");
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(&tar_bytes).expect("gzip");
        gz.finish().expect("gzip finish")
    }

    const LIB: &[u8] = b"library bytes";

    fn platform() -> Platform {
        Platform {
            id: "test",
            os: "linux",
            arch: "x86_64",
            url: "https://x.test/a.tgz",
            sha256: "0",
            size: 0,
            library: "lib/libpdfium.so",
            library_sha256: Box::leak(sha256_hex(LIB).into_boxed_str()),
            library_size: LIB.len() as u64,
        }
    }

    fn files_under(dir: &Path) -> Vec<String> {
        fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).expect("read dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    out.push(
                        path.strip_prefix(base)
                            .expect("prefix")
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn extracts_library_and_license_files_only() {
        let dir = TempDir::new("extract-ok");
        let archive = tgz(&[
            ("./lib/libpdfium.so", LIB),
            ("VERSION", b"MAJOR=1\n"),
            ("licenses/zlib.txt", b"z"),
            ("licenses/deep/nested.txt", b"n"),
            ("include/fpdfview.h", b"h"),
            ("args.gn", b"x"),
        ]);

        extract(&archive, &platform(), dir.path()).expect("extract");

        assert_eq!(
            files_under(dir.path()),
            ["VERSION", "lib/libpdfium.so", "licenses/zlib.txt"]
        );
        assert_eq!(fs::read(dir.path().join("lib/libpdfium.so")).expect("lib"), LIB);
    }

    #[test]
    fn escaping_names_are_ignored_not_followed() {
        let dir = TempDir::new("extract-slip");
        let outside = dir.path().join("outside");
        let into = dir.path().join("into");
        fs::create_dir_all(&outside).expect("outside");
        fs::create_dir_all(&into).expect("into");
        let archive = tgz(&[
            ("lib/libpdfium.so", LIB),
            ("../VERSION", b"slip"),
            ("../../LICENSE", b"slip"),
            ("/tmp/pdfxray-xtask-absolute", b"slip"),
            ("licenses/../../VERSION", b"slip"),
        ]);

        extract(&archive, &platform(), &into).expect("extract");

        assert_eq!(files_under(&into), ["lib/libpdfium.so"]);
        assert_eq!(files_under(&outside), Vec::<String>::new());
        assert!(!dir.path().join("VERSION").exists());
        assert!(!Path::new("/tmp/pdfxray-xtask-absolute").exists());
    }

    #[test]
    fn links_are_never_materialised() {
        let dir = TempDir::new("extract-link");
        let archive = tgz_with(&[
            ("lib/libpdfium.so", EntryType::Regular, LIB),
            ("LICENSE", EntryType::Symlink, b""),
            ("VERSION", EntryType::Link, b""),
        ]);

        extract(&archive, &platform(), dir.path()).expect("extract");

        assert_eq!(files_under(dir.path()), ["lib/libpdfium.so"]);
    }

    #[test]
    fn a_symlinked_library_does_not_count_as_the_library() {
        let dir = TempDir::new("extract-link-lib");
        let archive = tgz_with(&[("lib/libpdfium.so", EntryType::Symlink, b"")]);

        let err = extract(&archive, &platform(), dir.path()).expect_err("no library");

        assert!(
            err.to_string().contains("does not contain `lib/libpdfium.so`"),
            "{err}"
        );
        assert_eq!(files_under(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn library_size_must_match_the_pin_exactly() {
        let dir = TempDir::new("extract-size");
        let mut p = platform();
        p.library_size += 1;
        let archive = tgz(&[("lib/libpdfium.so", LIB)]);

        let err = extract(&archive, &p, dir.path()).expect_err("size");

        assert!(err.to_string().contains("is 13 bytes, expected 14"), "{err}");
    }

    #[test]
    fn library_hash_must_match_the_pin() {
        let dir = TempDir::new("extract-hash");
        let archive = tgz(&[("lib/libpdfium.so", b"library bytez")]);

        let err = extract(&archive, &platform(), dir.path()).expect_err("hash");

        assert!(err.to_string().contains("sha256 mismatch"), "{err}");
    }

    #[test]
    fn oversized_auxiliary_members_are_rejected() {
        let dir = TempDir::new("extract-big");
        let big = vec![b'a'; usize::try_from(MAX_AUX_BYTES).expect("fits") + 1];
        let archive = tgz(&[("lib/libpdfium.so", LIB), ("LICENSE", &big)]);

        let err = extract(&archive, &platform(), dir.path()).expect_err("too big");

        assert!(err.to_string().contains("at most"), "{err}");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let dir = TempDir::new("extract-garbage");

        let err = extract(b"definitely not gzip", &platform(), dir.path()).expect_err("garbage");

        assert_ne!(err.to_string(), "");
        assert_eq!(files_under(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn clean_normalises_and_rejects_escapes() {
        assert_eq!(clean(Path::new("./lib/x")), Some(PathBuf::from("lib/x")));
        assert_eq!(clean(Path::new("a//b")), Some(PathBuf::from("a/b")));
        assert_eq!(clean(Path::new("../x")), None);
        assert_eq!(clean(Path::new("a/../../x")), None);
        assert_eq!(clean(Path::new("/abs")), None);
        assert_eq!(clean(Path::new("")), None);
        assert_eq!(clean(Path::new(".")), None);
    }

    #[test]
    fn classify_allows_exactly_the_documented_names() {
        let p = platform();
        assert_eq!(classify(Path::new("lib/libpdfium.so"), &p), Some(Member::Library));
        assert_eq!(classify(Path::new("VERSION"), &p), Some(Member::Aux));
        assert_eq!(classify(Path::new("LICENSE"), &p), Some(Member::Aux));
        assert_eq!(classify(Path::new("licenses/icu.txt"), &p), Some(Member::Aux));
        assert_eq!(classify(Path::new("licenses/a/b.txt"), &p), None);
        assert_eq!(classify(Path::new("lib/libother.so"), &p), None);
        assert_eq!(classify(Path::new("bin/VERSION"), &p), None);
        assert_eq!(classify(Path::new("include/fpdfview.h"), &p), None);
    }

    #[test]
    fn parses_the_version_file() {
        assert_eq!(
            parse_version("MAJOR=158\nMINOR=0\nBUILD=8125\nPATCH=0\n").as_deref(),
            Some("158.0.8125.0")
        );
        assert_eq!(parse_version("MAJOR=158\nMINOR=0\nBUILD=8125\n"), None);
        assert_eq!(parse_version("MAJOR=x\nMINOR=0\nBUILD=1\nPATCH=0"), None);
        assert_eq!(parse_version(""), None);
    }
}
