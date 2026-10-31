//! The `form-xobject-nesting` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn form_ops_nest_under_the_do_that_drew_them() {
    let doc = open("form-xobject-nesting");

    let tree = doc.tree(TreeKind::Content(0));

    assert_eq!(labels(&tree), ["BDC", "Do"]);
    let do_a = &tree[1];
    assert_eq!(do_a.detail.as_deref(), Some("/FmA"));
    assert_eq!(
        labels(&do_a.children),
        ["BDC", "Do"],
        "form A: its tagged text, then form B"
    );
    let do_b = &do_a.children[1];
    assert_eq!(do_b.detail.as_deref(), Some("/FmB"));
    assert_eq!(labels(&do_b.children), ["BDC"]);
    assert_eq!(
        labels(&do_b.children[0].children),
        ["rg", "re", "f", "BT", "g", "Tf", "Td", "Tj", "ET", "EMC"]
    );
    assert_eq!(count(&tree), doc.captured(0).unwrap().ops.len());
}

#[test]
fn form_operator_ctm_includes_both_matrices() {
    let doc = open("form-xobject-nesting");
    let facts = expected("form-xobject-nesting");
    assert_eq!(facts.forms[0].matrix, Some([0.8, 0.0, 0.0, 0.8, 20.0, 20.0]));
    assert_eq!(facts.forms[1].matrix, Some([1.0, 0.0, 0.0, 1.0, 10.0, 10.0]));

    // "Form B" is shown by operator 8 of form B (object 7): FmB at (10, 10) inside FmA.
    let in_b = op_props(&doc, 8, Some(7));
    let in_a = op_props(&doc, 4, Some(6));
    let on_page = op_props(&doc, 4, None);

    assert_eq!(value(&in_b, "Operator", "Operands"), "(Form B)");
    assert_eq!(value(&in_b, "Graphics state", "CTM"), "[0.8 0 0 0.8 28 28]");
    assert_eq!(value(&in_b, "Text state", "Effective size"), "8");
    assert_eq!(value(&in_a, "Graphics state", "CTM"), "[0.8 0 0 0.8 20 20]");
    assert_eq!(value(&in_a, "Text state", "Effective size"), "9.6");
    assert_eq!(value(&on_page, "Graphics state", "CTM"), "[1 0 0 1 0 0]");
    assert_eq!(value(&on_page, "Text state", "Effective size"), "14");
}

#[test]
fn form_operator_streams_and_do_describe_the_form() {
    let doc = open("form-xobject-nesting");

    let in_b = op_props(&doc, 8, Some(7));
    let do_b = op_props(&doc, 7, Some(6));
    let do_a = op_props(&doc, 7, None);

    assert_eq!(value(&in_b, "Operator", "Stream"), "Form /FmB (7 0 R)");
    assert_eq!(
        link_of(&in_b, "Operator", "Stream"),
        Some(NodeId::Object(ObjRef::new(7, 0)))
    );
    assert_eq!(value(&do_a, "Form XObject", "Name"), "/FmA");
    assert_eq!(value(&do_a, "Form XObject", "Matrix"), "[0.8 0 0 0.8 20 20]");
    assert_eq!(
        link_of(&do_a, "Form XObject", "Object"),
        Some(NodeId::Object(ObjRef::new(6, 0)))
    );
    assert_eq!(value(&do_b, "Form XObject", "Name"), "/FmB");
    assert_eq!(value(&do_b, "Form XObject", "CTM inside"), "[0.8 0 0 0.8 28 28]");
    assert!(do_a.get("Form XObject", "Page-space box").is_some());
}

#[test]
fn form_content_hit_finds_the_form_text_and_the_form_mcid() {
    let doc = open("form-xobject-nesting");
    let form_b_text = op_id(&doc, 8, Some(7));
    let NodeId::Op { ix, .. } = form_b_text else {
        panic!("not an op")
    };
    let on_text = centre(rect_of(&doc, &glyph(ix, 0)));

    let hits = doc.hit(0, on_text, LayerMode::All);

    // The light-green background fill of form B (op 19) lies under the text.
    assert_eq!(
        &hits[..4],
        &[
            glyph(ix, 0),
            form_b_text.clone(),
            NodeId::Op { page: 0, ix: 19 },
            mc(0)
        ]
    );
    assert_eq!(doc.bbox(&form_b_text).map(|b| b.0), Some(0));
    // The Do op has no items of its own and reports the form's page-space box.
    let do_b = rect_of(&doc, &op_id(&doc, 7, Some(6)));
    assert!(do_b.width() > 0.0);
}

#[test]
fn form_mcid_is_reported_once_per_content_stream() {
    let doc = open("form-xobject-nesting");

    let p = props(&doc, &mc(0));

    assert_eq!(value(&p, "Marked content", "Occurrences"), "3");
    assert_eq!(value(&p, "Marked content", "Content streams"), "3");
    assert_eq!(value(&p, "Marked content", "Text"), "Page textForm AForm B");
}
