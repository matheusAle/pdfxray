//! Stream bytes, byte ranges and the revision-aware object queries the object view reads,
//! checked against the generated fixtures.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pdfxray_core::{Document, NodeId, ObjRef};
use pretty_assertions::assert_eq;

fn open(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn r(num: u32) -> ObjRef {
    ObjRef::new(num, 0)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_flate_stream_gives_its_encoded_and_decoded_bytes() {
    let doc = open("text-state");

    let stream = doc.stream_bytes(r(4)).unwrap();

    assert_eq!(stream.filters, ["FlateDecode"]);
    assert_eq!(stream.raw.len(), 315);
    assert_eq!(stream.file_range, 311..626);
    assert_eq!(&doc.bytes()[stream.file_range.clone()], stream.raw.as_slice());
    assert!(stream.complete);
    let decoded = stream.decoded.unwrap();
    assert_eq!(decoded.len(), 541);
    let decoded = text(&decoded);
    assert!(decoded.starts_with("BT\n/F1 12 Tf"), "{decoded}");
    assert!(decoded.contains("(Plain Tj) Tj"), "{decoded}");
    // Smoke: the encoded form is not the text.
    assert!(!text(&doc.bytes()[311..626]).contains("Plain Tj"));
}

#[test]
fn an_unfiltered_stream_decodes_to_itself_and_other_objects_have_no_stream() {
    let doc = open("minimal-one-page");

    let stream = doc.stream_bytes(r(4)).unwrap();

    assert_eq!(text(&stream.raw), "BT /F1 18 Tf 20 40 Td (Hello pdfxray) Tj ET");
    assert_eq!(stream.decoded, Some(stream.raw.clone()));
    assert_eq!(stream.filters, Vec::<String>::new());
    assert_eq!(doc.stream_bytes(r(3)), None, "a page is a dictionary");
    assert_eq!(doc.stream_bytes(r(99)), None, "no such object");
}

#[test]
fn an_encrypted_stream_is_returned_as_stored_and_incomplete() {
    let doc = open("encrypted-stub");

    let stream = doc.stream_bytes(r(4)).unwrap();

    assert!(!stream.complete);
    assert_eq!(stream.decoded.as_deref(), Some(stream.raw.as_slice()));
    assert_eq!(stream.raw.len(), 47);
}

#[test]
fn object_ranges_are_where_the_file_really_has_the_object() {
    let doc = open("broken-xref-offsets");

    // The xref says 128 and 244; the objects start at 121 and 247.
    assert_eq!(doc.object_range(r(3)), Some(121..246));
    assert_eq!(doc.object_range(r(4)), Some(247..337));
    assert!(text(&doc.bytes()[121..246]).starts_with("3 0 obj"));
    assert!(text(&doc.bytes()[247..337]).starts_with("4 0 obj"));
    assert_eq!(doc.object_range(r(99)), None);
}

#[test]
fn a_stream_asked_for_by_revision_is_the_stream_as_of_that_revision() {
    let doc = open("incremental-update");

    let original = doc.stream_bytes_at(r(4), 0).unwrap();
    let edited = doc.stream_bytes_at(r(4), 1).unwrap();

    assert_eq!(text(&original.raw), "BT /F1 18 Tf 20 40 Td (Hello pdfxray) Tj ET");
    assert_eq!(original.file_range, 279..322);
    assert_eq!(
        text(&edited.raw),
        "BT /F1 18 Tf 20 40 Td (Hello pdfxray rev 2) Tj ET"
    );
    assert_eq!(edited.file_range, 701..750);
    assert_eq!(doc.stream_bytes(r(4)), Some(edited));
    assert_eq!(doc.object_range_at(r(4), 0), Some(247..339));
    assert_eq!(doc.object_range_at(r(4), 1), Some(669..767));
    assert_eq!(doc.object_range(r(4)), Some(669..767));
}

#[test]
fn objects_added_later_do_not_exist_in_older_revisions() {
    let doc = open("incremental-update");

    assert_eq!(doc.object_summary_at(r(6), 0), None);
    assert_eq!(doc.object_summary_at(r(6), 1).as_deref(), Some("<< 1 keys >>"));
    assert_eq!(doc.object_summary_at(r(3), 0).as_deref(), Some("/Type /Page"));
    assert_eq!(doc.object_props_at(r(6), 0), None);
    assert_eq!(doc.object_range_at(r(6), 0), None);
    assert_eq!(doc.stream_bytes_at(r(6), 0), None);
    assert_eq!(doc.object_summary_at(r(3), 5), None, "no such revision");
}

#[test]
fn object_props_at_the_newest_revision_are_the_inspector_props_and_older_ones_drop_referrers() {
    let doc = open("incremental-update");

    let newest = doc.object_props_at(r(4), 1).unwrap();
    let old = doc.object_props_at(r(4), 0).unwrap();

    assert_eq!(Some(newest.clone()), doc.node(&NodeId::Object(r(4))));
    assert_eq!(newest.get("Object", "Revision"), Some("1"));
    assert!(newest.groups.iter().any(|g| g.name == "Referenced by"));
    assert_eq!(old.get("Object", "Revision"), Some("0"));
    assert_eq!(old.get("Object", "Bytes"), Some("247..339 (92 bytes)"));
    assert!(!old.groups.iter().any(|g| g.name == "Referenced by"));
    assert_eq!(old.title, "4 0 obj");
    // Smoke: the two revisions differ in the stream, not in the shape of the dictionary.
    assert_eq!(old.get("Stream", "Encoded bytes"), Some("43"));
    assert_eq!(newest.get("Stream", "Encoded bytes"), Some("49"));
}
