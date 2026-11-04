//! Paint order: where an operator sits in the page's painting, what it draws over and what is
//! drawn over it later. Overlap is judged on bounding boxes, so every answer is approximate.

use serde::Serialize;

use super::PageData;
use crate::capture::{OpRecord, PaintItem, PaintKind};
use crate::numeric::usize_from_u32;
use crate::{Document, NodeId, PageIx, Rect};

/// Most operators listed in each of `paints_over` and `covered_by`.
pub const MAX_PAINT_STACK: usize = 64;

/// An operator by its place in the document: operator `ix` (execution order, see
/// [`OpRecord::ix`]) of page `page`. Ordered as the document paints: page by page, then by
/// operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize)]
pub struct OpPosition {
    /// Zero-based page.
    pub page: PageIx,
    /// Execution index of the operator on the page.
    pub ix: u32,
}

impl OpPosition {
    /// Operator `ix` of page `page`.
    #[must_use]
    pub const fn new(page: PageIx, ix: u32) -> Self {
        Self { page, ix }
    }
}

impl From<OpPosition> for NodeId {
    fn from(at: OpPosition) -> Self {
        Self::Op {
            page: at.page,
            ix: at.ix,
        }
    }
}

/// Where one operator sits in the painting of its page. See [`Document::paint_step`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintStep {
    /// How many operators have run once this one has: `op_ix + 1`, from 1 to `total`.
    pub step: u32,
    /// Number of operators on the page (including those inside Form `XObject`s).
    pub total: u32,
    /// Operators that painted earlier in the box this operator paints in, most recent first, at
    /// most [`MAX_PAINT_STACK`]. Empty means it paints on the bare page background.
    pub paints_over: Vec<NodeId>,
    /// Operators that paint later in this operator's box, in paint order, at most [`MAX_PAINT_STACK`].
    pub covered_by: Vec<NodeId>,
    /// The fraction (0 to 1) of this operator's box that no later paint reaches. 1 when it paints
    /// nothing or its box has no area.
    pub visible_fraction: f64,
    /// Always true: overlap is computed from bounding boxes, not from pixels. Rotated shapes,
    /// transparency and holes make the real visibility differ.
    pub approximate: bool,
}

impl Document {
    /// Paint-order facts for operator `op_ix` of `page`, or `None` when either does not exist.
    ///
    /// The operator's box is the union of the boxes of the items it painted (clipped); the
    /// earlier and later items are those whose clipped boxes intersect it. Clip-only items are
    /// ignored. See [`PaintStep`] for what each field means.
    pub fn paint_step(&self, page: PageIx, op_ix: u32) -> Option<PaintStep> {
        let data = self.content.data(page)?;
        let op = data.op(op_ix)?;
        let total = u32::try_from(data.cap.ops.len()).unwrap_or(u32::MAX);
        let own: Vec<&PaintItem> = painted(data, op).collect();
        let mut step = PaintStep {
            step: op_ix.saturating_add(1),
            total,
            paints_over: Vec::new(),
            covered_by: Vec::new(),
            visible_fraction: 1.0,
            approximate: true,
        };
        let (Some(first), Some(last)) = (own.first(), own.last()) else {
            return Some(step);
        };
        let Some(area) = own.iter().map(|i| visible_box(i)).reduce(union) else {
            return Some(step);
        };
        let mut covering = Vec::new();
        for item in data
            .cap
            .items
            .iter()
            .filter(|i| !matches!(i.kind, PaintKind::Clip))
        {
            if item.op == op_ix || !overlaps(&visible_box(item), &area) {
                continue;
            }
            if item.ix < first.ix {
                push_op(&mut step.paints_over, page, item.op);
            } else if item.ix > last.ix {
                push_op(&mut step.covered_by, page, item.op);
                covering.push(intersection(&visible_box(item), &area));
            }
        }
        step.paints_over.reverse();
        step.paints_over.truncate(MAX_PAINT_STACK);
        step.covered_by.truncate(MAX_PAINT_STACK);
        if area.area() > 0.0 {
            step.visible_fraction = (1.0 - union_area(&covering) / area.area()).clamp(0.0, 1.0);
        }
        Some(step)
    }

    /// The next operator after (`forward`) or before `op_ix` that paints something: the
    /// operator to jump to when stepping through "visible changes". `None` when there is none.
    pub fn next_visible_change(&self, page: PageIx, op_ix: u32, forward: bool) -> Option<u32> {
        let data = self.content.data(page)?;
        data.op(op_ix)?;
        let paints = |op: &&OpRecord| painted(data, op).next().is_some();
        if forward {
            data.cap
                .ops
                .iter()
                .skip(usize_from_u32(op_ix) + 1)
                .find(paints)
                .map(|o| o.ix)
        } else {
            data.cap
                .ops
                .iter()
                .take(usize_from_u32(op_ix))
                .rev()
                .find(paints)
                .map(|o| o.ix)
        }
    }

    /// True when `op_ix` changes no pixels: it paints nothing (graphics state, text state, path
    /// construction, clipping, marked content ...). The stepper skips these when asked to.
    /// `false` for an operator that does not exist.
    pub fn is_state_only(&self, page: PageIx, op_ix: u32) -> bool {
        self.content
            .data(page)
            .and_then(|data| Some(painted(data, data.op(op_ix)?).next().is_none()))
            .unwrap_or(false)
    }

