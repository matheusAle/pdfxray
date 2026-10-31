//! The `page-boxes` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn page_boxes_report_user_space_coordinates_before_rotation() {
    let pdf = load("page-boxes");

    let pages = capture_document(&pdf);

    assert_eq!(pages.len(), 3);
    assert!(capture_page(&pdf, 3).is_none());
    // Page 0 is rotated by 90 degrees; its strokes are still in unrotated space (+-0.5 line width).
    let rotated: Vec<_> = pages[0].items.iter().map(|i| i.bbox).collect();
    let want = [
        [9.5, 9.5, 390.5, 290.5],
        [19.5, 19.5, 380.5, 280.5],
        [29.5, 29.5, 370.5, 270.5],
        [39.5, 39.5, 360.5, 260.5],
    ];
    assert_eq!(rotated.len(), 4);
    for (got, w) in rotated.into_iter().zip(want) {
        assert_rect(got, w);
    }
    assert_eq!(
        pages[0]
            .items
            .iter()
            .map(|i| i.gstate.stroke.components.clone())
            .collect::<Vec<_>>(),
        vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 0.6, 0.0],
            vec![0.0, 0.0, 1.0],
            vec![1.0, 0.0, 1.0]
        ]
    );
    // Page 1 inherits its media box (300 x 400) and strokes just inside it.
    assert_eq!(pages[1].items.len(), 1);
    assert_rect(pages[1].items[0].bbox, [9.5, 9.5, 290.5, 390.5]);
    // Page 2 has /UserUnit 2: capture stays in default user space, and the second square
    // sticks out of the 200 x 200 media box.
    assert_rect(pages[2].items[0].bbox, [9.5, 9.5, 190.5, 190.5]);
    assert_rect(pages[2].items[1].bbox, [149.5, 149.5, 250.5, 250.5]);
    assert!(pages.iter().all(|p| p.alignment == Alignment::Exact));
}
