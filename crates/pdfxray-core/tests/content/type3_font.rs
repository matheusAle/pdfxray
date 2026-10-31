//! The `type3-font` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn type3_text_operators_and_glyph_boxes_match_the_facts() {
    let doc = open("type3-font");
    let facts = expected("type3-font");
    assert_eq!(facts.type3_glyphs.len(), 3);

    for fact in &facts.text_ops {
        let p = op_props(&doc, fact.op_index, None);

        assert_eq!(value(&p, "Text state", "Font"), "T3");
        assert_eq!(value(&p, "Text state", "Font size"), num(fact.font_size));
        assert_eq!(
            value(&p, "Text state", "Effective size"),
            num(fact.effective_size)
        );
    }
    for g in &facts.type3_glyphs {
        let NodeId::Op { ix, .. } = op_id(&doc, g.op_index, None) else {
            panic!("not an op")
        };
        let p = props(&doc, &glyph(ix, g.glyph_index));
        let b = g.declared_bbox;

        assert_eq!(
            value(&p, "Glyph", "Char code"),
            format!("{} (0x{:X})", g.code, g.code)
        );
        assert_eq!(
            value(&p, "Glyph", "Bounding box"),
            fmt_rect(Rect::new(b[0], b[1], b[2], b[3]))
        );
        assert_eq!(value(&p, "Text state", "Font"), "T3");
    }
}

#[test]
fn type3_glyph_is_hit_and_its_origin_matches() {
    let doc = open("type3-font");
    let facts = expected("type3-font");
    let g = &facts.type3_glyphs[0];
    let NodeId::Op { ix, .. } = op_id(&doc, g.op_index, None) else {
        panic!("not an op")
    };
    let b = g.declared_bbox;

    let hits = doc.hit(0, Point::new(b[0] + 4.0, b[1] + 4.0), LayerMode::Content);

    assert_eq!(&hits[..2], &[glyph(ix, 0), NodeId::Op { page: 0, ix }]);
    assert_eq!(
        value(&props(&doc, &glyph(ix, 0)), "Glyph", "Origin"),
        format!("({}, {})", num(g.origin[0]), num(g.origin[1]))
    );
}
