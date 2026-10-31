//! The revision timeline: which objects each save of the file (the original and every
//! incremental update) defined, and where each object was last changed.

use pdfxray_raw::{EntryState, XrefKind as RawXrefKind};
use serde::Serialize;

use crate::{Document, ObjRef};

/// How a revision indexes its objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum XrefKind {
    /// A classic `xref` table.
    Table,
    /// A cross-reference stream object.
    Stream(ObjRef),
}

/// One revision of the file: the original save or an incremental update appended to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevisionInfo {
    /// Position in the file, 0 for the original.
    pub index: u32,
    /// Byte offset where the revision's xref section starts.
    pub xref_offset: usize,
    /// Table or stream.
    pub xref_kind: XrefKind,
    /// The objects the revision's xref section lists as in use (written or re-written here), in
    /// object order. Objects freed by the revision are not listed.
    pub objects_defined: Vec<ObjRef>,
    /// True for the newest revision.
    pub is_latest: bool,
}

impl Document {
    /// The revisions of the file, oldest first.
    pub fn revisions(&self) -> Vec<RevisionInfo> {
        let all = self.raw.revisions();
        all.iter()
            .map(|rev| {
                let mut objects: Vec<ObjRef> = rev
                    .xref
                    .entries
                    .iter()
                    .filter(|e| !matches!(e.state, EntryState::Free { .. }))
                    .map(|e| ObjRef::new(e.num, e.generation))
                    .collect();
                objects.sort_unstable();
                objects.dedup();
                RevisionInfo {
                    index: u32::try_from(rev.index).unwrap_or(u32::MAX),
                    xref_offset: rev.xref.span.start,
                    xref_kind: match rev.xref.kind {
                        RawXrefKind::Table => XrefKind::Table,
                        RawXrefKind::Stream(r) => XrefKind::Stream(r),
                    },
                    objects_defined: objects,
                    is_latest: rev.index + 1 == all.len(),
                }
            })
            .collect()
    }

    /// The revision (see [`RevisionInfo::index`]) holding the newest definition of `r`, the one
    /// the rest of the API shows. `None` when the object does not exist.
    pub fn object_last_changed(&self, r: ObjRef) -> Option<u32> {
        u32::try_from(self.objects.get(r)?.revision).ok()
    }

    /// True when `r` was defined in an earlier revision and defined again in a later one: its
    /// current value is an edit, not the original. An object that only exists from a later
    /// revision on (it was added, not changed) gives `false`; ask
    /// [`Document::object_last_changed`] for where it came from.
    pub fn changed_in_later_revision(&self, r: ObjRef) -> bool {
        let Some(latest) = self.objects.get(r).map(|o| o.revision) else {
            return false;
        };
        latest > 0 && self.raw.object_in_revision(r, latest - 1).is_some()
    }
}
