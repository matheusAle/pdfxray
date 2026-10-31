//! The `images` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn image_operators_report_their_box_and_size() {
    let doc = open("images");
    let facts = expected("images");
    assert_eq!(facts.images.len(), 3);

    for fact in &facts.images {
        let p = op_props(&doc, fact.op_index, fact.stream);
        let b = fact.bbox;
        let label = format!(
            "{}Image {} \u{d7} {} px",
            if fact.name.is_none() { "Inline " } else { "" },
            fact.width,
            fact.height
        );

        assert_eq!(
            value(&p, "Geometry", "Bounding box"),
            fmt_rect(Rect::new(b[0], b[1], b[2], b[3]))
        );
        assert_eq!(value(&p, "Geometry", "Paints"), label);
        assert_eq!(value(&p, "Geometry", "Painted items"), "1");
    }
    let xobject = op_props(&doc, 2, None);
    assert_eq!(value(&xobject, "Operator", "Operator"), "Do");
    assert_eq!(value(&xobject, "Operator", "Operands"), "/Im1");
    let inline = op_props(&doc, 6, None);
    assert_eq!(value(&inline, "Operator", "Operator"), "BI");
}

#[test]
fn image_hit_selects_the_operator_without_a_glyph() {
    let doc = open("images");
    let on_image = Point::new(100.0, 200.0);

    let hits = doc.hit(0, on_image, LayerMode::Content);
    let NodeId::Op { ix, .. } = op_id(&doc, 2, None) else {
        panic!("not an op")
    };

    assert_eq!(hits, [NodeId::Op { page: 0, ix }, NodeId::Page(0)]);
    assert_eq!(fmt_rect(rect_of(&doc, &hits[0])), "[50 150 150 250]");
}
