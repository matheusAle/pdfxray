//! Footnote references tied to their notes, by `/Ref` (PDF 2.0) or by label (PDF 1.x).

use super::super::{Kid, TagTree};
use super::{NoteFlag, NoteLinkSource, NoteRole, PAINTED_MAX, Speaker};
use crate::Document;

/// A footnote reference tied to its note, by element index.
struct NoteLink {
    reference: usize,
    note: usize,
    via: NoteLinkSource,
}

impl Speaker<'_> {
    /// Flags the first item of each footnote reference and of its note.
    pub(super) fn flag_notes(&mut self) {
        let mut spoken_before = Vec::with_capacity(self.items.len() + 1);
        let mut count = 0;
        for item in &self.items {
            spoken_before.push(count);
            count += usize::from(item.skipped.is_none());
        }
        spoken_before.push(count);
        for link in self.tree.note_links(self.doc) {
            let (Some(&r), Some(&n)) = (
                self.first_item.get(&link.reference),
                self.first_item.get(&link.note),
            ) else {
                continue;
            };
            let (lo, hi) = (r.min(n), r.max(n));
            let items_apart = spoken_before[hi] - spoken_before[lo + 1];
            let (ref_id, note_id) = (self.tree.id_of(link.reference), self.tree.id_of(link.note));
            self.items[r].note = Some(NoteFlag {
                role: NoteRole::Reference,
                other: note_id,
                via: link.via,
                items_apart,
            });
            self.items[n].note = Some(NoteFlag {
                role: NoteRole::Note,
                other: ref_id,
                via: link.via,
                items_apart,
            });
        }
    }
}

/// Adds a link unless the pair is already listed.
fn add_link(out: &mut Vec<NoteLink>, reference: usize, note: usize, via: NoteLinkSource) {
    if !out.iter().any(|l| l.reference == reference && l.note == note) {
        out.push(NoteLink { reference, note, via });
    }
}

impl TagTree {
    fn is_note(&self, ei: usize) -> bool {
        matches!(
            self.elems[ei].resolved.standard.as_deref(),
            Some("Note" | "FENote")
        )
    }

    /// The text of the first `Lbl` at or below `ei`.
    fn label_of(&self, doc: &Document, ei: usize) -> Option<String> {
        if self.elems[ei].resolved.standard.as_deref() == Some("Lbl") {
            let mut label = String::new();
            self.collect_text(doc, ei, &mut label, PAINTED_MAX);
            return Some(label);
        }
        self.elems[ei].kids.iter().find_map(|k| match k {
            Kid::Elem(c) => self.label_of(doc, *c),
            _ => None,
        })
    }

    /// Footnote references tied to their notes: by `/Ref` (PDF 2.0), else by a reference whose
    /// text equals the `Lbl` of the next note after it (PDF 1.x).
    fn note_links(&self, doc: &Document) -> Vec<NoteLink> {
        let mut out: Vec<NoteLink> = Vec::new();
        for (ei, e) in self.elems.iter().enumerate() {
            for target in e.refs.iter().filter_map(|r| self.index.get(r)).copied() {
                match (self.is_note(ei), self.is_note(target)) {
                    (false, true) => add_link(&mut out, ei, target, NoteLinkSource::Ref),
                    (true, false) => add_link(&mut out, target, ei, NoteLinkSource::Ref),
                    _ => {}
                }
            }
        }
        let notes: Vec<usize> = (0..self.elems.len()).filter(|&ei| self.is_note(ei)).collect();
        for ei in 0..self.elems.len() {
            if self.elems[ei].resolved.standard.as_deref() != Some("Reference")
                || out.iter().any(|l| l.reference == ei)
            {
                continue;
            }
            let mut label = String::new();
            self.collect_text(doc, ei, &mut label, PAINTED_MAX);
            if label.is_empty() {
                continue;
            }
            let note = notes
                .iter()
                .copied()
                .filter(|&n| n > ei)
                .find(|&n| self.label_of(doc, n).as_deref() == Some(label.as_str()));
            if let Some(note) = note {
                add_link(&mut out, ei, note, NoteLinkSource::LabelMatch);
            }
        }
        out
    }
}
