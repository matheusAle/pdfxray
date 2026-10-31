//! Choosing a file to open, and opening it on a background thread.
//!
//! Opening takes a while on a big file, so the UI never waits for it: [`OpenJob`] runs
//! `Document::open_with_progress` on its own thread and the UI polls it once per frame. What the
//! thread reports ([`OpenProgress`]) is what the Opening card shows; raising the job's cancel
//! flag stops it at the next stage or page.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use pdfxray_core::numeric::f32_from_usize;
use pdfxray_core::{CoreError, Document, OpenStage};

use crate::object_facts::{human_bytes, thousands};
use crate::state::AppState;

/// Asks the user for a PDF and returns its path, or `None` when they cancel.
///
/// The app calls it when "Open…" is pressed. The native dialog is [`native_picker`]; tests
/// inject a closure that returns a fixture path instead.
pub type FilePicker = Box<dyn FnMut() -> Option<PathBuf>>;

/// The operating system's file dialog, filtered to `.pdf`.
pub fn native_picker() -> FilePicker {
    Box::new(|| {
        rfd::FileDialog::new()
            .set_title("Open PDF")
            .add_filter("PDF", &["pdf"])
            .pick_file()
    })
}

/// How long the Opening card waits before it shows, so a quick open does not flash it.
pub const CARD_DELAY: Duration = Duration::from_millis(150);

/// How often the UI looks at a running job.
const POLL: Duration = Duration::from_millis(30);

/// Name of the sample file.
pub const SAMPLE_NAME: &str = "struct-rich.pdf";

/// The sample: a small tagged file of ours, built into the app so "Try a sample" works without
/// the fixtures folder.
const SAMPLE: &[u8] = include_bytes!("../../../fixtures/generated/struct-rich.pdf");

/// Called on the opening thread with every progress report, after the UI was told. A test can
/// block in it to hold the job between two reports.
pub type Observer = Arc<dyn Fn(OpenStage, usize, usize) + Send + Sync>;

/// What is being opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A file on disk.
    Path(PathBuf),
    /// The built-in sample.
    Sample,
}

impl Source {
    /// The name the card and the title bar show.
    pub fn name(&self) -> String {
        match self {
            Self::Path(path) => path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            Self::Sample => SAMPLE_NAME.to_owned(),
        }
    }
}

/// The stages of opening, in the order they run.
pub const STAGES: [OpenStage; 5] = [
    OpenStage::ReadStructure,
    OpenStage::IndexObjects,
    OpenStage::ReadPageTree,
    OpenStage::CaptureContent,
    OpenStage::BuildStructureTree,
];

/// What a stage says on the card.
pub const fn stage_title(stage: OpenStage) -> &'static str {
    match stage {
        OpenStage::ReadStructure => "Read file structure",
        OpenStage::IndexObjects => "Index objects",
        OpenStage::ReadPageTree => "Read page tree",
        OpenStage::CaptureContent => "Capture page content",
        OpenStage::BuildStructureTree => "Build structure tree",
    }
}

/// Where a stage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    /// Not started.
    Pending,
    /// Working.
    Running,
    /// Finished.
    Done,
}

/// What the opening thread reported so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenProgress {
    /// Size of the file in bytes, once known.
    pub bytes: Option<u64>,
    /// The last `(done, total)` of every stage in [`STAGES`] that reported.
    reports: [Option<(usize, usize)>; 5],
}

fn index_of(stage: OpenStage) -> usize {
    STAGES.iter().position(|s| *s == stage).unwrap_or(0)
}

impl OpenProgress {
    fn report(&mut self, stage: OpenStage, done: usize, total: usize) {
        self.reports[index_of(stage)] = Some((done, total));
    }

