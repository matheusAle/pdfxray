//! Streams in Form `XObject`s.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn nested_forms_list_the_resources_of_their_own_stream_only() {
    let doc = open("form-xobject-nesting");

    let page = doc.object_refs(r(3));
    let outer = doc.object_refs(r(6));
    let inner = doc.object_refs(r(7));

    let summary = |refs: &pdfxray_core::ObjectRefs| -> Vec<(String, Option<ObjRef>, ResKind, String)> {
        refs.uses
            .iter()
            .map(|u| (u.name.clone(), u.obj, u.kind, u.summary.clone()))
            .collect()
    };
    // The page's stream shows "Page text" (9 glyphs) and draws FmA; FmA and FmB have their own
    // resources, so their operators do not count for the page.
    assert_eq!(
        summary(&page),
        [
            (
                "F1".to_owned(),
                Some(r(5)),
                ResKind::Font,
                "Helvetica · 9 glyphs".to_owned()
            ),
            (
                "FmA".to_owned(),
                Some(r(6)),
                ResKind::XObject,
                "form XObject".to_owned()
            ),
        ]
    );
    assert_eq!(
        summary(&outer),
        [
            (
                "F1".to_owned(),
                Some(r(5)),
                ResKind::Font,
                "Helvetica · 6 glyphs".to_owned()
            ),
            (
                "FmB".to_owned(),
                Some(r(7)),
                ResKind::XObject,
                "form XObject".to_owned()
            ),
        ]
    );
    assert_eq!(
        summary(&inner),
        [(
            "F1".to_owned(),
            Some(r(5)),
            ResKind::Font,
            "Helvetica · 6 glyphs".to_owned()
        )]
    );
    assert_eq!(
        outer.used_by,
        vec![
            (r(3), "/Resources/XObject/FmA".to_owned()),
            (r(11), "/K/Stm".to_owned())
        ]
    );
    assert_eq!(
        inner.used_by,
        vec![
            (r(6), "/Resources/XObject/FmB".to_owned()),
            (r(12), "/K/Stm".to_owned())
        ]
    );
    assert_eq!(
        doc.object_refs(r(4)).uses,
        page.uses,
        "the page's only content stream"
    );
    assert_eq!(
        doc.object_refs(r(8)).used_by,
        vec![(r(1), "/StructTreeRoot".to_owned()), (r(9), "/P".to_owned())]
    );
}
