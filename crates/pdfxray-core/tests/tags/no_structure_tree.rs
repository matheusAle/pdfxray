//! Documents without a structure tree.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn untagged_document_has_no_tags_tree_and_no_tag_diagnostics() {
    let doc = fixture("minimal-one-page");

    assert_eq!(doc.tree(TreeKind::Tags), Vec::<TreeNode>::new());
    assert_eq!(diagnostics(&doc), []);
    assert!(doc.node(&elem(7)).is_none());
    assert_eq!(doc.breadcrumb(&elem(7)), vec![NodeId::Document, elem(7)]);
    assert_eq!(doc.bbox(&elem(7)), None);
}
