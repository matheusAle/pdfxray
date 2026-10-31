//! Hit testing and breadcrumbs through the `Document` API on all fixtures.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn every_struct_node_of_every_fixture_has_props_a_breadcrumb_and_resolves() {
    let mut seen_nodes = 0;
    let mut tagged = 0;
    for fixture_def in pdfxray_fixtures::all() {
        let doc = fixture(fixture_def.name);
        let top = doc.tree(TreeKind::Tags);
        if top.is_empty() {
            continue;
        }
        tagged += 1;
        let mut stack = top;
        while let Some(node) = stack.pop() {
            stack.extend(node.children.iter().cloned());
            if let NodeId::StructElem(_) = node.id {
                seen_nodes += 1;
                let p = doc
                    .node(&node.id)
                    .unwrap_or_else(|| panic!("{} {}", fixture_def.name, node.id));
                assert_eq!(p.id, node.id);
                assert_eq!(doc.breadcrumb(&node.id).last(), Some(&node.id));
            }
        }
        for b in doc.info().diagnostics.iter().zip(0..) {
            assert!(
                doc.node(&NodeId::Diagnostic(b.1)).is_some(),
                "{}",
                fixture_def.name
            );
        }
    }
    assert!(tagged >= 8, "tagged fixtures: {tagged}");
    assert!(seen_nodes > 250, "struct nodes: {seen_nodes}");
}

#[test]
fn marked_content_of_struct_elem_collects_mcids_in_k_order() {
    let doc = fixture("tagged-simple");
    let root = doc.tree(pdfxray_core::TreeKind::Tags);
    let document = root[0].children[0].id.clone();

    assert_eq!(
        doc.marked_content(&document),
        vec![
            pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 0 },
            pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 1 },
        ]
    );
    let mc = pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 1 };
    assert_eq!(doc.marked_content(&mc), vec![mc.clone()]);
    assert_eq!(doc.marked_content(&pdfxray_core::NodeId::Page(0)), Vec::new());
}
