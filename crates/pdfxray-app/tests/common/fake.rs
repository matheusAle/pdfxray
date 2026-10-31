//! A veraPDF stand-in: no Java, no downloads, canned reports.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use pdfxray_app::{Availability, InstallProgress, Validator, ValidatorError};
use pdfxray_core::validation::Report;
use pdfxray_verapdf::{Profile, parse_report};

/// The captured veraPDF report for the `orphan-mcid` fixture, PDF/UA-1 profile.
pub(crate) fn orphan_mcid_report() -> Report {
    let xml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pdfxray-verapdf/tests/data/orphan-mcid-ua1.xml"
    ))
    .unwrap();
    parse_report(&xml).unwrap()
}

/// A hand-edited report for the `struct-rich` fixture (two pages): five Figure findings on pages
/// 1 and 2, one untagged content finding on page 2 and one on the document. See the comment in
/// `tests/data/struct-rich-ua1.xml`: it is not the output of a real veraPDF run.
pub(crate) fn struct_rich_report() -> Report {
    let xml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/struct-rich-ua1.xml"
    ))
    .unwrap();
    parse_report(&xml).unwrap()
}

/// A door that tasks wait at until the test opens it.
#[derive(Debug, Default)]
pub(crate) struct Gate(AtomicBool);

impl Gate {
    pub(crate) fn open(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Blocks until the gate opens or `cancel` is set; true when it opened.
    fn pass(&self, cancel: Option<&AtomicBool>) -> bool {
        loop {
            if self.0.load(Ordering::SeqCst) {
                return true;
            }
            if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
                return false;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

/// What the app asked the fake to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Call {
    Discover(Option<PathBuf>),
    Install,
    Validate(PathBuf, Profile),
}

/// A configurable [`Validator`].
#[derive(Debug)]
pub(crate) struct FakeValidator {
    /// What discovery finds until something is installed.
    pub(crate) initial: Availability,
    /// A path that makes discovery succeed, like a real install folder would.
    pub(crate) good_path: Option<PathBuf>,
    /// Runs and downloads wait here.
    pub(crate) run_gate: Gate,
    pub(crate) install_gate: Gate,
    pub(crate) report: Report,
    pub(crate) install_error: Option<String>,
    installed: AtomicBool,
    calls: Mutex<Vec<Call>>,
}

impl FakeValidator {
    pub(crate) fn new(initial: Availability) -> Self {
        Self {
            initial,
            good_path: None,
            run_gate: Gate::default(),
            install_gate: Gate::default(),
            report: orphan_mcid_report(),
            install_error: None,
            installed: AtomicBool::new(false),
            calls: Mutex::default(),
        }
    }

    /// veraPDF 1.30.3 is available; runs finish immediately.
    pub(crate) fn ready() -> Arc<Self> {
        let fake = Self::new(Availability::Ready {
            version: "1.30.3".to_owned(),
        });
        fake.run_gate.open();
        fake.install_gate.open();
        Arc::new(fake)
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn record(&self, call: Call) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}

impl Validator for FakeValidator {
    fn discover(&self, explicit: Option<&Path>) -> Availability {
        self.record(Call::Discover(explicit.map(Path::to_path_buf)));
        let good = explicit.is_some() && self.good_path.as_deref() == explicit;
        if good || self.installed.load(Ordering::SeqCst) {
            return Availability::Ready {
                version: "1.30.3".to_owned(),
            };
        }
        self.initial.clone()
    }

    fn install(&self, progress: InstallProgress<'_>) -> Result<String, ValidatorError> {
        self.record(Call::Install);
        progress(10_000_000, Some(34_000_000));
        self.install_gate.pass(None);
        if let Some(message) = &self.install_error {
            return Err(ValidatorError::Failed(message.clone()));
        }
        progress(34_000_000, Some(34_000_000));
        self.installed.store(true, Ordering::SeqCst);
        Ok("1.30.3".to_owned())
    }

    fn validate(
        &self,
        pdf: &Path,
        profile: &Profile,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Report, ValidatorError> {
        self.record(Call::Validate(pdf.to_path_buf(), profile.clone()));
        if self.run_gate.pass(Some(cancel)) {
            Ok(self.report.clone())
        } else {
            Err(ValidatorError::Cancelled)
        }
    }
}