    /// The state of `stage`: running from its first report, done when it reports that all of its
    /// work is done or the next stage has begun.
    pub fn state(&self, stage: OpenStage) -> StageState {
        let at = index_of(stage);
        let Some((done, total)) = self.reports[at] else {
            return if self.reports[at + 1..].iter().any(Option::is_some) {
                StageState::Done
            } else {
                StageState::Pending
            };
        };
        if self.reports[at + 1..].iter().any(Option::is_some) || (total > 0 && done == total) {
            StageState::Done
        } else {
            StageState::Running
        }
    }

    fn counts(&self, stage: OpenStage) -> Option<(usize, usize)> {
        self.reports[index_of(stage)]
    }

    /// The figure at the right of a stage's row: what it found once it knows, how far it is while
    /// it counts pages.
    pub fn detail(&self, stage: OpenStage) -> Option<String> {
        let (done, total) = self.counts(stage)?;
        let stage_state = self.state(stage);
        if stage_state == StageState::Pending || (total == 0 && stage_state == StageState::Running) {
            return None;
        }
        let plural =
            |n: usize, one: &str, many: &str| format!("{} {}", thousands(n), if n == 1 { one } else { many });
        Some(match stage {
            OpenStage::ReadStructure => format!("xref + {}", plural(total, "revision", "revisions")),
            OpenStage::IndexObjects => plural(total, "object", "objects"),
            OpenStage::CaptureContent if stage_state == StageState::Running => {
                format!("{} / {} pages", thousands(done), thousands(total))
            }
            OpenStage::ReadPageTree | OpenStage::CaptureContent => plural(total, "page", "pages"),
            OpenStage::BuildStructureTree => plural(total, "element", "elements"),
        })
    }

    /// The line under the title: size, objects and pages, as far as they are known.
    pub fn facts(&self) -> String {
        let mut parts = Vec::new();
        if let Some(bytes) = self.bytes {
            parts.push(human_bytes(usize::try_from(bytes).unwrap_or(usize::MAX)));
        }
        for (stage, one, many) in [
            (OpenStage::IndexObjects, "object", "objects"),
            (OpenStage::ReadPageTree, "page", "pages"),
        ] {
            if self.state(stage) == StageState::Done
                && let Some((_, total)) = self.counts(stage)
            {
                parts.push(format!(
                    "{} {}",
                    thousands(total),
                    if total == 1 { one } else { many }
                ));
            }
        }
        parts.join(" · ")
    }

    /// How far along the whole open is, 0 to 1: finished stages plus the share of the running one.
    pub fn fraction(&self) -> f32 {
        let mut stages = 0.0_f32;
        for stage in STAGES {
            stages += match self.state(stage) {
                StageState::Done => 1.0,
                StageState::Pending => 0.0,
                StageState::Running => match self.counts(stage) {
                    Some((done, total)) if total > 0 => f32_from_usize(done) / f32_from_usize(total),
                    _ => 0.0,
                },
            };
        }
        stages / 5.0
    }
}

/// A message from the opening thread.
enum Update {
    Bytes(u64),
    Stage(OpenStage, usize, usize),
    Finished(Result<Box<Document>, CoreError>),
}

/// How a job ended.
pub enum Outcome {
    /// The document is open.
    Opened(Box<Document>),
    /// The file could not be opened, or the job was cancelled.
    Failed(CoreError),
    /// The thread ended without an answer.
    Lost,
}

/// A document being opened on its own thread.
pub struct OpenJob {
    source: Source,
    started: Instant,
    cancel: Arc<AtomicBool>,
    updates: Receiver<Update>,
    /// What the thread reported so far.
    pub progress: OpenProgress,
}

impl std::fmt::Debug for OpenJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenJob")
            .field("source", &self.source)
            .field("progress", &self.progress)
            .finish_non_exhaustive()
    }
}

