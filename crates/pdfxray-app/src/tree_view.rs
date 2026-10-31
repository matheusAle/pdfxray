//! A collapsible, virtualised tree over [`TreeNode`]s, drawn with [`TreeRow`].
//!
//! [`TreeModel`] flattens the tree once into pre-order slots. Expanding or collapsing only
//! rebuilds the list of visible slot indices, and [`TreeModel::ui`] builds widgets for the
//! rows inside the scroll viewport, so a frame costs the same for 100 nodes and for 100 000.
//!
//! The model also does what makes a tree feel alive: it scrolls to and expands the way to the
//! selected node when the selection changes, moves the selection with the arrow keys, and
//! reports the row under the pointer and double clicks so the canvas can preview and zoom.

use std::collections::HashMap;

use egui::{Color32, Key, Modifiers, Response, ScrollArea, Ui};
use pdfxray_core::numeric::{f32_from_usize, usize_from_u32};
use pdfxray_core::{Badge, NodeId, Severity, TreeNode};

use crate::components::{Annotation, Tone, TreeRow};
use crate::theme::tokens;

/// How a panel wants one node drawn, instead of the plain label and detail of the core.
#[derive(Debug, Default, Clone)]
pub struct Decor {
    /// Replaces the label.
    pub label: Option<String>,
    /// Replaces the dim text after the label.
    pub detail: Option<String>,
    /// Shown instead of the detail while the row is selected or pointed at (the object
    /// reference, say, where the quiet text is a preview). The filter matches it as well.
    pub detail_when_hot: Option<String>,
    /// A tooltip for the row.
    pub hover_text: Option<String>,
    /// The label is set in semibold mono (an operator name).
    pub mono_label: bool,
    /// Replaces the kind dot colour.
    pub dot: Option<Color32>,
    /// Shown at the right end of the row, before the core's badges.
    pub right: Vec<Annotation>,
    /// Extra text the filter matches, besides the core's label and detail.
    pub search: String,
    /// The node is a reference into a content stream, not something to select: its row is
    /// drawn quietly and never reports a click or hover, the arrow keys step over it and the
    /// tree never highlights or reveals it. See [`TreeRow::content_ref`].
    pub content_ref: bool,
}

/// One flattened node.
#[derive(Debug)]
struct Slot {
    id: NodeId,
    label: String,
    detail: Option<String>,
    detail_when_hot: Option<String>,
    hover_text: Option<String>,
    mono_label: bool,
    dot: Color32,
    right: Vec<Annotation>,
    badges: Vec<Badge>,
    /// A content reference row: shown, but not interactive.
    content_ref: bool,
    depth: u32,
    parent: Option<usize>,
    has_children: bool,
    /// Index one past the last descendant, so `end == index + 1` for a leaf.
    end: usize,
    /// Lower-case text the filter looks at.
    haystack: String,
}

/// A flattened tree with its expansion state and filter.
#[derive(Debug)]
pub struct TreeModel {
    slots: Vec<Slot>,
    open: Vec<bool>,
    visible: Vec<usize>,
    filter: Vec<String>,
    index: HashMap<NodeId, usize>,
    /// The node the tree last scrolled to, so it only scrolls when the selection changes.
    seen: Option<NodeId>,
    /// Scroll offset as of the last frame.
    offset: f32,
    /// A row had the keyboard focus last frame.
    had_focus: bool,
}

/// What happened while drawing a [`TreeModel`].
#[derive(Debug, Default, Clone)]
pub struct TreeOutput {
    /// The node whose row was clicked.
    pub clicked: Option<NodeId>,
    /// The node whose row was double clicked.
    pub double_clicked: Option<NodeId>,
    /// The node whose row the pointer is over.
    pub hovered: Option<NodeId>,
    /// The node an arrow key moved the selection to.
    pub key_selected: Option<NodeId>,
    /// The node whose row was right-clicked, or whose context menu is open, with the row's
    /// response: the owner of the app state attaches the node's menu to it.
    pub context: Option<(NodeId, Response)>,
    /// How many rows were built this frame; bounded by the viewport, not by the tree size.
    pub rows_built: usize,
}

