//! Embedding Sparkle in the app: `Contents/Frameworks/Sparkle.framework`, signed inside out.
//!
//! The framework is copied with `ditto` (its `Versions/Current` symlinks must survive), then its
//! XPC services are removed: they exist for sandboxed apps, and pdfxray is not sandboxed, so they
//! would only be more code to sign and ship. What remains is signed in the order Sparkle's
//! documentation prescribes, innermost first, each with the hardened runtime (and a secure
//! timestamp for Developer ID): `Autoupdate`, `Updater.app`, then the framework. The app itself is
//! signed afterwards by the caller; nothing uses `--deep`.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::cmd;
use super::sign::{self, Identity};
use super::version::Version;
use crate::corpus::fetch::UreqDownloader;
use crate::sparkle::manifest::{FRAMEWORK, PINNED};
use crate::sparkle::{self as fetch, Install, Outcome};

/// The nested code that is signed before the framework, innermost first, relative to the framework.
pub const NESTED_CODE: [&str; 2] = ["Versions/B/Autoupdate", "Versions/B/Updater.app"];

/// What is removed from the framework: the XPC services (sandboxed apps only) and their link.
pub const REMOVED: [&str; 2] = ["Versions/B/XPCServices", "XPCServices"];

/// The pinned Sparkle install below `target/sparkle/`, fetched (and verified) when it is missing,
/// after checking that `version` has a `CFBundleVersion` Sparkle can order (so a bad pre-release
/// fails before the long build).
///
/// Fetching here instead of failing keeps a release from ever shipping without its updater and
/// needs no extra CI step; the download is pinned and verified exactly like `sparkle fetch`.
pub fn prepare(root: &Path, version: &Version, out: &mut dyn Write) -> Result<Install> {
    version.bundle_version()?;
    let (install, outcome) = fetch::install(&fetch::default_dest(root), &PINNED, &UreqDownloader::new()?)
        .context("cannot fetch Sparkle (`cargo xtask sparkle fetch` retries on its own)")?;
    let what = match outcome {
        Outcome::Cached => "cached",
        Outcome::Installed => "downloaded and verified",
    };
    writeln!(out, "Sparkle {} {what}", PINNED.version)?;
    Ok(install)
}

/// `Contents/Frameworks/Sparkle.framework` of `app`.
pub fn framework_in(app: &Path) -> PathBuf {
    app.join("Contents").join("Frameworks").join(FRAMEWORK)
}

/// Copies the framework of `install` into `app` (replacing an older copy), drops the XPC services
/// and signs it with `identity`. Call after the app is assembled and before it is signed.
pub fn embed(install: &Install, app: &Path, identity: &Identity) -> Result<PathBuf> {
    let framework = framework_in(app);
    if fs::symlink_metadata(&framework).is_ok() {
        fs::remove_dir_all(&framework).with_context(|| format!("cannot remove {}", framework.display()))?;
    }
    if let Some(parent) = framework.parent() {
        fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    cmd::run(Command::new("ditto").arg(install.framework()).arg(&framework))
        .context("cannot copy Sparkle.framework into the app")?;
    for removed in REMOVED {
        remove(&framework.join(removed))?;
    }
    for path in sign_order(&framework) {
        cmd::run(Command::new("codesign").args(codesign_args(identity, &path)))
            .with_context(|| format!("cannot sign {}", path.display()))?;
    }
    sign::verify(&framework)?;
    Ok(framework)
}

/// Removes a file, a symlink or a directory tree; absent is fine.
fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(meta) if meta.is_dir() => {
            fs::remove_dir_all(path).with_context(|| format!("cannot remove {}", path.display()))
        }
        Ok(_) => fs::remove_file(path).with_context(|| format!("cannot remove {}", path.display())),
    }
}

/// Every path to sign, in order: the nested code, then the framework itself.
pub fn sign_order(framework: &Path) -> Vec<PathBuf> {
    NESTED_CODE
        .iter()
        .map(|nested| framework.join(nested))
        .chain(std::iter::once(framework.to_path_buf()))
        .collect()
}

