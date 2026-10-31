//! Scratch directories for unit tests (integration tests use `CARGO_TARGET_TMPDIR`, which cargo
//! only defines for them).

use std::path::PathBuf;

/// A fresh, empty directory `<os temp>/pdfxray-verapdf-unit/<name>-<pid>`.
pub(crate) fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("pdfxray-verapdf-unit")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}
