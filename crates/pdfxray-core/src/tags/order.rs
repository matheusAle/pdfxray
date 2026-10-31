//! Tag boxes and reading order of a page: where each structure element sits, and in which
//! sequence a screen reader (structure tree) and the painter (content stream) visit them.
//!
//! The reading order of a page lists the elements that *directly own* content on it (marked
//! content or referenced objects), once each. Container elements such as `Document` or `Sect`
//! own nothing themselves, so they take no step; their content belongs to the leaves below them.
//!
//! # Step kinds
//!
//! Every step after the first is classified against the step before it (see [`StepKind`]). All
//! geometry is in page space (y grows upwards) and assumes left-to-right text; the tolerances are
//! the constants of this module.
//!
//! - **Forward.** The step's top edge does not rise above the previous step's by more than
//!   `max(2 pt, three quarters of the shorter of the two boxes' heights)`: it sits below, or on
//!   the same line (a bullet and the text beside it, a wrapped line). The relative part absorbs
//!   differing font sizes on one line, the absolute part is the floor for tiny boxes.
//! - **Column break.** The top edge rises past that tolerance, but the box starts at or to the
//!   right of the previous box's right edge (within 2 pt of overlap): the next column.
//! - **Detour.** A `Note`, `FENote` or `Aside` (or anything inside one; the outermost such
//!   ancestor counts as one unit) that follows a step outside it and is followed, in turn, by a
//!   step that climbs back to the main flow: that step rises past the tolerance above the last
//!   step of the note, is not itself inside a note, and does not start above the top edge of the
//!   step the note interrupted (the anchor) by more than the tolerance. The first step of the
//!   note and the step that returns are the detour; the steps inside the note keep their own
//!   kind. Best practice puts a footnote right after the paragraph that cites it
//!   (ISO 32000-1 §14.8.4.4, PDF Association Best Practice Guide §4.2.8), so this jump is
//!   legitimate and must not be reported. A `Caption` is *not* a detour: reading it before the
//!   figure above it is a backtrack.
//! - **Backtrack.** Every other climb: a move back up the page within the same column or flow, for
//!   example a caption tagged before its figure, or a footer tagged first. Only these are reported
//!   as `reading-order-backtrack`.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use super::parse::Builder;
use super::roles::RoleClass;
use super::speech::{SpeechItem, SpeechOrder};
use super::text;
use super::view::union;
use super::{Elem, Kid, TagTree};
use crate::{Document, NodeId, PageIx, Rect, Severity};

/// Smallest rise of a step's top edge over the previous step's that counts as going back up.
const BACKTRACK_MIN: f64 = 2.0;
/// Share of the shorter box's height that the rise must also exceed.
const BACKTRACK_LINE_FRACTION: f64 = 0.75;
/// How far a new column's box may reach back over the right edge of the previous box.
const COLUMN_OVERLAP_SLACK: f64 = 2.0;
/// Standard types whose content is read at the position of its reference but sits elsewhere.
const DETOUR_TYPES: [&str; 3] = ["Note", "FENote", "Aside"];
/// Characters of text previewed per reading-order step.
const STEP_PREVIEW: usize = 60;

/// A structure element's box on one page. See [`Document::tag_boxes`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagBox {
    /// The structure element.
    pub id: NodeId,
    /// The structure type as written in `/S`.
    pub role: String,
    /// The standard type the role resolves to through `/RoleMap` and the namespaces; empty when
    /// it never reaches one.
    pub standard: String,
    /// The family of the standard type.
    pub class: RoleClass,
    /// The union of the boxes of everything the element draws on the page, in page space.
    pub rect: Rect,
    /// Number of ancestors: 0 for a child of the structure tree root.
    pub depth: u32,
}

/// Which sequence to read a page's elements in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadingOrderSource {
    /// The structure tree's document order: what a screen reader follows.
    Structure,
    /// The order in which the content stream starts painting each element's content.
    Paint,
}

/// How a reading-order step relates to the step before it. See the module documentation for the
/// geometric rules. Only [`StepKind::Backtrack`] raises `reading-order-backtrack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// The first step of the page.
    First,
    /// Below the previous step, or on the same line.
    Forward,
    /// Up to the top of the next column, to the right of the previous step.
    ColumnBreak,
    /// Enters a `Note`, `FENote` or `Aside` that sits elsewhere, or returns from it to the main
    /// flow.
    Detour,
    /// Back up the page within the same column or flow.
    Backtrack,
}

