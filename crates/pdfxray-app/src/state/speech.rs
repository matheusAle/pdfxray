//! Speech: what a screen reader announces, for the preset the Speech tab shows.
//!
//! [`SpeechState`] is the tab's settings. [`SpeechData`] is the document's announcements under one
//! preset with the lookups the panels need, read once per preset and shared by the Speech tab,
//! the reading order drawn from speech and the footnote connectors of the canvas.

use std::collections::HashMap;
use std::sync::Arc;

use pdfxray_core::{NodeId, NoteRole, PageIx, ReadingOrder, SpeechItem, SpeechOrder, SpeechPreset};

use super::AppState;
use crate::modes::OrderSource;

/// Settings of the Speech tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechState {
    /// The reader the transcript, the reading order from speech and the footnote connectors
    /// model.
    pub preset: SpeechPreset,
    /// The transcript also lists what is not announced, dimmed, with the reason.
    pub show_skipped: bool,
}

impl Default for SpeechState {
    fn default() -> Self {
        Self {
            preset: SpeechPreset::Structure,
            show_skipped: false,
        }
    }
}

/// A footnote reference and the note it points to, for the connector the canvas draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteLink {
    /// The reference element.
    pub reference: NodeId,
    /// The page the reference is announced on.
    pub reference_page: Option<PageIx>,
    /// The note element.
    pub note: NodeId,
    /// The element the note's announcement comes from, when it is not the note itself: its label.
    pub note_item: Option<NodeId>,
    /// The page the note is announced on.
    pub note_page: Option<PageIx>,
    /// Spoken items between the two.
    pub items_apart: usize,
}

impl NoteLink {
    /// True when `node` is the reference, the note or the element the note is announced from.
    pub fn involves(&self, node: &NodeId) -> bool {
        self.reference == *node || self.note == *node || self.note_item.as_ref() == Some(node)
    }
}

/// The announcements of the document under one preset, indexed.
#[derive(Debug)]
pub struct SpeechData {
    /// What core announces.
    pub order: SpeechOrder,
    /// Items that are announced.
    pub spoken: usize,
    /// Items that are not.
    pub skipped: usize,
    /// Every footnote reference with its note.
    pub notes: Vec<NoteLink>,
    /// The first item of each node, for the lookups of covering elements and header cells.
    by_node: HashMap<NodeId, usize>,
}

impl SpeechData {
    /// Indexes the announcements `order`.
    pub fn new(order: SpeechOrder) -> Self {
        let mut by_node = HashMap::new();
        for (at, item) in order.items.iter().enumerate() {
            by_node.entry(item.node.clone()).or_insert(at);
        }
        let spoken = order.spoken().count();
        // A note is announced from its first item (its label), which points back at the reference.
        let note_items: HashMap<&NodeId, &SpeechItem> = order
            .items
            .iter()
            .filter_map(|item| {
                let flag = item.note.as_ref().filter(|flag| flag.role == NoteRole::Note)?;
                Some((&flag.other, item))
            })
            .collect();
        let notes = order
            .items
            .iter()
            .filter_map(|item| {
                let flag = item
                    .note
                    .as_ref()
                    .filter(|flag| flag.role == NoteRole::Reference)?;
                let note = note_items
                    .get(&item.node)
                    .copied()
                    .or_else(|| by_node.get(&flag.other).map(|&at| &order.items[at]));
                Some(NoteLink {
                    reference: item.node.clone(),
                    reference_page: item.page,
                    note: flag.other.clone(),
                    note_item: note
                        .map(|note| note.node.clone())
                        .filter(|node| *node != flag.other),
                    note_page: note.and_then(|note| note.page),
                    items_apart: flag.items_apart,
                })
            })
            .collect();
        Self {
            skipped: order.items.len() - spoken,
            spoken,
            notes,
            by_node,
            order,
        }
    }

    /// The first item that stands for `node`.
    pub fn item_of(&self, node: &NodeId) -> Option<&SpeechItem> {
        self.by_node.get(node).map(|&at| &self.order.items[at])
    }
}

impl AppState {
    /// What the open document announces under `preset`, read the first time it is asked for;
    /// `None` without a document.
    pub fn speech(&mut self, preset: SpeechPreset) -> Option<Arc<SpeechData>> {
        let doc = &self.document.as_ref()?.doc;
        Some(self.derived.speech(doc, preset))
    }

    /// The announcements under `preset` when they were read already: what a badge can show
    /// without paying for them.
    pub fn speech_if_read(&self, preset: SpeechPreset) -> Option<&Arc<SpeechData>> {
        self.derived.speech_cached(preset)
    }

    /// The reading order of `page` as the View menu's "Reading order from" says; `None` without a
    /// document. Speech is read as the Speech tab's preset announces it.
    pub fn shown_order(&mut self, page: PageIx, source: OrderSource) -> Option<Arc<ReadingOrder>> {
        match source {
            OrderSource::Structure => self.reading_order(page, pdfxray_core::ReadingOrderSource::Structure),
            OrderSource::Paint => self.reading_order(page, pdfxray_core::ReadingOrderSource::Paint),
            OrderSource::Speech => {
                let preset = self.drawer.speech.preset;
                let doc = &self.document.as_ref()?.doc;
                Some(self.derived.speech_page(doc, page, preset))
            }
        }
    }

    /// Selects the node of a transcript row and takes the canvas to it. A node with no box of its
    /// own (an annotation) still takes it to the page it is announced on.
    pub fn select_speech_item(&mut self, item: &SpeechItem) {
        self.select(item.node.clone());
        if self.selection_rect().is_none()
            && let Some(page) = item.page
        {
            self.go_to_page(page);
        }
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{ObjRef, SpeechPreset};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    fn data(name: &str, preset: SpeechPreset) -> SpeechData {
        let doc = fixture(name);
        SpeechData::new(doc.speech_order(preset))
    }

    fn elem(n: u32) -> NodeId {
        NodeId::StructElem(ObjRef::new(n, 0))
    }

    #[test]
    fn a_reference_is_tied_to_its_note_and_the_label_the_note_is_announced_from() {
        let notes = data("speech-footnote", SpeechPreset::Structure);

        assert_eq!(
            notes.notes,
            [NoteLink {
                reference: elem(9),
                reference_page: Some(0),
                note: elem(11),
                note_item: Some(elem(12)),
                note_page: Some(0),
                items_apart: 1,
            }]
        );
        let link = &notes.notes[0];
        assert_eq!(
            [elem(9), elem(11), elem(12), elem(10)].map(|node| link.involves(&node)),
            [true, true, true, false]
        );
        // Smoke: Chrome flags no notes.
        assert_eq!(data("speech-footnote", SpeechPreset::ChromiumDefault).notes, []);
    }

    #[test]
    fn the_counts_split_spoken_from_skipped_and_nodes_are_found_by_their_first_item() {
        let hidden = data("speech-alt-hides", SpeechPreset::Structure);

        assert_eq!((hidden.spoken, hidden.skipped), (5, 2));
        assert_eq!(hidden.item_of(&elem(9)).map(|item| item.index), Some(2));
        assert_eq!(hidden.item_of(&elem(99)), None);
    }

    #[test]
    fn the_default_is_the_spec_reader_without_the_skipped() {
        assert_eq!(
            SpeechState::default(),
            SpeechState {
                preset: SpeechPreset::Structure,
                show_skipped: false
            }
        );
    }
}
