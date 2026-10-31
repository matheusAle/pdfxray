//! Footnote connectors: a dashed line from a footnote reference to its note, labelled with how
//! far the listener travels between the two (`3 apart`: spoken items in between).
//!
//! They show while the reading order or the Speech tab is shown (every link of the document that
//! touches the page), and for a reference or note that is selected. When the note is on another
//! page the line is drawn on both pages: it leaves the reference towards the page's right edge
//! with an arrow and `p2 · 3 apart`, and arrives at the note from the edge with `from p1 · 3 apart`.
//! A label is a button that goes to the other end.
//!
//! The links are those of the reader the Speech tab models.

use std::collections::HashMap;

use egui::{Rect as UiRect, Ui};
use pdfxray_core::{NodeId, PageIx, Rect};

use super::connector::{self, LineStyle};
use super::geometry::PageGeometry;
use crate::components::tooltip;
use crate::state::{AppState, DrawerTab, NoteLink};
use crate::theme::tokens;

/// Standard types whose selection brings the connector of their link.
const NOTE_TYPES: [&str; 4] = ["Reference", "Note", "FENote", "Lbl"];

/// How far inside the page edge a connector to another page ends.
const MARGIN: f32 = 10.0;

/// The label of a connector on the page `here`: `3 apart` when both ends are on it, `p2 · 3
/// apart` where it leaves for the page of the note, `from p1 · 3 apart` where it arrives from the
/// page of the reference.
pub fn label_text(items_apart: usize, side: Side) -> String {
    match side {
        Side::Both => format!("{items_apart} apart"),
        Side::Leaves { to } => format!("p{} · {items_apart} apart", to + 1),
        Side::Arrives { from } => format!("from p{} · {items_apart} apart", from + 1),
    }
}

/// The accessible name of a connector label.
pub fn label_name(items_apart: usize, side: Side) -> String {
    format!("Footnote link: {}", label_text(items_apart, side))
}

/// Which ends of a link are on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The reference and the note.
    Both,
    /// The reference; the note is on page `to`.
    Leaves {
        /// The page of the note, from 0.
        to: PageIx,
    },
    /// The note; the reference is on page `from`.
    Arrives {
        /// The page of the reference, from 0.
        from: PageIx,
    },
}

/// A link as it shows on one page.
#[derive(Debug, Clone)]
struct PageLink {
    reference: Option<Rect>,
    note: Option<Rect>,
    side: Side,
    items_apart: usize,
    /// What a click on the label selects: the end that is not selected.
    jump_to: NodeId,
}

/// What the overlay draws on one page.
#[derive(Debug, Default)]
pub(super) struct NoteLayer {
    links: Vec<PageLink>,
}

/// True when selecting `id` should bring the connector of its link: it is a reference or a note.
fn note_related(state: &mut AppState, id: &NodeId) -> bool {
    if let Some(data) = state.speech_if_read(state.drawer.speech.preset) {
        return data.notes.iter().any(|link| link.involves(id));
    }
    // Not read yet: only what looks like a footnote part is worth reading the document for.
    let Some(page) = state.selection_rect().map(|s| s.page) else {
        return false;
    };
    state
        .tag_boxes(page)
        .iter()
        .any(|tag| tag.id == *id && NOTE_TYPES.contains(&tag.standard.as_str()))
}

