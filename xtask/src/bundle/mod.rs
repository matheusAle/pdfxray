//! `bundle`: builds, signs, notarizes and zips the macOS release.
//!
//! Output below `<target dir>/bundle/`:
//!
//! ```text
//! pdfxray.app                              the signed (and, with --notarize, stapled) app
//! pdfxray-<version>-macos-<arch>/          what the zip contains: the app, `pdfxray`, README.md,
//!                                          LICENSE, THIRD-PARTY.txt
//! pdfxray-<version>-macos-<arch>.zip       the release asset
//! pdfxray-<version>-macos-<arch>.dmg       with --dmg: the installer (the app and a link to
//!                                          /Applications), signed and notarized like the app
//! ```
//!
//! The app also carries the CLI as `Contents/MacOS/pdfxray`, so updating the app updates it.
//!
//! Without `--sign` the binaries get an ad-hoc signature, which is what lets them run on Apple
//! Silicon. See `docs/releasing.md` for what that means for Gatekeeper.

pub mod build;
pub mod cmd;
pub mod dmg;
pub mod layout;
pub mod plist;
pub mod sign;
pub mod sparkle;
pub mod target;
pub mod thirdparty;
pub mod version;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use self::build::{APP, CLI};
use self::layout::{APP_DIR, AppContents, CLI_BINARY, ReleaseFiles, THIRD_PARTY_FILE};
use self::sign::{Identity, NotaryAuth};
use self::target::{Arch, Target};
use self::version::Version;
use crate::cli::BundleArgs;
use crate::hash::sha256_file;
use crate::paths;

/// The version being bundled: `requested` (or the workspace version when absent).
///
/// The binaries report the version in `Cargo.toml`, so a request that disagrees with it in
/// `MAJOR.MINOR.PATCH` is an error (a tag that was not preceded by a version bump).
/// A pre-release suffix is allowed to differ.
pub fn resolve_version(requested: Option<&str>, workspace: &str) -> Result<Version> {
    let workspace = Version::parse(workspace).context("workspace version")?;
    let Some(requested) = requested else {
        return Ok(workspace);
    };
    let version = Version::parse(requested)?;
    if !version.same_core(&workspace) {
        anyhow::bail!(
            "--version {version} does not match the workspace version {workspace}; \
             bump `[workspace.package].version` in Cargo.toml first"
        );
    }
    Ok(version)
}

/// Runs `cargo xtask bundle`.
pub fn bundle_cmd(root: &Path, args: &BundleArgs, out: &mut dyn Write) -> Result<()> {
    Arch::host()?;
    let target = Target::select(args.target)?;
    let version = resolve_version(args.version.as_deref(), env!("CARGO_PKG_VERSION"))?;
    let identity = args
        .sign
        .as_ref()
        .map_or(Identity::AdHoc, |name| Identity::Developer(name.clone()));
    // Fail before the long build when credentials are missing.
    let auth = if args.notarize {
        Some(NotaryAuth::resolve(args.keychain_profile.as_deref(), |name| {
            std::env::var(name).ok()
        })?)
    } else {
        None
    };
    let icon = paths::icon_path(root);
    if !icon.is_file() {
        anyhow::bail!("{} is missing; run `cargo xtask icon`", icon.display());
    }
    let sparkle = sparkle::prepare(root, &version, out)?;

    let release = layout::release_name(&version, target);
    writeln!(out, "bundling {release} ({})", describe(&identity, auth.as_ref()))?;

    let documents = metadata(root, target)?;
    let target_dir = documents[0].target_directory.clone();
    let bundle_dir = paths::bundle_dir(&target_dir);
    let bin_dir = bundle_dir.join("bin");
    fs::create_dir_all(&bundle_dir).with_context(|| format!("cannot create {}", bundle_dir.display()))?;

    writeln!(out, "building the app and the CLI in release mode")?;
    let app_exe = build::build(&target_dir, &bin_dir, APP, target)?;
    let cli_exe = build::build(&target_dir, &bin_dir, CLI, target)?;

    let third_party = bundle_dir.join(THIRD_PARTY_FILE);
    let entries = thirdparty::collect(&documents)?;
    fs::write(&third_party, thirdparty::render(&entries))
        .with_context(|| format!("cannot write {}", third_party.display()))?;
    writeln!(out, "third-party notice: {} crates", entries.len())?;

    let entitlements = bundle_dir.join("entitlements.plist");
    fs::write(&entitlements, plist::entitlements_plist())
        .with_context(|| format!("cannot write {}", entitlements.display()))?;
    writeln!(out, "signing")?;
    // The CLI is signed first: the app carries a copy of it, and nested code must be signed
    // before the bundle around it.
    sign::codesign(&identity, &entitlements, Some(plist::CLI_SIGN_ID), &cli_exe)?;
    sign::verify(&cli_exe)?;

    let app = bundle_dir.join(APP_DIR);
    layout::assemble_app(
        &app,
        &AppContents {
            executable: &app_exe,
            helpers: &[(&cli_exe, CLI_BINARY)],
            icon: &icon,
            info_plist: &plist::info_plist(&version)?,
            resources: &[
                (&root.join("LICENSE"), "LICENSE"),
                (&third_party, THIRD_PARTY_FILE),
            ],
        },
    )?;

    sparkle::embed(&sparkle, &app, &identity)?;
    sign::codesign(&identity, &entitlements, None, &app)?;
    sign::verify(&app)?;

    let stage = layout::stage_release(
        &bundle_dir.join(&release),
        &ReleaseFiles {
            app: &app,
            cli: &cli_exe,
            readme: &root.join("README.md"),
            license: &root.join("LICENSE"),
            third_party: &third_party,
        },
    )?;
    let zip = bundle_dir.join(format!("{release}.zip"));

    if let Some(auth) = &auth {
        let submit = bundle_dir.join(format!("{release}-notarize.zip"));
        layout::zip_dir(&stage, &submit)?;
        writeln!(
            out,
            "submitting to the Apple notary service (waits for the verdict)"
        )?;
        let submission = sign::notarize(auth, &submit)?;
        writeln!(out, "notarization {} accepted", submission.id)?;
        let _ = fs::remove_file(&submit);
        let staged_app = stage.join(APP_DIR);
        sign::staple(&staged_app)?;
        // the app at target/bundle/pdfxray.app carries the ticket too
        fs::remove_dir_all(&app).with_context(|| format!("cannot remove {}", app.display()))?;
        cmd::run(Command::new("ditto").arg(&staged_app).arg(&app)).context("cannot copy the stapled app")?;
    }

    layout::zip_dir(&stage, &zip)?;
    let (digest, size) = sha256_file(&zip)?;
    writeln!(out, "app: {}", app.display())?;
    writeln!(out, "zip: {} ({size} bytes)", zip.display())?;
    writeln!(out, "sha256: {digest}")?;

    if args.dmg {
        let dmg_path = bundle_dir.join(format!("{release}.dmg"));
        disk_image(
            &bundle_dir,
            &app,
            &version,
            &identity,
            auth.as_ref(),
            &dmg_path,
            out,
        )?;
        let (digest, size) = sha256_file(&dmg_path)?;
        writeln!(out, "dmg: {} ({size} bytes)", dmg_path.display())?;
        writeln!(out, "sha256: {digest}")?;
    }
    Ok(())
}

