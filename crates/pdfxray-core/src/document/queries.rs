//! The navigation queries: trees, inspector properties, hit testing, breadcrumbs and bounding
//! boxes.

use std::collections::HashSet;

use super::Document;
use crate::numeric::usize_from_u32;
use crate::objects::ObjectIndex;
use crate::page::PageInfo;
use crate::{Badge, LayerMode, NodeId, PageIx, Point, PropGroup, Props, Rect, Severity, TreeKind, TreeNode};

impl Document {
    /// Builds a navigation tree. Kinds not implemented yet return an empty list.
    pub fn tree(&self, kind: TreeKind) -> Vec<TreeNode> {
        let mut roots = match kind {
            TreeKind::Pages => self.pages.iter().map(PageInfo::tree_node).collect(),
            TreeKind::Objects | TreeKind::Tags | TreeKind::Resources => self.cached_tree(kind).to_vec(),
            TreeKind::Diagnostics => self.diagnostics_tree(),
            TreeKind::Content(page) => self.content_tree(page),
            TreeKind::Artifacts => self.artifacts_tree(),
        };
        if let Some(run) = &self.validation {
            let targets: HashSet<&NodeId> = run.findings.iter().filter_map(|f| f.target.as_ref()).collect();
            for root in &mut roots {
                add_validation_badges(root, &targets);
            }
        }
        roots
    }

    /// Inspector properties for `id`, or `None` if the node does not exist.
    pub fn node(&self, id: &NodeId) -> Option<Props> {
        match id {
            NodeId::Document => Some(self.document_props()),
            NodeId::Page(p) => self.page(*p).map(PageInfo::props),
            NodeId::PageBox { page, kind } => self.page(*page)?.box_props(*kind),
            NodeId::Diagnostic(i) => self.diagnostic_props(*i),
            NodeId::Trailer => ObjectIndex::trailer_props(&self.raw),
            NodeId::Revision(i) => ObjectIndex::revision_props(&self.raw, *i),
            NodeId::Object(r) => {
                let diags: Vec<NodeId> = self
                    .diagnostics
                    .iter()
                    .zip(0..)
                    .filter(|(d, _)| d.object == Some(*r))
                    .map(|(_, i)| NodeId::Diagnostic(i))
                    .collect();
                self.objects.object_props(&self.raw, *r, &diags)
            }
            NodeId::Validation(i) => self.validation.as_ref()?.props(*i),
            NodeId::Op { .. }
            | NodeId::Glyph { .. }
            | NodeId::MarkedContent { .. }
            | NodeId::Artifact { .. } => self.content_props(id),
            NodeId::StructElem(_) => self.tag_props(id),
            NodeId::Resource { .. } => self.resource_props(id),
            NodeId::Group(_) => None,
        }
    }

    /// Everything under `point` (page space) on `page` that `layer` allows, most specific
    /// first. The page itself is always last.
    pub fn hit(&self, page: PageIx, point: Point, layer: LayerMode) -> Vec<NodeId> {
        let Some(info) = self.page(page) else {
            return Vec::new();
        };
        let mut hits = self.content_hits(page, point, layer);
        if matches!(layer, LayerMode::All | LayerMode::Tags) {
            let elems = self.tag_hits(&hits);
            hits.extend(elems);
        }
        if layer == LayerMode::Tags {
            hits.retain(|h| matches!(h, NodeId::MarkedContent { .. } | NodeId::StructElem(_)));
        }
        if matches!(layer, LayerMode::All | LayerMode::Boxes) {
            let mut boxes: Vec<_> = info
                .defined_boxes()
                .filter(|(_, rect)| rect.contains(point))
                .collect();
            boxes.sort_by(|a, b| a.1.area().total_cmp(&b.1.area()));
            hits.extend(boxes.into_iter().map(|(kind, _)| NodeId::PageBox { page, kind }));
        }
        hits.push(NodeId::Page(page));
        hits
    }

    /// Path from the document root down to `id`, inclusive.
    pub fn breadcrumb(&self, id: &NodeId) -> Vec<NodeId> {
        let mut path = vec![NodeId::Document];
        match id {
            NodeId::Document => return path,
            NodeId::PageBox { page, .. } => path.push(NodeId::Page(*page)),
            NodeId::Op { page, .. }
            | NodeId::Glyph { page, .. }
            | NodeId::MarkedContent { page, .. }
            | NodeId::Artifact { page, .. } => {
                path.push(NodeId::Page(*page));
                path.extend(self.content_ancestors(id));
            }
            NodeId::StructElem(_) => path.extend(self.tag_ancestors(id)),
            _ => {}
        }
        path.push(id.clone());
        path
    }