/// Collects the links to draw on `page`.
pub(super) fn collect(state: &mut AppState, page: PageIx) -> NoteLayer {
    let on =
        state.canvas.view.show.reading_order || (state.drawer.open && state.drawer.tab == DrawerTab::Speech);
    let selection = state.selection.primary.clone();
    let selected_note = !on && selection.as_ref().is_some_and(|id| note_related(state, id));
    if !on && !selected_note {
        return NoteLayer::default();
    }
    let Some(data) = state.speech(state.drawer.speech.preset) else {
        return NoteLayer::default();
    };
    let wanted: Vec<&NoteLink> = data
        .notes
        .iter()
        .filter(|link| on || selection.as_ref().is_some_and(|id| link.involves(id)))
        .collect();
    if wanted.is_empty() {
        return NoteLayer::default();
    }
    let boxes = state.tag_boxes(page);
    let by_node: HashMap<&NodeId, Rect> = boxes.iter().map(|tag| (&tag.id, tag.rect)).collect();
    let links = wanted
        .into_iter()
        .filter_map(|link| {
            let reference = by_node.get(&link.reference).copied();
            let note = by_node
                .get(&link.note)
                .or_else(|| link.note_item.as_ref().and_then(|item| by_node.get(item)))
                .copied();
            let side = match (reference, note) {
                (Some(_), Some(_)) => Side::Both,
                (Some(_), None) => Side::Leaves { to: link.note_page? },
                (None, Some(_)) => Side::Arrives {
                    from: link.reference_page?,
                },
                (None, None) => return None,
            };
            // A click goes to the end the selection is not at.
            let at_note = selection
                .as_ref()
                .is_some_and(|id| link.note == *id || link.note_item.as_ref() == Some(id));
            let jump_to = if at_note { &link.reference } else { &link.note };
            Some(PageLink {
                reference,
                note,
                side,
                items_apart: link.items_apart,
                jump_to: jump_to.clone(),
            })
        })
        .collect();
    NoteLayer { links }
}

/// What the user did on the overlay this frame.
#[derive(Debug, Default)]
pub(super) struct Output {
    /// The other end of the link whose label was clicked.
    pub(super) jump: Option<NodeId>,
}

/// Draws the connectors over the page at `geo`.
pub(super) fn draw(ui: &Ui, geo: &PageGeometry, page: PageIx, layer: &NoteLayer) -> Output {
    let mut out = Output::default();
    if layer.links.is_empty() {
        return out;
    }
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    let color = tokens::KIND_TAG;
    let mut placed: Vec<UiRect> = Vec::new();
    for (n, link) in layer.links.iter().enumerate() {
        let reference = link.reference.map(|rect| geo.rect_to_screen(rect));
        let note = link.note.map(|rect| geo.rect_to_screen(rect));
        let (start, end) = match (reference, note) {
            (Some(reference), Some(note)) => (
                connector::edge_toward(reference, note.center()),
                connector::edge_toward(note, reference.center()),
            ),
            (Some(reference), None) => {
                let edge = connector::margin_point(geo.rect, reference.center().y, MARGIN);
                (connector::edge_toward(reference, edge), edge)
            }
            (None, Some(note)) => {
                let edge = connector::margin_point(geo.rect, note.center().y, MARGIN);
                (edge, connector::edge_toward(note, edge))
            }
            (None, None) => continue,
        };
        connector::line(&painter, start, end, color, LineStyle::Dashed);
        connector::anchor(&painter, start, color);
        connector::arrow_head(&painter, end, start, color);
        let (text, name) = (
            label_text(link.items_apart, link.side),
            label_name(link.items_apart, link.side),
        );
        let response = connector::label(
            ui,
            &painter,
            &mut placed,
            start.lerp(end, 0.5),
            &connector::Label {
                text: &text,
                ink: color,
                id: ui.make_persistent_id(("footnote-connector", page, n)),
                name: &name,
            },
        );
        let response = tooltip::text(response, "Click to go to the other end of the footnote link");
        if response.clicked() {
            out.jump = Some(link.jump_to.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_label_says_how_far_apart_and_which_page_the_other_end_is_on() {
        assert_eq!(label_text(3, Side::Both), "3 apart");
        assert_eq!(label_text(37, Side::Leaves { to: 1 }), "p2 · 37 apart");
        assert_eq!(label_text(37, Side::Arrives { from: 0 }), "from p1 · 37 apart");
        assert_eq!(label_name(1, Side::Both), "Footnote link: 1 apart");
    }
}