impl OpenJob {
    /// Starts opening `source`.
    pub fn start(source: Source, observer: Option<Observer>) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, updates) = channel();
        let job = Self {
            source: source.clone(),
            started: Instant::now(),
            cancel: Arc::clone(&cancel),
            updates,
            progress: OpenProgress::default(),
        };
        let work = move || {
            let mut report = |stage: OpenStage, done: usize, total: usize| {
                // A job nobody listens to any more has nothing to tell.
                if sender.send(Update::Stage(stage, done, total)).is_ok()
                    && let Some(observer) = &observer
                {
                    observer(stage, done, total);
                }
            };
            let opened = match &source {
                Source::Path(path) => {
                    // Only the card's byte count needs it; opening reports why the file is unreadable.
                    if let Ok(meta) = std::fs::metadata(path) {
                        sender.send(Update::Bytes(meta.len())).ok();
                    }
                    Document::open_with_progress(path, &mut report, &cancel)
                }
                Source::Sample => {
                    sender
                        .send(Update::Bytes(u64::try_from(SAMPLE.len()).unwrap_or(0)))
                        .ok();
                    Document::from_bytes_with_progress(SAMPLE_NAME, SAMPLE.to_vec(), &mut report, &cancel)
                }
            };
            sender.send(Update::Finished(opened.map(Box::new))).ok();
        };
        // When the thread cannot start, `work` is dropped with its sender and the job reads as lost.
        let _thread = std::thread::Builder::new()
            .name("pdfxray-open".to_owned())
            .spawn(work);
        job
    }

    /// What is being opened.
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// How long the job has been running.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Takes in what the thread said since the last call; `Some` once the job has ended.
    pub fn poll(&mut self) -> Option<Outcome> {
        loop {
            match self.updates.try_recv() {
                Ok(Update::Bytes(bytes)) => self.progress.bytes = Some(bytes),
                Ok(Update::Stage(stage, done, total)) => self.progress.report(stage, done, total),
                Ok(Update::Finished(Ok(doc))) => return Some(Outcome::Opened(doc)),
                Ok(Update::Finished(Err(error))) => return Some(Outcome::Failed(error)),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => return Some(Outcome::Lost),
            }
        }
    }

    /// Tells the thread to stop; the job's answer is no longer wanted.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The sample could not be written where veraPDF reads it.
#[derive(Debug, thiserror::Error)]
#[error("cannot write {}", path.display())]
pub(crate) struct SampleWriteError {
    /// Where the sample should be.
    pub(crate) path: PathBuf,
    /// What the file system said.
    #[source]
    source: std::io::Error,
}

/// Writes the sample under `temp` where veraPDF can read it: validation runs on files. The
/// document does not depend on it, so a failure is only reported.
fn write_sample(temp: &Path) -> Result<PathBuf, SampleWriteError> {
    let dir = temp.join("pdfxray-sample");
    let path = dir.join(SAMPLE_NAME);
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, SAMPLE)) {
        Ok(()) => Ok(path),
        Err(source) => Err(SampleWriteError { path, source }),
    }
}

/// The opening machinery of the app: the job in flight and how it is shown.
pub struct Opener {
    job: Option<OpenJob>,
    /// How long a job runs before the Opening card shows.
    pub card_delay: Duration,
    /// Told about every report of the next jobs.
    pub observer: Option<Observer>,
    /// The spinner of the running stage turns. Off for a screen that must look the same every
    /// time it is drawn (snapshots), and for anyone who wants less motion.
    pub spinner_turns: bool,
}

impl Default for Opener {
    fn default() -> Self {
        Self {
            job: None,
            card_delay: CARD_DELAY,
            observer: None,
            spinner_turns: true,
        }
    }
}

impl Opener {
    /// The job in flight, if any.
    pub fn job(&self) -> Option<&OpenJob> {
        self.job.as_ref()
    }

    /// True when a job runs and has run long enough for the card.
    pub fn card_visible(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.elapsed() >= self.card_delay)
    }
}

impl AppState {
    /// Starts opening `path` on a background thread. A document that is open stays until the new
    /// one is ready; a job that was running is stopped.
    pub fn open_path(&mut self, path: &Path) {
        self.start_open(Source::Path(path.to_path_buf()));
    }

    /// Starts opening the built-in sample.
    pub fn open_sample(&mut self) {
        self.start_open(Source::Sample);
    }

