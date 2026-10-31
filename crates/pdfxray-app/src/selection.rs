//! What a selected node means on the canvas.

use pdfxray_core::{Document, NodeId, PageIx, Rect};

/// A page-space rectangle to highlight on one page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionRect {
    /// Page the rectangle lives on.
    pub page: PageIx,
    /// The rectangle in page space.
    pub rect: Rect,
}

/// The page a node belongs to, for nodes that live on one.
pub fn node_page(id: &NodeId) -> Option<PageIx> {
    match id {
        NodeId::Page(page)
        | NodeId::PageBox { page, .. }
        | NodeId::MarkedContent { page, .. }
        | NodeId::Artifact { page, .. }
        | NodeId::Op { page, .. }
        | NodeId::Glyph { page, .. } => Some(*page),
        NodeId::Document
        | NodeId::Trailer
        | NodeId::Group(_)
        | NodeId::Object(_)
        | NodeId::Revision(_)
        | NodeId::StructElem(_)
        | NodeId::Resource { .. }
        | NodeId::Diagnostic(_)
        | NodeId::Validation(_) => None,
    }
}

/// The rectangle to outline on the canvas for `id`: whatever geometry the core knows for the
/// node (page boxes, operators, glyphs, marked content, artifacts, structure elements,
/// validation findings via their target). Pages themselves are not outlined.
pub fn selection_rect(doc: &Document, id: &NodeId) -> Option<SelectionRect> {
    if matches!(id, NodeId::Page(_)) {
        return None;
    }
    let (page, rect) = doc.bbox(id)?;
    Some(SelectionRect { page, rect })
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{BoxKind, ObjRef};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;

    #[test]
    fn page_scoped_nodes_know_their_page() {
        assert_eq!(node_page(&NodeId::Page(2)), Some(2));
        assert_eq!(
            node_page(&NodeId::PageBox {
                page: 1,
                kind: BoxKind::Crop
            }),
            Some(1)
        );
        assert_eq!(node_page(&NodeId::Op { page: 4, ix: 9 }), Some(4));
        assert_eq!(node_page(&NodeId::Document), None);
        assert_eq!(node_page(&NodeId::StructElem(ObjRef::new(5, 0))), None);
    }

    #[test]
    fn page_boxes_have_a_rectangle_and_everything_else_does_not_yet() {
        let doc = fixture("page-boxes");

        assert_eq!(
            selection_rect(
                &doc,
                &NodeId::PageBox {
                    page: 0,
                    kind: BoxKind::Crop
                }
            ),
            Some(SelectionRect {
                page: 0,
                rect: Rect::new(10.0, 10.0, 390.0, 290.0)
            })
        );
        assert_eq!(selection_rect(&doc, &NodeId::Page(0)), None);
        assert_eq!(
            selection_rect(
                &doc,
                &NodeId::PageBox {
                    page: 9,
                    kind: BoxKind::Crop
                }
            ),
            None
        );
    }
}
