//! One-click installation of a pinned veraPDF release.
//!
//! veraPDF is GPLv3+/MPL-2.0 licensed. pdfxray never bundles or links it: the user's machine
//! downloads the official installer, verifies it against a SHA-256 pinned in this file and
//! runs the veraPDF CLI as a separate process.
//!
//! # What is downloaded
//!
//! * the official Greenfield installer zip from `software.verapdf.org` (SHA-256 pinned,
//!   GPG signed by the veraPDF Consortium upstream),
//! * three extra validation profile files from the `veraPDF-validation-profiles` repository
//!   (CC BY 4.0), pinned by commit and SHA-256, for the profiles that are not built into the
//!   CLI: WCAG 2.2 machine, ISO 32000-1 tagged and ISO 32000-2 tagged.
//!
//! # Layout
//!
//! ```text
//! <target_dir>/1.30.3/
//!     verapdf               launcher script (not used by pdfxray, kept for users)
//!     bin/cli-1.30.3.jar    the CLI, run as `java -jar`
//!     bin/gui-1.30.3.jar    the GUI (unused, part of the validation-model pack)
//!     profiles/*.xml        extra profile files (not part of the installer; see below)
//! ```
//!
//! # Safety
//!
//! Downloads are untrusted until their SHA-256 matches. Everything happens in a freshly
//! created staging directory inside `target_dir`; it is removed on every outcome, so a failed
//! install leaves nothing behind. The only code executed is the `java -jar` run of the
//! installer jar taken from the verified archive (headless, unattended, installing into the
//! staging directory) and, afterwards, the installed CLI.
//!
//! # Bumping the pinned release
//!
//! 1. Pick the new version at <https://software.verapdf.org/releases/> and download
//!    `<minor>/verapdf-greenfield-<version>-installer.zip` plus its `.asc` signature.
//! 2. Verify the signature (`gpg --verify`, key fingerprint
//!    `13DD 102B 4DD6 9354 D12D E5A8 3184 8632 78B1 7FE7`) and compute
//!    `shasum -a 256` and the byte size of the zip.
//! 3. For the profile files choose the commit of `veraPDF-validation-profiles` on the branch
//!    `rel/<minor>`, download `PDF_UA/{WCAG-2-2-Machine,ISO-32000-1-Tagged,ISO-32000-2-Tagged}.xml`
//!    from `raw.githubusercontent.com/.../<commit>/PDF_UA/` and compute their hashes and sizes.
//! 4. Update the constants in [`Release::pinned`] (version, URLs, hashes, sizes, jar name).
//! 5. Run `PDFXRAY_VERAPDF=required cargo test -p pdfxray-verapdf`.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pdfxray_core::numeric::u64_from_usize;
use sha2::{Digest, Sha256};

use crate::java::{JavaRuntime, find_java};
use crate::process::{ProcError, run_captured};
use crate::verapdf::VeraPdf;
use crate::zip::{ZipError, extract_entry_to_file};

/// Upper bound for the extracted installer jar.
const MAX_INSTALLER_JAR: u64 = 128 * 1024 * 1024;
const INSTALLER_TIMEOUT: Duration = Duration::from_secs(600);

/// One downloadable, hash-pinned file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// File name it is stored under.
    pub file_name: String,
    /// Download URL.
    pub url: String,
    /// Lower-case hex SHA-256.
    pub sha256: String,
    /// Exact size in bytes; also the download cap.
    pub size: u64,
}

/// A pinned veraPDF release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// veraPDF version, e.g. `1.30.3`.
    pub version: String,
    /// The official installer zip.
    pub installer: Asset,
    /// Path of the installer jar inside the zip.
    pub installer_entry: String,
    /// Extra profile files (installed into `profiles/`).
    pub profiles: Vec<Asset>,
}

