//! What is selected, and the operations that change it.
//!
//! The selection is an ordered set of nodes with a *primary*: the node every single-node view
//! (inspector, breadcrumb, overlays, references, stepper, clip) shows. One node is the common
//! case; two are the compare view (the gap is measured from the primary); three or more are a
//! multi-selection the inspector summarises. See `docs/spec/multi-selection.md`.

use pdfxray_core::diff::Side;
use pdfxray_core::{NodeId, Point, Rect};

use super::AppState;
use super::canvas::Reveal;
use super::drawer::{HitEntry, HitStack};
use crate::measure::Comparison;
use crate::selection::{SelectionRect, node_page, role_label, selection_rect};

/// From this many selected nodes on, the inspector shows the summary instead of the primary (two
/// are the compare view).
pub const SUMMARY_FROM: usize = 3;

/// A rectangle around everything a page can hold, for "select all on the page".
const WHOLE_PAGE: Rect = Rect {
    x0: -1.0e9,
    y0: -1.0e9,
    x1: 1.0e9,
    y1: 1.0e9,
};

/// How a set of nodes (a marquee's catch, ⌘A) changes the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combine {
    /// The nodes become the selection.
    Replace,
    /// The nodes join the selection (Shift).
    Add,
    /// The nodes leave the selection (⌥).
    Subtract,
}

/// The selected nodes, in the order they joined, and which one is the primary.
#[derive(Debug)]
pub struct SelectionState {
    /// Never twice the same node.
    nodes: Vec<NodeId>,
    /// Index of the primary in `nodes`; meaningless while `nodes` is empty.
    primary: usize,
    /// The inspector shows the primary alone although three or more are selected: the user
    /// stepped into one of them from the summary.
    stepped_in: bool,
    /// The node the inspector keeps showing while the selection moves on (the pin button).
    pub pinned: Option<NodeId>,
    /// The document the nodes are of: A (the open document) but for a node of B picked in a
    /// comparison. A [`NodeId`] has no side of its own.
    side: Side,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            primary: 0,
            stepped_in: false,
            pinned: None,
            side: Side::A,
        }
    }
}

impl SelectionState {
    /// The primary node: the one every single-node view shows.
    pub fn primary(&self) -> Option<&NodeId> {
        self.nodes.get(self.primary)
    }

    /// Every selected node, in the order they joined.
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Number of selected nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// True when `id` is selected.
    pub fn contains(&self, id: &NodeId) -> bool {
        self.nodes.contains(id)
    }

    /// With exactly two selected, the one that is not the primary: what the compare view
    /// measures to.
    pub fn other(&self) -> Option<&NodeId> {
        (self.nodes.len() == 2).then(|| &self.nodes[1 - self.primary.min(1)])
    }

    /// Where the primary is among the selected nodes, from 0.
    pub fn primary_index(&self) -> Option<usize> {
        (!self.nodes.is_empty()).then_some(self.primary)
    }

    /// True when the inspector shows the summary of a multi-selection.
    pub fn shows_summary(&self) -> bool {
        self.nodes.len() >= SUMMARY_FROM && !self.stepped_in
    }

    /// True when the user stepped into one node of a multi-selection: the inspector shows it with
    /// a way back to the summary.
    pub fn stepped_in(&self) -> bool {
        self.nodes.len() >= SUMMARY_FROM && self.stepped_in
    }

    /// The document the selected nodes are of.
    pub fn side(&self) -> Side {
        self.side
    }

    /// `id` alone, a node of A.
    fn set_one(&mut self, id: NodeId) {
        self.set_one_on(Side::A, id);
    }

    /// `id` alone, a node of `side`.
    pub(super) fn set_one_on(&mut self, side: Side, id: NodeId) {
        self.nodes = vec![id];
        self.primary = 0;
        self.stepped_in = false;
        self.side = side;
    }

