//! What a marquee touches: `Document::hit_rect` per Pick target.

use pretty_assertions::assert_eq;

use super::*;

/// Page boxes of page 1 of `page-boxes`: Media [0 0 400 300], Crop [10 10 390 290], Bleed
/// [20 20 380 280], Trim [30 30 370 270], Art [40 40 360 260].
fn page_box(kind: pdfxray_core::BoxKind) -> NodeId {
    NodeId::PageBox { page: 0, kind }
}

#[test]
fn tags_pick_the_elements_that_own_the_touched_content_in_structure_order() {
    let doc = fixture("struct-rich");
    // Across the Figure (20..140 × 230..310) and the second cell (120..150 × 197.5..208.6).
    let rect = Rect::new(130.0, 260.0, 100.0, 200.0);

    assert_eq!(doc.hit_rect(0, rect, LayerMode::Tags), [elem(14), elem(18)]);
    // Above the figure: the heading and the paragraph, not their Sect or Document.
    assert_eq!(
        doc.hit_rect(0, Rect::new(10.0, 320.0, 200.0, 390.0), LayerMode::Tags),
        [elem(12), elem(13)]
    );
    // Smoke: a rectangle over nothing painted touches nothing.
    assert_eq!(
        doc.hit_rect(0, Rect::new(300.0, 10.0, 390.0, 60.0), LayerMode::Tags),
        Vec::<NodeId>::new()
    );
}

#[test]
fn content_picks_the_operators_and_all_picks_their_elements() {
    let doc = fixture("struct-rich");
    let rect = Rect::new(100.0, 200.0, 130.0, 260.0);
    let op_at = |x, y| {
        doc.hit(0, Point::new(x, y), LayerMode::Content)
            .into_iter()
            .find(|n| matches!(n, NodeId::Op { .. }))
            .unwrap()
    };
    let (figure, cell) = (op_at(110.0, 250.0), op_at(130.0, 203.0));

    assert_eq!(doc.hit_rect(0, rect, LayerMode::Content), [figure, cell]);
    // Everything is tagged: All picks the same elements as Tags.
    assert_eq!(doc.hit_rect(0, rect, LayerMode::All), [elem(14), elem(18)]);
    // Smoke: annotations are not hit-tested, so nothing is touched.
    assert_eq!(
        doc.hit_rect(0, rect, LayerMode::Annotations),
        Vec::<NodeId>::new()
    );
}

#[test]
fn artifacts_pick_the_artifact_sequences_in_paint_order() {
    let doc = fixture("artifacts");
    // Over the header (20..77 × 178..187) and the rule (20..180 × 90..91), inside the border.
    let rect = Rect::new(30.0, 85.0, 60.0, 185.0);

    let artifact = |seq| NodeId::Artifact { page: 0, seq };
    assert_eq!(
        doc.hit_rect(0, rect, LayerMode::Artifacts),
        [artifact(0), artifact(1), artifact(3)]
    );
    // Smoke: the footer (7.8..17.3 high) is outside the rectangle.
    assert!(!doc.hit_rect(0, rect, LayerMode::Artifacts).contains(&artifact(2)));
}

#[test]
fn boxes_are_touched_where_the_rectangle_meets_their_outline() {
    use pdfxray_core::BoxKind;
    let doc = fixture("page-boxes");

    // Across the Trim box's left edge (x 30), inside Media, Crop and Bleed.
    assert_eq!(
        doc.hit_rect(0, Rect::new(25.0, 100.0, 35.0, 120.0), LayerMode::Boxes),
        [page_box(BoxKind::Trim)]
    );
    // Across the Trim and the Art edges: the smaller box first.
    assert_eq!(
        doc.hit_rect(0, Rect::new(25.0, 100.0, 45.0, 120.0), LayerMode::Boxes),
        [page_box(BoxKind::Art), page_box(BoxKind::Trim)]
    );
    // A rectangle around the whole page touches every box.
    assert_eq!(
        doc.hit_rect(0, Rect::new(-5.0, -5.0, 405.0, 305.0), LayerMode::Boxes)
            .len(),
        5
    );
    // Smoke: strictly inside the Art box touches none, and other modes never list boxes.
    assert_eq!(
        doc.hit_rect(0, Rect::new(100.0, 100.0, 120.0, 120.0), LayerMode::Boxes),
        Vec::<NodeId>::new()
    );
    assert!(
        !doc.hit_rect(0, Rect::new(-5.0, -5.0, 405.0, 305.0), LayerMode::All)
            .iter()
            .any(|n| matches!(n, NodeId::PageBox { .. }))
    );
}