impl Release {
    /// veraPDF v1.30.3 (released 2026-10-09).
    #[must_use]
    pub fn pinned() -> Self {
        let profiles_commit = "c4f57effb0d3618827c87bd71882930d752be6e3"; // rel/1.30
        let profile = |name: &str, sha256: &str, size: u64| Asset {
            file_name: name.to_owned(),
            url: format!(
                "https://raw.githubusercontent.com/veraPDF/veraPDF-validation-profiles/{profiles_commit}/PDF_UA/{name}"
            ),
            sha256: sha256.to_owned(),
            size,
        };
        Self {
            version: "1.30.3".to_owned(),
            installer: Asset {
                file_name: "verapdf-greenfield-1.30.3-installer.zip".to_owned(),
                url: "https://software.verapdf.org/releases/1.30/verapdf-greenfield-1.30.3-installer.zip"
                    .to_owned(),
                sha256: "b512bf24945503a33fc10738ecffc1f2dccaaf8593a448e94ee9ac86a9559a59".to_owned(),
                size: 32_971_125,
            },
            installer_entry: "verapdf-greenfield-1.30.3/verapdf-izpack-installer-1.30.3.jar".to_owned(),
            profiles: vec![
                profile(
                    "WCAG-2-2-Machine.xml",
                    "7da399cf5baa8eee04ddd80207739a1ff7bb4a1908c3942991267a7e1d2276fb",
                    86_714,
                ),
                profile(
                    "ISO-32000-1-Tagged.xml",
                    "65c334eb7aafc846b073c12d71a72fb4c9638dc1bc85c93f752284e4132e46cd",
                    750_806,
                ),
                profile(
                    "ISO-32000-2-Tagged.xml",
                    "ba2af40c2c0bd1789e4630ce7afb4f4a23b909a0e65512163b4c976e019a1344",
                    973_336,
                ),
            ],
        }
    }
}

/// Why a download failed.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// Transport or HTTP error.
    #[error("{0}")]
    Transport(String),
    /// Writing the downloaded bytes failed.
    #[error("writing download failed: {0}")]
    Io(#[from] io::Error),
}

/// Download abstraction, so installation can be tested without a network.
pub trait Fetcher {
    /// Streams the body of `url` into `sink` and reports `(bytes so far, total if known)`.
    /// Implementations should stop once `max_bytes` have been delivered.
    fn fetch(
        &self,
        url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), FetchError>;
}

/// [`Fetcher`] backed by `ureq` (HTTPS, redirects followed).
#[derive(Debug, Clone, Copy, Default)]
pub struct UreqFetcher;

impl Fetcher for UreqFetcher {
    fn fetch(
        &self,
        url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), FetchError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_mins(15)))
            .build()
            .into();
        let response = agent
            .get(url)
            .call()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        let total = response.body().content_length();
        let mut reader = response.into_body().into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            sink.write_all(&buf[..n])?;
            done += u64_from_usize(n);
            progress(done, total);
            if done > max_bytes {
                // The sink rejects the excess as well; stop reading regardless.
                return Ok(());
            }
        }
    }
}

/// Everything that can go wrong during [`install`].
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// No Java 11+ runtime was found; veraPDF cannot run without it.
    #[error("no Java 11+ runtime found")]
    JavaMissing,
    /// A filesystem operation failed.
    #[error("{action}: {source}")]
    Io {
        /// What was being done.
        action: String,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// Downloading failed.
    #[error("downloading {url} failed: {source}")]
    Fetch {
        /// URL.
        url: String,
        /// Underlying error.
        #[source]
        source: FetchError,
    },
    /// A download delivered more bytes than pinned.
    #[error("download of {url} exceeded the pinned size of {limit} bytes")]
    TooLarge {
        /// URL.
        url: String,
        /// Pinned size.
        limit: u64,
    },
    /// A download does not match its pinned SHA-256; it was discarded.
    #[error("SHA-256 mismatch for {file}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// File name.
        file: String,
        /// Pinned hash.
        expected: String,
        /// Computed hash.
        actual: String,
    },
    /// The installer archive could not be unpacked.
    #[error("installer archive is unusable: {0}")]
    Archive(#[from] ZipError),
    /// The installer process could not be started.
    #[error("could not start the veraPDF installer: {0}")]
    Spawn(#[source] io::Error),
    /// The installer did not finish in time.
    #[error("the veraPDF installer did not finish within {0:?}")]
    InstallerTimeout(Duration),
    /// The installer exited unsuccessfully or did not produce the expected files.
    #[error("the veraPDF installer failed ({detail}): {output}")]
    InstallerFailed {
        /// Exit status or missing file description.
        detail: String,
        /// Captured stderr/stdout tail.
        output: String,
    },
    /// The installed CLI does not run or reports an unexpected version.
    #[error("the installed veraPDF failed verification: {0}")]
    Verification(String),
}

impl InstallError {
    fn io(action: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            action: action.into(),
            source,
        }
    }
}

