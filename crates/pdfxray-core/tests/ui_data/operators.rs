//! Matrices and operator notes.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn ctms_captured_from_a_fixture_decompose_into_what_the_content_did() {
    let doc = open("graphics-state");
    let page = doc.captured(0).unwrap();
    // `1 0 0 1 150 100 cm`, then `2 0 0 2 0 0 cm` (op 35), later `0 1 -1 0 0 0 cm` (op 40).
    let scaled = Matrix::from(page.ops[35].gstate.ctm).decompose();
    let turned = Matrix::from(page.ops[40].gstate.ctm).decompose();

    assert_eq!(page.ops[35].gstate.ctm, [2.0, 0.0, 0.0, 2.0, 150.0, 100.0]);
    assert_eq!(
        (
            scaled.scale_x,
            scaled.scale_y,
            scaled.rotation_deg,
            scaled.skew_deg,
            scaled.uniform
        ),
        (2.0, 2.0, 0.0, 0.0, true)
    );
    assert_eq!(page.ops[40].gstate.ctm, [0.0, 1.0, -1.0, 0.0, 150.0, 100.0]);
    assert_eq!(
        (
            turned.scale_x,
            turned.scale_y,
            turned.rotation_deg,
            turned.skew_deg,
            turned.ratio
        ),
        (1.0, 1.0, 90.0, 0.0, 1.0)
    );
    let moved = Matrix::from(page.ops[40].gstate.ctm).apply(Point::new(10.0, 0.0));
    assert_eq!(
        moved,
        Point::new(150.0, 110.0),
        "a quarter turn sends +x to +y, then the shift applies"
    );
}

#[test]
fn every_operator_of_the_generated_fixtures_has_a_note_and_the_notes_read_right() {
    let mut seen = BTreeSet::new();
    for name in [
        "graphics-state",
        "text-state",
        "tagged-simple",
        "artifacts",
        "type3-font",
        "images",
    ] {
        let doc = open(name);
        for page in 0..doc.page_count() {
            for op in &doc.captured(u32::try_from(page).unwrap()).unwrap().ops {
                let note = explain::operator(&op.operator, &op.operands);
                assert!(
                    !note.starts_with("unknown"),
                    "{name}: {} {:?} -> {note}",
                    op.operator,
                    op.operands
                );
                assert!(note.chars().count() <= 60, "{name}: {note}");
                seen.insert(op.operator.clone());
            }
        }
    }
    let tagged = open("tagged-simple");
    let ops = &tagged.captured(0).unwrap().ops;
    let notes: Vec<String> = ops[..6]
        .iter()
        .map(|op| explain::operator(&op.operator, &op.operands))
        .collect();

    assert_eq!(
        notes,
        [
            "start tagged content: H1, MCID 0",
            "begin text",
            "font F1 at 24 pt",
            "move to x 20, y 150",
            "draw “Heading”",
            "end text",
        ]
    );
    assert!(seen.len() >= 25, "the fixtures exercise many operators: {seen:?}");
}

#[test]
fn a_text_operator_reads_the_decoded_text_of_its_glyphs() {
    let doc = open("tagged-simple");
    let tj = first_op(&doc, "Tj");
    let page = doc.captured(0).unwrap();
    let text: String = page
        .items
        .iter()
        .filter(|i| i.op == tj)
        .filter_map(|i| match &i.kind {
            PaintKind::Glyph { unicode, .. } => unicode.clone(),
            _ => None,
        })
        .collect();
    let op = &page.ops[tj as usize];

    assert_eq!(text, "Heading");
    assert_eq!(
        explain::operator_with_text(&op.operator, &op.operands, Some(&text)),
        "draw “Heading”"
    );
    assert_eq!(
        explain::operator_with_text("q", &[], Some("ignored")),
        "save graphics state"
    );
}
