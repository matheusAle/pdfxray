//! Locations of the repository directories xtask works with.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Repository root: the parent of this crate's directory.
pub fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask manifest directory has no parent")
}

/// `fixtures/corpus.toml`: the manifest of fetched files.
pub fn corpus_manifest(root: &Path) -> PathBuf {
    root.join("fixtures").join("corpus.toml")
}

/// `fixtures/cache/`: where fetched files are stored (gitignored).
pub fn cache_dir(root: &Path) -> PathBuf {
    root.join("fixtures").join("cache")
}

/// `fixtures/third_party/verapdf/`: the vendored veraPDF sample.
pub fn vendored_verapdf_dir(root: &Path) -> PathBuf {
    root.join("fixtures").join("third_party").join("verapdf")
}

/// `target/pdfium/`: where `pdfium fetch` installs by default (inside the ignored build dir).
pub fn pdfium_dir(root: &Path) -> PathBuf {
    root.join("target").join("pdfium")
}

/// `target/sparkle/`: where `sparkle fetch` (and `bundle`) install Sparkle (inside the ignored
/// build dir).
pub fn sparkle_dir(root: &Path) -> PathBuf {
    root.join("target").join("sparkle")
}

/// `assets/icon/`: the committed icon artwork (the iconset, the window icon and the logo mark).
pub fn icon_assets_dir(root: &Path) -> PathBuf {
    root.join("assets").join("icon")
}

/// `assets/icon/pdfxray.iconset/`: the PNGs `cargo xtask icon` folds into the `.icns`.
pub fn iconset_dir(root: &Path) -> PathBuf {
    icon_assets_dir(root).join("pdfxray.iconset")
}

/// `assets/pdfxray.icns`: the committed app icon (rebuild with `cargo xtask icon`).
pub fn icon_path(root: &Path) -> PathBuf {
    root.join("assets").join("pdfxray.icns")
}

/// `<target>/bundle/`: output of `cargo xtask bundle`, below the Cargo target directory.
pub fn bundle_dir(target_dir: &Path) -> PathBuf {
    target_dir.join("bundle")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations_hang_off_the_root() {
        let root = Path::new("/repo");
        assert_eq!(corpus_manifest(root), Path::new("/repo/fixtures/corpus.toml"));
        assert_eq!(cache_dir(root), Path::new("/repo/fixtures/cache"));
        assert_eq!(pdfium_dir(root), Path::new("/repo/target/pdfium"));
        assert_eq!(sparkle_dir(root), Path::new("/repo/target/sparkle"));
        assert_eq!(icon_path(root), Path::new("/repo/assets/pdfxray.icns"));
        assert_eq!(icon_assets_dir(root), Path::new("/repo/assets/icon"));
        assert_eq!(iconset_dir(root), Path::new("/repo/assets/icon/pdfxray.iconset"));
        assert_eq!(bundle_dir(&root.join("target")), Path::new("/repo/target/bundle"));
        assert_eq!(
            vendored_verapdf_dir(root),
            Path::new("/repo/fixtures/third_party/verapdf")
        );
    }

    #[test]
    fn workspace_root_contains_the_xtask_crate() {
        let root = workspace_root().expect("root");
        assert!(root.join("xtask").join("Cargo.toml").is_file());
    }
}