/// Arguments for `codesign` to sign one piece of Sparkle: forced (it arrives ad-hoc signed by the
/// Sparkle project), hardened runtime, and no entitlements of ours (Sparkle's tools keep their own
/// identifiers and need no exceptions).
pub fn codesign_args(identity: &Identity, path: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["--force".into(), "--options".into(), "runtime".into()];
    match identity {
        Identity::AdHoc => args.extend(["--timestamp=none".into(), "--sign".into(), "-".into()]),
        Identity::Developer(name) => {
            args.extend(["--timestamp".into(), "--sign".into(), name.into()]);
        }
    }
    args.push(path.into());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn nested_code_is_signed_before_the_framework() {
        let framework = Path::new("/a/pdfxray.app/Contents/Frameworks/Sparkle.framework");
        assert_eq!(framework_in(Path::new("/a/pdfxray.app")), framework);
        assert_eq!(
            sign_order(framework),
            [
                framework.join("Versions/B/Autoupdate"),
                framework.join("Versions/B/Updater.app"),
                framework.to_path_buf(),
            ]
        );
        // smoke: the XPC services are removed, not signed
        assert!(
            sign_order(framework)
                .iter()
                .all(|p| !p.to_string_lossy().contains("XPC"))
        );
        assert_eq!(REMOVED, ["Versions/B/XPCServices", "XPCServices"]);
    }

    #[test]
    fn developer_id_signing_has_runtime_and_timestamp_and_no_deep() {
        let id = Identity::Developer("Developer ID Application: Example (ABCDE12345)".into());
        assert_eq!(
            strings(&codesign_args(&id, Path::new("/f/Sparkle.framework"))),
            [
                "--force",
                "--options",
                "runtime",
                "--timestamp",
                "--sign",
                "Developer ID Application: Example (ABCDE12345)",
                "/f/Sparkle.framework",
            ]
        );
        let adhoc = strings(&codesign_args(&Identity::AdHoc, Path::new("/f/Autoupdate")));
        assert_eq!(
            adhoc,
            [
                "--force",
                "--options",
                "runtime",
                "--timestamp=none",
                "--sign",
                "-",
                "/f/Autoupdate"
            ]
        );
        assert!(!adhoc.iter().any(|a| a == "--deep" || a == "--entitlements"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn embeds_and_signs_a_framework_shaped_tree() {
        use crate::test_util::TempDir;

        // A stand-in framework with Sparkle's layout: real Mach-O executables (copies of `true`)
        // where Sparkle has code, and its symlinks.
        let dir = TempDir::new("sparkle-embed");
        let install = Install::of(dir.path(), &PINNED);
        let src = install.framework();
        let b = src.join("Versions/B");
        let exe = fs::read("/usr/bin/true").expect("a Mach-O executable");
        for (path, body) in [
            ("Sparkle", &exe[..]),
            ("Autoupdate", &exe[..]),
            ("Updater.app/Contents/MacOS/Updater", &exe[..]),
            ("XPCServices/Installer.xpc/Contents/MacOS/Installer", &exe[..]),
            ("Resources/Info.plist", INFO.as_bytes()),
            ("Updater.app/Contents/Info.plist", UPDATER_INFO.as_bytes()),
        ] {
            let file = b.join(path);
            fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            fs::write(&file, body).expect("write");
        }
        for name in ["Sparkle", "Autoupdate", "Updater.app/Contents/MacOS/Updater"] {
            cmd::run(Command::new("chmod").arg("755").arg(b.join(name))).expect("chmod");
        }
        for (link, to) in [
            ("Versions/Current", "B"),
            ("Sparkle", "Versions/Current/Sparkle"),
            ("Resources", "Versions/Current/Resources"),
            ("Autoupdate", "Versions/Current/Autoupdate"),
            ("Updater.app", "Versions/Current/Updater.app"),
            ("XPCServices", "Versions/Current/XPCServices"),
        ] {
            std::os::unix::fs::symlink(to, src.join(link)).expect("symlink");
        }
        let app = dir.path().join("pdfxray.app");

        let framework = embed(&install, &app, &Identity::AdHoc).expect("embed");

        assert_eq!(framework, framework_in(&app));
        assert_eq!(
            fs::read_link(framework.join("Versions/Current")).expect("link kept"),
            Path::new("B")
        );
        assert!(fs::symlink_metadata(framework.join("XPCServices")).is_err());
        assert!(!framework.join("Versions/B/XPCServices").exists());
        // smoke: every piece carries a valid signature of its own
        for path in sign_order(&framework) {
            sign::verify(&path).expect("signed");
        }
    }

    #[cfg(target_os = "macos")]
    const INFO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Sparkle</string>
<key>CFBundleIdentifier</key><string>org.sparkle-project.Sparkle</string>
<key>CFBundlePackageType</key><string>FMWK</string>
</dict></plist>
"#;

    #[cfg(target_os = "macos")]
    const UPDATER_INFO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Updater</string>
<key>CFBundleIdentifier</key><string>org.sparkle-project.Sparkle.Updater</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"#;
}