/// Installs the [pinned](Release::pinned) veraPDF release below `target_dir`, downloading over
/// HTTPS. `progress` receives `(bytes so far, total if known)` for each download in turn.
///
/// Returns the installed, version-checked [`VeraPdf`]. Blocking: call it off the UI thread.
pub fn install(target_dir: &Path, progress: impl FnMut(u64, Option<u64>)) -> Result<VeraPdf, InstallError> {
    let java = find_java(None).ok_or(InstallError::JavaMissing)?;
    install_with(target_dir, &Release::pinned(), &UreqFetcher, &java, progress)
}

/// [`install`] with an explicit release, downloader and Java runtime.
pub fn install_with(
    target_dir: &Path,
    release: &Release,
    fetcher: &dyn Fetcher,
    java: &JavaRuntime,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<VeraPdf, InstallError> {
    fs::create_dir_all(target_dir)
        .map_err(|e| InstallError::io(format!("creating {}", target_dir.display()), e))?;
    let staging = Staging::create(target_dir)?;
    let downloads = staging.path().join("downloads");
    let work = staging.path().join("work");
    let root = staging.path().join("root");
    for dir in [&downloads, &work, &root.join("profiles")] {
        fs::create_dir_all(dir).map_err(|e| InstallError::io(format!("creating {}", dir.display()), e))?;
    }

    // 1. Download everything and verify it before anything is opened or executed.
    let installer_zip = downloads.join("installer.zip");
    download_verified(fetcher, &release.installer, &installer_zip, &mut progress)?;
    for profile in &release.profiles {
        download_verified(
            fetcher,
            profile,
            &root.join("profiles").join(&profile.file_name),
            &mut progress,
        )?;
    }

    // 2. Unpack the installer jar from the verified archive, under a fixed name.
    let installer_jar = downloads.join("installer.jar");
    extract_entry_to_file(
        &installer_zip,
        &release.installer_entry,
        MAX_INSTALLER_JAR,
        &installer_jar,
    )?;

    // 3. Run the (verified) veraPDF installer headless into the staging root.
    let answers = work.join("auto-install.xml");
    fs::write(&answers, auto_install_xml(&root))
        .map_err(|e| InstallError::io(format!("writing {}", answers.display()), e))?;
    run_installer(java, &installer_jar, &answers, &work, &release.version, &root)?;

    // 4. Move into place and check that the result actually runs.
    let final_dir = target_dir.join(&release.version);
    if final_dir.exists() {
        fs::remove_dir_all(&final_dir)
            .map_err(|e| InstallError::io(format!("replacing {}", final_dir.display()), e))?;
    }
    fs::rename(&root, &final_dir)
        .map_err(|e| InstallError::io(format!("moving the install to {}", final_dir.display()), e))?;
    let verified = VeraPdf::open(&java.executable, &final_dir)
        .filter(|v| v.version() == release.version)
        .ok_or_else(|| {
            let _ = fs::remove_dir_all(&final_dir);
            InstallError::Verification(format!(
                "`{}` does not run or is not version {}",
                final_dir.display(),
                release.version
            ))
        })?;
    Ok(verified)
}

fn run_installer(
    java: &JavaRuntime,
    installer_jar: &Path,
    answers: &Path,
    work: &Path,
    version: &str,
    root: &Path,
) -> Result<(), InstallError> {
    let mut cmd = Command::new(&java.executable);
    cmd.current_dir(work)
        .arg("-Djava.awt.headless=true")
        .arg("-jar")
        .arg(installer_jar)
        .arg(answers);
    let out =
        run_captured(&mut cmd, INSTALLER_TIMEOUT, None, 256 * 1024, 256 * 1024).map_err(|e| match e {
            ProcError::Spawn(e) | ProcError::Wait(e) => InstallError::Spawn(e),
            ProcError::Timeout => InstallError::InstallerTimeout(INSTALLER_TIMEOUT),
            ProcError::Cancelled => InstallError::InstallerFailed {
                detail: "cancelled".to_owned(),
                output: String::new(),
            },
        })?;
    let jar = root.join("bin").join(format!("cli-{version}.jar"));
    if !out.status.success() || !jar.is_file() {
        let detail = if out.status.success() {
            format!("missing {}", jar.display())
        } else {
            out.status.to_string()
        };
        let stderr = out.stderr_text();
        let output = if stderr.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        } else {
            stderr
        };
        return Err(InstallError::InstallerFailed { detail, output });
    }
    Ok(())
}

