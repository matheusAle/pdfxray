//! Validation runs: find veraPDF, install it, validate the open file. The UI is the Validation
//! navigator ([`crate::navigator::validation`]) and drawer tab ([`crate::drawer`]).
//!
//! Everything slow (looking for veraPDF, downloading it, validating) runs on a background
//! thread behind the [`Validator`] trait; the UI only polls channels once per frame. The real
//! implementation is [`VeraPdfValidator`]; tests inject a fake with canned reports.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use egui::Context;
use pdfxray_core::NodeId;
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::validation::Report;
use pdfxray_verapdf::{
    Discovery, DiscoveryConfig, Profile, RunError, RunOptions, VeraPdf, default_install_dir, discover,
    install,
};

use crate::state::{AppState, RailItem};

/// How often the UI wakes up while a background task runs.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// What the environment offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// A working veraPDF.
    Ready {
        /// Its version, e.g. `1.30.3`.
        version: String,
    },
    /// No Java 11+ runtime: veraPDF can neither run nor be installed.
    JavaMissing,
    /// Java is there but veraPDF is not.
    NotInstalled,
}

/// Why a [`Validator`] operation did not produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatorError {
    /// The user cancelled.
    Cancelled,
    /// It failed; the text says why.
    Failed(String),
}

impl fmt::Display for ValidatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ValidatorError {}

/// Progress callback of [`Validator::install`]: bytes so far and the total, if known.
pub type InstallProgress<'a> = &'a mut dyn FnMut(u64, Option<u64>);

/// Everything the Validation navigator needs from veraPDF. All methods block; the app calls them on
/// a background thread.
pub trait Validator: Send + Sync {
    /// Looks for a working veraPDF, trying `explicit` (an install folder, launcher or jar) first.
    fn discover(&self, explicit: Option<&Path>) -> Availability;

    /// Downloads and installs veraPDF; returns its version.
    fn install(&self, progress: InstallProgress<'_>) -> Result<String, ValidatorError>;

    /// Validates the file at `pdf`; stops early with [`ValidatorError::Cancelled`] when `cancel`
    /// is set.
    fn validate(
        &self,
        pdf: &Path,
        profile: &Profile,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Report, ValidatorError>;
}

/// The real thing: the `pdfxray-verapdf` crate.
#[derive(Debug, Default)]
pub struct VeraPdfValidator {
    found: Mutex<Option<VeraPdf>>,
}

impl VeraPdfValidator {
    fn remember(&self, found: VeraPdf) -> String {
        let version = found.version().to_owned();
        *self.found.lock().unwrap_or_else(PoisonError::into_inner) = Some(found);
        version
    }
}

impl Validator for VeraPdfValidator {
    fn discover(&self, explicit: Option<&Path>) -> Availability {
        let config = DiscoveryConfig {
            explicit_path: explicit.map(Path::to_path_buf),
            ..DiscoveryConfig::default()
        };
        match discover(&config) {
            Discovery::Found(found) => Availability::Ready {
                version: self.remember(found),
            },
            Discovery::JavaMissing => Availability::JavaMissing,
            Discovery::NotInstalled => Availability::NotInstalled,
        }
    }

    fn install(&self, progress: InstallProgress<'_>) -> Result<String, ValidatorError> {
        let dir = default_install_dir().ok_or_else(|| {
            ValidatorError::Failed("there is no per-user data directory to install into".to_owned())
        })?;
        let found = install(&dir, progress).map_err(|e| ValidatorError::Failed(e.to_string()))?;
        Ok(self.remember(found))
    }

    fn validate(
        &self,
        pdf: &Path,
        profile: &Profile,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Report, ValidatorError> {
        let found = self
            .found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| ValidatorError::Failed("veraPDF has not been found yet".to_owned()))?;
        let options = RunOptions {
            cancel: Some(Arc::clone(cancel)),
            ..RunOptions::default()
        };
        found
            .validate(pdf, profile, &options)
            .map_err(|error| match error {
                RunError::Cancelled => ValidatorError::Cancelled,
                other => ValidatorError::Failed(other.to_string()),
            })
    }
}

/// Where the search for veraPDF stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// Not looked yet; the navigator does it the first time it is shown.
    Unchecked,
    /// A background search is running.
    Checking,
    /// The search finished.
    Known(Availability),
}

/// The finished run handed from [`ValidationState::poll`] to the app.
#[derive(Debug)]
pub(crate) struct Outcome {
    /// File that was validated.
    pub(crate) path: PathBuf,
    /// Profile that was used.
    pub(crate) profile: Profile,
    /// How long the run took.
    pub(crate) took: Duration,
    /// What came back.
    pub(crate) result: Result<Report, ValidatorError>,
}

struct Run {
    rx: Receiver<Result<Report, ValidatorError>>,
    cancel: Arc<AtomicBool>,
    path: PathBuf,
    profile: Profile,
    started: Instant,
}

/// When and how the results shown were produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStamp {
    /// The profile that was checked.
    pub profile: Profile,
    /// When the run finished.
    pub finished: Instant,
    /// How long it took.
    pub took: Duration,
}

