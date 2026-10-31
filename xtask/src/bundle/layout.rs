//! On-disk layout of the `.app` and of the release folder that gets zipped.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::cmd;
use super::plist::{APP_EXECUTABLE, APP_NAME, ICON_NAME};
use super::target::Target;
use super::version::Version;

/// File name of the `.app` directory.
pub const APP_DIR: &str = "pdfxray.app";

/// Name of the CLI binary (the Cargo binary of `pdfxray-cli`).
pub const CLI_BINARY: &str = "pdfxray";

/// Name of the folder inside the zip, and the zip without extension:
/// `pdfxray-<version>-macos-<arch>`.
pub fn release_name(version: &Version, target: Target) -> String {
    format!("{APP_NAME}-{version}-macos-{}", target.label())
}

/// Everything that goes into the `.app`.
pub struct AppContents<'a> {
    /// The app executable (already merged for universal builds).
    pub executable: &'a Path,
    /// Further executables for `Contents/MacOS`, already signed: source path and file name.
    pub helpers: &'a [(&'a Path, &'a str)],
    /// The `.icns` file.
    pub icon: &'a Path,
    /// Text of `Info.plist`.
    pub info_plist: &'a str,
    /// Extra files for `Contents/Resources`: source path and file name.
    pub resources: &'a [(&'a Path, &'a str)],
}

/// Creates `app` from scratch (an existing directory is removed first).
pub fn assemble_app(app: &Path, contents: &AppContents<'_>) -> Result<()> {
    if app.exists() {
        fs::remove_dir_all(app).with_context(|| format!("cannot remove {}", app.display()))?;
    }
    let macos = app.join("Contents").join("MacOS");
    let resources = app.join("Contents").join("Resources");
    for dir in [&macos, &resources] {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }

    let exe = macos.join(APP_EXECUTABLE);
    fs::copy(contents.executable, &exe)
        .with_context(|| format!("cannot copy {}", contents.executable.display()))?;
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("cannot make {} executable", exe.display()))?;
    // The signature of a Mach-O file is inside it, so a plain copy keeps it.
    for (source, name) in contents.helpers {
        let dest = macos.join(name);
        fs::copy(source, &dest).with_context(|| format!("cannot copy {}", source.display()))?;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("cannot make {} executable", dest.display()))?;
    }

    write(
        &app.join("Contents").join("Info.plist"),
        contents.info_plist.as_bytes(),
    )?;
    write(&app.join("Contents").join("PkgInfo"), b"APPL????")?;
    let icon = resources.join(format!("{ICON_NAME}.icns"));
    fs::copy(contents.icon, &icon)
        .with_context(|| format!("cannot copy the icon {}", contents.icon.display()))?;
    for (source, name) in contents.resources {
        let dest = resources.join(name);
        fs::copy(source, &dest).with_context(|| format!("cannot copy {}", source.display()))?;
    }
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))
}

/// Files that accompany the binaries in the zip.
pub struct ReleaseFiles<'a> {
    /// The finished `.app`.
    pub app: &'a Path,
    /// The CLI binary.
    pub cli: &'a Path,
    /// `README.md`.
    pub readme: &'a Path,
    /// `LICENSE`.
    pub license: &'a Path,
    /// Generated third-party notice.
    pub third_party: &'a Path,
}

/// Name of the third-party notice file.
pub const THIRD_PARTY_FILE: &str = "THIRD-PARTY.txt";

/// Fills `dir` (recreated empty) with the release contents; returns it.
///
/// The app is copied with `ditto`, which keeps signatures, symlinks and extended attributes.
pub fn stage_release(dir: &Path, files: &ReleaseFiles<'_>) -> Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("cannot remove {}", dir.display()))?;
    }
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    cmd::run(Command::new("ditto").arg(files.app).arg(dir.join(APP_DIR)))
        .context("cannot copy the app into the release folder")?;
    let cli = dir.join(CLI_BINARY);
    fs::copy(files.cli, &cli).with_context(|| format!("cannot copy {}", files.cli.display()))?;
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o755))?;
    for (source, name) in [
        (files.readme, "README.md"),
        (files.license, "LICENSE"),
        (files.third_party, THIRD_PARTY_FILE),
    ] {
        fs::copy(source, dir.join(name)).with_context(|| format!("cannot copy {}", source.display()))?;
    }
    Ok(dir.to_path_buf())
}