/// Unattended `IzPack` answers: the scripts and validation-model packs (which carry the CLI
/// jar), without documentation and sample plugins, into `root`. The pack names are those of
/// the 1.30 installer; bump them together with the release if upstream renames them.
fn auto_install_xml(root: &Path) -> String {
    let path = xml_escape(&root.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<AutomatedInstallation langpack="eng">
    <com.izforge.izpack.panels.htmlhello.HTMLHelloPanel id="welcome"/>
    <com.izforge.izpack.panels.target.TargetPanel id="install_dir">
        <installpath>{path}</installpath>
    </com.izforge.izpack.panels.target.TargetPanel>
    <com.izforge.izpack.panels.packs.PacksPanel id="sdk_pack_select">
        <pack index="0" name="veraPDF Mac and *nix Scripts" selected="true"/>
        <pack index="1" name="veraPDF Validation model" selected="true"/>
        <pack index="2" name="veraPDF Documentation" selected="false"/>
        <pack index="3" name="veraPDF Sample Plugins" selected="false"/>
    </com.izforge.izpack.panels.packs.PacksPanel>
    <com.izforge.izpack.panels.install.InstallPanel id="install"/>
    <com.izforge.izpack.panels.finish.FinishPanel id="finish"/>
</AutomatedInstallation>
"#
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Downloads `asset` to `dest` and checks its SHA-256. On mismatch `dest` is removed.
fn download_verified(
    fetcher: &dyn Fetcher,
    asset: &Asset,
    dest: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), InstallError> {
    let file = File::create(dest).map_err(|e| InstallError::io(format!("creating {}", dest.display()), e))?;
    let mut sink = HashingWriter {
        inner: file,
        hasher: Sha256::new(),
        written: 0,
        limit: asset.size,
        exceeded: false,
    };
    let outcome = fetcher.fetch(&asset.url, asset.size, &mut sink, progress);
    if sink.exceeded {
        let _ = fs::remove_file(dest);
        return Err(InstallError::TooLarge {
            url: asset.url.clone(),
            limit: asset.size,
        });
    }
    if let Err(source) = outcome {
        let _ = fs::remove_file(dest);
        return Err(InstallError::Fetch {
            url: asset.url.clone(),
            source,
        });
    }
    sink.inner
        .flush()
        .map_err(|e| InstallError::io(format!("writing {}", dest.display()), e))?;
    let actual = hex(&sink.hasher.finalize());
    if !actual.eq_ignore_ascii_case(&asset.sha256) {
        let _ = fs::remove_file(dest);
        return Err(InstallError::ChecksumMismatch {
            file: asset.file_name.clone(),
            expected: asset.sha256.clone(),
            actual,
        });
    }
    Ok(())
}

/// Writes to a file while hashing and enforcing a size cap.
struct HashingWriter {
    inner: File,
    hasher: Sha256,
    written: u64,
    limit: u64,
    exceeded: bool,
}

impl Write for HashingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written + u64_from_usize(buf.len()) > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("download exceeds its pinned size"));
        }
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written += u64_from_usize(n);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

