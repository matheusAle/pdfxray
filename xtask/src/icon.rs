//! The macOS app icon: `assets/icon/pdfxray.iconset` folded into `assets/pdfxray.icns`.
//!
//! The iconset is exported finished from the design file (`design/pdfxray-revamp.pen`, see
//! `assets/icon/SOURCES.md`): Big Sur layout, small sizes hand-tuned. Nothing here draws or
//! resizes it. `cargo xtask icon` checks that every member is there at its pixel size and has
//! macOS `iconutil` build the `.icns`, which is committed so bundling never needs `iconutil`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Iconset members: file name and the pixel size it holds.
pub const ICONSET: [(&str, u32); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

/// Default location of the committed icon: `assets/pdfxray.icns`.
pub fn default_icns(root: &Path) -> PathBuf {
    crate::paths::icon_path(root)
}

/// `cargo xtask icon`: checks the iconset in `iconset` and converts it with `iconutil` into `out`.
pub fn icon_cmd(iconset: &Path, out: &Path, log: &mut dyn Write) -> Result<()> {
    check_iconset(iconset)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let status = Command::new("iconutil")
        .args(["--convert", "icns", "--output"])
        .arg(out)
        .arg(iconset)
        .status()
        .context("cannot run `iconutil` (macOS only)")?;
    if !status.success() {
        bail!("`iconutil` failed ({status})");
    }
    writeln!(log, "wrote {}", out.display())?;
    Ok(())
}

/// Fails unless every member of [`ICONSET`] is in `dir` as a PNG of its pixel size.
pub fn check_iconset(dir: &Path) -> Result<()> {
    for (name, size) in ICONSET {
        let path = dir.join(name);
        let (width, height) = image::image_dimensions(&path)
            .with_context(|| format!("iconset member {} is missing or unreadable", path.display()))?;
        if (width, height) != (size, size) {
            bail!("iconset member {name} is {width}x{height} px, it must be {size}x{size}");
        }
    }
    Ok(())
}

/// The PNG stored under `kind` (four-character code, e.g. `ic10`) in an `.icns` file.
pub fn icns_entry<'a>(icns: &'a [u8], kind: &[u8; 4]) -> Result<&'a [u8]> {
    if icns.get(..4) != Some(b"icns") {
        bail!("not an icns file");
    }
    let mut at = 8;
    while at + 8 <= icns.len() {
        let len = usize::try_from(u32::from_be_bytes([
            icns[at + 4],
            icns[at + 5],
            icns[at + 6],
            icns[at + 7],
        ]))
        .with_context(|| format!("corrupt icns entry at byte {at}"))?;
        if len < 8 || at + len > icns.len() {
            bail!("corrupt icns entry at byte {at}");
        }
        if &icns[at..at + 4] == kind {
            return Ok(&icns[at + 8..at + len]);
        }
        at += len;
    }
    bail!("icns has no `{}` entry", String::from_utf8_lossy(kind))
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use super::*;
    use crate::test_util::TempDir;

    /// The window icon next to the iconset, a copy of its 256 px entry.
    const WINDOW_ICON: &str = "window-icon-256.png";

    #[test]
    fn the_iconset_check_names_the_wrong_member() {
        let dir = TempDir::new("iconset-check");
        for (name, size) in ICONSET {
            RgbaImage::new(size, size)
                .save(dir.path().join(name))
                .expect("save");
        }
        check_iconset(dir.path()).expect("complete");

        RgbaImage::new(30, 30)
            .save(dir.path().join("icon_32x32.png"))
            .expect("save");
        let err = check_iconset(dir.path()).expect_err("wrong size");
        assert!(err.to_string().contains("icon_32x32.png is 30x30 px"), "{err}");

        RgbaImage::new(32, 32)
            .save(dir.path().join("icon_32x32.png"))
            .expect("save");
        fs::remove_file(dir.path().join("icon_512x512@2x.png")).expect("remove");
        let err = check_iconset(dir.path()).expect_err("missing");
        assert!(format!("{err:#}").contains("icon_512x512@2x.png"), "{err:#}");
    }

    #[test]
    fn iconset_is_the_ten_standard_members() {
        for (name, size) in ICONSET {
            let base: u32 = name
                .trim_start_matches("icon_")
                .split('x')
                .next()
                .and_then(|n| n.parse().ok())
                .expect("size in file name");
            let scale = if name.contains("@2x") { 2 } else { 1 };
            assert_eq!(size, base * scale, "{name}");
        }
        // smoke
        assert_eq!(ICONSET.len(), 10);
    }

    #[test]
    fn parses_icns_entries() {
        let mut icns = b"icns".to_vec();
        icns.extend_from_slice(&0u32.to_be_bytes());
        for (kind, body) in [(b"ic07", &b"abc"[..]), (b"ic10", &b"wxyz"[..])] {
            icns.extend_from_slice(kind);
            icns.extend_from_slice(&u32::try_from(8 + body.len()).expect("len").to_be_bytes());
            icns.extend_from_slice(body);
        }
        assert_eq!(icns_entry(&icns, b"ic10").expect("ic10"), b"wxyz");
        assert_eq!(icns_entry(&icns, b"ic07").expect("ic07"), b"abc");
        assert!(icns_entry(&icns, b"ic08").is_err());
        assert!(icns_entry(b"nope", b"ic10").is_err());
        icns.truncate(icns.len() - 1);
        assert!(icns_entry(&icns, b"ic10").is_err());
    }

    #[test]
    fn committed_assets_are_complete_and_agree() {
        let root = crate::paths::workspace_root().expect("root");
        let icon_dir = crate::paths::icon_assets_dir(&root);
        let iconset = crate::paths::iconset_dir(&root);
        check_iconset(&iconset).expect("the committed iconset is complete");
        let pixels = |path: &Path| image::open(path).expect("png").to_rgba8();
        assert_eq!(
            pixels(&icon_dir.join(WINDOW_ICON)),
            pixels(&iconset.join("icon_256x256.png")),
            "{WINDOW_ICON} must be the 256 px iconset entry"
        );
        // the .icns was built from this iconset
        let bytes = fs::read(default_icns(&root)).expect("assets/pdfxray.icns is committed");
        let png = icns_entry(&bytes, b"ic10").expect("1024 px entry");
        let stored = image::load_from_memory(png).expect("png").to_rgba8();
        assert!(
            stored == pixels(&iconset.join("icon_512x512@2x.png")),
            "assets/pdfxray.icns is stale: run `cargo xtask icon`"
        );
    }
}