/// One element in a reading order. See [`Document::reading_order`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderStep {
    /// Position in the order, from 1.
    pub index: usize,
    /// The structure element.
    pub id: NodeId,
    /// The structure type as written in `/S`.
    pub role: String,
    /// The family of the element's standard type.
    pub class: RoleClass,
    /// The box of the content the element itself holds on the page, in page space.
    pub rect: Rect,
    /// How this step relates to the previous one.
    pub kind: StepKind,
    /// True when `kind` is [`StepKind::Backtrack`]: the step starts clearly above the previous one
    /// in the same flow.
    pub backtrack: bool,
    /// Start of the element's text (or its alternate text), whitespace collapsed; empty when it
    /// has none.
    pub preview: String,
}

/// The order a page's content is read in. See [`Document::reading_order`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadingOrder {
    /// The steps, first to last.
    pub steps: Vec<OrderStep>,
    /// How many steps are [`StepKind::Backtrack`]; columns and detours do not count.
    pub backtracks: usize,
}

/// How far the structure order and the paint order of a page agree. See
/// [`Document::reading_order_comparison`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderComparison {
    /// Positions where both orders hold the same element.
    pub same: usize,
    /// Number of positions (both orders list the same elements).
    pub total: usize,
    /// The first position (from 1) where the orders differ, with the element the structure
    /// order holds there.
    pub first_difference: Option<(usize, NodeId)>,
}

/// An element placed in an order, before names and previews are attached.
#[derive(Debug, Clone, Copy)]
struct Placed {
    elem: usize,
    rect: Rect,
    kind: StepKind,
}

/// What the boxes of two consecutive steps say, before notes are taken into account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Forward,
    NewColumn,
    Up,
}

/// Rise of a top edge over another that is still the same line (see the module docs).
fn tolerance(a: Rect, b: Rect) -> f64 {
    BACKTRACK_MIN.max(BACKTRACK_LINE_FRACTION * a.height().min(b.height()))
}

/// True when `cur` starts clearly above `prev` (see the module docs).
fn goes_back_up(prev: Rect, cur: Rect) -> bool {
    cur.y1 - prev.y1 > tolerance(prev, cur)
}

/// Classifies the move from `prev` to `cur` by their boxes alone.
fn geometry(prev: Rect, cur: Rect) -> Move {
    if !goes_back_up(prev, cur) {
        Move::Forward
    } else if cur.x0 >= prev.x1 - COLUMN_OVERLAP_SLACK {
        Move::NewColumn
    } else {
        Move::Up
    }
}

/// `role`, or `role (standard)` for a custom role, for messages.
fn name(e: &Elem) -> String {
    let role = if e.role.is_empty() { "(no type)" } else { &e.role };
    match &e.resolved.standard {
        Some(standard) if *standard != e.role => format!("{role} ({standard})"),
        _ => role.to_owned(),
    }
}

impl TagTree {
    /// The class of element `ei`, from the standard type its role resolves to.
    pub(super) fn class(&self, ei: usize) -> RoleClass {
        let resolved = &self.elems[ei].resolved;
        resolved.standard.as_deref().map_or(RoleClass::Other, |standard| {
            RoleClass::of_standard(standard, resolved.standard_ns.as_deref())
        })
    }

