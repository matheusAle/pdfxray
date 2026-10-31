//! The engine edge: runs `hayro-interpret` over an instrumented stream and collects its
//! warnings. The only place the walker hands geometry to the engine (as `kurbo` types).

use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hayro_interpret::{Context, InterpreterCache, InterpreterSettings, interpret};
use hayro_syntax::Pdf;
use hayro_syntax::content::TypedIter;
use hayro_syntax::page::Resources;
use kurbo::Affine;

use super::recorder::Recorder;
use super::types::CaptureNote;
use crate::{Matrix, Rect};

/// The engine's warnings for one page, as notes. The engine's warning sink holds a clone.
///
/// A panic while the list is locked poisons it; the warnings in it are still the page's, so a
/// poisoned lock is recovered instead of losing them.
#[derive(Clone, Default)]
struct EngineWarnings(Arc<Mutex<Vec<CaptureNote>>>);

impl EngineWarnings {
    fn push(&self, warning: &dyn Debug) {
        let note = CaptureNote {
            op: None,
            message: format!("engine warning: {warning:?}"),
        };
        self.lock().push(note);
    }

    /// The warnings so far, in the order the engine gave them; the list is left empty.
    fn take(&self) -> Vec<CaptureNote> {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<CaptureNote>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The engine set up for one page: its caches, settings and warning sink.
pub(super) struct Engine<'a> {
    pdf: &'a Pdf,
    cache: InterpreterCache<'a>,
    settings: InterpreterSettings,
    warnings: EngineWarnings,
    media_box: kurbo::Rect,
}

impl<'a> Engine<'a> {
    pub(super) fn new(pdf: &'a Pdf, media_box: kurbo::Rect) -> Self {
        let warnings = EngineWarnings::default();
        let mut settings = InterpreterSettings::default();
        {
            let sink = warnings.clone();
            settings.warning_sink = Arc::new(move |w| sink.push(&w));
        }
        Self {
            pdf,
            cache: InterpreterCache::new(),
            settings,
            warnings,
            media_box,
        }
    }

    /// The settings fonts are read with, so they resolve `CMaps` as the engine does.
    pub(super) fn settings(&self) -> &InterpreterSettings {
        &self.settings
    }

    /// The page's media box in page space; `None` when it is not finite.
    pub(super) fn media_box(&self) -> Option<Rect> {
        let r = self.media_box;
        [r.x0, r.y0, r.x1, r.y1]
            .iter()
            .all(|v| v.is_finite())
            .then(|| Rect::new(r.x0, r.y0, r.x1, r.y1))
    }

    /// Executes `content` with `resources`, starting from `base_ctm`, into `recorder`. Returns
    /// false when the engine panicked; the recorder keeps what it saw before the panic.
    pub(super) fn interpret(
        &self,
        content: &[u8],
        resources: &Resources<'a>,
        base_ctm: Matrix,
        recorder: &mut Recorder,
    ) -> bool {
        let mut ctx = Context::new(
            Affine::new(base_ctm.to_array()),
            self.media_box,
            &self.cache,
            self.pdf.xref(),
            self.settings.clone(),
        );
        catch_unwind(AssertUnwindSafe(|| {
            interpret(TypedIter::new(content), resources, &mut ctx, recorder);
        }))
        .is_ok()
    }

    /// The engine's warnings so far, as notes; they are handed out once.
    pub(super) fn take_warnings(&self) -> Vec<CaptureNote> {
        self.warnings.take()
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn engine_warnings_survive_a_panic_while_the_list_was_locked() {
        let warnings = EngineWarnings::default();
        warnings.push(&"first");
        let sink = warnings.clone();
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _held = sink.lock();
            std::panic::panic_any("engine panicked mid-warning");
        }));
        warnings.push(&"second");

        let notes = warnings.take();

        assert!(
            panicked.is_err() && warnings.0.is_poisoned(),
            "the lock is poisoned"
        );
        assert_eq!(
            notes,
            [
                CaptureNote {
                    op: None,
                    message: "engine warning: \"first\"".to_owned(),
                },
                CaptureNote {
                    op: None,
                    message: "engine warning: \"second\"".to_owned(),
                },
            ]
        );
        // smoke: taking empties the list
        assert_eq!(warnings.take(), []);
    }
}