impl RunStamp {
    /// `ran 12 s ago`, `ran 3 min ago`, `ran 2 h ago`.
    pub fn ago(&self) -> String {
        let secs = self.finished.elapsed().as_secs();
        match secs {
            0..=59 => format!("ran {secs} s ago"),
            60..=3599 => format!("ran {} min ago", secs / 60),
            _ => format!("ran {} h ago", secs / 3600),
        }
    }

    /// The run time with one decimal: `0.8 s`.
    pub fn took_text(&self) -> String {
        format!("{:.1} s", self.took.as_secs_f64())
    }
}

enum InstallMessage {
    Progress(u64, Option<u64>),
    Done(Result<String, ValidatorError>),
}

struct Install {
    rx: Receiver<InstallMessage>,
    done: u64,
    total: Option<u64>,
}

/// State of validation runs (the results live in the document).
pub struct ValidationState {
    /// The profile the next run uses.
    pub profile: Profile,
    /// Text of the veraPDF location field.
    pub explicit_path: String,
    /// Whether veraPDF was found.
    pub environment: Environment,
    /// The finding last chosen in a list, a badge or by Previous and Next.
    pub selected_finding: Option<u32>,
    /// How the results shown were produced; `None` before the first run on a document.
    pub last: Option<RunStamp>,
    /// Press Validate as soon as the search for veraPDF finds it.
    pub autorun: bool,
    /// Rules whose group is folded in the navigator.
    pub folded: HashSet<String>,
    /// Rules whose group lists every node instead of the first few.
    pub listed_all: HashSet<String>,
    /// The navigator's list should scroll to the current finding the next time it is drawn.
    pub(crate) reveal_in_list: bool,
    /// The drawer's table should scroll to the current finding the next time it is drawn.
    pub(crate) reveal_in_table: bool,
    /// Why the last run, download or search failed.
    pub error: Option<String>,
    run: Option<Run>,
    install: Option<Install>,
    check: Option<Receiver<Availability>>,
}

impl Default for ValidationState {
    fn default() -> Self {
        Self {
            profile: Profile::PdfUa1,
            explicit_path: String::new(),
            environment: Environment::Unchecked,
            selected_finding: None,
            last: None,
            autorun: false,
            folded: HashSet::new(),
            listed_all: HashSet::new(),
            reveal_in_list: false,
            reveal_in_table: false,
            error: None,
            run: None,
            install: None,
            check: None,
        }
    }
}

impl fmt::Debug for ValidationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationState")
            .field("profile", &self.profile)
            .field("environment", &self.environment)
            .field("running", &self.run.is_some())
            .field("installing", &self.install.is_some())
            .finish_non_exhaustive()
    }
}

/// Runs `work` on a new thread and repaints the UI when it ends.
fn spawn<T: Send + 'static>(ctx: &Context, work: impl FnOnce(&Sender<T>) + Send + 'static) -> Receiver<T> {
    let (tx, rx) = mpsc::channel();
    let ctx = ctx.clone();
    thread::spawn(move || {
        work(&tx);
        ctx.request_repaint();
    });
    rx
}

impl ValidationState {
    /// True while a run is in progress.
    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// True while veraPDF is being downloaded.
    pub fn is_installing(&self) -> bool {
        self.install.is_some()
    }

    /// True while any background task runs.
    pub fn is_busy(&self) -> bool {
        self.run.is_some() || self.install.is_some() || self.check.is_some()
    }

    /// Bytes downloaded so far and the total, while installing.
    pub fn install_progress(&self) -> Option<(u64, Option<u64>)> {
        self.install.as_ref().map(|i| (i.done, i.total))
    }

    /// Profile of the run in progress.
    pub fn running_profile(&self) -> Option<&Profile> {
        self.run.as_ref().map(|r| &r.profile)
    }

    /// True once veraPDF was found.
    pub fn is_ready(&self) -> bool {
        matches!(self.environment, Environment::Known(Availability::Ready { .. }))
    }

