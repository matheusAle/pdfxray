//! The `minimal-one-page` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn minimal_content_tree_lists_the_operators_in_order() {
    let doc = open("minimal-one-page");

    let tree = doc.tree(TreeKind::Content(0));

    assert_eq!(labels(&tree), ["BT", "Tf", "Td", "Tj", "ET"]);
    assert_eq!(
        tree.iter().map(|n| n.detail.as_deref()).collect::<Vec<_>>(),
        [None, Some("/F1 18"), Some("20 40"), Some("(Hello pdfxray)"), None]
    );
    assert_eq!(tree[3].id, NodeId::Op { page: 0, ix: 3 });
    assert_eq!(count(&tree), doc.captured(0).unwrap().ops.len());
}

#[test]
fn minimal_hit_on_the_h_glyph_returns_glyph_then_operator() {
    let doc = open("minimal-one-page");
    // Helvetica 18 pt "H" starts at (20, 40) and is about 13 pt wide and 13 pt tall.
    let on_h = Point::new(25.0, 46.0);

    let content = doc.hit(0, on_h, LayerMode::Content);
    let all = doc.hit(0, on_h, LayerMode::All);

    assert_eq!(
        content,
        [glyph(3, 0), NodeId::Op { page: 0, ix: 3 }, NodeId::Page(0)]
    );
    assert_eq!(&all[..2], &[glyph(3, 0), NodeId::Op { page: 0, ix: 3 }]);
    assert_eq!(all.last(), Some(&NodeId::Page(0)));
    assert!(all.len() > content.len(), "All adds the page boxes: {all:?}");
}

#[test]
fn minimal_hit_between_glyph_boxes_misses_content() {
    let doc = open("minimal-one-page");

    let far = doc.hit(0, Point::new(190.0, 95.0), LayerMode::Content);

    assert_eq!(far, [NodeId::Page(0)]);
}

#[test]
fn minimal_glyph_props_describe_the_h() {
    let doc = open("minimal-one-page");
    let p = props(&doc, &glyph(3, 0));

    assert_eq!(p.kind, "glyph");
    assert_eq!(p.title, "Glyph \"H\"");
    assert_eq!(value(&p, "Glyph", "Unicode"), "H");
    assert_eq!(value(&p, "Glyph", "Code point"), "U+0048");
    assert_eq!(value(&p, "Glyph", "Char code"), "72 (0x48)");
    assert_eq!(value(&p, "Glyph", "Glyph index"), "0");
    assert_eq!(value(&p, "Glyph", "Origin"), "(20, 40)");
    assert_eq!(value(&p, "Text state", "Font"), "F1");
    assert_eq!(value(&p, "Text state", "Font size"), "18");
    assert_eq!(value(&p, "Text state", "Effective size"), "18");
    assert_eq!(value(&p, "Text state", "Render mode"), "0 (fill)");
    assert_eq!(
        link_of(&p, "Context", "Operator"),
        Some(NodeId::Op { page: 0, ix: 3 })
    );
    // The advance of an Helvetica "H" is 722/1000 em.
    let advance: f64 = value(&p, "Glyph", "Advance").parse().unwrap();
    assert!((advance - 0.722 * 18.0).abs() < 1e-3, "{advance}");
}

#[test]
fn minimal_breadcrumb_runs_document_page_operator_glyph() {
    let doc = open("minimal-one-page");

    let path = doc.breadcrumb(&glyph(3, 0));

    assert_eq!(
        path,
        [
            NodeId::Document,
            NodeId::Page(0),
            NodeId::Op { page: 0, ix: 3 },
            glyph(3, 0)
        ]
    );
    assert_eq!(path.last(), Some(&glyph(3, 0)));
}

#[test]
fn minimal_text_operator_lists_its_glyphs_and_text() {
    let doc = open("minimal-one-page");
    let p = op_props(&doc, 3, None);

    assert_eq!(value(&p, "Operator", "Operator"), "Tj");
    assert_eq!(value(&p, "Operator", "Operands"), "(Hello pdfxray)");
    assert_eq!(value(&p, "Operator", "Stream"), "Page content #0");
    assert_eq!(value(&p, "Text state", "Text"), "Hello pdfxray");
    assert_eq!(value(&p, "Text state", "Font size"), "18");
    assert_eq!(value(&p, "Geometry", "Painted items"), "13");
    assert_eq!(value(&p, "Geometry", "Paints"), "Glyph \u{d7}13");
    let glyphs = &p.groups.iter().find(|g| g.name == "Painted glyphs").unwrap().rows;
    assert_eq!(glyphs.len(), 13);
    assert_eq!(glyphs[0].value, "H");
    assert_eq!(glyphs[0].link, Some(glyph(3, 0)));
    assert_eq!(glyphs[12].link, Some(glyph(3, 12)));
    // `Tf` sits in the same text-state group but shows no text.
    let tf = op_props(&doc, 1, None);
    assert_eq!(tf.get("Text state", "Text"), None);
    assert_eq!(value(&tf, "Text state", "Font size"), "18");
}

#[test]
fn minimal_text_operator_font_links_to_the_page_resource() {
    let doc = open("minimal-one-page");
    let p = op_props(&doc, 3, None);
    let page_obj = doc.page(0).unwrap().resources_from.unwrap();

    let link = link_of(&p, "Text state", "Font");

    assert_eq!(
        link,
        Some(NodeId::Resource {
            owner: page_obj,
            kind: pdfxray_core::ResKind::Font,
            name: "F1".into()
        })
    );
    assert!(link_of(&p, "Text state", "Font object").is_some());
}
