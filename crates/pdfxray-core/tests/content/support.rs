//! Helpers shared by the `content` tests.

use super::*;

pub(crate) fn open(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

pub(crate) fn expected(name: &str) -> Expected {
    serde_json::from_slice(&fs::read(fixtures_dir().join(format!("{name}.expected.json"))).unwrap()).unwrap()
}

pub(crate) fn props(doc: &Document, id: &NodeId) -> Props {
    doc.node(id).unwrap_or_else(|| panic!("no props for {id}"))
}

/// The operator at `local_ix` of the page's own content (`form` `None`) or of Form `XObject`
/// `form`, found through the capture so the tests do not depend on execution numbering.
pub(crate) fn op_id(doc: &Document, local_ix: u32, form: Option<u32>) -> NodeId {
    let page = doc.captured(0).unwrap();
    let op = page
        .ops
        .iter()
        .find(|o| {
            o.local_ix == local_ix
                && match (&o.stream, form) {
                    (StreamRef::PageContent { .. }, None) => true,
                    (StreamRef::FormXObject { obj, .. }, Some(n)) => *obj == Some(ObjRef::new(n, 0)),
                    _ => false,
                }
        })
        .unwrap_or_else(|| panic!("no operator {local_ix} in {form:?}"));
    NodeId::Op { page: 0, ix: op.ix }
}

pub(crate) fn op_props(doc: &Document, local_ix: u32, form: Option<u32>) -> Props {
    props(doc, &op_id(doc, local_ix, form))
}

pub(crate) fn labels(nodes: &[TreeNode]) -> Vec<&str> {
    nodes.iter().map(|n| n.label.as_str()).collect()
}

pub(crate) fn count(nodes: &[TreeNode]) -> usize {
    nodes.iter().map(TreeNode::len).sum()
}

pub(crate) fn value<'p>(p: &'p Props, group: &str, key: &str) -> &'p str {
    p.get(group, key)
        .unwrap_or_else(|| panic!("{} has no {group}/{key}", p.id))
}

pub(crate) fn link_of(p: &Props, group: &str, key: &str) -> Option<NodeId> {
    p.groups
        .iter()
        .find(|g| g.name == group)?
        .rows
        .iter()
        .find(|r| r.key == key)?
        .link
        .clone()
}

pub(crate) fn rect_of(doc: &Document, id: &NodeId) -> Rect {
    doc.bbox(id).unwrap_or_else(|| panic!("no bbox for {id}")).1
}

pub(crate) fn centre(r: Rect) -> Point {
    Point::new(f64::midpoint(r.x0, r.x1), f64::midpoint(r.y0, r.y1))
}

pub(crate) fn mc(mcid: i32) -> NodeId {
    NodeId::MarkedContent { page: 0, mcid }
}

pub(crate) fn artifact(seq: u32) -> NodeId {
    NodeId::Artifact { page: 0, seq }
}

pub(crate) fn glyph(op: u32, glyph: u32) -> NodeId {
    NodeId::Glyph { page: 0, op, glyph }
}
