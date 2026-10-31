//! What the pointer does on the canvas and what a node means there: hovering, cycling down the
//! hit stack, zooming to a node and the label shown above the selection.

use pdfxray_core::capture::PaintKind;
use pdfxray_core::numeric::{f32_from_f64, usize_from_u32};
use pdfxray_core::{LayerMode, NodeId, PageIx, Point, num};

use super::AppState;
use super::path::crumb_label;

/// Padding around a node when the canvas zooms to it: the node fills this fraction of the view.
const ZOOM_FILL: f64 = 1.0 / 1.4;

/// The text on the tag above the selected node's box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionLabel {
    /// The text, e.g. `Figure · MCID 9 · Do /Im1`.
    pub text: String,
    /// The node is a problem (untagged content): the tag is drawn in red.
    pub warn: bool,
}

impl AppState {
    /// Records the node under the pointer, a point on a page. `None` when the pointer is off
    /// every page.
    pub fn update_hover(&mut self, at: Option<(PageIx, Point)>) {
        let Some((page, point)) = at else {
            self.canvas.hover = None;
            return;
        };
        let layer = self.canvas.layer_mode;
        self.canvas.hover = self.document.as_ref().and_then(|loaded| {
            loaded
                .doc
                .hit(page, point, layer)
                .into_iter()
                .find(|id| match id {
                    NodeId::Page(_) => false,
                    NodeId::PageBox { .. } => layer == LayerMode::Boxes,
                    _ => true,
                })
        });
    }

    /// Shift-click: makes the topmost node under `point` the second selection, to measure
    /// against the first. The hit stack lists what is under the point as for a plain click.
    pub fn shift_click_page_point(&mut self, point: Point) {
        let Some(stack) = self.collect_hit_stack(point) else {
            return;
        };
        let top = stack.entries.first().map(|entry| entry.id.clone());
        self.drawer.show_hits(stack);
        if let Some(id) = top {
            self.select_additional(id);
        }
    }

    /// Option/Alt-click: goes down the hit stack under `point`, selecting the next node below
    /// the selected one and wrapping to the top after the last. A click somewhere else starts a
    /// new stack at its top.
    pub fn cycle_page_point(&mut self, point: Point) {
        let zoom = f64::from(self.canvas.zoom).max(f64::EPSILON);
        let tolerance = 2.0 / zoom;
        let same_spot = self.drawer.hit_stack.as_ref().is_some_and(|stack| {
            stack.page == self.canvas.page
                && (stack.point.x - point.x).abs() <= tolerance
                && (stack.point.y - point.y).abs() <= tolerance
        });
        if !same_spot {
            self.click_page_point(point);
            return;
        }
        let Some(stack) = &self.drawer.hit_stack else {
            return;
        };
        let at = self
            .selection
            .primary
            .as_ref()
            .and_then(|selected| stack.entries.iter().position(|e| &e.id == selected));
        let next = match at {
            Some(at) => (at + 1) % stack.entries.len(),
            None => 0,
        };
        if let Some(entry) = stack.entries.get(next) {
            let id = entry.id.clone();
            self.history.visit(&id);
            self.selection.primary = Some(id);
            self.selection.secondary = None;
            self.canvas.reveal = true;
        }
    }

    /// Selects `id` and zooms the canvas so its box fills the view (the page itself fits).
    pub fn zoom_to_node(&mut self, id: NodeId) {
        let target = self
            .document
            .as_ref()
            .and_then(|loaded| {
                let (page, rect) = loaded.doc.bbox(&id)?;
                let rotate = loaded.doc.page(page).map_or(0, |info| info.rotate);
                Some((rect, rotate))
            })
            .filter(|_| !matches!(id, NodeId::Page(_)));
        let is_page = matches!(id, NodeId::Page(_));
        self.select(id);
        if is_page {
            self.fit_page();
            return;
        }
        let Some((rect, rotate)) = target else {
            return;
        };
        let (width, height) = if rotate % 180 == 90 {
            (rect.height(), rect.width())
        } else {
            (rect.width(), rect.height())
        };
        let view = self.canvas.viewport;
        if width <= 0.0 || height <= 0.0 || view.x <= 0.0 || view.y <= 0.0 {
            return;
        }
        let zoom = (f64::from(view.x) * ZOOM_FILL / width).min(f64::from(view.y) * ZOOM_FILL / height);
        // Zoom is clamped to 0.1..=8 right away.
        self.set_zoom(f32_from_f64(zoom));
    }

