//! The `untagged-content` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn untagged_duplicate_mcid_reports_occurrences() {
    let doc = open("untagged-content");
    let facts = expected("untagged-content");
    assert!(facts.defects.iter().any(|d| matches!(
        d,
        pdfxray_fixtures::Defect::DuplicateMcid {
            mcid: 1,
            count: 2,
            ..
        }
    )));

    let dup = props(&doc, &mc(1));
    let single = props(&doc, &mc(0));

    assert_eq!(value(&dup, "Marked content", "Occurrences"), "2");
    assert_eq!(value(&dup, "Marked content", "Text"), "Tagged twoTagged again");
    assert_eq!(value(&dup, "Marked content", "Glyphs"), "22");
    assert_eq!(value(&dup, "Marked content", "Begin operator"), "BDC (op 19)");
    let occurrences = &dup.groups.iter().find(|g| g.name == "Occurrences").unwrap().rows;
    assert_eq!(
        occurrences.iter().map(|r| r.link.clone()).collect::<Vec<_>>(),
        [
            Some(NodeId::Op { page: 0, ix: 19 }),
            Some(NodeId::Op { page: 0, ix: 29 })
        ]
    );
    assert_eq!(single.get("Marked content", "Occurrences"), None);
    assert!(single.groups.iter().all(|g| g.name != "Occurrences"));
}

#[test]
fn untagged_duplicate_mcid_bbox_unites_both_sequences() {
    let doc = open("untagged-content");

    let both = rect_of(&doc, &mc(1));
    let first = rect_of(&doc, &NodeId::Op { page: 0, ix: 19 });
    let second = rect_of(&doc, &NodeId::Op { page: 0, ix: 29 });

    assert_eq!(fmt_rect(both), "[20 57.36 93.38 108.616]");
    assert!(both.contains_rect(&first) && both.contains_rect(&second));
    assert!(first.y0 > second.y1, "the occurrences are on different lines");
}

#[test]
fn untagged_tree_keeps_untagged_operators_at_the_top_level() {
    let doc = open("untagged-content");

    let tree = doc.tree(TreeKind::Content(0));

    assert_eq!(
        labels(&tree),
        [
            "BDC", "BDC", "BT", "Tf", "Td", "Tj", "ET", "BDC", "g", "re", "f", "BDC", "BMC"
        ]
    );
    // The span nested in the artifact keeps its place.
    let artifact_bmc = tree.last().unwrap();
    assert_eq!(artifact_bmc.detail.as_deref(), Some("Artifact"));
    assert_eq!(labels(&artifact_bmc.children), ["BDC", "EMC"]);
    assert_eq!(count(&tree), doc.captured(0).unwrap().ops.len());
}

#[test]
fn nested_marked_content_lists_all_enclosing_nodes_outermost_first() {
    let doc = open("untagged-content");
    let tj = NodeId::Op { page: 0, ix: 41 };

    let path = doc.breadcrumb(&tj);
    let props = props(&doc, &tj);

    assert_eq!(
        path,
        [NodeId::Document, NodeId::Page(0), artifact(1), mc(2), tj.clone()]
    );
    let rows: Vec<(&str, &str)> = props
        .groups
        .iter()
        .find(|g| g.name == "Marked content")
        .unwrap()
        .rows
        .iter()
        .map(|r| (r.key.as_str(), r.value.as_str()))
        .collect();
    assert_eq!(
        rows,
        [("Marked content", "Span MCID 2"), ("Artifact", "Artifact")]
    );
    // A glyph in the span hits innermost first: the span, then the artifact.
    let hits = doc.hit(0, centre(rect_of(&doc, &glyph(41, 0))), LayerMode::All);
    assert_eq!(&hits[..4], &[glyph(41, 0), tj, mc(2), artifact(1)]);
}

#[test]
fn untagged_operator_outside_marked_content_has_no_marked_group() {
    let doc = open("untagged-content");

    let untagged = op_props(&doc, 17, None);

    assert!(untagged.groups.iter().all(|g| g.name != "Marked content"));
    assert_eq!(value(&untagged, "Text state", "Text"), "Untagged text");
    assert_eq!(
        doc.breadcrumb(&NodeId::Op { page: 0, ix: 17 }),
        [NodeId::Document, NodeId::Page(0), NodeId::Op { page: 0, ix: 17 }]
    );
}
