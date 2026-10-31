//! Sparkle 2, the auto-updater embedded in the macOS app: `sparkle fetch | keygen | appcast`.
//!
//! * `fetch` downloads the pinned release ([`manifest::PINNED`]) and installs
//!   `Sparkle.framework`, `bin/sign_update`, `bin/generate_keys` and the license into
//!   `target/sparkle/<version>/`. `cargo xtask bundle` does the same by itself when the install is
//!   missing. The archive is hashed before anything reads it; it is then unpacked with the system
//!   `tar` (which keeps the framework's symlinks and refuses members that leave the directory).
//! * `keygen` makes the Ed25519 key pair that signs updates ([`keys`]).
//! * `appcast` signs the update archive with the private key from `SPARKLE_ED_PRIVATE_KEY` and
//!   writes the feed ([`appcast`]).
//!
//! The app loads the framework at run time from `Contents/Frameworks` (`pdfxray-macos`); nothing
//! links against it, so builds from source simply have no updater.

pub mod appcast;
pub mod keys;
pub mod manifest;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use self::keys::{ENV_PRIVATE_KEY, KeyPair};
use self::manifest::{FRAMEWORK, FRAMEWORK_BINARY, KEPT, Pin};
use crate::bundle::cmd;
use crate::bundle::version::Version;
use crate::corpus::fetch::{Downloader, require_ok};
use crate::hash::{FileStatus, check_bytes, check_file};
use crate::paths;
use crate::staging::Staging;

/// The feed installed copies read (`SUFeedURL`).
pub const FEED_URL: &str = "https://dl.pdf-xray.app/appcast.xml";

/// Name of the file in an install that records the Sparkle version it holds.
const VERSION_FILE: &str = "VERSION";

/// Seconds since 1970 (UTC).
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// A complete install of a Sparkle release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    root: PathBuf,
}

impl Install {
    /// The install of `pin` below `dest` (it may not exist yet).
    pub fn of(dest: &Path, pin: &Pin) -> Self {
        Self {
            root: dest.join(pin.version),
        }
    }

    /// The install directory, `<dest>/<version>`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `Sparkle.framework`.
    pub fn framework(&self) -> PathBuf {
        self.root.join(FRAMEWORK)
    }

    /// Sparkle's `sign_update` tool.
    pub fn sign_update(&self) -> PathBuf {
        self.root.join(manifest::SIGN_UPDATE)
    }

    /// Whether the install is complete and its framework binary matches the pin.
    fn status(&self, pin: &Pin) -> Result<FileStatus> {
        let status = check_file(
            &self.root.join(FRAMEWORK_BINARY),
            pin.binary_sha256,
            pin.binary_size,
        )?;
        if !status.is_ok() {
            return Ok(status);
        }
        let recorded = fs::read_to_string(self.root.join(VERSION_FILE)).unwrap_or_default();
        let complete = KEPT
            .iter()
            .all(|member| fs::symlink_metadata(self.root.join(member)).is_ok());
        Ok(if recorded.trim() == pin.version && complete {
            FileStatus::Ok
        } else {
            FileStatus::Missing
        })
    }
}

/// Directory that holds `<version>/` installs: `target/sparkle/`.
pub fn default_dest(root: &Path) -> PathBuf {
    paths::sparkle_dir(root)
}

/// What [`install`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A verified install was already in place; nothing was downloaded.
    Cached,
    /// The release was downloaded, verified and installed.
    Installed,
}