    /// Adds `id` at the end, leaving the primary where it is. False when it was selected.
    fn insert(&mut self, id: NodeId) -> bool {
        if self.side != Side::A {
            // A multi-selection is of A's nodes: a node of A joining one of B's starts over.
            self.set_one(id);
            return true;
        }
        if self.contains(&id) {
            return false;
        }
        self.nodes.push(id);
        true
    }

    /// Removes `id`. The node after the primary takes its place when the primary goes (the one
    /// before when it was last).
    fn remove(&mut self, id: &NodeId) -> bool {
        let Some(at) = self.nodes.iter().position(|n| n == id) else {
            return false;
        };
        self.nodes.remove(at);
        if at < self.primary || self.primary >= self.nodes.len() {
            self.primary = self.primary.saturating_sub(1);
        }
        true
    }

    /// Makes `id` the primary, when it is selected.
    fn set_primary(&mut self, id: &NodeId) -> bool {
        match self.nodes.iter().position(|n| n == id) {
            Some(at) => {
                self.primary = at;
                true
            }
            None => false,
        }
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.primary = 0;
        self.stepped_in = false;
        self.side = Side::A;
    }
}

/// Everything under `point` on page `page` of `doc` that `layer` picks, most specific first, as
/// the hit stack lists it.
pub(crate) fn hit_entries(
    doc: &pdfxray_core::Document,
    page: pdfxray_core::PageIx,
    point: Point,
    layer: pdfxray_core::LayerMode,
) -> Vec<HitEntry> {
    doc.hit(page, point, layer)
        .into_iter()
        .map(|id| {
            let props = doc.node(&id);
            HitEntry {
                label: props.as_ref().map_or_else(|| id.to_string(), |p| p.title.clone()),
                kind: props.map_or_else(String::new, |p| p.kind.to_owned()),
                id,
            }
        })
        .collect()
}

impl AppState {
    /// Selects `id` alone everywhere; the canvas shows the node's page and scrolls it into view.
    pub fn select(&mut self, id: NodeId) {
        let page = self
            .document
            .as_ref()
            .and_then(|loaded| node_page(&id).or_else(|| loaded.doc.bbox(&id).map(|(page, _)| page)));
        if let Some(page) = page {
            self.go_to_page(page);
        }
        self.select_on_canvas(id);
        self.canvas.reveal = Some(Reveal::Centre);
    }

    /// Selects `id` alone without moving the canvas, unlike [`Self::select`]: the user clicked it
    /// on the canvas, so it is in view, or the stepper moved to it and says itself whether to
    /// scroll.
    pub fn select_on_canvas(&mut self, id: NodeId) {
        self.history.visit(&id);
        self.selection.set_one(id);
    }

    /// Shift-click: adds `id` to the selection, or takes it out when it is already in. The primary
    /// stays where it is, so the first two make the compare view with the gap measured from the
    /// first; Shift-clicking the primary does nothing. Without a selection this is a plain
    /// [`Self::select`].
    pub fn select_additional(&mut self, id: NodeId) {
        if self.selection.is_empty() {
            self.select(id);
        } else if self.selection.primary() != Some(&id) && !self.selection.remove(&id) {
            self.selection.insert(id);
        }
    }

    /// Shift+↑/↓ in a tree: `to`, the next row, joins the selection and becomes the primary. When
    /// it is already selected, the row being left goes out instead, so going back shrinks the run.
    pub fn extend_selection(&mut self, to: NodeId) {
        let Some(from) = self.selection.primary().cloned() else {
            self.select(to);
            return;
        };
        if self.selection.contains(&to) {
            self.selection.remove(&from);
        } else {
            self.selection.insert(to.clone());
        }
        self.selection.set_primary(&to);
        self.visit_primary();
        self.canvas.reveal = Some(Reveal::IntoView);
    }

    /// ⌘+Space in a tree: `id` leaves the selection when it is in, else joins it (the primary stays).
    pub fn toggle_selected(&mut self, id: NodeId) {
        if self.selection.contains(&id) {
            self.deselect(&id);
        } else if self.selection.is_empty() {
            self.select(id);
        } else {
            self.selection.insert(id);
        }
    }

