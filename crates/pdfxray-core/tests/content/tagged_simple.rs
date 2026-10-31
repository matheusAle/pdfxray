//! The `tagged-simple` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn tagged_tree_nests_operators_under_their_bdc() {
    let doc = open("tagged-simple");

    let tree = doc.tree(TreeKind::Content(0));

    assert_eq!(labels(&tree), ["BDC", "BDC"]);
    assert_eq!(
        tree[0].detail.as_deref(),
        Some("H1 MCID 0 \u{b7} /H1 << /MCID 0 >>")
    );
    assert_eq!(
        tree[1].detail.as_deref(),
        Some("P MCID 1 \u{b7} /P << /MCID 1 >>")
    );
    assert_eq!(labels(&tree[0].children), ["BT", "Tf", "Td", "Tj", "ET", "EMC"]);
    assert_eq!(count(&tree), doc.captured(0).unwrap().ops.len());
}

#[test]
fn tagged_hit_in_heading_with_tags_layer_finds_its_mcid() {
    let doc = open("tagged-simple");
    // The 24 pt heading starts at (20, 150).
    let in_heading = Point::new(30.0, 156.0);

    let hits = doc.hit(0, in_heading, LayerMode::Tags);

    assert_eq!(hits.first(), Some(&mc(0)));
    assert_eq!(hits.last(), Some(&NodeId::Page(0)));
    assert!(!hits.contains(&mc(1)), "{hits:?}");
    assert!(
        !hits
            .iter()
            .any(|h| matches!(h, NodeId::Glyph { .. } | NodeId::Op { .. }))
    );
}

#[test]
fn tagged_content_hit_lists_glyph_operator_then_marked_content() {
    let doc = open("tagged-simple");

    let hits = doc.hit(0, Point::new(30.0, 156.0), LayerMode::Content);
    let all = doc.hit(0, Point::new(30.0, 156.0), LayerMode::All);

    assert_eq!(
        hits,
        [glyph(4, 0), NodeId::Op { page: 0, ix: 4 }, NodeId::Page(0)]
    );
    assert_eq!(&all[..3], &[glyph(4, 0), NodeId::Op { page: 0, ix: 4 }, mc(0)]);
    assert_eq!(all.iter().filter(|h| **h == mc(0)).count(), 1);
}

#[test]
fn tagged_marked_content_props_carry_text_and_operators() {
    let doc = open("tagged-simple");
    let p = props(&doc, &mc(0));

    assert_eq!(p.kind, "marked-content");
    assert_eq!(p.title, "H1 MCID 0");
    assert_eq!(value(&p, "Marked content", "Tag"), "H1");
    assert_eq!(value(&p, "Marked content", "MCID"), "0");
    assert_eq!(value(&p, "Marked content", "Text"), "Heading");
    assert_eq!(value(&p, "Marked content", "Glyphs"), "7");
    assert_eq!(value(&p, "Marked content", "Begin operator"), "BDC (op 0)");
    assert_eq!(value(&p, "Marked content", "End operator"), "EMC (op 6)");
    assert_eq!(
        link_of(&p, "Marked content", "Begin operator"),
        Some(NodeId::Op { page: 0, ix: 0 })
    );
    assert_eq!(value(&p, "Properties", "/MCID"), "0");
    assert_eq!(p.get("Marked content", "Occurrences"), None);
    let second = props(&doc, &mc(1));
    assert_eq!(value(&second, "Marked content", "Text"), "Body paragraph");
}

#[test]
fn tagged_marked_content_bbox_covers_its_glyphs() {
    let doc = open("tagged-simple");
    let heading = rect_of(&doc, &mc(0));
    let body = rect_of(&doc, &mc(1));

    let first_glyph = rect_of(&doc, &glyph(4, 0));
    let last_glyph = rect_of(&doc, &glyph(4, 6));

    assert!((heading.x0 - 20.0).abs() < 1e-3, "{heading:?}");
    assert!(heading.contains_rect(&first_glyph) && heading.contains_rect(&last_glyph));
    assert!(
        body.y1 < heading.y0,
        "body {body:?} lies below heading {heading:?}"
    );
    assert_eq!(doc.bbox(&mc(0)).map(|b| b.0), Some(0));
}

#[test]
fn tagged_ancestors_of_a_glyph_are_its_marked_content_then_operator() {
    let doc = open("tagged-simple");

    let path = doc.breadcrumb(&glyph(4, 2));

    assert_eq!(
        path,
        [
            NodeId::Document,
            NodeId::Page(0),
            mc(0),
            NodeId::Op { page: 0, ix: 4 },
            glyph(4, 2)
        ]
    );
    assert_eq!(doc.breadcrumb(&mc(1)), [NodeId::Document, NodeId::Page(0), mc(1)]);
}

#[test]
fn begin_and_enclosed_operators_link_to_the_sequence() {
    let doc = open("tagged-simple");

    let bdc = op_props(&doc, 0, None);
    let tj = op_props(&doc, 4, None);

    assert_eq!(link_of(&bdc, "Marked content", "Begins"), Some(mc(0)));
    assert_eq!(value(&bdc, "Marked content", "Begins"), "H1 MCID 0");
    assert_eq!(link_of(&tj, "Marked content", "Marked content"), Some(mc(0)));
    assert_eq!(
        rect_of(&doc, &NodeId::Op { page: 0, ix: 0 }),
        rect_of(&doc, &mc(0))
    );
}