    /// Where `id` sits on a page: its page and bounding box in page space. `None` for nodes
    /// without geometry (objects, revisions, diagnostics).
    pub fn bbox(&self, id: &NodeId) -> Option<(PageIx, Rect)> {
        match id {
            NodeId::Page(p) => Some((*p, self.page(*p)?.crop_box)),
            NodeId::PageBox { page, kind } => Some((*page, self.page(*page)?.page_box(*kind)?.rect)),
            NodeId::Op { .. }
            | NodeId::Glyph { .. }
            | NodeId::MarkedContent { .. }
            | NodeId::Artifact { .. } => self.content_bbox(id),
            NodeId::StructElem(_) => self.tag_bbox(id),
            NodeId::Validation(i) => self.bbox(
                self.validation
                    .as_ref()?
                    .findings
                    .get(usize_from_u32(*i))?
                    .target
                    .as_ref()?,
            ),
            _ => None,
        }
    }

    fn document_props(&self) -> Props {
        // Not `self.info()`: that would clone every diagnostic only to read four fields.
        let mut file = PropGroup::new("File")
            .row("Name", &self.name)
            .row("Size", format!("{} bytes", self.bytes.len()))
            .row("Header", format!("%PDF-{}", self.raw.header().version));
        if let Some(e) = self.engine_error() {
            file = file.row("Engine", format!("{e:?}"));
        }
        Props {
            id: NodeId::Document,
            title: self.name.clone(),
            kind: "document",
            groups: vec![
                file,
                PropGroup::new("Pages").row("Count", self.pages.len().to_string()),
            ],
        }
    }
}

/// Marks every node that a validation finding points at with a `validation` error badge.
fn add_validation_badges(node: &mut TreeNode, targets: &HashSet<&NodeId>) {
    if !matches!(node.id, NodeId::Validation(_)) && targets.contains(&node.id) {
        node.badges.push(Badge {
            code: "validation".to_owned(),
            severity: Severity::Error,
        });
    }
    for child in &mut node.children {
        add_validation_badges(child, targets);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;
    use crate::{BoxKind, Rect};

    #[test]
    fn pages_tree_lists_pages_with_boxes() {
        let doc = fixture("minimal-one-page");

        let tree = doc.tree(TreeKind::Pages);
        let labels: Vec<_> = tree.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["Page 1"]);
        let page = &tree[0];
        assert_eq!(page.id, NodeId::Page(0));
        assert!(
            page.find(&NodeId::PageBox {
                page: 0,
                kind: BoxKind::Media
            })
            .is_some()
        );
    }

    #[test]
    fn hit_returns_content_then_boxes_smallest_first_then_page() {
        let doc = fixture("page-boxes");
        let crop = doc.page(0).unwrap().crop_box;
        let inside = Point::new(crop.x0 + 1.0, crop.y0 + 1.0);

        let boxes = doc.hit(0, inside, LayerMode::Boxes);
        assert_eq!(
            boxes,
            vec![
                NodeId::PageBox {
                    page: 0,
                    kind: BoxKind::Crop
                },
                NodeId::PageBox {
                    page: 0,
                    kind: BoxKind::Media
                },
                NodeId::Page(0),
            ]
        );
        let all = doc.hit(0, inside, LayerMode::All);
        assert_eq!(all.last(), Some(&NodeId::Page(0)));
        assert!(matches!(all.first(), Some(NodeId::Op { page: 0, .. })), "{all:?}");
        assert!(all.ends_with(&boxes));
        assert_eq!(doc.hit(0, inside, LayerMode::Tags), vec![NodeId::Page(0)]);
        assert_eq!(doc.hit(99, inside, LayerMode::All), Vec::<NodeId>::new());
    }

    #[test]
    fn node_props_for_page_and_box() {
        let doc = fixture("minimal-one-page");

        let page = doc.node(&NodeId::Page(0)).unwrap();
        assert_eq!(page.title, "Page 1");
        assert_eq!(page.get("Geometry", "MediaBox"), Some("[0 0 200 100]"));

        let media = doc
            .node(&NodeId::PageBox {
                page: 0,
                kind: BoxKind::Media,
            })
            .unwrap();
        assert_eq!(media.get("Geometry", "Width"), Some("200 pt"));
        assert_eq!(
            doc.page(0).map(|p| p.media_box),
            Some(Rect::new(0.0, 0.0, 200.0, 100.0))
        );
        assert!(doc.node(&NodeId::Page(5)).is_none());
    }

    #[test]
    fn breadcrumb_walks_from_document() {
        let doc = fixture("minimal-one-page");
        let leaf = NodeId::PageBox {
            page: 0,
            kind: BoxKind::Crop,
        };

        assert_eq!(
            doc.breadcrumb(&leaf),
            vec![NodeId::Document, NodeId::Page(0), leaf.clone()]
        );
    }
}