/// Downloads, verifies and installs `pin` below `dest`, unless a verified install is there.
pub fn install(dest: &Path, pin: &Pin, downloader: &dyn Downloader) -> Result<(Install, Outcome)> {
    let target = Install::of(dest, pin);
    if target.status(pin)?.is_ok() {
        return Ok((target, Outcome::Cached));
    }
    fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    let archive = downloader.download(pin.url, pin.size.saturating_add(1))?;
    // Nothing reads the archive before the whole of it matches its pin.
    require_ok(&check_bytes(&archive, pin.sha256, pin.size), pin.url)?;

    // The archive is unpacked in one scratch directory, the install is assembled in another and
    // moved into place only when it is complete; both vanish on any error.
    let scratch = Staging::new(dest, &format!("{}-archive", pin.version))?;
    let tree = Staging::new(dest, pin.version)?;
    let tarball = scratch.path().join("archive.tar.xz");
    let unpacked = scratch.path().join("unpacked");
    fs::write(&tarball, &archive).with_context(|| format!("writing {}", tarball.display()))?;
    fs::create_dir(&unpacked).with_context(|| format!("creating {}", unpacked.display()))?;
    cmd::output(
        Command::new("tar")
            .arg("-xJf")
            .arg(&tarball)
            .arg("-C")
            .arg(&unpacked),
    )
    .context("cannot unpack the Sparkle archive")?;
    for member in KEPT {
        let from = unpacked.join(member);
        if fs::symlink_metadata(&from).is_err() {
            bail!("the Sparkle archive does not contain `{member}`");
        }
        let to = tree.path().join(member);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::rename(&from, &to).with_context(|| format!("moving {member} into place"))?;
    }
    let binary = check_file(
        &tree.path().join(FRAMEWORK_BINARY),
        pin.binary_sha256,
        pin.binary_size,
    )?;
    if !binary.is_ok() {
        bail!("{FRAMEWORK_BINARY} in the Sparkle archive: {binary}");
    }
    fs::write(tree.path().join(VERSION_FILE), format!("{}\n", pin.version))
        .with_context(|| format!("writing {VERSION_FILE}"))?;
    tree.keep(target.root())?;
    Ok((target, Outcome::Installed))
}

/// `sparkle fetch [--dest DIR]`.
pub fn fetch_cmd(dest: &Path, pin: &Pin, downloader: &dyn Downloader, out: &mut dyn Write) -> Result<()> {
    let (install, outcome) = install(dest, pin, downloader)?;
    let verb = match outcome {
        Outcome::Cached => "cached   ",
        Outcome::Installed => "installed",
    };
    writeln!(out, "{verb} Sparkle {} {}", pin.version, install.root().display())?;
    Ok(())
}

/// `sparkle keygen`: prints a new key pair.
pub fn keygen_cmd(out: &mut dyn Write) -> Result<()> {
    out.write_all(keys::keygen_report(&KeyPair::generate()?).as_bytes())?;
    Ok(())
}

/// What `sparkle appcast` publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppcastRequest<'a> {
    /// Release version (`0.2.0`, `v0.2.0-rc.1`).
    pub version: &'a str,
    /// The update archive (the DMG).
    pub archive: &'a Path,
    /// Public URL the archive is downloaded from.
    pub url: &'a str,
    /// Where the feed is written.
    pub out: &'a Path,
}

