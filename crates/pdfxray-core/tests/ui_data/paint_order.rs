//! Paint order.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn an_orange_fill_is_partly_covered_by_the_stroked_square_painted_after_it() {
    let doc = open("graphics-state");
    // op 49 fills 220 100 100 100 (orange); op 53 strokes 250 130 100 100 with a 6 pt line;
    // op 59 paints a shading through the clip 200 220 150 100.

    let fill = doc.paint_step(0, 49).unwrap();
    let stroke = doc.paint_step(0, 53).unwrap();

    assert_eq!((fill.step, fill.total), (50, 80));
    assert_eq!(
        fill.paints_over,
        vec![],
        "nothing earlier reaches that corner of the page"
    );
    assert_eq!(fill.covered_by, vec![NodeId::Op { page: 0, ix: 53 }]);
    // The stroke's box [247 127 353 233] overlaps the fill's [220 100 320 200] in 73 x 73 pt.
    let want = 1.0 - (73.0 * 73.0) / (100.0 * 100.0);
    assert!(
        (fill.visible_fraction - want).abs() < 1e-9,
        "{}",
        fill.visible_fraction
    );
    assert!(fill.approximate);
    assert_eq!(stroke.paints_over, vec![NodeId::Op { page: 0, ix: 49 }]);
    assert_eq!(
        stroke.covered_by,
        vec![NodeId::Op { page: 0, ix: 59 }],
        "the shading's clip box reaches y 233"
    );
    // The shading's box ends at x 350, so the overlap is 103 x 13 pt of the stroke's 106 x 106.
    let want = 1.0 - (103.0 * 13.0) / (106.0 * 106.0);
    assert!(
        (stroke.visible_fraction - want).abs() < 1e-9,
        "{}",
        stroke.visible_fraction
    );
    assert!(fill.visible_fraction < 1.0 && stroke.visible_fraction > fill.visible_fraction);
}

#[test]
fn an_untouched_shape_is_fully_visible_and_state_operators_paint_nothing() {
    let doc = open("graphics-state");

    let red = doc.paint_step(0, 3).unwrap();
    let state = doc.paint_step(0, 1).unwrap();

    assert_eq!((red.step, red.total, red.visible_fraction), (4, 80, 1.0));
    assert_eq!(red.covered_by, vec![]);
    assert_eq!(
        (state.step, state.visible_fraction, state.paints_over.len()),
        (2, 1.0, 0)
    );
    assert_eq!(doc.paint_step(0, 80), None);
    assert_eq!(doc.paint_step(5, 0), None);
    assert_eq!(
        doc.paint_step(0, 79).unwrap().step,
        80,
        "the last operator is step total of total"
    );
}

#[test]
fn stepping_jumps_to_the_next_operator_that_paints_and_skips_state_operators() {
    let doc = open("graphics-state");

    assert_eq!(doc.next_visible_change(0, 49, true), Some(53));
    assert_eq!(doc.next_visible_change(0, 49, false), Some(43));
    assert_eq!(doc.next_visible_change(0, 0, true), Some(3));
    assert_eq!(
        doc.next_visible_change(0, 3, false),
        None,
        "nothing painted before the first fill"
    );
    assert_eq!(doc.next_visible_change(0, 79, true), None);
    assert_eq!(doc.next_visible_change(0, 200, true), None);
    assert!(doc.is_state_only(0, 50), "RG changes the stroke colour");
    assert!(doc.is_state_only(0, 2), "re only builds the path");
    assert!(
        doc.is_state_only(0, 58),
        "n ends a clip: the clip is not a painted item"
    );
    assert!(!doc.is_state_only(0, 49));
    assert!(!doc.is_state_only(0, 59), "sh paints");
    assert!(
        !doc.is_state_only(0, 500),
        "an operator that does not exist is not state-only"
    );
    let visible: Vec<u32> = (0..80).filter(|ix| !doc.is_state_only(0, *ix)).collect();
    let mut walked = Vec::new();
    let mut at = doc.next_visible_change(0, 0, true);
    while let Some(ix) = at {
        walked.push(ix);
        at = doc.next_visible_change(0, ix, true);
    }
    assert_eq!(
        walked, visible,
        "forward jumps visit exactly the painting operators"
    );
}

#[test]
fn the_stack_lists_are_capped() {
    let mut content = String::from("0 g 0 0 100 100 re f\n");
    for _ in 0..(pdfxray_core::MAX_PAINT_STACK + 10) {
        content.push_str("10 10 50 50 re f\n");
    }
    let doc = single_page("/MediaBox [0 0 200 200]", &content, |_| {});

    let bottom = doc.paint_step(0, 2).unwrap();
    let top = doc
        .paint_step(0, u32::try_from(doc.captured(0).unwrap().ops.len() - 1).unwrap())
        .unwrap();

    assert_eq!(bottom.covered_by.len(), pdfxray_core::MAX_PAINT_STACK);
    assert_eq!(top.paints_over.len(), pdfxray_core::MAX_PAINT_STACK);
    assert!(
        top.visible_fraction > 0.999,
        "nothing is painted after the last square"
    );
    assert!(
        bottom.visible_fraction < 0.8,
        "the 50 x 50 squares hide a quarter of the 100 x 100 one"
    );
}
