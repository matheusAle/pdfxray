//! The `artifacts` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn artifacts_tree_groups_by_page_in_paint_order() {
    let doc = open("artifacts");
    let facts = expected("artifacts");

    let tree = doc.tree(TreeKind::Artifacts);

    assert_eq!(labels(&tree), ["Page 1"]);
    assert_eq!(tree[0].id, NodeId::Page(0));
    assert_eq!(
        labels(&tree[0].children),
        [
            "Artifact /Pagination /Header",
            "Artifact /Layout",
            "Artifact /Pagination /Footer",
            "Artifact"
        ]
    );
    assert_eq!(
        tree[0].children.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
        [artifact(0), artifact(1), artifact(2), artifact(3)]
    );
    assert_eq!(
        tree[0].children[0].detail.as_deref(),
        Some("[20 177.8 77.26 187.18]")
    );
    assert_eq!(tree[0].children.len(), facts.artifacts.len());
}

#[test]
fn artifacts_tree_matches_the_generator_facts() {
    let doc = open("artifacts");
    let facts = expected("artifacts");

    let from_facts: Vec<String> = facts
        .artifacts
        .iter()
        .map(|a| {
            let parts: Vec<String> = [&a.artifact_type, &a.subtype]
                .into_iter()
                .flatten()
                .map(|part| format!(" /{part}"))
                .collect();
            format!("Artifact{}", parts.concat())
        })
        .collect();

    let tree = doc.tree(TreeKind::Artifacts);

    assert_eq!(labels(&tree[0].children), from_facts);
    assert_eq!(from_facts.len(), 4);
}

#[test]
fn artifact_hit_on_the_header_text_with_artifacts_layer() {
    let doc = open("artifacts");
    // "Page header" is 10 pt at (20, 180).
    let on_header = Point::new(25.0, 182.0);

    let hits = doc.hit(0, on_header, LayerMode::Artifacts);
    let content = doc.hit(0, on_header, LayerMode::Content);
    let tags = doc.hit(0, on_header, LayerMode::Tags);

    assert_eq!(hits, [artifact(0), NodeId::Page(0)]);
    // The page-border stroke of the last artifact (op 29) covers the whole page, text included.
    assert_eq!(
        content,
        [
            glyph(4, 0),
            NodeId::Op { page: 0, ix: 4 },
            NodeId::Op { page: 0, ix: 29 },
            NodeId::Page(0)
        ]
    );
    assert_eq!(tags, [NodeId::Page(0)]);
    assert_eq!(doc.hit(0, on_header, LayerMode::Annotations), [NodeId::Page(0)]);
}

#[test]
fn artifact_props_report_type_subtype_and_text() {
    let doc = open("artifacts");
    let header = props(&doc, &artifact(0));
    let layout = props(&doc, &artifact(1));
    let bare = props(&doc, &artifact(3));

    assert_eq!(header.kind, "artifact");
    assert_eq!(header.title, "Artifact /Pagination /Header");
    assert_eq!(value(&header, "Artifact", "Tag"), "Artifact");
    assert_eq!(value(&header, "Artifact", "Sequence"), "0");
    assert_eq!(value(&header, "Artifact", "Type"), "/Pagination");
    assert_eq!(value(&header, "Artifact", "Subtype"), "/Header");
    assert_eq!(value(&header, "Artifact", "Text"), "Page header");
    assert_eq!(value(&header, "Artifact", "Glyphs"), "11");
    assert_eq!(value(&header, "Artifact", "Begin operator"), "BDC (op 0)");
    assert_eq!(
        link_of(&header, "Artifact", "End operator"),
        Some(NodeId::Op { page: 0, ix: 6 })
    );
    assert_eq!(value(&layout, "Artifact", "Type"), "/Layout");
    assert_eq!(layout.get("Artifact", "Subtype"), None);
    assert_eq!(value(&layout, "Artifact", "Glyphs"), "0");
    assert_eq!(bare.get("Artifact", "Type"), None);
    assert_eq!(value(&bare, "Artifact", "Begin operator"), "BMC (op 26)");
    assert_eq!(value(&bare, "Artifact", "Bounding box"), "[4.5 4.5 195.5 195.5]");
}

#[test]
fn artifact_bbox_and_breadcrumb() {
    let doc = open("artifacts");

    let rect = rect_of(&doc, &artifact(2));

    assert_eq!(fmt_rect(rect), "[20 7.8 71.7 17.28]");
    assert_eq!(
        doc.breadcrumb(&artifact(2)),
        [NodeId::Document, NodeId::Page(0), artifact(2)]
    );
    assert!(doc.bbox(&artifact(9)).is_none());
}