    /// True when `op` of the page on the canvas paints something outside every marked-content
    /// sequence.
    pub fn is_untagged_op(&mut self, op: u32) -> bool {
        let page = self.canvas.page;
        let paints = self
            .document
            .as_ref()
            .and_then(|loaded| loaded.doc.captured(page))
            .and_then(|captured| captured.ops.get(usize_from_u32(op)))
            .is_some_and(|record| record.ix == op && record.item_count > 0);
        paints
            && self
                .page_stats(page)
                .is_some_and(|stats| stats.coverage.untagged_ops.binary_search(&op).is_ok())
    }

    /// The tag over the selected node's box, `None` when the node has no box or no label.
    pub fn selection_label(&mut self) -> Option<SelectionLabel> {
        let selected = self.selection.primary.clone()?;
        let label = |text: String, warn: bool| Some(SelectionLabel { text, warn });
        match &selected {
            NodeId::Glyph { page, op, glyph } => {
                let info = self.document.as_ref()?.doc.glyph_info(&NodeId::Glyph {
                    page: *page,
                    op: *op,
                    glyph: *glyph,
                })?;
                let shown = info.unicode.as_deref().filter(|u| !u.is_empty()).unwrap_or("?");
                let points: Vec<String> = info.code_points.iter().map(|c| format!("U+{c:04X}")).collect();
                let mut parts = vec![format!("'{shown}' {}", points.join(" ")).trim_end().to_owned()];
                if let Some(font) = &info.font
                    && let Some(resource) = &font.resource
                {
                    parts.push(format!("/{resource} {} pt", num(info.font_size)));
                }
                parts.push(format!("w {}", num(info.width_units)));
                label(parts.join(" · "), false)
            }
            NodeId::Op { page, ix } if *page == self.canvas.page && self.is_untagged_op(*ix) => {
                label(format!("⚠ untagged · op #{ix} · not in an artifact"), true)
            }
            NodeId::Op { .. } | NodeId::MarkedContent { .. } | NodeId::Artifact { .. } => {
                let path = self.path_to(&selected);
                let doc = &self.document.as_ref()?.doc;
                let parts: Vec<String> = path
                    .iter()
                    .filter(|id| !matches!(id, NodeId::Page(_)))
                    .map(|id| match id {
                        NodeId::StructElem(_) => doc.node(id).map_or_else(|| id.to_string(), |p| p.title),
                        other => crumb_label(doc, other),
                    })
                    .collect();
                (!parts.is_empty()).then(|| SelectionLabel {
                    text: parts.join(" · "),
                    warn: false,
                })
            }
            NodeId::StructElem(object) => {
                let title = self.document.as_ref()?.doc.node(&selected)?.title;
                label(format!("{title} · {object}"), false)
            }
            NodeId::PageBox { .. } | NodeId::Validation(_) => {
                let title = self.document.as_ref()?.doc.node(&selected)?.title;
                label(title, false)
            }
            _ => None,
        }
    }

    /// The boxes of the glyphs of the text operator behind the selection (a glyph or the
    /// operator itself), with the index of the selected glyph. Empty for other nodes.
    pub fn selected_glyph_boxes(&self) -> (Vec<pdfxray_core::Rect>, Option<usize>) {
        let (page, op, selected) = match self.selection.primary.as_ref() {
            Some(NodeId::Glyph { page, op, glyph }) => (*page, *op, Some(*glyph)),
            Some(NodeId::Op { page, ix }) => (*page, *ix, None),
            _ => return (Vec::new(), None),
        };
        let Some(captured) = self.document.as_ref().and_then(|l| l.doc.captured(page)) else {
            return (Vec::new(), None);
        };
        let Some(record) = captured.ops.get(usize_from_u32(op)).filter(|r| r.ix == op) else {
            return (Vec::new(), None);
        };
        let Some(first) = record.first_item.and_then(|f| usize::try_from(f).ok()) else {
            return (Vec::new(), None);
        };
        let mut boxes = Vec::new();
        let mut chosen = None;
        for item in captured
            .items
            .iter()
            .skip(first)
            .take(usize_from_u32(record.item_count))
        {
            if let PaintKind::Glyph { glyph_index, .. } = item.kind {
                if Some(glyph_index) == selected {
                    chosen = Some(boxes.len());
                }
                boxes.push(item.bbox);
            }
        }
        (boxes, chosen)
    }
}