    /// ⌥-click: takes `id` out of the selection. The selection may end up empty.
    pub fn deselect(&mut self, id: &NodeId) {
        let was_primary = self.selection.primary() == Some(id);
        if self.selection.remove(id) && was_primary {
            self.visit_primary();
        }
    }

    /// Applies a set of nodes (a marquee's catch, in reading order) to the selection. Replacing
    /// makes the first node the primary; adding keeps the primary (or makes the first new node
    /// the primary when nothing was selected); subtracting removes them.
    pub fn select_nodes(&mut self, nodes: Vec<NodeId>, how: Combine) {
        match how {
            Combine::Replace => {
                self.selection.clear();
                for node in nodes {
                    self.selection.insert(node);
                }
                self.visit_primary();
            }
            Combine::Add => {
                let was_empty = self.selection.is_empty();
                for node in nodes {
                    self.selection.insert(node);
                }
                if was_empty {
                    self.visit_primary();
                }
            }
            Combine::Subtract => {
                for node in &nodes {
                    self.deselect(node);
                }
            }
        }
    }

    /// ⌘A: selects everything of the Pick target on the current page, in reading order.
    pub fn select_all(&mut self) {
        let Some(loaded) = &self.document else {
            return;
        };
        let nodes = loaded
            .doc
            .hit_rect(self.canvas.page, WHOLE_PAGE, self.canvas.layer_mode);
        if !nodes.is_empty() {
            self.select_nodes(nodes, Combine::Replace);
        }
    }

    /// Esc on a multi-selection: keeps the primary alone. False when there was nothing to drop.
    pub fn keep_primary(&mut self) -> bool {
        let Some(primary) = self
            .selection
            .primary()
            .cloned()
            .filter(|_| self.selection.len() > 1)
        else {
            return false;
        };
        self.selection.set_one(primary);
        true
    }

    /// Escape, last step: forgets the selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Opens one node of a multi-selection in the inspector: it becomes the primary, the others
    /// stay selected.
    pub fn step_into(&mut self, id: &NodeId) {
        if self.selection.set_primary(id) {
            self.selection.stepped_in = true;
            self.visit_primary();
            self.canvas.reveal = Some(Reveal::IntoView);
        }
    }

    /// Back from one node to the summary of the multi-selection.
    pub fn step_out(&mut self) {
        self.selection.stepped_in = false;
    }

    /// `[` and `]` while stepped in: the previous or next selected node becomes the primary,
    /// wrapping at either end.
    pub fn step_selection(&mut self, forward: bool) {
        let len = self.selection.len();
        let Some(at) = self
            .selection
            .primary_index()
            .filter(|_| self.selection.stepped_in())
        else {
            return;
        };
        let next = if forward {
            (at + 1) % len
        } else {
            (at + len - 1) % len
        };
        if let Some(id) = self.selection.nodes().get(next).cloned() {
            self.step_into(&id);
        }
    }

    /// "Only P": keeps the selected nodes whose role reads `role`; the first of them becomes the
    /// primary.
    pub fn narrow_selection(&mut self, role: &str) {
        let Some(loaded) = &self.document else {
            return;
        };
        let keep: Vec<NodeId> = self
            .selection
            .nodes()
            .iter()
            .filter(|id| role_label(&loaded.doc, id) == role)
            .cloned()
            .collect();
        if !keep.is_empty() {
            self.select_nodes(keep, Combine::Replace);
        }
    }

    /// Records the primary in the object history after it changed.
    fn visit_primary(&mut self) {
        if let Some(id) = self.selection.primary().cloned() {
            self.history.visit(&id);
        }
    }