impl TreeModel {
    /// Flattens `roots` in document order. Top-level nodes start expanded.
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self::with_decor(roots, |_| Decor::default())
    }

    /// Like [`Self::new`], drawing each node as `decorate` says.
    pub fn with_decor(roots: Vec<TreeNode>, mut decorate: impl FnMut(&TreeNode) -> Decor) -> Self {
        let mut slots: Vec<Slot> = Vec::new();
        // Each frame is the unvisited siblings of one level and the slot of their parent.
        let mut stack = vec![(roots.into_iter(), None::<usize>)];
        while let Some((siblings, parent)) = stack.last_mut() {
            let parent = *parent;
            if let Some(node) = siblings.next() {
                let decor = decorate(&node);
                let TreeNode {
                    id,
                    label,
                    detail,
                    badges,
                    children,
                } = node;
                let index = slots.len();
                let has_children = !children.is_empty();
                let haystack = format!(
                    "{label} {} {} {}",
                    detail.as_deref().unwrap_or_default(),
                    decor.detail_when_hot.as_deref().unwrap_or_default(),
                    decor.search
                )
                .to_lowercase();
                slots.push(Slot {
                    dot: decor.dot.unwrap_or_else(|| tokens::kind_color(&id)),
                    label: decor.label.unwrap_or(label),
                    detail: decor.detail.or(detail),
                    detail_when_hot: decor.detail_when_hot,
                    hover_text: decor.hover_text,
                    mono_label: decor.mono_label,
                    right: decor.right,
                    content_ref: decor.content_ref,
                    id,
                    badges,
                    depth: u32::try_from(stack.len() - 1).unwrap_or(u32::MAX),
                    parent,
                    has_children,
                    end: index + 1,
                    haystack,
                });
                stack.push((children.into_iter(), Some(index)));
            } else if let Some((_, Some(parent))) = stack.pop() {
                slots[parent].end = slots.len();
            }
        }
        let open = slots
            .iter()
            .map(|slot| slot.depth == 0 && slot.has_children)
            .collect();
        let mut index = HashMap::with_capacity(slots.len());
        // Content references are not selectable, so no id leads to their rows.
        for (i, slot) in slots.iter().enumerate().filter(|(_, slot)| !slot.content_ref) {
            index.entry(slot.id.clone()).or_insert(i);
        }
        let mut model = Self {
            slots,
            open,
            visible: Vec::new(),
            filter: Vec::new(),
            index,
            seen: None,
            offset: 0.0,
            had_focus: false,
        };
        model.rebuild_visible();
        model
    }

    /// The colour of the dot of the row with `id`.
    pub fn dot_of(&self, id: &NodeId) -> Option<Color32> {
        self.index.get(id).map(|&at| self.slots[at].dot)
    }

    /// Number of nodes, collapsed or not.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// True when the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// True when the tree has a selectable row for this id: content reference rows are shown
    /// but never found by id.
    pub fn contains(&self, id: &NodeId) -> bool {
        self.index.contains_key(id)
    }

    /// Number of rows currently shown, i.e. nodes whose ancestors are all expanded.
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    /// Labels of the visible rows in order, each prefixed by two spaces per level.
    pub fn visible_labels(&self) -> Vec<String> {
        self.visible
            .iter()
            .map(|&i| {
                format!(
                    "{}{}",
                    "  ".repeat(usize_from_u32(self.slots[i].depth)),
                    self.slots[i].label
                )
            })
            .collect()
    }

    /// Expands or collapses the node with `id`; false when it is absent or a leaf.
    pub fn toggle(&mut self, id: &NodeId) -> bool {
        let Some(&index) = self.index.get(id) else {
            return false;
        };
        self.toggle_index(index)
    }

    fn toggle_index(&mut self, index: usize) -> bool {
        if !self.slots[index].has_children {
            return false;
        }
        self.open[index] = !self.open[index];
        self.rebuild_visible();
        true
    }

    /// Expands the ancestors of the node with `id` so its row is shown. Returns the row's
    /// position among the visible rows, `None` when the node is absent or a filter hides it.
    pub fn reveal(&mut self, id: &NodeId) -> Option<usize> {
        let &index = self.index.get(id)?;
        if !self.is_filtered() {
            let mut changed = false;
            let mut at = self.slots[index].parent;
            while let Some(parent) = at {
                changed |= !self.open[parent];
                self.open[parent] = true;
                at = self.slots[parent].parent;
            }
            if changed {
                self.rebuild_visible();
            }
        }
        self.visible.binary_search(&index).ok()
    }

    /// Expands every node.
    pub fn expand_all(&mut self) {
        for (open, slot) in self.open.iter_mut().zip(&self.slots) {
            *open = slot.has_children;
        }
        self.rebuild_visible();
    }

    /// Collapses every node.
    pub fn collapse_all(&mut self) {
        self.open.fill(false);
        self.rebuild_visible();
    }

    /// Shows only the nodes whose label or id contains every word of `words` (case does not
    /// matter), with their ancestors and descendants. No words show everything again.
    pub fn set_filter(&mut self, words: &[String]) {
        let words: Vec<String> = words
            .iter()
            .map(|w| w.trim().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        if words != self.filter {
            self.filter = words;
            self.rebuild_visible();
        }
    }

    /// True while a filter hides rows.
    pub fn is_filtered(&self) -> bool {
        !self.filter.is_empty()
    }

    fn matches(&self, slot: &Slot) -> bool {
        self.filter
            .iter()
            .all(|word| slot.haystack.contains(word.as_str()))
    }

    fn rebuild_visible(&mut self) {
        self.visible.clear();
        if self.is_filtered() {
            let hit: Vec<bool> = self.slots.iter().map(|s| self.matches(s)).collect();
            // Slots are in pre-order, so a parent always comes before its children: one forward
            // pass marks everything at or under a match, one backward pass its ancestors.
            let mut under = hit.clone();
            for (index, slot) in self.slots.iter().enumerate() {
                if let Some(parent) = slot.parent {
                    under[index] |= under[parent];
                }
            }
            let mut ancestor = vec![false; self.slots.len()];
            for index in (0..self.slots.len()).rev() {
                if let Some(parent) = self.slots[index].parent {
                    ancestor[parent] |= hit[index] || ancestor[index];
                }
            }
            self.visible = (0..self.slots.len())
                .filter(|&index| under[index] || ancestor[index])
                .collect();
            return;
        }
        let mut index = 0;
        while index < self.slots.len() {
            self.visible.push(index);
            index = if self.open[index] {
                index + 1
            } else {
                self.slots[index].end
            };
        }
    }

    /// Where an arrow key takes the selection `from`: a new node to select, or `None` when the
    /// key only collapsed or expanded a row, or there is nowhere to go.
    ///
    /// Up and Down move between the visible rows, stepping over content references. Left collapses an open row, else goes to the
    /// parent; Right expands a closed row, else goes to its first child.
    pub fn key_target(&mut self, from: Option<&NodeId>, key: Key) -> Option<NodeId> {
        let from_index = from.and_then(|id| self.index.get(id)).copied();
        let row = from_index.and_then(|i| self.visible.binary_search(&i).ok());
        let filtered = self.is_filtered();
        match key {
            Key::ArrowDown | Key::ArrowUp => {
                let selectable = |&&i: &&usize| !self.slots[i].content_ref;
                let target = match (row, key) {
                    (None, _) => self.visible.iter().find(selectable),
                    (Some(row), Key::ArrowDown) => self.visible[row + 1..].iter().find(selectable),
                    (Some(row), _) => self.visible[..row].iter().rev().find(selectable),
                };
                Some(self.slots[*target?].id.clone())
            }
            Key::ArrowLeft => {
                let index = from_index?;
                if self.has_open_children(index) && !filtered {
                    self.toggle_index(index);
                    return None;
                }
                let parent = self.slots[index].parent?;
                Some(self.slots[parent].id.clone())
            }
            Key::ArrowRight => {
                let index = from_index?;
                if !self.slots[index].has_children {
                    return None;
                }
                if !self.open[index] && !filtered {
                    self.toggle_index(index);
                    return None;
                }
                let child = self.visible[row? + 1..]
                    .iter()
                    .copied()
                    .take_while(|&c| c < self.slots[index].end)
                    .find(|&c| !self.slots[c].content_ref)?;
                Some(self.slots[child].id.clone())
            }
            _ => None,
        }
    }

    fn has_open_children(&self, index: usize) -> bool {
        self.slots[index].has_children && self.open[index]
    }

    /// Draws the visible part of the tree, highlighting `selected`.
    pub fn ui(&mut self, ui: &mut Ui, selected: Option<&NodeId>, id_salt: impl egui::AsIdSalt) -> TreeOutput {
        let row_height = tokens::ROW_H;
        let base = ui.make_persistent_id(id_salt);
        let mut output = TreeOutput::default();
        let mut toggled = None;
        let filtered = self.is_filtered();
        let viewport = ui.available_height();

        output.key_selected = self.keys(ui, selected);
        let mut scroll_to = None;
        if selected != self.seen.as_ref() {
            self.seen = selected.cloned();
            if let Some(row) = selected.and_then(|id| self.reveal(id)) {
                // Rows are a row height and the item spacing apart, as `show_rows` lays them out.
                let pitch = row_height + ui.spacing().item_spacing.y;
                scroll_to = self.scroll_for(row, viewport, pitch);
            }
        }

        let mut area = ScrollArea::vertical()
            .id_salt(base.with("scroll"))
            .auto_shrink(false);
        if let Some(offset) = scroll_to {
            area = area.vertical_scroll_offset(offset);
        }
        let mut focused = false;
        let shown = area.show_rows(ui, row_height, self.visible.len(), |ui, rows| {
            for row in rows {
                let index = self.visible[row];
                let slot = &self.slots[index];
                output.rows_built += 1;
                let mut annotations = slot.right.clone();
                annotations.extend(badge_annotations(&slot.badges));
                let expanded = if filtered {
                    slot.has_children
                } else {
                    self.open[index]
                };
                let mut tree_row = TreeRow::new(slot.depth, &slot.label)
                    .detail(slot.detail.as_deref())
                    .detail_when_hot(slot.detail_when_hot.as_deref())
                    .hover_text(slot.hover_text.as_deref())
                    .dot(slot.dot)
                    .selected(selected == Some(&slot.id))
                    .annotations(&annotations);
                if slot.has_children {
                    tree_row = tree_row.expandable(expanded);
                }
                if slot.mono_label {
                    tree_row = tree_row.mono_label();
                }
                if slot.content_ref {
                    tree_row.content_ref().show(ui, base.with(("row", index)));
                    continue;
                }
                let result = tree_row.show(ui, base.with(("row", index)));
                if result.toggled && !filtered {
                    toggled = Some(index);
                }
                focused |= result.row.has_focus();
                if result.row.hovered() {
                    output.hovered = Some(slot.id.clone());
                }
                if result.row.secondary_clicked() || result.row.context_menu_opened() {
                    output.context = Some((slot.id.clone(), result.row.clone()));
                }
                if result.row.clicked() && !result.toggled {
                    output.clicked = Some(slot.id.clone());
                }
                if result.row.double_clicked() && !result.toggled {
                    output.double_clicked = Some(slot.id.clone());
                }
            }
        });
        self.offset = shown.state.clamped_offset().y;
        self.had_focus = focused;

        if let Some(index) = toggled {
            self.toggle_index(index);
        }
        output
    }

    /// The scroll offset that brings visible row `row` into view, `None` when it already is.
    /// `pitch` is the distance between the tops of two rows.
    fn scroll_for(&self, row: usize, viewport: f32, pitch: f32) -> Option<f32> {
        // Row counts are far below f32's exact integer range.
        let top = f32_from_usize(row) * pitch;
        let inside = top >= self.offset && top + tokens::ROW_H <= self.offset + viewport;
        (!inside).then(|| (top - (viewport - tokens::ROW_H) / 2.0).max(0.0))
    }

    /// Arrow keys, while the pointer is over the tree or a row has the focus.
    fn keys(&mut self, ui: &Ui, selected: Option<&NodeId>) -> Option<NodeId> {
        let ctx = ui.ctx();
        if !(self.had_focus || ui.rect_contains_pointer(ui.max_rect())) || ctx.text_edit_focused() {
            return None;
        }
        for key in [Key::ArrowDown, Key::ArrowUp, Key::ArrowLeft, Key::ArrowRight] {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                return self.key_target(selected, key);
            }
        }
        None
    }
}