    fn start_open(&mut self, source: Source) {
        if let Some(previous) = self.opening.job.take() {
            previous.cancel();
        }
        self.opening.job = Some(OpenJob::start(source, self.opening.observer.clone()));
    }

    /// Stops the job in flight (the Cancel button): the app goes back to what it showed before.
    pub fn cancel_open(&mut self) {
        if let Some(job) = self.opening.job.take() {
            job.cancel();
            self.status = format!("Cancelled opening {}", job.source().name());
        }
    }

    /// What the job in flight reported, if one runs.
    pub fn open_progress(&self) -> Option<&OpenProgress> {
        self.opening.job().map(|job| &job.progress)
    }

    /// The name of the file being opened, if one is.
    pub fn opening_name(&self) -> Option<String> {
        self.opening.job().map(|job| job.source().name())
    }

    /// Takes in what the opening thread said, and installs the document when it is ready. Called
    /// once per frame.
    pub(crate) fn poll_open(&mut self, ctx: &egui::Context) {
        let Some(job) = self.opening.job.as_mut() else {
            return;
        };
        let Some(outcome) = job.poll() else {
            ctx.request_repaint_after(POLL);
            return;
        };
        let Some(job) = self.opening.job.take() else {
            return;
        };
        match outcome {
            Outcome::Opened(doc) => {
                let (path, unwritten) = match job.source() {
                    Source::Path(path) => (path.clone(), None),
                    Source::Sample => match write_sample(&std::env::temp_dir()) {
                        Ok(path) => (path, None),
                        Err(error) => (error.path.clone(), Some(error)),
                    },
                };
                let remember = matches!(job.source(), Source::Path(_));
                self.install(*doc, &path, job.elapsed(), remember);
                if let Some(error) = unwritten {
                    self.status = sample_not_written(&error);
                }
            }
            Outcome::Failed(CoreError::Cancelled) => {}
            Outcome::Failed(error) => {
                self.fail_open(&job.source().name(), &crate::state::describe(&error));
            }
            Outcome::Lost => self.fail_open(&job.source().name(), "the opening thread stopped unexpectedly"),
        }
    }

    fn fail_open(&mut self, name: &str, why: &str) {
        let message = format!("Cannot open {name}: {why}");
        self.status.clone_from(&message);
        self.open_error = Some(message);
    }
}

/// The status bar's news when the sample is open but could not be written for veraPDF.
fn sample_not_written(error: &SampleWriteError) -> String {
    format!(
        "The sample cannot be validated: {}",
        crate::state::describe(error)
    )
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// A fresh folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pdfxray-open-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_sample_is_written_where_validation_reads_it() {
        let temp = scratch("sample-ok");

        let written = write_sample(&temp).map_err(|e| crate::state::describe(&e));
        let bytes = std::fs::read(temp.join("pdfxray-sample").join(SAMPLE_NAME)).unwrap();
        std::fs::remove_dir_all(&temp).unwrap();

        assert_eq!(written, Ok(temp.join("pdfxray-sample").join(SAMPLE_NAME)));
        // smoke: the bytes are the sample's
        assert_eq!(bytes, SAMPLE);
    }

    #[test]
    fn a_sample_that_cannot_be_written_is_reported_with_its_path() {
        let temp = scratch("sample-blocked");
        // A file where the sample's folder should be.
        std::fs::write(temp.join("pdfxray-sample"), b"in the way").unwrap();

        let error = write_sample(&temp).expect_err("the folder cannot be made");
        std::fs::remove_dir_all(&temp).unwrap();

        let path = temp.join("pdfxray-sample").join(SAMPLE_NAME);
        assert_eq!(error.path, path);
        let status = sample_not_written(&error);
        let expected = format!(
            "The sample cannot be validated: cannot write {}: ",
            path.display()
        );
        assert!(status.starts_with(&expected), "{status}");
        // smoke: the system's reason follows the path
        assert!(status.len() > expected.len(), "{status}");
    }
}