    /// Looks for veraPDF in the background, trying the typed path first.
    pub(crate) fn start_check(&mut self, ctx: &Context, validator: &Arc<dyn Validator>) {
        let explicit = Some(self.explicit_path.trim())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        let validator = Arc::clone(validator);
        self.environment = Environment::Checking;
        self.error = None;
        self.check = Some(spawn(ctx, move |tx| {
            // The receiver is gone when the check was superseded; nothing to report then.
            let _ = tx.send(validator.discover(explicit.as_deref()));
        }));
    }

    /// Downloads veraPDF in the background.
    pub(crate) fn start_install(&mut self, ctx: &Context, validator: &Arc<dyn Validator>) {
        if self.install.is_some() {
            return;
        }
        let validator = Arc::clone(validator);
        self.error = None;
        let rx = spawn(ctx, move |tx| {
            let progress_tx = tx.clone();
            let result = validator.install(&mut |done, total| {
                let _ = progress_tx.send(InstallMessage::Progress(done, total));
            });
            let _ = tx.send(InstallMessage::Done(result));
        });
        self.install = Some(Install {
            rx,
            done: 0,
            total: None,
        });
    }

    /// Validates `path` in the background.
    pub(crate) fn start_run(&mut self, ctx: &Context, validator: &Arc<dyn Validator>, path: &Path) {
        if self.run.is_some() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (validator, flag) = (Arc::clone(validator), Arc::clone(&cancel));
        let (file, profile) = (path.to_path_buf(), self.profile.clone());
        let job_profile = profile.clone();
        let rx = spawn(ctx, move |tx| {
            let _ = tx.send(validator.validate(&file, &job_profile, &flag));
        });
        self.error = None;
        self.run = Some(Run {
            rx,
            cancel,
            path: path.to_path_buf(),
            profile,
            started: Instant::now(),
        });
    }

    /// Asks the run in progress to stop.
    pub fn cancel_run(&mut self) {
        if let Some(run) = &self.run {
            run.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Stops the run in progress and ignores its result, e.g. because another file was opened.
    pub(crate) fn abandon_run(&mut self) {
        self.cancel_run();
        self.run = None;
        self.selected_finding = None;
        self.last = None;
        self.autorun = false;
        self.folded.clear();
        self.listed_all.clear();
    }

    /// Takes in whatever the background tasks have produced since the last frame. A finished
    /// run is returned for the app to attach to the document.
    pub(crate) fn poll(&mut self) -> Option<Outcome> {
        self.poll_check();
        self.poll_install();
        self.poll_run()
    }

    fn poll_check(&mut self) {
        let Some(rx) = &self.check else {
            return;
        };
        match rx.try_recv() {
            Ok(found) => {
                self.environment = Environment::Known(found);
                self.check = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.check = None;
                self.environment = Environment::Known(Availability::NotInstalled);
                self.error = Some("The search for veraPDF stopped unexpectedly.".to_owned());
            }
        }
    }

    fn poll_install(&mut self) {
        let Some(install) = &mut self.install else {
            return;
        };
        let finished = loop {
            match install.rx.try_recv() {
                Ok(InstallMessage::Progress(done, total)) => {
                    install.done = done;
                    install.total = total;
                }
                Ok(InstallMessage::Done(result)) => break Some(result),
                Err(TryRecvError::Empty) => break None,
                Err(TryRecvError::Disconnected) => {
                    break Some(Err(ValidatorError::Failed(
                        "the download stopped unexpectedly".to_owned(),
                    )));
                }
            }
        };
        let Some(result) = finished else {
            return;
        };
        self.install = None;
        match result {
            Ok(version) => self.environment = Environment::Known(Availability::Ready { version }),
            Err(ValidatorError::Cancelled) => {}
            Err(ValidatorError::Failed(message)) => {
                self.error = Some(format!("Installing veraPDF failed: {message}"));
            }
        }
    }

    fn poll_run(&mut self) -> Option<Outcome> {
        let run = self.run.as_ref()?;
        let result = match run.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(ValidatorError::Failed(
                "the validation thread stopped unexpectedly".to_owned(),
            )),
        };
        let run = self.run.take()?;
        Some(Outcome {
            path: run.path,
            profile: run.profile,
            took: run.started.elapsed(),
            result,
        })
    }
}

impl AppState {
    /// Starts validating the open file with the selected profile. Does nothing without a
    /// document, without veraPDF, or while a run is in progress.
    pub fn validate(&mut self, ctx: &Context) {
        let Some(loaded) = &self.document else {
            return;
        };
        if !self.validation.is_ready() || self.validation.is_running() {
            return;
        }
        self.validation.start_run(ctx, &self.validator, &loaded.path);
        self.status = format!("Validating against {}…", self.validation.profile.label());
    }