    /// Everything under `point` on the page shown, most specific first.
    pub(super) fn collect_hit_stack(&self, point: Point) -> Option<HitStack> {
        let doc = &self.document.as_ref()?.doc;
        Some(HitStack {
            side: Side::A,
            page: self.canvas.page,
            point,
            entries: hit_entries(doc, self.canvas.page, point, self.canvas.layer_mode),
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

    /// The rectangle to outline on the canvas for the primary, if it has one.
    pub fn selection_rect(&self) -> Option<SelectionRect> {
        let loaded = self.document.as_ref()?;
        selection_rect(&loaded.doc, self.selection.primary()?)
    }

    /// The rectangles of every selected node that has one, with its node, in selection order.
    pub fn selection_rects(&self) -> Vec<(NodeId, SelectionRect)> {
        let Some(loaded) = self.document.as_ref() else {
            return Vec::new();
        };
        self.selection
            .nodes()
            .iter()
            .filter_map(|id| Some((id.clone(), selection_rect(&loaded.doc, id)?)))
            .collect()
    }

    /// The rectangle to outline for the other node of a two-node selection, if it has one.
    pub fn compare_rect(&self) -> Option<SelectionRect> {
        let loaded = self.document.as_ref()?;
        selection_rect(&loaded.doc, self.selection.other()?)
    }

    /// Both nodes' boxes of a two-node selection, the primary first, when they are on the same
    /// page.
    pub fn comparison(&self) -> Option<Comparison> {
        let (first, second) = (self.selection_rect()?, self.compare_rect()?);
        (first.page == second.page).then_some(Comparison {
            first: first.rect,
            second: second.rect,
        })
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::ObjRef;
    use pretty_assertions::assert_eq;

    use super::*;

    fn elem(n: u32) -> NodeId {
        NodeId::StructElem(ObjRef::new(n, 0))
    }

    fn state_with(nodes: &[u32], primary: usize) -> SelectionState {
        SelectionState {
            nodes: nodes.iter().map(|n| elem(*n)).collect(),
            primary,
            ..SelectionState::default()
        }
    }

    #[test]
    fn the_set_keeps_its_order_and_never_holds_a_node_twice() {
        let mut selection = SelectionState::default();
        selection.set_one(elem(1));

        assert!(selection.insert(elem(2)));
        assert!(!selection.insert(elem(1)));
        assert!(selection.insert(elem(3)));

        assert_eq!(selection.nodes(), [elem(1), elem(2), elem(3)]);
        assert_eq!(selection.primary(), Some(&elem(1)));
        // Smoke: three nodes are summarised, not compared.
        assert!(selection.shows_summary() && selection.other().is_none());
    }

    #[test]
    fn removing_the_primary_hands_it_to_the_next_node_or_the_last() {
        let mut selection = state_with(&[1, 2, 3], 1);
        assert!(selection.remove(&elem(2)));
        assert_eq!(selection.primary(), Some(&elem(3)));

        assert!(selection.remove(&elem(3)));
        assert_eq!(selection.primary(), Some(&elem(1)));

        // Removing a node before the primary keeps the same node primary.
        let mut selection = state_with(&[1, 2, 3], 2);
        assert!(selection.remove(&elem(1)));
        assert_eq!(selection.primary(), Some(&elem(3)));
        // Smoke: a node that is not selected is not removed.
        assert!(!selection.remove(&elem(9)));
        assert_eq!(selection.len(), 2);
    }

    #[test]
    fn two_nodes_are_the_compare_case_with_the_other_one_named() {
        let selection = state_with(&[1, 2], 0);
        assert_eq!(selection.other(), Some(&elem(2)));
        assert_eq!(state_with(&[1, 2], 1).other(), Some(&elem(1)));
        // Smoke: two nodes are never the summary.
        assert!(!selection.shows_summary());
    }

    #[test]
    fn stepping_in_only_counts_for_a_multi_selection() {
        let mut selection = state_with(&[1, 2, 3], 0);
        selection.stepped_in = true;
        assert!(selection.stepped_in() && !selection.shows_summary());

        assert!(selection.remove(&elem(3)));
        // Two left: the compare view, not a stepped-in summary.
        assert!(!selection.stepped_in());
        assert!(!selection.shows_summary());
    }
}