    /// Number of operators the document runs, every page's together, in the order the pages
    /// are painted. The paint-order positions are `0..op_total()`.
    pub fn op_total(&self) -> u32 {
        self.content.op_total()
    }

    /// The paint-order position of operator `at` in the whole document (0 is the first operator
    /// of the first page that has any). `None` when there is no such operator.
    pub fn paint_order_index(&self, at: OpPosition) -> Option<u32> {
        self.content.data(at.page)?.op(at.ix)?;
        Some(self.content.op_start(at.page)? + at.ix)
    }

    /// The operator at paint-order position `index` of the whole document, the inverse of
    /// [`Document::paint_order_index`]. `None` past the last operator.
    pub fn op_at(&self, index: u32) -> Option<OpPosition> {
        let (page, start) = self.content.page_holding(index)?;
        Some(OpPosition::new(page, index - start))
    }

    /// The next operator after (`forward`) or before `from` that paints something, looking past
    /// the end of `from`'s page into the next (or previous) pages: where stepping through visible
    /// changes of the whole document goes. `None` when there is none.
    pub fn next_visible_change_in_document(&self, from: OpPosition, forward: bool) -> Option<OpPosition> {
        if let Some(ix) = self.next_visible_change(from.page, from.ix, forward) {
            return Some(OpPosition::new(from.page, ix));
        }
        let paints = |page: PageIx, ix: u32| !self.is_state_only(page, ix);
        let on = |page: PageIx| {
            let last = self.content.page(page)?.ops.last()?.ix;
            let (edge, inward) = if forward { (0, true) } else { (last, false) };
            let ix = if paints(page, edge) {
                edge
            } else {
                self.next_visible_change(page, edge, inward)?
            };
            Some(OpPosition::new(page, ix))
        };
        let pages = PageIx::try_from(self.pages.len()).unwrap_or(PageIx::MAX);
        if forward {
            (from.page.saturating_add(1)..pages).find_map(on)
        } else {
            (0..from.page.min(pages)).rev().find_map(on)
        }
    }
}

/// The items `op` painted, clips excluded.
fn painted<'a>(data: &'a PageData, op: &'a OpRecord) -> impl Iterator<Item = &'a PaintItem> {
    data.op_items(op)
        .iter()
        .filter(|i| !matches!(i.kind, PaintKind::Clip))
}

fn push_op(list: &mut Vec<NodeId>, page: PageIx, op: u32) {
    let node = NodeId::Op { page, ix: op };
    if list.last() != Some(&node) {
        list.push(node);
    }
}

/// An item's box cut to the clip it was painted through.
fn visible_box(item: &PaintItem) -> Rect {
    item.clip
        .map_or(item.bbox, |clip| intersection(&item.bbox, &clip))
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// The overlap of two boxes; a zero-sized box at `a`'s corner when they do not meet.
fn intersection(a: &Rect, b: &Rect) -> Rect {
    let (x0, y0) = (a.x0.max(b.x0), a.y0.max(b.y0));
    Rect {
        x0,
        y0,
        x1: a.x1.min(b.x1).max(x0),
        y1: a.y1.min(b.y1).max(y0),
    }
}

/// True when the boxes share area (touching edges do not count).
fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// Area of the union of `rects`, by sweeping the distinct x edges.
fn union_area(rects: &[Rect]) -> f64 {
    let mut xs: Vec<f64> = rects.iter().flat_map(|r| [r.x0, r.x1]).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs.windows(2)
        .map(|pair| {
            let (left, right) = (pair[0], pair[1]);
            let mut spans: Vec<(f64, f64)> = rects
                .iter()
                .filter(|r| r.x0 <= left && r.x1 >= right && r.y1 > r.y0)
                .map(|r| (r.y0, r.y1))
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut covered = 0.0;
            let mut reach = f64::NEG_INFINITY;
            for (y0, y1) in spans {
                let from = y0.max(reach);
                if y1 > from {
                    covered += y1 - from;
                }
                reach = reach.max(y1);
            }
            covered * (right - left)
        })
        .sum()
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the operands are exactly representable and the arithmetic exact"
)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn union_area_counts_overlaps_once() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 15.0, 15.0);

        assert_eq!(union_area(&[a]), 100.0);
        assert_eq!(union_area(&[a, b]), 175.0);
        assert_eq!(union_area(&[a, a]), 100.0);
        assert_eq!(union_area(&[]), 0.0);
        assert_eq!(union_area(&[a, Rect::new(20.0, 0.0, 30.0, 10.0)]), 200.0);
    }

    #[test]
    fn touching_boxes_do_not_overlap() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);

        assert!(!overlaps(&a, &Rect::new(10.0, 0.0, 20.0, 10.0)));
        assert!(overlaps(&a, &Rect::new(9.9, 0.0, 20.0, 10.0)));
        assert_eq!(intersection(&a, &Rect::new(20.0, 20.0, 30.0, 30.0)).area(), 0.0);
    }
}