    /// The Validate button of the title bar: shows the Validation entry and runs the selected
    /// profile. When veraPDF has not been looked for yet, the run starts as soon as it is found.
    pub fn request_validation(&mut self, ctx: &Context) {
        if self.document.is_none() {
            return;
        }
        self.set_rail(RailItem::Validation);
        if self.validation.environment == Environment::Unchecked {
            self.recheck_verapdf(ctx);
        }
        self.validation.autorun = true;
        self.start_pending_validation(ctx);
    }

    /// Picks the profile of the next run. When results are showing, they no longer answer the
    /// picker, so the new profile runs right away.
    pub fn change_profile(&mut self, ctx: &Context, profile: Profile) {
        if self.validation.is_running() || self.validation.profile == profile {
            return;
        }
        self.validation.profile = profile;
        let has_results = self
            .document
            .as_ref()
            .is_some_and(|loaded| loaded.doc.validation().is_some());
        if has_results {
            self.validate(ctx);
        }
    }

    /// Runs a validation that waited for veraPDF to be found.
    fn start_pending_validation(&mut self, ctx: &Context) {
        if !self.validation.autorun {
            return;
        }
        match self.validation.environment {
            Environment::Known(Availability::Ready { .. }) => {
                self.validation.autorun = false;
                self.validate(ctx);
            }
            Environment::Known(_) => self.validation.autorun = false,
            Environment::Unchecked | Environment::Checking => {}
        }
    }

    /// Cancels the run in progress.
    pub fn cancel_validation(&mut self) {
        self.validation.cancel_run();
    }

    /// Looks for veraPDF again, honouring the typed location.
    pub fn recheck_verapdf(&mut self, ctx: &Context) {
        self.validation.start_check(ctx, &self.validator);
    }

    /// Downloads and installs veraPDF.
    pub fn install_verapdf(&mut self, ctx: &Context) {
        self.validation.start_install(ctx, &self.validator);
    }

    /// Selects a finding's target, which also shows its page; a finding without a target
    /// selects the finding itself.
    pub fn select_finding(&mut self, index: u32) {
        let target = self
            .document
            .as_ref()
            .and_then(|l| l.doc.validation())
            .and_then(|run| run.findings.get(usize_from_u32(index)))
            .map(|f| f.target.clone());
        let Some(target) = target else {
            return;
        };
        self.validation.selected_finding = Some(index);
        self.select(target.unwrap_or(NodeId::Validation(index)));
        self.show_finding_in_lists(index);
    }

    /// Opens the group of `index` and scrolls the lists to it.
    fn show_finding_in_lists(&mut self, index: u32) {
        let Some(run) = self.document.as_ref().and_then(|l| l.doc.validation()) else {
            return;
        };
        let Some(rule) = run.findings.get(usize_from_u32(index)).map(|f| f.rule.clone()) else {
            return;
        };
        let place = run
            .findings
            .iter()
            .zip(0u32..)
            .filter(|(f, _)| f.rule == rule)
            .position(|(_, i)| i == index)
            .unwrap_or(0);
        self.validation.folded.remove(&rule);
        if place >= crate::navigator::validation::SHOWN_NODES {
            self.validation.listed_all.insert(rule);
        }
        self.validation.reveal_in_list = true;
        self.validation.reveal_in_table = true;
    }

    /// Applies what background tasks finished and keeps the UI awake while some are running.
    /// Called once per frame.
    pub fn poll_background(&mut self, ctx: &Context) {
        self.poll_renders();
        if let Some(outcome) = self.validation.poll() {
            self.finish_validation(outcome);
        }
        self.start_pending_validation(ctx);
        if self.validation.is_busy() {
            ctx.request_repaint_after(POLL_INTERVAL);
        }
    }

    fn finish_validation(&mut self, outcome: Outcome) {
        let Outcome {
            path,
            profile,
            took,
            result,
        } = outcome;
        let report = match result {
            Ok(report) => report,
            Err(ValidatorError::Cancelled) => {
                "Validation cancelled".clone_into(&mut self.status);
                return;
            }
            Err(ValidatorError::Failed(message)) => {
                self.status = format!("Validation failed: {message}");
                self.validation.error = Some(message);
                return;
            }
        };
        let Some(loaded) = &mut self.document else {
            return;
        };
        if loaded.path != path {
            return;
        }
        loaded.doc.set_validation(&report);
        self.validation.selected_finding = None;
        self.validation.last = Some(RunStamp {
            profile: profile.clone(),
            finished: Instant::now(),
            took,
        });
        self.invalidate_trees();
        let verdict = if report.compliant {
            "compliant"
        } else {
            "not compliant"
        };
        self.status = format!("Validated against {}: {verdict}", profile.label());
    }
}
