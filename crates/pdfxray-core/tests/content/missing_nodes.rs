//! Queries on things that do not exist.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn missing_content_nodes_are_none() {
    let doc = open("minimal-one-page");

    assert!(doc.node(&NodeId::Op { page: 0, ix: 99 }).is_none());
    assert!(doc.node(&glyph(3, 99)).is_none());
    assert!(doc.node(&glyph(0, 0)).is_none(), "BT paints no glyph");
    assert!(doc.node(&mc(7)).is_none());
    assert!(doc.node(&artifact(0)).is_none());
    assert!(doc.node(&NodeId::Op { page: 5, ix: 0 }).is_none());
    assert_eq!(doc.tree(TreeKind::Content(5)), Vec::new());
    assert_eq!(doc.tree(TreeKind::Artifacts), Vec::new());
    assert!(
        doc.bbox(&NodeId::Op { page: 0, ix: 1 }).is_none(),
        "Tf paints nothing"
    );
}
