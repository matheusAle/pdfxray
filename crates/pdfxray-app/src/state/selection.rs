//! What is selected, and the operations that change it.

use pdfxray_core::{NodeId, Point};

use super::AppState;
use super::drawer::{HitEntry, HitStack};
use crate::measure::Comparison;
use crate::selection::{SelectionRect, node_page, selection_rect};

/// The node every panel highlights and the optional second one to measure against.
#[derive(Debug, Default)]
pub struct SelectionState {
    /// The selected node.
    pub primary: Option<NodeId>,
    /// A second selected node (shift-click), shown in pink.
    pub secondary: Option<NodeId>,
    /// The node the inspector keeps showing while the selection moves on (the pin button).
    pub pinned: Option<NodeId>,
}

impl AppState {
    /// Selects `id` everywhere; the canvas shows the node's page and scrolls it into view.
    /// Any second selection is dropped.
    pub fn select(&mut self, id: NodeId) {
        let page = self
            .document
            .as_ref()
            .and_then(|loaded| node_page(&id).or_else(|| loaded.doc.bbox(&id).map(|(page, _)| page)));
        if let Some(page) = page {
            self.go_to_page(page);
        }
        self.history.visit(&id);
        self.selection.primary = Some(id);
        self.selection.secondary = None;
        self.canvas.reveal = true;
    }

    /// Shift-click: makes `id` the second selection, to measure against the first. Selecting the
    /// second node again removes it; without a first selection this is a plain [`Self::select`].
    pub fn select_additional(&mut self, id: NodeId) {
        match &self.selection.primary {
            None => self.select(id),
            Some(first) if *first == id => {}
            Some(_) if self.selection.secondary.as_ref() == Some(&id) => self.selection.secondary = None,
            Some(_) => self.selection.secondary = Some(id),
        }
    }

    /// Escape, third step: forgets the selection.
    pub fn clear_selection(&mut self) {
        self.selection.primary = None;
        self.selection.secondary = None;
    }

    /// Everything under `point` on the page shown, most specific first.
    pub(super) fn collect_hit_stack(&self, point: Point) -> Option<HitStack> {
        let doc = &self.document.as_ref()?.doc;
        let entries: Vec<HitEntry> = doc
            .hit(self.canvas.page, point, self.canvas.layer_mode)
            .into_iter()
            .map(|id| {
                let props = doc.node(&id);
                HitEntry {
                    label: props.as_ref().map_or_else(|| id.to_string(), |p| p.title.clone()),
                    kind: props.map_or_else(String::new, |p| p.kind.to_owned()),
                    id,
                }
            })
            .collect();
        Some(HitStack {
            page: self.canvas.page,
            point,
            entries,
        })
    }

    /// Reacts to a click on the canvas at `point` (page space): collects the hit stack and
    /// selects its first entry.
    pub fn click_page_point(&mut self, point: Point) {
        let Some(stack) = self.collect_hit_stack(point) else {
            return;
        };
        let first = stack.entries.first().map(|e| e.id.clone());
        self.drawer.show_hits(stack);
        if let Some(id) = first {
            self.select_on_canvas(id);
        }
    }

    /// Selects `id` because the user clicked it on the canvas: unlike [`Self::select`] the canvas
    /// does not scroll to it, it is already in view.
    pub fn select_on_canvas(&mut self, id: NodeId) {
        self.history.visit(&id);
        self.selection.primary = Some(id);
        self.selection.secondary = None;
    }

    /// The rectangle to outline on the canvas for the current selection, if it has one.
    pub fn selection_rect(&self) -> Option<SelectionRect> {
        let loaded = self.document.as_ref()?;
        selection_rect(&loaded.doc, self.selection.primary.as_ref()?)
    }

    /// The rectangle to outline for the second selection, if it has one.
    pub fn compare_rect(&self) -> Option<SelectionRect> {
        let loaded = self.document.as_ref()?;
        selection_rect(&loaded.doc, self.selection.secondary.as_ref()?)
    }

    /// Both selected nodes' boxes, when they are on the same page.
    pub fn comparison(&self) -> Option<Comparison> {
        let (first, second) = (self.selection_rect()?, self.compare_rect()?);
        (first.page == second.page).then_some(Comparison {
            first: first.rect,
            second: second.rect,
        })
    }
}
