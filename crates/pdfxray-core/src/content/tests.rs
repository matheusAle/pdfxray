//! The `pub(crate)` helpers the tags module builds on: per-page MCIDs, their boxes and text.

#![allow(clippy::unwrap_used)]

use pretty_assertions::assert_eq;

use crate::test_support::fixture;
use crate::{ObjRef, Rect};

#[test]
fn mcids_on_page_are_sorted_and_unique() {
    let tagged = fixture("tagged-simple");
    let untagged = fixture("untagged-content");
    let none = fixture("minimal-one-page");

    assert_eq!(tagged.mcids_on_page(0), [0, 1]);
    // Facts list [0, 1, 1, 2]: the duplicate collapses.
    assert_eq!(untagged.mcids_on_page(0), [0, 1, 2]);
    assert_eq!(none.mcids_on_page(0), Vec::<i32>::new());
    assert_eq!(tagged.mcids_on_page(9), Vec::<i32>::new());
}

#[test]
fn mc_text_and_bbox_follow_the_sequences() {
    let doc = fixture("tagged-simple");

    assert_eq!(doc.mc_text_in(0, 0, None), "Heading");
    assert_eq!(doc.mc_text_in(0, 1, None), "Body paragraph");
    assert_eq!(doc.mc_text_in(0, 7, None), "");
    assert_eq!(doc.mc_text_in(3, 0, None), "");
    let heading = doc.mc_bbox(0, 0).unwrap();
    assert_eq!(
        (
            heading.x0,
            heading.y0,
            (heading.x1 * 1000.0).round() / 1000.0,
            heading.y1
        ),
        (20.0, 144.72, 109.376, 167.232)
    );
    assert_eq!(doc.mc_bbox(0, 7), None);
    assert!(doc.mc_bbox(0, 1).unwrap().y1 < heading.y0);
}

#[test]
fn duplicate_mcids_aggregate_in_paint_order() {
    let doc = fixture("untagged-content");

    assert_eq!(doc.mc_text_in(0, 1, None), "Tagged twoTagged again");
    assert_eq!(doc.mc_text_in(0, 2, None), "Nested");
    let both = doc.mc_bbox(0, 1).unwrap();
    assert_eq!((both.y0, both.y1), (57.36, 108.616));
    assert!(both.contains_rect(&Rect::new(20.0, 60.0, 30.0, 70.0)));
}

#[test]
fn stream_scoped_variants_separate_page_and_forms() {
    let doc = fixture("form-xobject-nesting");
    let form_a = Some(ObjRef::new(6, 0));
    let form_b = Some(ObjRef::new(7, 0));

    assert_eq!(doc.mc_text_in(0, 0, None), "Page text");
    assert_eq!(doc.mc_text_in(0, 0, form_a), "Form A");
    assert_eq!(doc.mc_text_in(0, 0, form_b), "Form B");
    assert_eq!(doc.mc_text_in(0, 0, Some(ObjRef::new(99, 0))), "");
    let page = doc.mc_bbox_in(0, 0, None).unwrap();
    let a = doc.mc_bbox_in(0, 0, form_a).unwrap();
    let all = doc.mc_bbox(0, 0).unwrap();
    assert!(all.contains_rect(&page) && all.contains_rect(&a));
    assert!(page.y0 > a.y1, "page text {page:?} is above form A {a:?}");
    assert_eq!(doc.mc_bbox_in(0, 0, Some(ObjRef::new(99, 0))), None);
}