    /// Elements that directly own content, per page (only `only` when given), each once per
    /// page, in structure-tree reading order: an element stands where its first content on the
    /// page sits among its `/K` entries.
    fn owners_by_page(&self, only: Option<PageIx>) -> BTreeMap<PageIx, Vec<usize>> {
        /// The walk's bookkeeping: elements visited (cycle guard) and `(element, page)` pairs
        /// already listed.
        struct Walk {
            only: Option<PageIx>,
            seen: HashSet<usize>,
            listed: HashSet<(usize, PageIx)>,
            out: BTreeMap<PageIx, Vec<usize>>,
        }
        fn visit(tree: &TagTree, ei: usize, walk: &mut Walk) {
            let e = &tree.elems[ei];
            for kid in &e.kids {
                let page = match kid {
                    Kid::Elem(child) => {
                        if walk.seen.insert(*child) {
                            visit(tree, *child, walk);
                        }
                        continue;
                    }
                    Kid::Content { page, .. } => *page,
                    Kid::Object { obj: Some(_), page } => page.or(e.eff_page),
                    Kid::Object { obj: None, .. } => None,
                };
                if let Some(page) = page
                    && walk.only.is_none_or(|p| p == page)
                    && walk.listed.insert((ei, page))
                {
                    walk.out.entry(page).or_default().push(ei);
                }
            }
        }
        let mut walk = Walk {
            only,
            seen: HashSet::new(),
            listed: HashSet::new(),
            out: BTreeMap::new(),
        };
        for kid in self.root.iter().flat_map(|r| &r.kids) {
            if let Kid::Elem(ei) = kid
                && walk.seen.insert(*ei)
            {
                visit(self, *ei, &mut walk);
            }
        }
        walk.out
    }

    /// Execution index at which element `ei` starts painting on `page`; elements without marked
    /// content there (annotations) come last.
    fn first_paint_op(&self, doc: &Document, ei: usize, page: PageIx) -> u32 {
        self.elems[ei]
            .kids
            .iter()
            .filter_map(|kid| match kid {
                Kid::Content {
                    page: Some(p),
                    mcid,
                    stream,
                    ..
                } if *p == page => doc.mc_first_op(page, *mcid, *stream),
                _ => None,
            })
            .min()
            .unwrap_or(u32::MAX)
    }

    /// Orders `owners` (see [`TagTree::owners_by_page`]) for `source` and classifies each step.
    /// Elements whose own content paints nothing on the page have no box and are left out.
    fn place(
        &self,
        doc: &Document,
        page: PageIx,
        owners: &[usize],
        source: ReadingOrderSource,
    ) -> Vec<Placed> {
        let mut boxed: Vec<(usize, Rect)> = owners
            .iter()
            .filter_map(|&ei| Some((ei, self.own_box(doc, ei, page)?)))
            .collect();
        if source == ReadingOrderSource::Paint {
            // Stable: elements that start at the same operator keep their structure order.
            boxed.sort_by_key(|&(ei, _)| self.first_paint_op(doc, ei, page));
        }
        let flow: Vec<(Option<usize>, Rect)> = boxed.iter().map(|&(ei, rect)| (Some(ei), rect)).collect();
        let kinds = self.step_kinds(&flow);
        boxed
            .into_iter()
            .zip(kinds)
            .map(|((elem, rect), kind)| Placed { elem, rect, kind })
            .collect()
    }

    /// The outermost `Note`, `FENote` or `Aside` that holds element `ei` (or is `ei`).
    fn note_of(&self, ei: usize) -> Option<usize> {
        let mut found = None;
        let mut cur = Some(ei);
        while let Some(c) = cur {
            let e = &self.elems[c];
            if e.resolved
                .standard
                .as_deref()
                .is_some_and(|standard| DETOUR_TYPES.contains(&standard))
            {
                found = Some(c);
            }
            cur = e.parent;
        }
        found
    }

    /// The kind of every step of `boxed` (see the module docs): geometry first, then the notes
    /// that are left and returned from.
    fn step_kinds(&self, boxed: &[(Option<usize>, Rect)]) -> Vec<StepKind> {
        let moves: Vec<Move> = boxed
            .iter()
            .zip(boxed.iter().skip(1))
            .map(|(&(_, prev), &(_, cur))| geometry(prev, cur))
            .collect();
        let mut kinds = Vec::with_capacity(boxed.len());
        if !boxed.is_empty() {
            kinds.push(StepKind::First);
        }
        kinds.extend(moves.iter().map(|m| match m {
            Move::Forward => StepKind::Forward,
            Move::NewColumn => StepKind::ColumnBreak,
            Move::Up => StepKind::Backtrack,
        }));
        let notes: Vec<Option<usize>> = boxed
            .iter()
            .map(|&(ei, _)| ei.and_then(|ei| self.note_of(ei)))
            .collect();
        for entry in 1..boxed.len() {
            let Some(note) = notes[entry] else { continue };
            if notes[entry - 1] == Some(note) {
                continue;
            }
            let anchor = boxed[entry - 1].1;
            let back = (entry..boxed.len())
                .take_while(|&i| notes[i] == Some(note))
                .count()
                + entry;
            // `moves[i]` is the move into step `i + 1`.
            let returns = back < boxed.len()
                && notes[back].is_none()
                && moves[back - 1] == Move::Up
                && boxed[back].1.y1 - anchor.y1 <= tolerance(anchor, boxed[back].1);
            if returns {
                kinds[entry] = StepKind::Detour;
                kinds[back] = StepKind::Detour;
            }
        }
        kinds
    }

