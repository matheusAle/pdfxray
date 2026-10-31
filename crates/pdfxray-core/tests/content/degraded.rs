//! Degraded capture.

use pretty_assertions::assert_eq;

use super::*;

fn synthetic(content: &[u8]) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R >>",
    );
    w.stream(4, "", content);
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("synthetic.pdf", w.into_output().unwrap().bytes).unwrap()
}

#[test]
fn degraded_capture_is_announced_first_in_the_tree() {
    let doc = synthetic(b"0 0 10 re f 20 20 10 10 re f");

    let tree = doc.tree(TreeKind::Content(0));

    assert_eq!(labels(&tree), ["Capture degraded", "re", "f", "re", "f"]);
    assert_eq!(tree[0].id, NodeId::Page(0));
    assert!(
        tree[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("stopped at operator 1"),
        "{:?}",
        tree[0].detail
    );
    assert_eq!(tree.len(), 5);
}

#[test]
fn exact_capture_has_no_degraded_row() {
    let doc = synthetic(b"20 20 10 10 re f");

    let tree = doc.tree(TreeKind::Content(0));

    assert_eq!(labels(&tree), ["re", "f"]);
    assert_eq!(
        rect_of(&doc, &NodeId::Op { page: 0, ix: 1 }),
        Rect::new(20.0, 20.0, 30.0, 30.0)
    );
    let hits = doc.hit(0, Point::new(25.0, 25.0), LayerMode::Content);
    assert_eq!(hits, [NodeId::Op { page: 0, ix: 1 }, NodeId::Page(0)]);
}

#[test]
fn long_operands_are_truncated_in_the_tree_but_not_in_props() {
    let long = format!("[({})] TJ", "x".repeat(200));
    let doc = synthetic(format!("BT {long} ET").as_bytes());

    let tree = doc.tree(TreeKind::Content(0));
    let detail = tree[1].detail.as_deref().unwrap();

    assert_eq!(detail.chars().count(), 80);
    assert!(detail.ends_with('\u{2026}'));
    assert_eq!(
        value(&props(&doc, &tree[1].id), "Operator", "Operands")
            .chars()
            .count(),
        204
    );
}