/// Badge code of nodes a validation finding points at.
pub const VALIDATION_BADGE: &str = "validation";

/// Right-hand annotations of a row: validation findings are a red dot, so they stand out from
/// the reader's own diagnostics, which get a warning mark and a short word. Only problems are
/// marked: information-level diagnostics stay in the Diagnostics drawer.
fn badge_annotations(badges: &[Badge]) -> Vec<Annotation> {
    badges
        .iter()
        .filter_map(|badge| {
            if badge.code == VALIDATION_BADGE {
                Some(Annotation::Dot {
                    color: tokens::ERR,
                    name: "Validation finding".to_owned(),
                })
            } else if badge.severity == Severity::Info {
                None
            } else {
                Some(Annotation::Tag(
                    badge_word(&badge.code).to_owned(),
                    severity_tone(badge.severity),
                ))
            }
        })
        .collect()
}

/// The word a tree row shows for a diagnostic: what is wrong, in one word or two (`Alt`,
/// `order`). The full code is in the Diagnostics drawer and the inspector's problem card.
pub fn badge_word(code: &str) -> &str {
    match code {
        "figure-alt-missing" | "alt-missing" => "Alt",
        "alt-hides-content" => "Alt hides",
        "alt-and-actualtext" | "actualtext-on-structure" => "ActualText",
        "reading-order-backtrack" => "order",
        "rolemap-multi-hop" => "role map",
        "private-hides-content" => "Private",
        "table-header-missing" | "table-header-unresolved" | "table-headers-id-missing" => "header",
        "table-irregular" => "table",
        // Otherwise the last word of the code: `mcid-orphan` → `orphan`.
        other => other.rsplit('-').next().unwrap_or(other),
    }
}

/// The chip tone of a severity.
pub(crate) const fn severity_tone(severity: Severity) -> Tone {
    match severity {
        Severity::Info => Tone::Kind(tokens::KIND_CONTENT),
        Severity::Warning => Tone::Warn,
        Severity::Error => Tone::Err,
    }
}
