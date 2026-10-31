//! Raw-layer integration: objects, revisions, trailer, page tree and diagnostics as the
//! query API exposes them, checked against the generated fixtures' expected facts.

use pdfxray_core::{BoxKind, BoxSource, Document, NodeId, ObjRef, PageBox, Rect, TreeKind};
use pretty_assertions::assert_eq;

#[expect(
    clippy::unwrap_used,
    reason = "test helper: a missing fixture must fail the test loudly"
)]
fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn obj(n: u32) -> ObjRef {
    ObjRef::new(n, 0)
}

#[test]
fn page_boxes_carry_values_and_provenance() {
    let doc = fixture("page-boxes");
    let own = BoxSource::Own;

    let p0 = doc.page(0).unwrap();
    assert_eq!(
        p0.boxes,
        vec![
            PageBox {
                kind: BoxKind::Media,
                rect: Rect::new(0.0, 0.0, 400.0, 300.0),
                source: own
            },
            PageBox {
                kind: BoxKind::Crop,
                rect: Rect::new(10.0, 10.0, 390.0, 290.0),
                source: own
            },
            PageBox {
                kind: BoxKind::Bleed,
                rect: Rect::new(20.0, 20.0, 380.0, 280.0),
                source: own
            },
            PageBox {
                kind: BoxKind::Trim,
                rect: Rect::new(30.0, 30.0, 370.0, 270.0),
                source: own
            },
            PageBox {
                kind: BoxKind::Art,
                rect: Rect::new(40.0, 40.0, 360.0, 260.0),
                source: own
            },
        ]
    );
    assert_eq!(
        (p0.obj, p0.rotate, p0.display_size()),
        (Some(obj(3)), 90, (280.0, 380.0))
    );

    let p1 = doc.page(1).unwrap();
    assert_eq!(
        p1.page_box(BoxKind::Media).map(|b| b.source),
        Some(BoxSource::Inherited(obj(2)))
    );
    assert_eq!(
        p1.page_box(BoxKind::Crop).map(|b| b.source),
        Some(BoxSource::Default(BoxKind::Media))
    );
    assert_eq!(p1.media_box, Rect::new(0.0, 0.0, 300.0, 400.0));

    let p2 = doc.page(2).unwrap();
    assert_eq!((p2.user_unit, p2.display_size()), (2.0, (400.0, 400.0)));
}

#[test]
fn box_outside_mediabox_is_a_diagnostic_linked_to_its_page() {
    let doc = fixture("page-boxes");

    let diags: Vec<_> = doc
        .info()
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "box-outside-mediabox")
        .collect();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].object, Some(obj(5)));
    assert!(
        diags[0]
            .message
            .starts_with("TrimBox of Page 3 [150 150 250 250]"),
        "{}",
        diags[0].message
    );

    let trim = doc
        .node(&NodeId::PageBox {
            page: 2,
            kind: BoxKind::Trim,
        })
        .unwrap();
    assert_eq!(
        trim.get("Geometry", "Warning"),
        Some("extends outside the MediaBox")
    );
}

#[test]
fn inherited_box_props_link_to_the_pages_node() {
    let doc = fixture("page-boxes");

    let media = doc
        .node(&NodeId::PageBox {
            page: 1,
            kind: BoxKind::Media,
        })
        .unwrap();
    let row = media.groups[0]
        .rows
        .iter()
        .find(|r| r.key == "Inherited from")
        .unwrap();
    assert_eq!(
        (row.value.as_str(), row.link.clone()),
        ("2 0 R", Some(NodeId::Object(obj(2))))
    );
}

#[test]
fn broken_xref_offsets_are_reported_on_their_objects() {
    let doc = fixture("broken-xref-offsets");

    let info = doc.info();
    let diags: Vec<(&str, Option<ObjRef>)> = info.diagnostics.iter().map(|d| (d.code, d.object)).collect();
    assert_eq!(
        diags,
        vec![
            ("xref-offset-wrong", Some(obj(3))),
            ("xref-offset-wrong", Some(obj(4)))
        ]
    );

    let props = doc.node(&NodeId::Object(obj(3))).unwrap();
    let linked: Vec<_> = props
        .groups
        .iter()
        .find(|g| g.name == "Diagnostics")
        .unwrap()
        .rows
        .iter()
        .map(|r| r.link.clone())
        .collect();
    assert_eq!(linked, vec![Some(NodeId::Diagnostic(0))]);
    // Smoke: the object still renders as found, not as repaired.
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn objects_tree_lists_trailer_revisions_and_objects() {
    let doc = fixture("incremental-update");

    let tree = doc.tree(TreeKind::Objects);
    let labels: Vec<&str> = tree.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(labels, ["Trailer", "Revisions", "Objects"]);
    let revisions: Vec<&str> = tree[1].children.iter().map(|n| n.label.as_str()).collect();
    assert_eq!(revisions, ["Revision 1", "Revision 2"]);
    let objs: Vec<String> = tree[2].children.iter().map(|n| n.id.to_string()).collect();
    assert_eq!(
        objs,
        ["obj:1:0", "obj:2:0", "obj:3:0", "obj:4:0", "obj:5:0", "obj:6:0"]
    );

    let second = doc.node(&NodeId::Revision(1)).unwrap();
    assert_eq!(second.get("Cross-reference", "/Prev"), Some("410"));
    assert_eq!(doc.info().revision_count, 2);
}

#[test]
fn object_props_show_value_links_stream_and_referrers() {
    let doc = fixture("minimal-one-page");
    let page = doc.page(0).unwrap().obj.unwrap();

    let props = doc.node(&NodeId::Object(page)).unwrap();
    assert_eq!(props.get("Object", "Type"), Some("dictionary"));
    assert_eq!(props.get("Value", "/Type"), Some("/Page"));
    let referrers: Vec<_> = props
        .groups
        .iter()
        .find(|g| g.name == "Referenced by")
        .unwrap()
        .rows
        .iter()
        .map(|r| r.link.clone())
        .collect();
    assert_eq!(referrers, vec![Some(NodeId::Object(obj(2)))]);

    let contents = props.groups[1]
        .rows
        .iter()
        .find(|r| r.key == "/Contents")
        .and_then(|r| r.link.clone())
        .unwrap();
    let stream = doc.node(&contents).unwrap();
    assert_eq!(stream.get("Stream", "Filters"), Some("none"));
    assert!(
        stream
            .get("Stream", "Preview")
            .is_some_and(|p| p.contains("(Hello pdfxray) Tj"))
    );
}

#[test]
fn trailer_props_link_root() {
    let doc = fixture("minimal-one-page");

    let trailer = doc.node(&NodeId::Trailer).unwrap();
    let root = trailer.groups[0].rows.iter().find(|r| r.key == "/Root").unwrap();
    assert_eq!(root.link, Some(NodeId::Object(obj(1))));
}

#[test]
fn encrypted_file_shows_structure_without_engine() {
    let doc = fixture("encrypted-stub");

    let trailer = doc.node(&NodeId::Trailer).unwrap();
    assert!(
        trailer
            .get("Encryption", "Encrypt")
            .is_some_and(|e| e.contains("/Filter /Standard"))
    );
    assert_eq!(doc.page(0).map(|p| p.obj.is_some()), Some(true));
}
