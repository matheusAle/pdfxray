//! A located veraPDF installation and the validation run.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use pdfxray_core::validation::Report;

use crate::process::{Captured, ProcError, run_captured};
use crate::profile::{Profile, Selection};
use crate::report::{ParseError, parse_report};

/// Upper bound on the XML report we are willing to buffer.
const MAX_REPORT_BYTES: usize = 1 << 30;
/// How much stderr is kept for error messages.
const STDERR_BYTES: usize = 64 * 1024;
/// Time allowed for `--version` when opening an installation.
const VERSION_TIMEOUT: Duration = Duration::from_secs(60);

/// Options of one validation run.
#[derive(Debug, Clone)]
pub struct RunOptions {
    /// Stop validating after this many failed checks (`--maxfailures`); `None` = never stop.
    pub max_failures: Option<u32>,
    /// Failed checks listed per rule (`--maxfailuresdisplayed`); `None` = all of them.
    pub max_failures_displayed: Option<u32>,
    /// The child is killed and [`RunError::Timeout`] returned after this long.
    pub timeout: Duration,
    /// Set the flag to kill the child and get [`RunError::Cancelled`].
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_failures: None,
            max_failures_displayed: None,
            timeout: Duration::from_secs(300),
            cancel: None,
        }
    }
}

/// Everything that can go wrong during [`VeraPdf::validate`].
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The PDF to validate does not exist or cannot be resolved.
    #[error("cannot read input PDF {path}: {source}")]
    InputNotFound {
        /// Path as given.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The profile needs a file that is not available (see [`crate::install`]).
    #[error("profile {profile} needs {path}, which does not exist")]
    ProfileUnavailable {
        /// Profile label.
        profile: String,
        /// Missing profile file.
        path: PathBuf,
    },
    /// The process could not be started.
    #[error("could not start veraPDF: {0}")]
    Spawn(#[source] std::io::Error),
    /// Waiting for the process failed.
    #[error("waiting for veraPDF failed: {0}")]
    Wait(#[source] std::io::Error),
    /// The run exceeded [`RunOptions::timeout`]; the child was killed.
    #[error("veraPDF did not finish within {0:?} and was killed")]
    Timeout(Duration),
    /// The run was cancelled through [`RunOptions::cancel`]; the child was killed.
    #[error("validation was cancelled")]
    Cancelled,
    /// Exit code 2: veraPDF rejected the command line.
    #[error("veraPDF rejected its arguments: {0}")]
    BadParameters(String),
    /// Exit code 3: the JVM ran out of heap.
    #[error("veraPDF ran out of memory (try a larger JVM heap)")]
    OutOfMemory,
    /// Exit code 4: veraPDF found no file to process.
    #[error("veraPDF found no file to process: {0}")]
    NoFiles(String),
    /// Exit code 6: I/O failure inside veraPDF.
    #[error("veraPDF hit an I/O error: {0}")]
    Io(String),
    /// Exit code 7: the PDF could not be parsed.
    #[error("veraPDF could not parse the PDF: {0}")]
    DamagedPdf(String),
    /// Exit code 8: the PDF is encrypted and could not be opened.
    #[error("the PDF is encrypted: {0}")]
    Encrypted(String),
    /// Exit code 9: internal veraPDF exception.
    #[error("veraPDF failed internally: {0}")]
    Internal(String),
    /// Exit code 10: the report could not be written.
    #[error("veraPDF could not generate its report: {0}")]
    ReportGeneration(String),
    /// Exit code 12: veraPDF was interrupted.
    #[error("veraPDF was interrupted")]
    Interrupted,
    /// Any other termination, including death by signal (`code` is `None`).
    #[error("veraPDF exited unexpectedly (code {code:?}): {stderr}")]
    UnexpectedExit {
        /// Exit code, if the process exited normally.
        code: Option<i32>,
        /// Captured stderr.
        stderr: String,
    },
    /// veraPDF finished but its report could not be understood.
    #[error("veraPDF report could not be parsed: {0}")]
    Report(#[from] ParseError),
}

/// A usable veraPDF CLI together with the Java runtime that runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeraPdf {
    java: PathBuf,
    launcher: PathBuf,
    cli_jar: Option<PathBuf>,
    version: String,
    profiles_dir: Option<PathBuf>,
}

impl VeraPdf {
    /// Opens the installation at `entry` (install directory, `verapdf` launcher script or CLI
    /// jar) by asking it for its version. `None` when it is not a working veraPDF.
    #[must_use]
    pub fn open(java: &Path, entry: &Path) -> Option<Self> {
        let entry = entry.canonicalize().ok()?;
        let (root, launcher, jar) = if entry.is_dir() {
            (entry.clone(), entry.join("verapdf"), find_cli_jar(&entry))
        } else if entry.extension().is_some_and(|e| e == "jar") {
            let root = entry
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_default();
            (root, entry.clone(), Some(entry.clone()))
        } else {
            let root = entry.parent().map(Path::to_path_buf).unwrap_or_default();
            let jar = find_cli_jar(&root);
            (root, entry.clone(), jar)
        };
        if jar.is_none() && !launcher.is_file() {
            return None;
        }
        let profiles = root.join("profiles");
        let mut candidate = Self {
            java: java.to_path_buf(),
            launcher,
            cli_jar: jar,
            version: String::new(),
            profiles_dir: profiles.is_dir().then_some(profiles),
        };
        candidate.version = candidate.query_version()?;
        Some(candidate)
    }

    /// veraPDF version string, e.g. `1.30.3`.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The `java` executable used to run veraPDF.
    #[must_use]
    pub fn java(&self) -> &Path {
        &self.java
    }

    /// The `verapdf` launcher script, or the CLI jar when no launcher was found.
    #[must_use]
    pub fn launcher(&self) -> &Path {
        &self.launcher
    }

    /// Directory holding the extra profile files used by [`Profile::Wcag22Machine`],
    /// [`Profile::Iso32000_1Tagged`] and [`Profile::Iso32000_2Tagged`], if known.
    #[must_use]
    pub fn profiles_dir(&self) -> Option<&Path> {
        self.profiles_dir.as_deref()
    }

    /// Uses `dir` as the profile directory (for installations not made by [`crate::install`]).
    #[must_use]
    pub fn with_profiles_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.profiles_dir = Some(dir.into());
        self
    }

    /// Validates `pdf` against `profile` and parses the machine readable report.
    ///
    /// Exit codes 0 (compliant) and 1 (non-compliant) both yield a [`Report`]; every other
    /// exit code maps onto a [`RunError`] variant.
    pub fn validate(&self, pdf: &Path, profile: &Profile, opts: &RunOptions) -> Result<Report, RunError> {
        let pdf = pdf.canonicalize().map_err(|source| RunError::InputNotFound {
            path: pdf.to_path_buf(),
            source,
        })?;
        let mut cmd = self.base_command();
        cmd.args(["--format", "xml", "--maxfailuresdisplayed"]).arg(
            opts.max_failures_displayed
                .map_or_else(|| "-1".to_owned(), |n| n.to_string()),
        );
        if let Some(n) = opts.max_failures {
            cmd.arg("--maxfailures").arg(n.to_string());
        }
        match profile.selection() {
            Selection::Flavour(flag) => cmd.args(["--flavour", flag]),
            Selection::ProfileFile(file) => {
                let path = self
                    .profiles_dir
                    .as_deref()
                    .map_or_else(|| PathBuf::from(file), |d| d.join(file));
                if !path.is_file() {
                    return Err(RunError::ProfileUnavailable {
                        profile: profile.label(),
                        path,
                    });
                }
                cmd.arg("--profile").arg(path)
            }
            Selection::Path(path) => {
                if !path.is_file() {
                    return Err(RunError::ProfileUnavailable {
                        profile: profile.label(),
                        path,
                    });
                }
                cmd.arg("--profile").arg(path)
            }
        };
        cmd.arg(&pdf);

        let captured = run_captured(
            &mut cmd,
            opts.timeout,
            opts.cancel.as_deref(),
            MAX_REPORT_BYTES,
            STDERR_BYTES,
        )
        .map_err(|e| match e {
            ProcError::Spawn(e) => RunError::Spawn(e),
            ProcError::Wait(e) => RunError::Wait(e),
            ProcError::Timeout => RunError::Timeout(opts.timeout),
            ProcError::Cancelled => RunError::Cancelled,
        })?;
        interpret(&captured)
    }

    fn base_command(&self) -> Command {
        if let Some(jar) = &self.cli_jar {
            let mut cmd = Command::new(&self.java);
            cmd.args([
                "-XX:+IgnoreUnrecognizedVMOptions",
                "-Dfile.encoding=UTF8",
                "--add-exports=java.base/sun.security.pkcs=ALL-UNNAMED",
                "-jar",
            ])
            .arg(jar);
            cmd
        } else {
            let mut cmd = Command::new(&self.launcher);
            cmd.env("JAVACMD", OsString::from(&self.java));
            cmd
        }
    }

    fn query_version(&self) -> Option<String> {
        let mut cmd = self.base_command();
        cmd.arg("--version");
        let out = run_captured(&mut cmd, VERSION_TIMEOUT, None, 64 * 1024, 64 * 1024).ok()?;
        if !out.status.success() {
            return None;
        }
        parse_version(&String::from_utf8_lossy(&out.stdout))
    }
}

/// `veraPDF 1.30.3` banner line -> `1.30.3`.
fn parse_version(banner: &str) -> Option<String> {
    banner.lines().find_map(|line| {
        let version = line.trim().strip_prefix("veraPDF ")?.trim();
        version
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
            .then(|| version.to_owned())
    })
}

fn find_cli_jar(root: &Path) -> Option<PathBuf> {
    // No readable `bin` folder means no CLI jar: the install is not a working veraPDF.
    let mut jars: Vec<PathBuf> = std::fs::read_dir(root.join("bin"))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "jar")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("cli-"))
        })
        .collect();
    jars.sort();
    jars.pop()
}

