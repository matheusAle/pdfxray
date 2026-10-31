//! Form `XObject` scopes: every drawn form invocation of the page, the ones being walked now,
//! and the guard against a form that invokes itself.

use std::ops::Range;

use pdfxray_raw::ObjRef;

use super::types::{StreamRef, XObjectScope};
use crate::numeric::usize_from_u32;
use crate::{Matrix, Rect};

/// What is known about a form when its walk starts.
pub(super) struct ScopeStart {
    /// Resource name used by `Do`.
    pub name: String,
    /// Object reference of the form.
    pub obj: Option<ObjRef>,
    /// [`OpRecord::ix`](super::OpRecord::ix) of the invoking `Do`.
    pub op: u32,
    /// The form's `/Matrix`.
    pub matrix: Matrix,
    /// The form's `/BBox` as written.
    pub bbox: Option<[f64; 4]>,
    /// CTM inside the form.
    pub ctm: Matrix,
    /// `/BBox` in page space.
    pub page_bbox: Option<Rect>,
    /// The stream holding the form's operators.
    pub stream: StreamRef,
}

/// The scopes seen so far and the stack of forms being walked.
#[derive(Default)]
pub(super) struct XObjectScopes {
    scopes: Vec<XObjectScope>,
    /// Forms being walked, outermost first, as indices into `scopes`.
    open: Vec<u32>,
    /// Object references of the forms being walked, to stop recursion.
    active: Vec<ObjRef>,
}

impl XObjectScopes {
    /// Forms being walked, outermost first.
    pub(super) fn open(&self) -> &[u32] {
        &self.open
    }

    /// The form being walked now.
    pub(super) fn innermost(&self) -> Option<u32> {
        self.open.last().copied()
    }

    /// True while `obj` is being walked: invoking it again would recurse.
    pub(super) fn is_active(&self, obj: ObjRef) -> bool {
        self.active.contains(&obj)
    }

    /// Opens the scope of a form whose walk starts now; returns its index.
    pub(super) fn enter(&mut self, start: ScopeStart) -> u32 {
        let ix = u32::try_from(self.scopes.len()).unwrap_or(u32::MAX);
        if let Some(obj) = start.obj {
            self.active.push(obj);
        }
        self.scopes.push(XObjectScope {
            ix,
            name: start.name,
            obj: start.obj,
            parent: self.innermost(),
            op: start.op,
            matrix: start.matrix.to_array(),
            bbox: start.bbox,
            ctm: start.ctm.to_array(),
            page_bbox: start.page_bbox,
            stream: start.stream,
            first_op: None,
            last_op: None,
        });
        self.open.push(ix);
        ix
    }

    /// Closes scope `ix`, whose walk executed the operators `ops` (page-wide indices).
    pub(super) fn leave(&mut self, ix: u32, ops: Range<usize>) {
        let scope = self.scopes.get_mut(usize_from_u32(ix));
        if let Some(scope) = scope {
            if !ops.is_empty() {
                scope.first_op = u32::try_from(ops.start).ok();
                scope.last_op = u32::try_from(ops.end - 1).ok();
            }
            if scope.obj.is_some() {
                self.active.pop();
            }
        }
        self.open.pop();
    }

    /// Every drawn form invocation, in execution order.
    pub(super) fn into_scopes(self) -> Vec<XObjectScope> {
        self.scopes
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the matrices are stored and read back, never computed"
)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn start(name: &str, obj: Option<ObjRef>, op: u32) -> ScopeStart {
        ScopeStart {
            name: name.to_owned(),
            obj,
            op,
            matrix: Matrix::IDENTITY,
            bbox: Some([0.0, 0.0, 10.0, 10.0]),
            ctm: Matrix::from([2.0, 0.0, 0.0, 2.0, 5.0, 5.0]),
            page_bbox: Some(Rect::new(5.0, 5.0, 25.0, 25.0)),
            stream: StreamRef::PageContent { index: 0 },
        }
    }

    #[test]
    fn nested_scopes_know_their_parent_and_operator_range() {
        let (a_ref, b_ref) = (ObjRef::new(6, 0), ObjRef::new(7, 0));
        let mut scopes = XObjectScopes::default();

        let a = scopes.enter(start("FmA", Some(a_ref), 3));
        let b = scopes.enter(start("FmB", Some(b_ref), 5));
        let inside = (
            scopes.open().to_vec(),
            scopes.is_active(a_ref),
            scopes.is_active(b_ref),
        );
        scopes.leave(b, 6..9);
        scopes.leave(a, 4..9);
        let after = (scopes.innermost(), scopes.is_active(a_ref));
        let all = scopes.into_scopes();

        assert_eq!(inside, (vec![0, 1], true, true));
        assert_eq!(after, (None, false));
        assert_eq!(
            all.iter()
                .map(|s| (s.name.as_str(), s.parent, s.op, s.first_op, s.last_op))
                .collect::<Vec<_>>(),
            [
                ("FmA", None, 3, Some(4), Some(8)),
                ("FmB", Some(0), 5, Some(6), Some(8)),
            ]
        );
        // smoke: the matrices are stored as the six numbers of the data model
        assert_eq!(all[1].ctm, [2.0, 0.0, 0.0, 2.0, 5.0, 5.0]);
    }

    #[test]
    fn an_empty_form_has_no_operator_range_and_a_direct_form_no_guard() {
        let mut scopes = XObjectScopes::default();
        let outer = scopes.enter(start("Fm0", Some(ObjRef::new(9, 0)), 1));

        let direct = scopes.enter(start("Inline", None, 2));
        scopes.leave(direct, 3..3);
        let still_guarded = scopes.is_active(ObjRef::new(9, 0));
        scopes.leave(outer, 2..3);

        assert!(still_guarded, "leaving a direct form keeps the outer guard");
        let all = scopes.into_scopes();
        assert_eq!((all[1].first_op, all[1].last_op), (None, None));
        // smoke: the outer scope covers its one operator
        assert_eq!((all[0].first_op, all[0].last_op), (Some(2), Some(2)));
    }
}