/// `ditto -c -k --keepParent <dir> <zip>`: a Finder-compatible zip with `dir` as its top folder.
pub fn zip_dir(dir: &Path, zip: &Path) -> Result<()> {
    if zip.exists() {
        fs::remove_file(zip).with_context(|| format!("cannot remove {}", zip.display()))?;
    }
    cmd::run(
        Command::new("ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(dir)
            .arg(zip),
    )
    .with_context(|| format!("cannot create {}", zip.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::target::Arch;
    use crate::test_util::TempDir;

    #[test]
    fn release_names_follow_version_and_target() {
        let v = Version::parse("1.2.3-rc.1").expect("version");
        assert_eq!(
            release_name(&v, Target::Universal),
            "pdfxray-1.2.3-rc.1-macos-universal"
        );
        assert_eq!(
            release_name(&v, Target::Single(Arch::Aarch64)),
            "pdfxray-1.2.3-rc.1-macos-aarch64"
        );
        assert_eq!(
            release_name(&Version::parse("0.1.0").expect("v"), Target::Single(Arch::X86_64)),
            "pdfxray-0.1.0-macos-x86_64"
        );
    }

    #[test]
    fn assembles_the_bundle_layout() {
        let tmp = TempDir::new("layout-app");
        let exe = tmp.write("in/exe", b"#!exe");
        let cli = tmp.write("in/cli", b"#!cli");
        let icon = tmp.write("in/pdfxray.icns", b"icns-bytes");
        let notice = tmp.write("in/THIRD-PARTY.txt", b"notice");
        let app = tmp.path().join("out").join(APP_DIR);
        // a stale bundle is replaced, not merged
        tmp.write("out/pdfxray.app/Contents/stale", b"old");

        assemble_app(
            &app,
            &AppContents {
                executable: &exe,
                helpers: &[(&cli, CLI_BINARY)],
                icon: &icon,
                info_plist: "<plist/>",
                resources: &[(&notice, THIRD_PARTY_FILE)],
            },
        )
        .expect("assemble");

        let read = |rel: &str| fs::read(app.join(rel)).expect(rel);
        assert_eq!(read("Contents/MacOS/pdfxray-app"), b"#!exe");
        assert_eq!(read("Contents/MacOS/pdfxray"), b"#!cli");
        assert_eq!(read("Contents/Info.plist"), b"<plist/>");
        assert_eq!(read("Contents/PkgInfo"), b"APPL????");
        assert_eq!(read("Contents/Resources/pdfxray.icns"), b"icns-bytes");
        assert_eq!(read("Contents/Resources/THIRD-PARTY.txt"), b"notice");
        assert!(!app.join("Contents/stale").exists());
        for exe in ["Contents/MacOS/pdfxray-app", "Contents/MacOS/pdfxray"] {
            let mode = fs::metadata(app.join(exe)).expect("meta").permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "executable bits of {exe}");
        }
    }

    #[test]
    fn stages_and_zips_the_release_folder() {
        let tmp = TempDir::new("layout-stage");
        let exe = tmp.write("in/exe", b"exe");
        let icon = tmp.write("in/i.icns", b"i");
        let app = tmp.path().join("app").join(APP_DIR);
        assemble_app(
            &app,
            &AppContents {
                executable: &exe,
                helpers: &[],
                icon: &icon,
                info_plist: "p",
                resources: &[],
            },
        )
        .expect("assemble");
        let cli = tmp.write("in/cli", b"cli");
        let readme = tmp.write("in/README.md", b"readme");
        let license = tmp.write("in/LICENSE", b"license");
        let third = tmp.write("in/third", b"third");
        let dir = tmp.path().join("pdfxray-0.1.0-macos-aarch64");

        stage_release(
            &dir,
            &ReleaseFiles {
                app: &app,
                cli: &cli,
                readme: &readme,
                license: &license,
                third_party: &third,
            },
        )
        .expect("stage");
        let mut names: Vec<_> = fs::read_dir(&dir)
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "LICENSE",
                "README.md",
                "THIRD-PARTY.txt",
                "pdfxray",
                "pdfxray.app"
            ]
        );
        assert_eq!(fs::read(dir.join("THIRD-PARTY.txt")).expect("third"), b"third");
        assert!(dir.join("pdfxray.app/Contents/MacOS/pdfxray-app").is_file());

        let zip = tmp.path().join("out.zip");
        zip_dir(&dir, &zip).expect("zip");
        let listing = cmd::output(Command::new("unzip").arg("-Z1").arg(&zip)).expect("unzip -Z1");
        assert!(
            listing
                .lines()
                .all(|l| l.starts_with("pdfxray-0.1.0-macos-aarch64/")),
            "{listing}"
        );
        for member in ["pdfxray", "README.md", "pdfxray.app/Contents/Info.plist"] {
            let full = format!("pdfxray-0.1.0-macos-aarch64/{member}");
            assert!(
                listing.lines().any(|l| l == full),
                "{full} missing from {listing}"
            );
        }
    }
}