/// A freshly created staging directory that is removed when dropped.
struct Staging(PathBuf);

impl Staging {
    fn create(parent: &Path) -> Result<Self, InstallError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = parent.join(format!(".staging-{}-{nanos}", std::process::id()));
        // `create_dir` (not `create_dir_all`) fails if the directory already exists.
        fs::create_dir(&path).map_err(|e| InstallError::io(format!("creating {}", path.display()), e))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Serves canned bodies by URL, in two chunks, reporting progress.
    #[derive(Default)]
    struct FakeFetcher {
        bodies: HashMap<String, Vec<u8>>,
        requested: RefCell<Vec<String>>,
    }

    impl Fetcher for FakeFetcher {
        fn fetch(
            &self,
            url: &str,
            _max_bytes: u64,
            sink: &mut dyn Write,
            progress: &mut dyn FnMut(u64, Option<u64>),
        ) -> Result<(), FetchError> {
            self.requested.borrow_mut().push(url.to_owned());
            let body = self
                .bodies
                .get(url)
                .ok_or_else(|| FetchError::Transport(format!("404 for {url}")))?;
            let total = Some(body.len() as u64);
            let mut done = 0u64;
            for chunk in body.chunks(body.len().div_ceil(2).max(1)) {
                sink.write_all(chunk)?;
                done += chunk.len() as u64;
                progress(done, total);
            }
            Ok(())
        }
    }

