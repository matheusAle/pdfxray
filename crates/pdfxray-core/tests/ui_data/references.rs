//! Object references.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_content_stream_of_the_minimal_page_is_used_by_the_page_and_uses_its_font() {
    let doc = open("minimal-one-page");

    let stream = doc.object_refs(r(4));
    let font = doc.object_refs(r(5));
    let page = doc.object_refs(r(3));

    assert_eq!(stream.used_by, vec![(r(3), "/Contents".to_owned())]);
    assert_eq!(stream.uses.len(), 1);
    let f1 = &stream.uses[0];
    assert_eq!(
        (f1.name.as_str(), f1.obj, f1.kind, f1.summary.as_str()),
        ("F1", Some(r(5)), ResKind::Font, "Helvetica · 13 glyphs")
    );
    assert_eq!(font.used_by, vec![(r(3), "/Resources/Font/F1".to_owned())]);
    assert_eq!(font.uses, vec![], "a font runs no operators");
    assert_eq!(page.used_by, vec![(r(2), "/Kids[0]".to_owned())]);
    assert_eq!(
        page.uses, stream.uses,
        "the page's own operators are the stream's"
    );
    assert_eq!(doc.object_refs(r(99)).used_by, vec![]);
    assert_eq!(doc.object_refs(r(99)).uses, vec![]);
}

#[test]
fn inherited_resources_resolve_through_the_pages_node_and_forms_with_their_own() {
    let doc = open("resources-inheritance");

    let page1 = doc.object_refs(r(6));
    let form = doc.object_refs(r(12));
    let font = doc.object_refs(r(9));

    let used: Vec<(&str, Option<ObjRef>, ResKind)> = page1
        .uses
        .iter()
        .map(|u| (u.name.as_str(), u.obj, u.kind))
        .collect();
    assert_eq!(
        used,
        [
            ("F1", Some(r(9)), ResKind::Font),
            ("F2", Some(r(10)), ResKind::Font),
            ("Fm1", Some(r(12)), ResKind::XObject),
            ("Fm2", Some(r(13)), ResKind::XObject),
            ("GS1", Some(r(15)), ResKind::ExtGState),
        ]
    );
    assert_eq!(page1.uses[0].summary, "Helvetica · 32 glyphs");
    assert_eq!(page1.uses[2].summary, "form XObject");
    assert_eq!(page1.uses[4].summary, "ExtGState · fill alpha 0.5");
    assert_eq!(page1.used_by, vec![(r(3), "/Contents".to_owned())]);
    // Page 2's own font F1 is a different object; the page nodes inherit /Resources from /Pages.
    assert_eq!(
        doc.object_refs(r(7))
            .uses
            .iter()
            .map(|u| u.obj)
            .collect::<Vec<_>>(),
        [Some(r(14))]
    );
    assert_eq!(
        font.used_by,
        vec![
            (r(2), "/Resources/Font/F1".to_owned()),
            (r(12), "/Resources/Font/F1".to_owned())
        ],
        "the Pages node and the form's own resources both name it"
    );
    assert_eq!(form.uses.len(), 1);
    assert_eq!((form.uses[0].name.as_str(), form.uses[0].obj), ("F1", Some(r(9))));
    assert_eq!(
        doc.object_refs(r(2)).used_by.len(),
        4,
        "the catalog and three kids point at /Pages"
    );
}
