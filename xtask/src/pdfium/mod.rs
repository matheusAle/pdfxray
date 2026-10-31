//! Optional `PDFium` reference renderer: `pdfium fetch | verify`.
//!
//! `PDFium` is never vendored. `fetch` downloads the pinned build for this machine (see
//! [`manifest`]) and installs its dynamic library to `<dest>/<version>/lib/`, where
//! `pdfxray-render`'s `pdfium` feature finds it. Everything downloaded is untrusted: the archive
//! is hashed before it is opened, only an allow-list of members is written (to paths built here,
//! never from the archive), the library is hashed again, and nothing is ever executed.

pub mod extract;
pub mod manifest;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use self::manifest::{Platform, VERSION};
use crate::corpus::fetch::{Downloader, require_ok};
use crate::hash::{FileStatus, check_bytes, check_file};
use crate::paths;
use crate::staging::Staging;

/// Directory that holds `<version>/` installs unless `--dest` says otherwise.
pub fn default_dest(root: &Path) -> PathBuf {
    paths::pdfium_dir(root)
}

/// What [`install`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A verified install was already in place; nothing was downloaded.
    Cached,
    /// The build was downloaded, verified and installed.
    Installed,
}

/// Root of the install of the pinned version below `dest`.
pub fn install_dir(dest: &Path) -> PathBuf {
    dest.join(VERSION)
}

/// Whether the install under `dest` is complete and unmodified.
fn check_install(dest: &Path, platform: &Platform) -> Result<FileStatus> {
    let root = install_dir(dest);
    let status = check_file(
        &root.join(platform.library),
        platform.library_sha256,
        platform.library_size,
    )?;
    if !status.is_ok() {
        return Ok(status);
    }
    let version = fs::read_to_string(root.join("VERSION")).unwrap_or_default();
    if extract::parse_version(&version).as_deref() == Some(VERSION) {
        Ok(FileStatus::Ok)
    } else {
        Ok(FileStatus::Missing)
    }
}

/// Downloads, verifies and installs the pinned build for `platform` below `dest`.
pub fn install(dest: &Path, platform: &Platform, downloader: &dyn Downloader) -> Result<Outcome> {
    if check_install(dest, platform)?.is_ok() {
        return Ok(Outcome::Cached);
    }
    fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    let archive = downloader.download(platform.url, platform.size.saturating_add(1))?;
    // Nothing is parsed before the whole archive matches its pin.
    require_ok(
        &check_bytes(&archive, platform.sha256, platform.size),
        platform.url,
    )?;

    let staging = Staging::new(dest, VERSION)?;
    extract::extract(&archive, platform, staging.path())?;
    staging.keep(&install_dir(dest))?;
    Ok(Outcome::Installed)
}

/// `pdfium fetch [--dest DIR]`.
pub fn fetch_cmd(
    dest: &Path,
    platform: &Platform,
    downloader: &dyn Downloader,
    out: &mut dyn Write,
) -> Result<()> {
    let library = install_dir(dest).join(platform.library);
    match install(dest, platform, downloader)? {
        Outcome::Cached => writeln!(
            out,
            "cached    PDFium {VERSION} ({}) {}",
            platform.id,
            library.display()
        )?,
        Outcome::Installed => writeln!(
            out,
            "installed PDFium {VERSION} ({}) {}",
            platform.id,
            library.display()
        )?,
    }
    Ok(())
}