/// `sparkle appcast`: signs the archive with the private key from [`ENV_PRIVATE_KEY`] (read
/// through `env`) and writes a feed with that one release.
///
/// The key must belong to `public_key`, the public key the app ships with ([`keys::PUBLIC_ED_KEY`]);
/// otherwise installed copies would reject the update, so nothing is written.
pub fn appcast_cmd(
    request: &AppcastRequest<'_>,
    public_key: &str,
    env: impl Fn(&str) -> Option<String>,
    now: u64,
    out: &mut dyn Write,
) -> Result<()> {
    let version = Version::parse(request.version)?;
    let bundle_version = version.bundle_version()?;
    if !request.url.starts_with("https://") && !request.url.starts_with("http://") {
        bail!("--url must be an http(s) URL");
    }
    let Some(secret) = env(ENV_PRIVATE_KEY).filter(|v| !v.trim().is_empty()) else {
        bail!("set {ENV_PRIVATE_KEY} to the private key from `cargo xtask sparkle keygen`");
    };
    let pair = KeyPair::from_private_base64(&secret).context(ENV_PRIVATE_KEY)?;
    drop(secret);
    if pair.public_base64() != public_key {
        bail!(
            "{ENV_PRIVATE_KEY} does not belong to the public key the app ships with \
             (`PUBLIC_ED_KEY` in xtask/src/sparkle/keys.rs, SUPublicEDKey in Info.plist); \
             installed copies would reject this update"
        );
    }
    let archive =
        fs::read(request.archive).with_context(|| format!("cannot read {}", request.archive.display()))?;
    let signature = pair.sign_base64(&archive);
    keys::verify(public_key, &archive, &signature)?;
    let length = u64::try_from(archive.len())?;
    let display = version.to_string();
    let xml = appcast::render(&appcast::Item {
        bundle_version: &bundle_version,
        display_version: &display,
        url: request.url,
        length,
        ed_signature: &signature,
        published: now,
    });
    fs::write(request.out, xml).with_context(|| format!("cannot write {}", request.out.display()))?;
    writeln!(
        out,
        "appcast: {} (pdfxray {display}, sparkle:version {bundle_version}, {length} bytes, signed)",
        request.out.display()
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::hash::sha256_hex;
    use crate::test_util::TempDir;

    const BINARY: &[u8] = b"pretend this is the Sparkle framework binary";

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    /// A `.tar.xz` made with the system `tar` from a tree shaped like a Sparkle release (with the
    /// framework's symlinks), plus the pin that describes it.
    fn fake_release(dir: &TempDir, binary: &[u8], skip: Option<&str>) -> (Pin, Vec<u8>) {
        let src = dir.path().join("src");
        let files: [(&str, &[u8]); 6] = [
            (FRAMEWORK_BINARY, binary),
            ("Sparkle.framework/Versions/B/Autoupdate", b"autoupdate"),
            ("bin/sign_update", b"sign"),
            ("bin/generate_keys", b"keys"),
            ("LICENSE", b"MIT"),
            ("bin/BinaryDelta", b"not kept"),
        ];
        for (path, body) in files {
            if skip == Some(path) {
                continue;
            }
            dir.write(&format!("src/{path}"), body);
        }
        std::os::unix::fs::symlink("B", src.join("Sparkle.framework/Versions/Current")).expect("link");
        std::os::unix::fs::symlink("Versions/Current/Sparkle", src.join("Sparkle.framework/Sparkle"))
            .expect("link");
        let archive = dir.path().join("fake.tar.xz");
        cmd::output(
            Command::new("tar")
                .arg("-cJf")
                .arg(&archive)
                .arg("-C")
                .arg(&src)
                .arg("."),
        )
        .expect("tar");
        let bytes = fs::read(&archive).expect("archive");
        let pin = Pin {
            version: "2.99.0",
            url: "https://x.test/Sparkle-2.99.0.tar.xz",
            sha256: leak(sha256_hex(&bytes)),
            size: bytes.len() as u64,
            binary_sha256: leak(sha256_hex(BINARY)),
            binary_size: BINARY.len() as u64,
        };
        (pin, bytes)
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

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn installs_the_framework_with_its_symlinks_and_the_tools() {
        let dir = TempDir::new("sparkle-install");
        let (pin, archive) = fake_release(&dir, BINARY, None);
        let dest = dir.path().join("dest");
        let downloader = FakeDownloader::new(&archive);

        let (install, outcome) = install(&dest, &pin, &downloader).expect("install");

        assert_eq!(outcome, Outcome::Installed);
        assert_eq!(install.root(), dest.join("2.99.0"));
        assert_eq!(
            names(install.root()),
            ["LICENSE", "Sparkle.framework", "VERSION", "bin"]
        );
        assert_eq!(
            names(&install.root().join("bin")),
            ["generate_keys", "sign_update"]
        );
        assert_eq!(
            fs::read_link(install.framework().join("Versions/Current")).expect("link"),
            Path::new("B")
        );
        assert_eq!(
            fs::read(install.framework().join("Sparkle")).expect("via link"),
            BINARY
        );
        assert_eq!(fs::read(install.sign_update()).expect("tool"), b"sign");
        // smoke: no staging leftovers, and the download happened once
        assert_eq!(names(&dest), ["2.99.0"]);
        assert_eq!(*downloader.calls.borrow(), [pin.url.to_owned()]);
    }

    #[test]
    fn a_second_install_is_cached_and_a_damaged_one_is_replaced() {
        let dir = TempDir::new("sparkle-cached");
        let (pin, archive) = fake_release(&dir, BINARY, None);
        let dest = dir.path().join("dest");
        let downloader = FakeDownloader::new(&archive);
        let (first, _) = install(&dest, &pin, &downloader).expect("first");

        let (_, again) = install(&dest, &pin, &downloader).expect("second");
        assert_eq!(again, Outcome::Cached);
        assert_eq!(downloader.calls.borrow().len(), 1);

        fs::write(first.root().join(FRAMEWORK_BINARY), b"tampered").expect("tamper");
        let (_, healed) = install(&dest, &pin, &downloader).expect("heal");
        assert_eq!(healed, Outcome::Installed);
        assert_eq!(
            fs::read(first.root().join(FRAMEWORK_BINARY)).expect("binary"),
            BINARY
        );

        fs::remove_file(first.sign_update()).expect("remove tool");
        let (_, completed) = install(&dest, &pin, &downloader).expect("complete");
        assert_eq!(completed, Outcome::Installed);
        assert_eq!(downloader.calls.borrow().len(), 3);
    }

    #[test]
    fn an_archive_that_differs_from_its_pin_is_rejected_before_it_is_unpacked() {
        let dir = TempDir::new("sparkle-bad-archive");
        let (pin, mut archive) = fake_release(&dir, BINARY, None);
        let last = archive.len() - 1;
        archive[last] ^= 0xFF;
        let dest = dir.path().join("dest");

        let err = install(&dest, &pin, &FakeDownloader::new(&archive)).expect_err("rejected");

        assert!(format!("{err:#}").contains("sha256 mismatch"), "{err:#}");
        assert_eq!(names(&dest), Vec::<String>::new());
    }

    #[test]
    fn a_framework_binary_that_differs_from_its_pin_is_rejected() {
        let dir = TempDir::new("sparkle-bad-binary");
        let (pin, archive) = fake_release(&dir, b"an evil framework binary of other size", None);
        let dest = dir.path().join("dest");

        let err = install(&dest, &pin, &FakeDownloader::new(&archive)).expect_err("rejected");

        assert!(format!("{err:#}").contains("size mismatch"), "{err:#}");
        assert_eq!(names(&dest), Vec::<String>::new());
    }

    #[test]
    fn a_release_without_a_tool_is_rejected() {
        let dir = TempDir::new("sparkle-no-tool");
        let (pin, archive) = fake_release(&dir, BINARY, Some("bin/sign_update"));
        let dest = dir.path().join("dest");

        let err = install(&dest, &pin, &FakeDownloader::new(&archive)).expect_err("rejected");

        assert_eq!(
            err.to_string(),
            "the Sparkle archive does not contain `bin/sign_update`"
        );
        assert_eq!(names(&dest), Vec::<String>::new());
    }

    #[test]
    fn fetch_prints_what_it_did() {
        let dir = TempDir::new("sparkle-fetch-cmd");
        let (pin, archive) = fake_release(&dir, BINARY, None);
        let dest = dir.path().join("dest");
        let downloader = FakeDownloader::new(&archive);
        let mut out = Vec::new();

        fetch_cmd(&dest, &pin, &downloader, &mut out).expect("first");
        fetch_cmd(&dest, &pin, &downloader, &mut out).expect("second");

        let root = dest.join("2.99.0");
        assert_eq!(
            String::from_utf8(out).expect("utf8"),
            format!(
                "installed Sparkle 2.99.0 {0}\ncached    Sparkle 2.99.0 {0}\n",
                root.display()
            )
        );
    }

    #[test]
    fn default_dest_is_target_sparkle() {
        assert_eq!(
            default_dest(Path::new("/repo")),
            Path::new("/repo/target/sparkle")
        );
    }

    fn env(pairs: &[(&'static str, String)]) -> impl Fn(&str) -> Option<String> {
        let pairs = pairs.to_vec();
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.clone())
    }

    #[test]
    fn appcast_refuses_without_the_key_or_with_a_foreign_key() {
        let dir = TempDir::new("sparkle-appcast-keys");
        let archive = dir.write("a.dmg", b"dmg");
        let feed = dir.path().join("appcast.xml");
        let request = AppcastRequest {
            version: "0.2.0",
            archive: &archive,
            url: "https://dl.pdf-xray.app/releases/0.2.0/a.dmg",
            out: &feed,
        };
        let mut out = Vec::new();

        let err = appcast_cmd(&request, keys::PUBLIC_ED_KEY, env(&[]), 0, &mut out).expect_err("no key");
        assert_eq!(
            err.to_string(),
            "set SPARKLE_ED_PRIVATE_KEY to the private key from `cargo xtask sparkle keygen`"
        );

        let foreign = KeyPair::generate().expect("key").private_base64();
        let err = appcast_cmd(
            &request,
            keys::PUBLIC_ED_KEY,
            env(&[(ENV_PRIVATE_KEY, foreign.clone())]),
            0,
            &mut out,
        )
        .expect_err("foreign key");
        let text = format!("{err:#}");
        assert!(text.contains("would reject this update"), "{text}");
        assert!(!text.contains(&foreign), "{text}");

        let err = appcast_cmd(
            &request,
            keys::PUBLIC_ED_KEY,
            env(&[(ENV_PRIVATE_KEY, "x".into())]),
            0,
            &mut out,
        )
        .expect_err("garbage key");
        assert_eq!(
            format!("{err:#}"),
            "SPARKLE_ED_PRIVATE_KEY: the private key is not base64"
        );
        // smoke: nothing was written
        assert!(!feed.exists());
        assert_eq!(String::from_utf8(out).expect("utf8"), "");
    }

    #[test]
    fn appcast_signs_the_archive_and_writes_the_feed() {
        let dir = TempDir::new("sparkle-appcast");
        let body = b"pretend this is pdfxray-0.2.0-rc.1-macos-universal.dmg";
        let archive = dir.write("update.dmg", body);
        let feed = dir.path().join("appcast.xml");
        let pair = KeyPair::generate().expect("key");
        let request = AppcastRequest {
            version: "v0.2.0-rc.1",
            archive: &archive,
            url: "https://dl.pdf-xray.app/releases/0.2.0-rc.1/pdfxray-0.2.0-rc.1-macos-universal.dmg",
            out: &feed,
        };
        let mut out = Vec::new();

        appcast_cmd(
            &request,
            &pair.public_base64(),
            env(&[(ENV_PRIVATE_KEY, format!("{}\n", pair.private_base64()))]),
            1_793_448_000,
            &mut out,
        )
        .expect("appcast");

        let signature = pair.sign_base64(body);
        assert_eq!(
            fs::read_to_string(&feed).expect("feed"),
            appcast::render(&appcast::Item {
                bundle_version: "0.2.301",
                display_version: "0.2.0-rc.1",
                url: request.url,
                length: body.len() as u64,
                ed_signature: &signature,
                published: 1_793_448_000,
            })
        );
        let printed = String::from_utf8(out).expect("utf8");
        assert_eq!(
            printed,
            format!(
                "appcast: {} (pdfxray 0.2.0-rc.1, sparkle:version 0.2.301, {} bytes, signed)\n",
                feed.display(),
                body.len()
            )
        );
        // smoke: the signature in the feed verifies, and the secret is in neither output
        keys::verify(&pair.public_base64(), body, &signature).expect("verifies");
        assert!(!printed.contains(&pair.private_base64()));
        assert!(
            !fs::read_to_string(&feed)
                .expect("feed")
                .contains(&pair.private_base64())
        );
    }

    #[test]
    fn appcast_rejects_bad_versions_and_urls_before_reading_the_key() {
        let dir = TempDir::new("sparkle-appcast-args");
        let archive = dir.write("a.dmg", b"dmg");
        let feed = dir.path().join("appcast.xml");
        let mut request = AppcastRequest {
            version: "0.2.0-nightly",
            archive: &archive,
            url: "https://x.test/a.dmg",
            out: &feed,
        };
        let err = appcast_cmd(&request, keys::PUBLIC_ED_KEY, env(&[]), 0, &mut Vec::new())
            .expect_err("bad pre-release");
        assert!(err.to_string().contains("nightly"), "{err}");
        request.version = "0.2.0";
        request.url = "file:///tmp/a.dmg";
        let err =
            appcast_cmd(&request, keys::PUBLIC_ED_KEY, env(&[]), 0, &mut Vec::new()).expect_err("bad url");
        assert_eq!(err.to_string(), "--url must be an http(s) URL");
    }

    #[test]
    fn signatures_match_sparkles_sign_update_when_it_is_fetched() {
        // Interoperability with Sparkle's own tool, on machines where `sparkle fetch` (or
        // `bundle`) has installed it; Ed25519 is deterministic, so the signatures are equal.
        let install = Install::of(
            &default_dest(&paths::workspace_root().expect("root")),
            &manifest::PINNED,
        );
        let tool = install.sign_update();
        if !tool.is_file() {
            return;
        }
        let dir = TempDir::new("sparkle-sign-update");
        let archive = dir.write("update.dmg", b"some update archive bytes \x00\x01\x02");
        let pair = KeyPair::generate().expect("key");
        let mut child = Command::new(&tool)
            .args(["--ed-key-file", "-", "-p"])
            .arg(&archive)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("sign_update");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(pair.private_base64().as_bytes())
            .expect("key to stdin");
        let output = child.wait_with_output().expect("sign_update ran");
        assert!(output.status.success(), "{output:?}");
        let theirs = String::from_utf8(output.stdout).expect("utf8");
        assert_eq!(
            theirs.trim(),
            pair.sign_base64(&fs::read(&archive).expect("read"))
        );
    }
}