    /// The placed elements of one page for `source`.
    fn page_order(&self, doc: &Document, page: PageIx, source: ReadingOrderSource) -> Vec<Placed> {
        let owners = self.owners_by_page(Some(page)).remove(&page).unwrap_or_default();
        self.place(doc, page, &owners, source)
    }

    pub(super) fn id_of(&self, ei: usize) -> NodeId {
        NodeId::StructElem(self.elems[ei].obj)
    }
}

impl Builder<'_> {
    /// Reports every backtrack of every page's structure reading order.
    pub(super) fn check_reading_order(&mut self) {
        let doc = self.rd.doc;
        let mut found: Vec<(usize, String)> = Vec::new();
        for (page, owners) in self.tree.owners_by_page(None) {
            let placed = self.tree.place(doc, page, &owners, ReadingOrderSource::Structure);
            for (i, (prev, cur)) in placed.iter().zip(placed.iter().skip(1)).enumerate() {
                if cur.kind != StepKind::Backtrack {
                    continue;
                }
                let (before, after) = (&self.tree.elems[prev.elem], &self.tree.elems[cur.elem]);
                let (before, after) = (name(before), name(after));
                found.push((
                    cur.elem,
                    format!(
                        "{before} (step {}) is read before {after} (step {}), but {after} sits above it on \
                         page {}, so a screen reader jumps back up the page",
                        i + 1,
                        i + 2,
                        page + 1
                    ),
                ));
            }
        }
        for (ei, message) in found {
            let obj = self.tree.elems[ei].obj;
            self.find_on(
                "reading-order-backtrack",
                Severity::Warning,
                message,
                Some(obj),
                Some(ei),
            );
        }
    }
}

impl Document {
    /// The class of a structure element, from the standard type its role resolves to through
    /// `/RoleMap` and the namespaces (a custom role takes the class of its target). `None` when
    /// `id` is not a structure element of the tree.
    #[must_use]
    pub fn role_class(&self, id: &NodeId) -> Option<RoleClass> {
        let NodeId::StructElem(r) = id else {
            return None;
        };
        self.tags.index.get(r).map(|&ei| self.tags.class(ei))
    }

    /// The class of the structure element that owns marked-content sequence `mcid` of `page`
    /// (the element whose `/K` references it first). `None` when no element does. This is what
    /// colours the x-ray outlines by tag role.
    #[must_use]
    pub fn mcid_role_class(&self, page: PageIx, mcid: i32) -> Option<RoleClass> {
        let tree = &self.tags;
        tree.owners.get(&(page, mcid)).map(|&ei| tree.class(ei))
    }

    /// Every structure element that draws on `page`, with its box there, parents before
    /// children (document order). An element spanning several pages appears on each, with the
    /// part of its box on that page.
    #[must_use]
    pub fn tag_boxes(&self, page: PageIx) -> Vec<TagBox> {
        let tree = &self.tags;
        (0..tree.elems.len())
            .filter_map(|ei| {
                let rect = tree.bbox_on(self, ei, page)?;
                let e = &tree.elems[ei];
                Some(TagBox {
                    id: tree.id_of(ei),
                    role: e.role.clone(),
                    standard: e.resolved.standard.clone().unwrap_or_default(),
                    class: tree.class(ei),
                    rect,
                    depth: tree.depth_of(ei),
                })
            })
            .collect()
    }

