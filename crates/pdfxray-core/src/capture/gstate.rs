//! The graphics-state stack: the current [`GState`] with its font, `q`/`Q` saves scoped to the
//! stream being walked, and a shared snapshot for operators that leave the state unchanged.

use std::rc::Rc;
use std::sync::Arc;

use super::font::FontInfo;
use super::types::GState;
use crate::Matrix;

/// The current graphics state and the states saved by `q`.
#[derive(Default)]
pub(super) struct GStateStack {
    current: GState,
    /// The font object behind `current.text`; saved and restored with the state.
    font: Option<Rc<FontInfo>>,
    /// `current` as last shared; dropped by every change.
    shared: Option<Arc<GState>>,
    saved: Vec<(GState, Option<Rc<FontInfo>>)>,
    /// Saves below this index belong to an invoking stream; `Q` cannot reach them.
    base: usize,
}

/// What [`GStateStack::enter_form`] put aside, for [`GStateStack::leave_form`].
pub(super) struct FormState {
    state: GState,
    font: Option<Rc<FontInfo>>,
    base: usize,
}

impl GStateStack {
    pub(super) fn current(&self) -> &GState {
        &self.current
    }

    /// The current state, for a change.
    pub(super) fn edit(&mut self) -> &mut GState {
        self.shared = None;
        &mut self.current
    }

    /// The current state, shared: unchanged states share one allocation.
    pub(super) fn snapshot(&mut self) -> Arc<GState> {
        if let Some(s) = &self.shared {
            return Arc::clone(s);
        }
        let s = Arc::new(self.current.clone());
        self.shared = Some(Arc::clone(&s));
        s
    }

    pub(super) fn ctm(&self) -> Matrix {
        Matrix::from(self.current.ctm)
    }

    pub(super) fn font(&self) -> Option<&Rc<FontInfo>> {
        self.font.as_ref()
    }

    pub(super) fn set_font(&mut self, font: Rc<FontInfo>) {
        self.font = Some(font);
    }

    /// `q`.
    pub(super) fn save(&mut self) {
        self.saved.push((self.current.clone(), self.font.clone()));
    }

    /// `Q`. The text matrices are not part of the saved state and survive. Returns false, and
    /// changes nothing, when this stream has no matching `q`.
    pub(super) fn restore(&mut self) -> bool {
        if self.saved.len() <= self.base {
            return false;
        }
        if let Some((mut restored, font)) = self.saved.pop() {
            restored.text.tm = self.current.text.tm;
            restored.text.tlm = self.current.text.tlm;
            self.current = restored;
            self.font = font;
            self.shared = None;
        }
        true
    }

    /// Starts a Form `XObject`'s stream: the form's `q`/`Q` pairs cannot reach the invoker's
    /// saves, and its state changes are undone by [`leave_form`](Self::leave_form).
    pub(super) fn enter_form(&mut self) -> FormState {
        FormState {
            state: self.current.clone(),
            font: self.font.clone(),
            base: std::mem::replace(&mut self.base, self.saved.len()),
        }
    }

    /// Ends a Form `XObject`'s stream: drops its unbalanced saves and restores the state at the
    /// `Do`.
    pub(super) fn leave_form(&mut self, outer: FormState) {
        self.saved.truncate(self.base);
        self.base = outer.base;
        self.current = outer.state;
        self.font = outer.font;
        self.shared = None;
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the values are stored and read back, never computed"
)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn restore_brings_back_the_saved_state_but_keeps_the_text_matrices() {
        let mut stack = GStateStack::default();
        stack.save();
        stack.edit().line_width = 4.0;
        stack.edit().text.tm = [1.0, 0.0, 0.0, 1.0, 50.0, 60.0];

        let restored = stack.restore();

        assert!(restored, "the `q` matches");
        assert_eq!(stack.current().line_width, 1.0);
        assert_eq!(stack.current().text.tm, [1.0, 0.0, 0.0, 1.0, 50.0, 60.0]);
        // smoke: the stack is empty again
        assert!(!stack.restore(), "a second `Q` has nothing to restore");
    }

    #[test]
    fn a_form_cannot_restore_the_invokers_saves_and_leaves_no_trace() {
        let mut stack = GStateStack::default();
        stack.save();
        stack.edit().line_width = 2.0;
        let outer = stack.enter_form();
        stack.edit().line_width = 9.0;
        stack.save();

        let first = stack.restore();
        let second = stack.restore();
        stack.leave_form(outer);

        assert_eq!((first, second), (true, false));
        assert_eq!(stack.current().line_width, 2.0);
        assert!(stack.restore(), "the invoker's own `q` is still there");
        assert_eq!(stack.current().line_width, 1.0);
    }

    #[test]
    fn snapshots_are_shared_until_the_state_changes() {
        let mut stack = GStateStack::default();

        let a = stack.snapshot();
        let b = stack.snapshot();
        stack.edit().miter_limit = 3.0;
        let c = stack.snapshot();

        assert!(Arc::ptr_eq(&a, &b), "unchanged state shares one allocation");
        assert!(!Arc::ptr_eq(&b, &c), "a change makes a new snapshot");
        assert_eq!(c.miter_limit, 3.0);
        // smoke: the ctm reads back as the identity matrix
        assert!(stack.ctm().is_identity());
    }
}
