//! A scratch directory for an install that is moved into place only once it is complete.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A fresh empty directory that is removed on drop unless [`Staging::keep`] consumed it.
pub(crate) struct Staging(Option<PathBuf>);

impl Staging {
    /// Creates `<parent>/.staging-<label>-<pid>`, replacing a leftover of a crashed run. It sits
    /// next to the install so the final rename never crosses a file system.
    pub(crate) fn new(parent: &Path, label: &str) -> Result<Self> {
        let path = parent.join(format!(".staging-{label}-{}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        fs::create_dir(&path).with_context(|| format!("creating {}", path.display()))?;
        Ok(Self(Some(path)))
    }

    pub(crate) fn path(&self) -> &Path {
        self.0.as_deref().unwrap_or_else(|| Path::new(""))
    }

    /// Moves the staged tree to `target`, replacing whatever is there.
    pub(crate) fn keep(mut self, target: &Path) -> Result<()> {
        let Some(path) = self.0.take() else {
            return Ok(());
        };
        if target.exists() {
            fs::remove_dir_all(target).with_context(|| format!("replacing {}", target.display()))?;
        }
        fs::rename(&path, target).with_context(|| format!("installing into {}", target.display()))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TempDir;

    #[test]
    fn kept_trees_replace_the_target_and_dropped_ones_vanish() {
        let dir = TempDir::new("staging");
        let target = dir
            .write("target/old.txt", b"old")
            .parent()
            .expect("parent")
            .to_path_buf();

        let staging = Staging::new(dir.path(), "x").expect("staging");
        fs::write(staging.path().join("new.txt"), b"new").expect("write");
        staging.keep(&target).expect("keep");
        assert_eq!(fs::read(target.join("new.txt")).expect("new"), b"new");
        assert!(!target.join("old.txt").exists());

        let dropped = Staging::new(dir.path(), "y").expect("staging");
        let path = dropped.path().to_path_buf();
        assert!(path.is_dir());
        drop(dropped);
        assert!(!path.exists());
        // smoke: only the kept target remains
        let names: Vec<_> = fs::read_dir(dir.path())
            .expect("read")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["target"]);
    }
}
