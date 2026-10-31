//! Finding an existing veraPDF installation.

use std::path::{Path, PathBuf};

use crate::java::find_java;
use crate::verapdf::VeraPdf;

/// Where to look for veraPDF.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// User configured veraPDF: an install directory, the `verapdf` launcher or `bin/cli-*.jar`.
    pub explicit_path: Option<PathBuf>,
    /// User configured `java` executable; otherwise `$JAVA_HOME`, `PATH` and macOS'
    /// `java_home` are tried.
    pub java_path: Option<PathBuf>,
    /// Look for a `verapdf` launcher on `PATH`.
    pub search_path: bool,
    /// App data directory that [`crate::install`] installs into
    /// (`<dir>/<version>/{verapdf,bin,profiles}`); the newest version found is used.
    pub app_data_dir: Option<PathBuf>,
    /// Profile directory for installations that do not carry their own `profiles/` folder.
    pub profiles_dir: Option<PathBuf>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            explicit_path: None,
            java_path: None,
            search_path: true,
            app_data_dir: default_install_dir(),
            profiles_dir: None,
        }
    }
}

/// Where pdfxray installs veraPDF by default: `$PDFXRAY_DATA_DIR/verapdf` when set, else the
/// platform's per-user data directory (`~/Library/Application Support/pdfxray/verapdf` on
/// macOS, `$XDG_DATA_HOME` or `~/.local/share` elsewhere on Unix, `%APPDATA%` on Windows).
pub fn default_install_dir() -> Option<PathBuf> {
    data_dir_from(|k| std::env::var_os(k)).map(|d| d.join("verapdf"))
}

fn data_dir_from(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(dir) = var("PDFXRAY_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(windows) {
        return var("APPDATA").map(|d| PathBuf::from(d).join("pdfxray"));
    }
    if cfg!(target_os = "macos") {
        return var("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/pdfxray"));
    }
    var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("pdfxray"))
}

/// Result of [`discover`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// A working veraPDF was found.
    Found(VeraPdf),
    /// No Java 11+ runtime is available, so veraPDF cannot run (or be installed).
    JavaMissing,
    /// Java is available but no veraPDF installation was found.
    NotInstalled,
}

/// Looks for veraPDF in this order: explicit path, `verapdf` on `PATH`, the app data install.
///
/// This spawns `java` (and veraPDF with `--version`), so it takes a few hundred milliseconds;
/// call it off the UI thread.
#[must_use]
pub fn discover(config: &DiscoveryConfig) -> Discovery {
    let Some(java) = find_java(config.java_path.as_deref()) else {
        return Discovery::JavaMissing;
    };
    let candidates = candidate_entries(config);
    for entry in candidates {
        if let Some(mut found) = VeraPdf::open(&java.executable, &entry) {
            if found.profiles_dir().is_none()
                && let Some(dir) = config.profiles_dir.as_ref().filter(|d| d.is_dir())
            {
                found = found.with_profiles_dir(dir);
            }
            return Discovery::Found(found);
        }
    }
    Discovery::NotInstalled
}

fn candidate_entries(config: &DiscoveryConfig) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    if let Some(path) = &config.explicit_path {
        entries.push(path.clone());
    }
    if config.search_path
        && let Some(launcher) = find_on_path("verapdf")
    {
        entries.push(launcher);
    }
    if let Some(dir) = &config.app_data_dir {
        entries.extend(installed_versions(dir));
    }
    entries
}

/// First executable called `name` in `PATH`.
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Install roots below `dir`, newest version first.
fn installed_versions(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut versions: Vec<(Vec<u64>, PathBuf)> = read
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let key = version_key(e.file_name().to_str()?)?;
            Some((key, e.path()))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, path)| path).collect()
}

/// `1.30.3` -> `[1, 30, 3]`; anything with non-numeric parts (staging dirs) is not a version.
fn version_key(name: &str) -> Option<Vec<u64>> {
    name.split('.').map(|part| part.parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn scratch(name: &str) -> PathBuf {
        crate::scratch::dir(&format!("discovery-{name}"))
    }

    #[test]
    fn version_keys_sort_numerically_and_skip_staging_dirs() {
        let dir = scratch("versions");
        for name in ["1.9.0", "1.30.3", "1.30.10", ".staging-1-2", "profiles", "1.x"] {
            std::fs::create_dir_all(dir.join(name)).expect("mkdir");
        }
        let names: Vec<String> = installed_versions(&dir)
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
            .collect();
        assert_eq!(names, vec!["1.30.10", "1.30.3", "1.9.0"]);
    }

    #[test]
    fn candidate_order_is_explicit_then_app_data() {
        let dir = scratch("order");
        std::fs::create_dir_all(dir.join("1.30.3")).expect("mkdir");
        let config = DiscoveryConfig {
            explicit_path: Some(PathBuf::from("/opt/verapdf")),
            search_path: false,
            app_data_dir: Some(dir.clone()),
            ..DiscoveryConfig::default()
        };
        assert_eq!(
            candidate_entries(&config),
            vec![PathBuf::from("/opt/verapdf"), dir.join("1.30.3")]
        );
    }

    #[test]
    fn nothing_configured_finds_nothing() {
        let dir = scratch("empty");
        let config = DiscoveryConfig {
            explicit_path: Some(dir.join("nope")),
            search_path: false,
            app_data_dir: Some(dir.join("also-nope")),
            ..DiscoveryConfig::default()
        };
        // Either no Java at all, or Java without veraPDF: never `Found`.
        let found = discover(&config);
        assert!(
            matches!(found, Discovery::JavaMissing | Discovery::NotInstalled),
            "got {found:?}"
        );
    }

    #[test]
    fn default_config_searches_path() {
        let config = DiscoveryConfig::default();
        assert_eq!(
            (config.search_path, config.explicit_path, config.app_data_dir),
            (true, None, default_install_dir())
        );
        assert!(config.java_path.is_none());
    }
}

#[cfg(test)]
mod data_dir_tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::data_dir_from;

    #[test]
    fn explicit_env_wins() {
        let dir = data_dir_from(|k| (k == "PDFXRAY_DATA_DIR").then(|| OsString::from("/x/y")));

        assert_eq!(dir, Some(PathBuf::from("/x/y")));
    }

    #[test]
    fn falls_back_to_home() {
        let dir = data_dir_from(|k| (k == "HOME").then(|| OsString::from("/home/u")));

        let expected = if cfg!(target_os = "macos") {
            PathBuf::from("/home/u/Library/Application Support/pdfxray")
        } else {
            PathBuf::from("/home/u/.local/share/pdfxray")
        };
        assert_eq!(dir, Some(expected));
        assert_eq!(data_dir_from(|_| None), None);
    }
}
