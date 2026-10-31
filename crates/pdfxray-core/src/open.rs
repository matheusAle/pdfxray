//! Opening with progress: the stages a document goes through and the hook that reports them.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::CoreError;

/// A stage of opening a file, in the order they run.
///
/// [`Document::open_with_progress`](crate::Document::open_with_progress) reports each stage once
/// when it starts (`done` is 0; `total` is 0 until the stage knows its size) and once when it
/// ends (`done == total`). [`OpenStage::CaptureContent`] also reports after every page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum OpenStage {
    /// Read the header, the xref sections and the trailers. Ends with `total` = number of
    /// revisions.
    ReadStructure,
    /// Parse every indirect object and index who refers to whom. Ends with `total` = number of
    /// objects.
    IndexObjects,
    /// Walk the page tree and load the rendering engine. Ends with `total` = number of pages.
    ReadPageTree,
    /// Capture the content of every page. `total` = number of pages; `done` counts finished
    /// pages (in completion order, not page order).
    CaptureContent,
    /// Build the structure tree and the cached navigation trees. Ends with `total` = number of
    /// structure elements.
    BuildStructureTree,
}

/// The caller's progress hook and cancel flag.
pub(crate) struct Progress<'a> {
    sink: &'a mut dyn FnMut(OpenStage, usize, usize),
    cancel: &'a AtomicBool,
}

impl<'a> Progress<'a> {
    pub(crate) fn new(sink: &'a mut dyn FnMut(OpenStage, usize, usize), cancel: &'a AtomicBool) -> Self {
        Self { sink, cancel }
    }

    /// Reports progress. Always called on the thread that is opening the document.
    pub(crate) fn report(&mut self, stage: OpenStage, done: usize, total: usize) {
        (self.sink)(stage, done, total);
    }

    /// The flag workers poll.
    pub(crate) fn cancel_flag(&self) -> &'a AtomicBool {
        self.cancel
    }

    /// `Err(Cancelled)` once the caller has asked to stop.
    pub(crate) fn check(&self) -> Result<(), CoreError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(CoreError::Cancelled)
        } else {
            Ok(())
        }
    }
}