/// Maps the finished process onto a report or a typed error.
fn interpret(out: &Captured) -> Result<Report, RunError> {
    let stderr = out.stderr_text();
    let xml = || String::from_utf8_lossy(&out.stdout).into_owned();
    // For parse/encrypted failures veraPDF still prints a report whose taskException says why.
    let job_message = || match parse_report(&xml()) {
        Err(ParseError::JobFailed { message, .. }) => message,
        _ => stderr.clone(),
    };
    match out.status.code() {
        Some(0 | 1) => parse_report(&xml()).map_err(RunError::Report),
        Some(2) => Err(RunError::BadParameters(first_line(&stderr))),
        Some(3) => Err(RunError::OutOfMemory),
        Some(4) => Err(RunError::NoFiles(first_line(&stderr))),
        Some(6) => Err(RunError::Io(job_message())),
        Some(7) => Err(RunError::DamagedPdf(job_message())),
        Some(8) => Err(RunError::Encrypted(job_message())),
        Some(9) => Err(RunError::Internal(job_message())),
        Some(10) => Err(RunError::ReportGeneration(stderr)),
        Some(12) => Err(RunError::Interrupted),
        code => Err(RunError::UnexpectedExit { code, stderr }),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    fn captured(code: i32, stdout: &str, stderr: &str) -> Captured {
        Captured {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn banner_version() {
        let banner = "veraPDF 1.30.3\nBuilt: Wed Oct 07 13:52:00 BRT 2026\nDeveloped and released by the veraPDF Consortium.\n";
        assert_eq!(parse_version(banner), Some("1.30.3".to_owned()));
        assert_eq!(parse_version("veraPDF is great\n"), None);
    }

    #[test]
    fn exit_codes_map_to_typed_errors() {
        let encrypted = include_str!("../tests/data/job-failed-encrypted.xml");
        let cases: Vec<(Captured, &str)> = vec![
            (
                captured(2, "", "Illegal --flavour argument:zz\nUsage"),
                "BadParameters(\"Illegal --flavour argument:zz\")",
            ),
            (captured(3, "", ""), "OutOfMemory"),
            (
                captured(4, "", "SEVERE: File /x.pdf doesn't exist.\nmore"),
                "NoFiles(\"SEVERE: File /x.pdf doesn't exist.\")",
            ),
            (
                captured(8, encrypted, "noise"),
                "Encrypted(\"Exception: The PDF stream appears to be encrypted. caused by exception: encrypted PDF with unknown or wrong password\")",
            ),
            (captured(7, "", "boom"), "DamagedPdf(\"boom\")"),
            (captured(12, "", ""), "Interrupted"),
            (
                captured(77, "", "weird"),
                "UnexpectedExit { code: Some(77), stderr: \"weird\" }",
            ),
        ];
        for (out, expected) in cases {
            let err = interpret(&out).expect_err("is an error");
            assert_eq!(format!("{err:?}"), expected);
        }
    }

    #[test]
    fn a_signal_death_is_unexpected() {
        let out = Captured {
            status: ExitStatus::from_raw(9),
            stdout: vec![],
            stderr: b"killed".to_vec(),
        };
        let err = interpret(&out).expect_err("is an error");
        assert_eq!(
            format!("{err:?}"),
            "UnexpectedExit { code: None, stderr: \"killed\" }"
        );
    }

    #[test]
    fn exit_one_with_a_report_is_not_an_error() {
        let xml = include_str!("../tests/data/tagged-simple-ua1.xml");
        let report = interpret(&captured(1, xml, "")).expect("exit 1 carries a report");
        assert_eq!((report.compliant, report.checks_failed), (false, 3));
    }

    #[test]
    fn unreadable_input_never_spawns() {
        let vera = VeraPdf {
            java: PathBuf::from("/definitely/not/java"),
            launcher: PathBuf::from("/definitely/not/verapdf"),
            cli_jar: None,
            version: "0".to_owned(),
            profiles_dir: None,
        };
        let err = vera
            .validate(
                Path::new("/definitely/not/a.pdf"),
                &Profile::PdfUa1,
                &RunOptions::default(),
            )
            .expect_err("missing input");
        assert!(matches!(err, RunError::InputNotFound { .. }), "got {err:?}");
    }

    #[test]
    fn profile_files_must_exist() {
        let vera = VeraPdf {
            java: PathBuf::from("/definitely/not/java"),
            launcher: PathBuf::from("/definitely/not/verapdf"),
            cli_jar: None,
            version: "0".to_owned(),
            profiles_dir: None,
        };
        let this = PathBuf::from(file!());
        let pdf = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(pdf.is_file(), "{this:?}");
        let err = vera
            .validate(&pdf, &Profile::Wcag22Machine, &RunOptions::default())
            .expect_err("no profile dir");
        assert!(
            matches!(&err, RunError::ProfileUnavailable { profile, path } if profile == "WCAG 2.2 (machine)" && path == Path::new("WCAG-2-2-Machine.xml")),
            "got {err:?}"
        );
    }
}
