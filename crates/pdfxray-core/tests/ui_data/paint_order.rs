//! Paint order.

use pdfxray_core::OpPosition;
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

#[test]
fn the_document_paint_order_runs_page_after_page_and_every_operator_has_one_position() {
    let doc = open("long-doc");
    // Each of the 40 pages runs 4 x `BDC BT Tf Td Tj ET EMC`: 28 operators, the `Tj`s paint.
    let per_page: Vec<usize> = (0..40).map(|p| doc.captured(p).unwrap().ops.len()).collect();

    let positions: Vec<OpPosition> = (0..doc.op_total()).map(|i| doc.op_at(i).unwrap()).collect();
    let round_trip: Vec<u32> = positions
        .iter()
        .map(|&at| doc.paint_order_index(at).unwrap())
        .collect();

    assert_eq!(per_page, vec![28; 40]);
    assert_eq!(doc.op_total(), 40 * 28);
    assert_eq!(positions.first(), Some(&OpPosition::new(0, 0)));
    assert_eq!(
        doc.op_at(28),
        Some(OpPosition::new(1, 0)),
        "page 2 starts after page 1's 28"
    );
    assert_eq!(positions.last(), Some(&OpPosition::new(39, 27)));
    assert_eq!(round_trip, (0..doc.op_total()).collect::<Vec<u32>>());
    assert!(positions.is_sorted(), "positions order as the document paints");
    assert_eq!(doc.op_at(doc.op_total()), None, "past the last operator");
    assert_eq!(
        doc.paint_order_index(OpPosition::new(0, 28)),
        None,
        "no op 28 on page 1"
    );
    assert_eq!(doc.paint_order_index(OpPosition::new(40, 0)), None, "no page 41");
    // smoke: a position names the same operator as a node id
    assert_eq!(NodeId::from(OpPosition::new(3, 4)), NodeId::Op { page: 3, ix: 4 });
}

#[test]
fn visible_changes_of_the_document_cross_page_boundaries_both_ways() {
    let doc = open("long-doc");

    assert_eq!(
        doc.next_visible_change_in_document(OpPosition::new(0, 4), true),
        Some(OpPosition::new(0, 11)),
        "within the page, like next_visible_change"
    );
    assert_eq!(
        doc.next_visible_change_in_document(OpPosition::new(0, 25), true),
        Some(OpPosition::new(1, 4)),
        "past page 1's last Tj to page 2's first"
    );
    assert_eq!(
        doc.next_visible_change_in_document(OpPosition::new(1, 4), false),
        Some(OpPosition::new(0, 25)),
        "back from page 2's first Tj to page 1's last"
    );
    assert_eq!(
        doc.next_visible_change_in_document(OpPosition::new(0, 4), false),
        None
    );
    assert_eq!(
        doc.next_visible_change_in_document(OpPosition::new(39, 25), true),
        None
    );
    // smoke: every stop forward from the start paints, 4 per page
    let mut stops = Vec::new();
    let mut at = OpPosition::new(0, 0);
    while let Some(next) = doc.next_visible_change_in_document(at, true) {
        stops.push(next);
        at = next;
    }
    assert_eq!(stops.len(), 40 * 4);
    assert!(stops.iter().all(|s| !doc.is_state_only(s.page, s.ix)));
}

/// Four pages: a fill, nothing at all (no `/Contents`), only state (`q Q`), a fill.
fn pages_with_gaps() -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(
        2,
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R 6 0 R] /Count 4 /MediaBox [0 0 100 100] >>",
    );
    w.object(3, "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>");
    w.object(4, "<< /Type /Page /Parent 2 0 R >>");
    w.object(5, "<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>");
    w.object(6, "<< /Type /Page /Parent 2 0 R /Contents 9 0 R >>");
    w.stream(7, "", b"0 0 10 10 re f");
    w.stream(8, "", b"q Q");
    w.stream(9, "", b"50 50 10 10 re f");
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("gaps.pdf", w.into_output().unwrap().bytes).unwrap()
}

#[test]
fn pages_without_operators_take_no_positions_and_jumps_pass_over_pages_that_paint_nothing() {
    let doc = pages_with_gaps();

    let positions: Vec<OpPosition> = (0..doc.op_total()).map(|i| doc.op_at(i).unwrap()).collect();
    let forward = doc.next_visible_change_in_document(OpPosition::new(0, 1), true);
    let back = doc.next_visible_change_in_document(OpPosition::new(3, 1), false);

    assert_eq!(doc.page_count(), 4);
    assert_eq!(
        positions,
        [(0, 0), (0, 1), (2, 0), (2, 1), (3, 0), (3, 1)].map(|(p, ix)| OpPosition::new(p, ix)),
        "page 2 has no operators, so page 3 follows page 1 directly"
    );
    assert_eq!(doc.paint_order_index(OpPosition::new(1, 0)), None);
    assert_eq!(doc.paint_order_index(OpPosition::new(2, 0)), Some(2));
    assert_eq!(
        forward,
        Some(OpPosition::new(3, 1)),
        "past the empty and the state-only page"
    );
    assert_eq!(back, Some(OpPosition::new(0, 1)));
    // smoke: the only stops are the two fills
    assert!(doc.is_state_only(2, 0) && doc.is_state_only(2, 1));
    assert!(!doc.is_state_only(0, 1) && !doc.is_state_only(3, 1));
}