/// Builds the installer image `dmg` around the finished (stapled, when notarizing) `app`, then
/// signs, notarizes and staples the image itself. An ad-hoc build gets an unsigned image.
///
/// The image needs its own notarization: its ticket covers the image's signature, while the
/// app's stapled ticket keeps the app valid after it is copied out of the image.
fn disk_image(
    bundle_dir: &Path,
    app: &Path,
    version: &Version,
    identity: &Identity,
    auth: Option<&NotaryAuth>,
    dmg_path: &Path,
    out: &mut dyn Write,
) -> Result<()> {
    let stage = bundle_dir.join("dmg");
    dmg::stage(&stage, app)?;
    writeln!(out, "creating the disk image")?;
    dmg::create(&format!("{} {version}", plist::APP_NAME), &stage, dmg_path)?;
    fs::remove_dir_all(&stage).with_context(|| format!("cannot remove {}", stage.display()))?;
    let Identity::Developer(name) = identity else {
        return Ok(());
    };
    sign::codesign_dmg(name, dmg_path)?;
    sign::verify(dmg_path)?;
    if let Some(auth) = auth {
        writeln!(
            out,
            "submitting the disk image to the Apple notary service (waits for the verdict)"
        )?;
        let submission = sign::notarize(auth, dmg_path)?;
        writeln!(out, "notarization {} accepted", submission.id)?;
        sign::staple(dmg_path)?;
    }
    Ok(())
}

fn describe(identity: &Identity, auth: Option<&NotaryAuth>) -> &'static str {
    match (identity, auth) {
        (Identity::AdHoc, _) => "ad-hoc signed",
        (Identity::Developer(_), None) => "Developer ID signed",
        (Identity::Developer(_), Some(_)) => "Developer ID signed and notarized",
    }
}

/// `cargo metadata` for each architecture of `target` (the dependency graph differs by platform).
fn metadata(root: &Path, target: Target) -> Result<Vec<thirdparty::Metadata>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    target
        .arches()
        .into_iter()
        .map(|arch| {
            let json = cmd::output(
                Command::new(&cargo)
                    .args([
                        "metadata",
                        "--format-version",
                        "1",
                        "--locked",
                        "--filter-platform",
                    ])
                    .arg(arch.triple())
                    .current_dir(root),
            )
            .with_context(|| format!("`cargo metadata` failed for {}", arch.triple()))?;
            thirdparty::Metadata::parse(json.as_bytes())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_defaults_to_the_workspace() {
        assert_eq!(
            resolve_version(None, "0.4.2").expect("default").to_string(),
            "0.4.2"
        );
    }

    #[test]
    fn requested_version_must_share_the_workspace_core() {
        assert_eq!(
            resolve_version(Some("v0.4.2-rc.1"), "0.4.2")
                .expect("rc")
                .to_string(),
            "0.4.2-rc.1"
        );
        let err = resolve_version(Some("0.5.0"), "0.4.2").expect_err("mismatch");
        let text = err.to_string();
        assert!(
            text.contains("0.5.0") && text.contains("0.4.2") && text.contains("bump"),
            "{text}"
        );
        assert!(resolve_version(Some("nonsense"), "0.4.2").is_err());
    }

    #[test]
    fn describes_what_will_be_produced() {
        let dev = Identity::Developer("x".into());
        let auth = NotaryAuth::Profile("p".into());
        assert_eq!(describe(&Identity::AdHoc, None), "ad-hoc signed");
        assert_eq!(describe(&dev, None), "Developer ID signed");
        assert_eq!(describe(&dev, Some(&auth)), "Developer ID signed and notarized");
    }
}