/// `pdfium verify [--dest DIR]`: non-zero exit unless the installed library matches its pin.
pub fn verify_cmd(dest: &Path, platform: &Platform, out: &mut dyn Write) -> Result<()> {
    let library = install_dir(dest).join(platform.library);
    let status = check_install(dest, platform)?;
    if status.is_ok() {
        writeln!(
            out,
            "ok       PDFium {VERSION} ({}) {}",
            platform.id,
            library.display()
        )?;
        return Ok(());
    }
    writeln!(out, "PROBLEM  {}: {status}", library.display())?;
    bail!("PDFium {VERSION} is missing or does not match the pin; run `cargo xtask pdfium fetch`")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::hash::sha256_hex;
    use crate::test_util::TempDir;

    /// An in-memory archive builder plus the pin that describes it.
    struct Fake {
        platform: Platform,
        archive: Vec<u8>,
    }

    const LIB_BODY: &[u8] = b"pretend this is a dynamic library";

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    fn fake_with(members: &[(&str, &[u8])], library_body: &[u8]) -> Fake {
        let archive = extract::tests::tgz(members);
        Fake {
            platform: Platform {
                id: "test-x64",
                os: "linux",
                arch: "x86_64",
                url: "https://x.test/pdfium-test-x64.tgz",
                sha256: leak(sha256_hex(&archive)),
                size: archive.len() as u64,
                library: "lib/libpdfium.so",
                library_sha256: leak(sha256_hex(library_body)),
                library_size: library_body.len() as u64,
            },
            archive,
        }
    }

    fn version_file() -> Vec<u8> {
        let parts: Vec<&str> = VERSION.split('.').collect();
        format!(
            "MAJOR={}\nMINOR={}\nBUILD={}\nPATCH={}\n",
            parts[0], parts[1], parts[2], parts[3]
        )
        .into_bytes()
    }

    fn good() -> Fake {
        let version = version_file();
        fake_with(
            &[
                ("LICENSE", b"BSD"),
                ("VERSION", &version),
                ("lib/libpdfium.so", LIB_BODY),
                ("licenses/zlib.txt", b"zlib"),
                ("include/fpdfview.h", b"// header"),
            ],
            LIB_BODY,
        )
    }

    struct FakeDownloader {
        body: Vec<u8>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeDownloader {
        fn new(body: &[u8]) -> Self {
            Self {
                body: body.to_vec(),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Downloader for FakeDownloader {
        fn download(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
            self.calls.borrow_mut().push(url.to_owned());
            if u64::try_from(self.body.len())? > max_bytes {
                bail!("body larger than limit {max_bytes}");
            }
            Ok(self.body.clone())
        }
    }

    fn entries_of(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn installs_the_library_and_only_the_allow_listed_members() {
        let dir = TempDir::new("pdfium-install");
        let fake = good();
        let downloader = FakeDownloader::new(&fake.archive);

        let outcome = install(dir.path(), &fake.platform, &downloader).expect("install");

        assert_eq!(outcome, Outcome::Installed);
        let root = install_dir(dir.path());
        assert_eq!(fs::read(root.join("lib/libpdfium.so")).expect("lib"), LIB_BODY);
        assert_eq!(fs::read(root.join("LICENSE")).expect("license"), b"BSD");
        assert_eq!(
            fs::read(root.join("licenses/zlib.txt")).expect("licenses"),
            b"zlib"
        );
        assert_eq!(entries_of(&root), ["LICENSE", "VERSION", "lib", "licenses"]);
        // smoke: the staging directory is gone and only the version directory remains
        assert_eq!(entries_of(dir.path()), [VERSION]);
        assert_eq!(*downloader.calls.borrow(), vec![fake.platform.url.to_owned()]);
    }

    #[test]
    fn a_second_fetch_is_cached_and_does_not_download() {
        let dir = TempDir::new("pdfium-cached");
        let fake = good();
        let downloader = FakeDownloader::new(&fake.archive);
        install(dir.path(), &fake.platform, &downloader).expect("first");

        let again = install(dir.path(), &fake.platform, &downloader).expect("second");

        assert_eq!(again, Outcome::Cached);
        assert_eq!(downloader.calls.borrow().len(), 1);
    }

    #[test]
    fn a_tampered_install_is_replaced() {
        let dir = TempDir::new("pdfium-heal");
        let fake = good();
        let downloader = FakeDownloader::new(&fake.archive);
        install(dir.path(), &fake.platform, &downloader).expect("first");
        let library = install_dir(dir.path()).join("lib/libpdfium.so");
        fs::write(&library, b"tampered").expect("tamper");
        fs::write(install_dir(dir.path()).join("stray.txt"), b"x").expect("stray");

        let outcome = install(dir.path(), &fake.platform, &downloader).expect("heal");

        assert_eq!(outcome, Outcome::Installed);
        assert_eq!(fs::read(&library).expect("lib"), LIB_BODY);
        assert!(!install_dir(dir.path()).join("stray.txt").exists());
    }

    #[test]
    fn other_versions_are_left_alone() {
        let dir = TempDir::new("pdfium-siblings");
        dir.write("1.0.0.0/lib/libpdfium.so", b"old");
        let fake = good();

        install(dir.path(), &fake.platform, &FakeDownloader::new(&fake.archive)).expect("install");

        assert_eq!(
            fs::read(dir.path().join("1.0.0.0/lib/libpdfium.so")).expect("old"),
            b"old"
        );
    }

    #[test]
    fn archive_with_the_wrong_hash_is_rejected_before_anything_is_written() {
        let dir = TempDir::new("pdfium-bad-archive");
        let fake = good();
        let mut tampered = fake.archive.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xFF;

        let err = install(dir.path(), &fake.platform, &FakeDownloader::new(&tampered)).expect_err("rejected");

        let message = format!("{err:#}");
        assert!(message.contains("sha256 mismatch"), "{message}");
        assert!(message.contains(fake.platform.url), "{message}");
        assert_eq!(entries_of(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn archive_of_the_wrong_size_is_rejected() {
        let dir = TempDir::new("pdfium-bad-size");
        let mut fake = good();
        fake.platform.size += 10;

        let err =
            install(dir.path(), &fake.platform, &FakeDownloader::new(&fake.archive)).expect_err("rejected");

        assert!(format!("{err:#}").contains("size mismatch"), "{err:#}");
        assert_eq!(entries_of(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn library_that_differs_from_its_pin_is_rejected_and_nothing_is_installed() {
        let dir = TempDir::new("pdfium-bad-lib");
        let version = version_file();
        // the archive hash is right (it is pinned from what we serve) but the library is not
        let mut fake = fake_with(
            &[("VERSION", &version), ("lib/libpdfium.so", b"evil library body")],
            LIB_BODY,
        );
        fake.platform.library_size = b"evil library body".len() as u64;

        let err =
            install(dir.path(), &fake.platform, &FakeDownloader::new(&fake.archive)).expect_err("rejected");

        assert!(format!("{err:#}").contains("sha256 mismatch"), "{err:#}");
        assert_eq!(entries_of(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn missing_library_member_is_an_error() {
        let dir = TempDir::new("pdfium-no-lib");
        let fake = fake_with(&[("LICENSE", b"BSD")], LIB_BODY);

        let err =
            install(dir.path(), &fake.platform, &FakeDownloader::new(&fake.archive)).expect_err("rejected");

        assert!(
            format!("{err:#}").contains("does not contain `lib/libpdfium.so`"),
            "{err:#}"
        );
        assert_eq!(entries_of(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn download_failure_leaves_nothing_behind() {
        let dir = TempDir::new("pdfium-offline");
        let fake = good();
        let downloader = FakeDownloader::new(&vec![0u8; fake.archive.len() + 100]);

        let err = install(dir.path(), &fake.platform, &downloader).expect_err("too large");

        assert!(format!("{err:#}").contains("larger than limit"), "{err:#}");
        assert_eq!(entries_of(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn verify_accepts_an_install_and_reports_every_kind_of_damage() {
        let dir = TempDir::new("pdfium-verify");
        let fake = good();
        let mut out = Vec::new();

        let missing = verify_cmd(dir.path(), &fake.platform, &mut out).expect_err("not installed");
        assert!(
            missing.to_string().contains("cargo xtask pdfium fetch"),
            "{missing}"
        );
        assert!(String::from_utf8(out.clone()).expect("utf8").contains("missing"));

        install(dir.path(), &fake.platform, &FakeDownloader::new(&fake.archive)).expect("install");
        out.clear();
        verify_cmd(dir.path(), &fake.platform, &mut out).expect("ok");
        let text = String::from_utf8(out.clone()).expect("utf8");
        assert!(
            text.starts_with(&format!("ok       PDFium {VERSION} (test-x64) ")),
            "{text}"
        );

        let library = install_dir(dir.path()).join("lib/libpdfium.so");
        fs::write(&library, b"tampered!").expect("tamper");
        out.clear();
        verify_cmd(dir.path(), &fake.platform, &mut out).expect_err("tampered");
        assert!(String::from_utf8(out.clone()).expect("utf8").contains("PROBLEM"));

        fs::write(&library, LIB_BODY).expect("restore");
        fs::write(
            install_dir(dir.path()).join("VERSION"),
            b"MAJOR=1\nMINOR=2\nBUILD=3\nPATCH=4\n",
        )
        .expect("version");
        out.clear();
        verify_cmd(dir.path(), &fake.platform, &mut out).expect_err("wrong version file");
    }

    #[test]
    fn default_dest_is_target_pdfium() {
        assert_eq!(default_dest(Path::new("/repo")), Path::new("/repo/target/pdfium"));
    }

    #[test]
    fn fetch_prints_what_it_did() {
        let dir = TempDir::new("pdfium-fetch-cmd");
        let fake = good();
        let downloader = FakeDownloader::new(&fake.archive);
        let mut out = Vec::new();

        fetch_cmd(dir.path(), &fake.platform, &downloader, &mut out).expect("first");
        fetch_cmd(dir.path(), &fake.platform, &downloader, &mut out).expect("second");

        let text = String::from_utf8(out).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[0].starts_with(&format!("installed PDFium {VERSION} (test-x64) ")),
            "{text}"
        );
        assert!(
            lines[1].starts_with(&format!("cached    PDFium {VERSION} (test-x64) ")),
            "{text}"
        );
        assert!(lines[0].ends_with("lib/libpdfium.so"), "{text}");
    }
}