    /// The elements that directly own content on `page`, in the order `source` gives. See the
    /// module documentation of `tags::order` for how steps are classified.
    ///
    /// [`ReadingOrderSource::Structure`] follows the structure tree (what a screen reader
    /// reads); [`ReadingOrderSource::Paint`] orders the same elements by the operator that
    /// starts painting their content. An element with no painted content on the page has no
    /// box and takes no step.
    #[must_use]
    pub fn reading_order(&self, page: PageIx, source: ReadingOrderSource) -> ReadingOrder {
        let tree = &self.tags;
        let placed = tree.page_order(self, page, source);
        let steps = placed
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let e = &tree.elems[p.elem];
                OrderStep {
                    index: i + 1,
                    id: tree.id_of(p.elem),
                    role: e.role.clone(),
                    class: tree.class(p.elem),
                    rect: p.rect,
                    kind: p.kind,
                    backtrack: p.kind == StepKind::Backtrack,
                    preview: tree.preview(self, p.elem, STEP_PREVIEW).unwrap_or_default(),
                }
            })
            .collect();
        ReadingOrder {
            steps,
            backtracks: placed.iter().filter(|p| p.kind == StepKind::Backtrack).count(),
        }
    }

    /// The reading order of `page` as `speech` announces it: the spoken items on the page, in the
    /// order of the transcript, each placed where its content paints. Consecutive items of one
    /// element are one step. Steps are classified like those of [`Document::reading_order`], so
    /// the canvas path, the badges and the backtrack flag work the same for either.
    ///
    /// An item has a box when its marked content paints on the page, else when its element has
    /// one (an element that `/Alt` replaces keeps the box of what it covers), else from the `/Rect`
    /// of an annotation. An item with none has no step. Items the preset skips take none either.
    #[must_use]
    pub fn speech_reading_order(&self, page: PageIx, speech: &SpeechOrder) -> ReadingOrder {
        let tree = &self.tags;
        let mut items: Vec<&SpeechItem> = Vec::new();
        let mut boxed: Vec<(Option<usize>, Rect)> = Vec::new();
        for item in speech.spoken().filter(|item| item.page == Some(page)) {
            let Some(rect) = self.speech_item_box(item, page) else {
                continue;
            };
            if let (Some(last), Some(prev)) = (items.last(), boxed.last_mut())
                && last.node == item.node
            {
                prev.1 = union(prev.1, rect);
                continue;
            }
            let ei = match &item.node {
                NodeId::StructElem(r) => tree.index.get(r).copied(),
                _ => None,
            };
            items.push(item);
            boxed.push((ei, rect));
        }
        let kinds = tree.step_kinds(&boxed);
        let steps: Vec<OrderStep> = items
            .into_iter()
            .zip(boxed)
            .zip(kinds)
            .enumerate()
            .map(|(i, ((item, (_, rect)), kind))| OrderStep {
                index: i + 1,
                id: item.node.clone(),
                role: item.declared_role.clone(),
                class: item.class,
                rect,
                kind,
                backtrack: kind == StepKind::Backtrack,
                preview: text::squash(&item.text).chars().take(STEP_PREVIEW).collect(),
            })
            .collect();
        let backtracks = steps.iter().filter(|s| s.backtrack).count();
        ReadingOrder { steps, backtracks }
    }

    /// Where `item` paints on `page`.
    fn speech_item_box(&self, item: &SpeechItem, page: PageIx) -> Option<Rect> {
        let content = item
            .marked_content
            .iter()
            .filter(|id| matches!(id, NodeId::MarkedContent { page: p, .. } if *p == page))
            .filter_map(|id| self.content_bbox(id))
            .map(|(_, rect)| rect)
            .reduce(union);
        content.or_else(|| match &item.node {
            NodeId::StructElem(r) => {
                let ei = *self.tags.index.get(r)?;
                self.tags.bbox_on(self, ei, page)
            }
            NodeId::Object(o) => TagTree::object_rect(self, *o),
            NodeId::Artifact { page: p, .. } | NodeId::Op { page: p, .. } if *p == page => {
                self.content_bbox(&item.node).map(|(_, rect)| rect)
            }
            _ => None,
        })
    }

    /// Compares the structure order of `page` with its paint order, position by position.
    #[must_use]
    pub fn reading_order_comparison(&self, page: PageIx) -> OrderComparison {
        let tree = &self.tags;
        let structure = tree.page_order(self, page, ReadingOrderSource::Structure);
        let paint = tree.page_order(self, page, ReadingOrderSource::Paint);
        let first_difference = structure
            .iter()
            .zip(&paint)
            .position(|(s, p)| s.elem != p.elem)
            .map(|at| (at + 1, tree.id_of(structure[at].elem)));
        OrderComparison {
            same: structure
                .iter()
                .zip(&paint)
                .filter(|(s, p)| s.elem == p.elem)
                .count(),
            total: structure.len(),
            first_difference,
        }
    }

    /// A short preview of what a screen reader announces for a structure element, whitespace
    /// collapsed and cut to `max_chars` characters with a trailing `…` when something was dropped.
    /// As in Acrobat, the element's `/Alt` wins, then its `/ActualText`, then its `/E`; without
    /// any (a blank one counts as none) it is the text painted by its marked content (descendants
    /// included). `None` when there is nothing to show, `max_chars` is 0, or `id` is not a
    /// structure element of the tree.
    #[must_use]
    pub fn text_preview(&self, id: &NodeId, max_chars: usize) -> Option<String> {
        let NodeId::StructElem(r) = id else {
            return None;
        };
        let ei = *self.tags.index.get(r)?;
        self.tags.preview(self, ei, max_chars)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn line(top: f64, height: f64) -> Rect {
        Rect::new(40.0, top - height, 200.0, top)
    }

    #[test]
    fn a_step_must_start_clearly_above_the_previous_one_to_be_a_backtrack() {
        let text = line(300.0, 12.0);

        // The next line down, the same line, and a bullet whose top sits 3 pt lower than its text.
        assert_eq!(goes_back_up(text, line(286.0, 12.0)), false);
        assert_eq!(goes_back_up(text, text), false);
        assert_eq!(goes_back_up(line(297.0, 5.0), line(300.0, 12.0)), false);
        // 10 pt higher is more than three quarters of the 12 pt line; 8 pt higher is not.
        assert_eq!(goes_back_up(text, line(310.0, 12.0)), true);
        assert_eq!(goes_back_up(text, line(308.0, 12.0)), false);
        // Smoke: a figure read after the caption below it.
        assert!(goes_back_up(
            line(187.0, 9.0),
            Rect::new(40.0, 200.0, 140.0, 300.0)
        ));
    }

    #[test]
    fn tiny_boxes_use_the_two_point_floor() {
        let dot = line(100.0, 1.0);

        assert_eq!(goes_back_up(dot, line(101.5, 1.0)), false);
        assert_eq!(goes_back_up(dot, line(102.5, 1.0)), true);
        assert!(goes_back_up(dot, line(110.0, 1.0)));
    }

    #[test]
    fn a_climb_to_the_right_of_the_previous_box_is_a_new_column() {
        let column_one = Rect::new(40.0, 280.0, 140.0, 292.0);

        // The next column starts higher, to the right of the first one.
        assert_eq!(
            geometry(column_one, Rect::new(170.0, 340.0, 270.0, 352.0)),
            Move::NewColumn
        );
        // Touching the first column's edge, or reaching back over it by the 2 pt slack, still is.
        assert_eq!(
            geometry(column_one, Rect::new(140.0, 340.0, 270.0, 352.0)),
            Move::NewColumn
        );
        assert_eq!(
            geometry(column_one, Rect::new(138.0, 340.0, 270.0, 352.0)),
            Move::NewColumn
        );
        // Smoke: more overlap than that is a climb inside the same column.
        assert_eq!(
            geometry(column_one, Rect::new(137.0, 340.0, 270.0, 352.0)),
            Move::Up
        );
    }

    #[test]
    fn a_climb_over_the_previous_box_or_to_its_left_is_up_and_moves_down_or_along_are_forward() {
        let text = line(300.0, 12.0);

        assert_eq!(geometry(text, line(340.0, 12.0)), Move::Up);
        assert_eq!(geometry(text, Rect::new(0.0, 328.0, 30.0, 340.0)), Move::Up);
        assert_eq!(geometry(text, line(280.0, 12.0)), Move::Forward);
        assert_eq!(
            geometry(text, Rect::new(260.0, 288.0, 300.0, 300.0)),
            Move::Forward
        );
        // Smoke: the first column's last line to the figure above it, in the same column.
        assert_eq!(
            geometry(line(187.0, 9.0), Rect::new(40.0, 200.0, 140.0, 300.0)),
            Move::Up
        );
    }
}
