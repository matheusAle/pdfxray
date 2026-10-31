//! Locating and probing a Java runtime. veraPDF needs Java 11 or newer; the runtime is the
//! user's, we never bundle one.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::process::run_captured;

/// Oldest Java major version veraPDF 1.30 runs on.
pub const MIN_JAVA_MAJOR: u32 = 11;

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// A working Java runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntime {
    /// The real `java` executable (version-manager shims such as asdf's are resolved through
    /// the `java.home` property the runtime reports about itself).
    pub executable: PathBuf,
    /// `java.version`, e.g. `21.0.13`.
    pub version: String,
    /// Major version, e.g. `21`.
    pub major: u32,
}

/// Finds a usable Java runtime: `explicit`, then `$JAVA_HOME/bin/java`, then `java` on `PATH`,
/// then macOS' `/usr/libexec/java_home`.
#[must_use]
pub fn find_java(explicit: Option<&Path>) -> Option<JavaRuntime> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = explicit {
        candidates.push(path.to_path_buf());
    }
    if let Some(home) = std::env::var_os("JAVA_HOME").filter(|h| !h.is_empty()) {
        candidates.push(Path::new(&home).join("bin").join("java"));
    }
    candidates.push(PathBuf::from("java"));
    if let Some(home) = macos_java_home() {
        candidates.push(home.join("bin").join("java"));
    }
    candidates.into_iter().find_map(|c| probe_java(&c))
}

/// Runs `<candidate> -XshowSettings:properties -version` and accepts it when it is Java
/// [`MIN_JAVA_MAJOR`] or newer.
#[must_use]
pub fn probe_java(candidate: &Path) -> Option<JavaRuntime> {
    let mut cmd = Command::new(candidate);
    cmd.args(["-XshowSettings:properties", "-version"]);
    let out = run_captured(&mut cmd, PROBE_TIMEOUT, None, 64 * 1024, 256 * 1024).ok()?;
    if !out.status.success() {
        return None;
    }
    // The JVM prints settings and the version banner on stderr.
    let text = String::from_utf8_lossy(&out.stderr);
    let version = property(&text, "java.version")?;
    let major = parse_major(&version)?;
    if major < MIN_JAVA_MAJOR {
        return None;
    }
    let executable = property(&text, "java.home")
        .map(|home| Path::new(&home).join("bin").join("java"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| candidate.to_path_buf());
    Some(JavaRuntime {
        executable,
        version,
        major,
    })
}

fn property(settings: &str, key: &str) -> Option<String> {
    settings.lines().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_owned())
    })
}

/// `21.0.13` -> 21, `1.8.0_392` -> 8, `11` -> 11.
fn parse_major(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

#[cfg(target_os = "macos")]
fn macos_java_home() -> Option<PathBuf> {
    let mut cmd = Command::new("/usr/libexec/java_home");
    let out = run_captured(&mut cmd, Duration::from_secs(10), None, 4096, 4096).ok()?;
    if !out.status.success() {
        return None;
    }
    let home = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!home.is_empty()).then(|| PathBuf::from(home))
}

#[cfg(not(target_os = "macos"))]
fn macos_java_home() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn major_versions() {
        let got: Vec<Option<u32>> = ["21.0.13", "1.8.0_392", "11", "17.0.19+10", "25-ea", "x", ""]
            .iter()
            .map(|v| parse_major(v))
            .collect();
        assert_eq!(
            got,
            vec![Some(21), Some(8), Some(11), Some(17), Some(25), None, None]
        );
    }

    #[test]
    fn reads_properties_from_the_settings_dump() {
        let dump = "Property settings:\n    java.home = /opt/jdk\n    java.version = 21.0.13\n    java.versionx = no\n";
        assert_eq!(
            (
                property(dump, "java.home"),
                property(dump, "java.version"),
                property(dump, "nope")
            ),
            (Some("/opt/jdk".to_owned()), Some("21.0.13".to_owned()), None)
        );
    }

    #[test]
    fn a_missing_candidate_is_rejected() {
        assert_eq!(probe_java(Path::new("/definitely/not/java")), None);
    }
}
