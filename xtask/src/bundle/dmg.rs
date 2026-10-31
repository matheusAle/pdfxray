//! The disk image installer: `pdfxray.app` next to a link to `/Applications`, so installing is a
//! drag. Signing and notarizing the image is done by the caller (see [`super::sign`]).

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use super::cmd;
use super::layout::APP_DIR;

/// Name of the link to `/Applications` inside the image.
pub const APPLICATIONS_LINK: &str = "Applications";

/// Fills `dir` (recreated empty) with what the image shows: the app and the `/Applications` link.
///
/// The app is copied with `ditto`, which keeps its signature and stapled ticket.
pub fn stage(dir: &Path, app: &Path) -> Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| format!("cannot remove {}", dir.display()))?;
    }
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    cmd::run(Command::new("ditto").arg(app).arg(dir.join(APP_DIR)))
        .context("cannot copy the app into the disk image folder")?;
    symlink("/Applications", dir.join(APPLICATIONS_LINK)).context("cannot create the Applications link")
}

/// Arguments of `hdiutil` that make a compressed, read-only image of `source` named `volume`.
///
/// HFS+ rather than APFS: Apple recommends it for images that are distributed, and it mounts
/// on every macOS the app supports.
pub fn hdiutil_args(volume: &str, source: &Path, dmg: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "create",
        "-volname",
        volume,
        "-fs",
        "HFS+",
        "-format",
        "UDZO",
        "-imagekey",
        "zlib-level=9",
        "-ov",
        "-srcfolder",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(source.into());
    args.push(dmg.into());
    args
}

/// Creates `dmg` from the staged folder `source`.
pub fn create(volume: &str, source: &Path, dmg: &Path) -> Result<()> {
    cmd::run(Command::new("hdiutil").args(hdiutil_args(volume, source, dmg)))
        .with_context(|| format!("cannot create {}", dmg.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    #[test]
    fn hdiutil_makes_a_compressed_hfs_image_of_the_folder() {
        let args: Vec<String> = hdiutil_args(
            "pdfxray 1.2.3",
            Path::new("/b/dmg"),
            Path::new("/b/pdfxray-1.2.3-macos-universal.dmg"),
        )
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
        assert_eq!(
            args,
            [
                "create",
                "-volname",
                "pdfxray 1.2.3",
                "-fs",
                "HFS+",
                "-format",
                "UDZO",
                "-imagekey",
                "zlib-level=9",
                "-ov",
                "-srcfolder",
                "/b/dmg",
                "/b/pdfxray-1.2.3-macos-universal.dmg",
            ]
        );
    }

    #[test]
    fn stages_the_app_and_a_link_to_applications() {
        let tmp = TempDir::new("dmg-stage");
        let app = tmp.path().join("in").join(APP_DIR);
        tmp.write("in/pdfxray.app/Contents/Info.plist", b"plist");
        let dir = tmp.path().join("stage");
        // a stale folder is replaced, not merged
        tmp.write("stage/leftover", b"old");

        stage(&dir, &app).expect("stage");

        assert_eq!(
            fs::read(dir.join("pdfxray.app/Contents/Info.plist")).expect("plist"),
            b"plist"
        );
        assert_eq!(
            fs::read_link(dir.join(APPLICATIONS_LINK)).expect("link"),
            Path::new("/Applications")
        );
        let mut names: Vec<String> = fs::read_dir(&dir)
            .expect("list")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["Applications", "pdfxray.app"]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn creates_an_image_that_hdiutil_accepts() {
        let tmp = TempDir::new("dmg-create");
        tmp.write("src/hello.txt", b"hello");
        let dmg = tmp.path().join("t.dmg");
        create("pdfxray test", &tmp.path().join("src"), &dmg).expect("hdiutil create");
        assert!(fs::metadata(&dmg).expect("dmg").len() > 0);
        cmd::run(Command::new("hdiutil").arg("verify").arg(&dmg)).expect("hdiutil verify");
    }
}
