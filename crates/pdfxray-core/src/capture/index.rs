//! Spatial index over the items of a captured page.

use rstar::{AABB, RTree, RTreeObject};

use super::types::{CapturedPage, PaintKind};
use crate::Point;

/// One indexed item.
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Paint order of the item (its `PaintItem::ix`).
    ix: u32,
    area: f64,
    clip: Option<crate::Rect>,
    min: [f64; 2],
    max: [f64; 2],
}

impl RTreeObject for Entry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(self.min, self.max)
    }
}

/// An R-tree over the bounding boxes of a page's painted items, for hit testing.
///
/// Clip items are not indexed: they are not painted. Items are only hit inside the clip that
/// was in effect when they were painted.
#[derive(Debug, Clone)]
pub struct PageIndex {
    tree: RTree<Entry>,
}

impl PageIndex {
    /// Builds the index for `page`.
    pub fn new(page: &CapturedPage) -> Self {
        let entries = page
            .items
            .iter()
            .filter(|item| !matches!(item.kind, PaintKind::Clip))
            .map(|item| Entry {
                ix: item.ix,
                area: item.bbox.area(),
                clip: item.clip,
                min: [item.bbox.x0, item.bbox.y0],
                max: [item.bbox.x1, item.bbox.y1],
            })
            .collect();
        Self {
            tree: RTree::bulk_load(entries),
        }
    }

    /// Items whose box contains `p`, smallest area first; among equal areas the item painted
    /// last (topmost) comes first. The values are [`PaintItem::ix`](super::PaintItem::ix).
    pub fn hit(&self, p: Point) -> Vec<u32> {
        let mut found: Vec<&Entry> = self
            .tree
            .locate_in_envelope_intersecting(AABB::from_point([p.x, p.y]))
            .filter(|e| e.clip.is_none_or(|c| c.contains(p)))
            .collect();
        found.sort_by(|a, b| a.area.total_cmp(&b.area).then(b.ix.cmp(&a.ix)));
        found.into_iter().map(|e| e.ix).collect()
    }
}