    fn sha_of(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn asset(name: &str, body: &[u8]) -> Asset {
        Asset {
            file_name: name.to_owned(),
            url: format!("https://example.invalid/{name}"),
            sha256: sha_of(body),
            size: body.len() as u64,
        }
    }

    fn fake_release(installer_body: &[u8]) -> (Release, FakeFetcher) {
        let installer = asset("installer.zip", installer_body);
        let profile_body = b"<profile/>".to_vec();
        let profile = asset("Extra.xml", &profile_body);
        let mut fetcher = FakeFetcher::default();
        fetcher
            .bodies
            .insert(installer.url.clone(), installer_body.to_vec());
        fetcher.bodies.insert(profile.url.clone(), profile_body);
        let release = Release {
            version: "9.9.9".to_owned(),
            installer,
            installer_entry: "pkg/installer.jar".to_owned(),
            profiles: vec![profile],
        };
        (release, fetcher)
    }

    fn bogus_java() -> JavaRuntime {
        JavaRuntime {
            executable: PathBuf::from("/definitely/not/java"),
            version: "21.0.0".to_owned(),
            major: 21,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        crate::scratch::dir(&format!("install-{name}"))
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn pinned_release_is_well_formed() {
        let release = Release::pinned();
        let assets: Vec<&Asset> = std::iter::once(&release.installer)
            .chain(&release.profiles)
            .collect();
        for a in &assets {
            assert_eq!(a.sha256.len(), 64, "{}", a.file_name);
            assert!(
                a.sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{}",
                a.file_name
            );
            assert!(a.url.starts_with("https://"), "{}", a.url);
        }
        assert_eq!(
            (
                release.version.as_str(),
                release.installer.file_name.as_str(),
                release.installer.size,
                release.installer_entry.as_str(),
                release
                    .profiles
                    .iter()
                    .map(|p| p.file_name.as_str())
                    .collect::<Vec<_>>(),
            ),
            (
                "1.30.3",
                "verapdf-greenfield-1.30.3-installer.zip",
                32_971_125,
                "verapdf-greenfield-1.30.3/verapdf-izpack-installer-1.30.3.jar",
                vec![
                    "WCAG-2-2-Machine.xml",
                    "ISO-32000-1-Tagged.xml",
                    "ISO-32000-2-Tagged.xml"
                ],
            )
        );
    }

    #[test]
    fn checksum_mismatch_installs_nothing() {
        let target = scratch("mismatch");
        let (mut release, mut fetcher) = fake_release(b"genuine bytes");
        // The server now delivers something else than what was pinned.
        fetcher
            .bodies
            .insert(release.installer.url.clone(), b"tampered bytes".to_vec());
        release.installer.size = 14;
        let err = install_with(&target, &release, &fetcher, &bogus_java(), |_, _| {}).expect_err("must fail");
        match err {
            InstallError::ChecksumMismatch {
                file,
                expected,
                actual,
            } => {
                assert_eq!(file, "installer.zip");
                assert_eq!(expected, sha_of(b"genuine bytes"));
                assert_eq!(actual, sha_of(b"tampered bytes"));
            }
            other => panic!("expected a checksum mismatch, got {other:?}"),
        }
        assert_eq!(
            entries(&target),
            Vec::<String>::new(),
            "nothing may be left behind"
        );
        // Only the installer was requested: the profiles are not fetched after a bad hash.
        assert_eq!(
            fetcher.requested.borrow().clone(),
            vec![release.installer.url.clone()]
        );
    }

    #[test]
    fn oversized_download_is_rejected() {
        let target = scratch("oversized");
        let (mut release, fetcher) = fake_release(b"0123456789");
        release.installer.size = 4;
        let err = install_with(&target, &release, &fetcher, &bogus_java(), |_, _| {}).expect_err("must fail");
        assert!(
            matches!(err, InstallError::TooLarge { limit: 4, .. }),
            "got {err:?}"
        );
        assert_eq!(entries(&target), Vec::<String>::new());
    }

    #[test]
    fn failing_download_installs_nothing() {
        let target = scratch("offline");
        let (release, mut fetcher) = fake_release(b"whatever");
        fetcher.bodies.clear();
        let err = install_with(&target, &release, &fetcher, &bogus_java(), |_, _| {}).expect_err("must fail");
        assert!(
            matches!(&err, InstallError::Fetch { url, .. } if url == "https://example.invalid/installer.zip"),
            "got {err:?}"
        );
        assert_eq!(entries(&target), Vec::<String>::new());
    }

    #[test]
    fn archive_without_the_installer_jar_installs_nothing() {
        let target = scratch("no-jar");
        // A hash-correct download that is not a zip at all.
        let (release, fetcher) = fake_release(b"PK-not-really-a-zip-archive-at-all");
        let err = install_with(&target, &release, &fetcher, &bogus_java(), |_, _| {}).expect_err("must fail");
        assert!(matches!(err, InstallError::Archive(_)), "got {err:?}");
        assert_eq!(entries(&target), Vec::<String>::new());
    }

    #[test]
    fn progress_reports_every_download() {
        let target = scratch("progress");
        let (release, fetcher) = fake_release(b"abcdef");
        let mut seen = Vec::new();
        let _ = install_with(&target, &release, &fetcher, &bogus_java(), |done, total| {
            seen.push((done, total));
        });
        // installer (6 bytes in 3+3) then the profile (10 bytes in 5+5), where the archive step
        // then fails because the download is not a zip.
        assert_eq!(
            seen,
            vec![(3, Some(6)), (6, Some(6)), (5, Some(10)), (10, Some(10))]
        );
    }

    #[test]
    fn answers_file_escapes_the_install_path() {
        let xml = auto_install_xml(Path::new("/tmp/a&b <\"x\">/root"));
        assert!(
            xml.contains("<installpath>/tmp/a&amp;b &lt;&quot;x&quot;&gt;/root</installpath>"),
            "{xml}"
        );
        assert!(xml.contains(r#"<pack index="1" name="veraPDF Validation model" selected="true"/>"#));
        assert!(xml.contains(r#"<pack index="2" name="veraPDF Documentation" selected="false"/>"#));
    }

    #[test]
    fn hex_is_lower_case_and_padded() {
        assert_eq!(hex(&[0x00, 0x0a, 0xff]), "000aff");
    }
}
