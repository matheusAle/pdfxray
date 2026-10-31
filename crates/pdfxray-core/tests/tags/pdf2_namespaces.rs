//! The `pdf2-namespaces` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn pdf2_namespaces_match_their_expected_outline() {
    let doc = fixture("pdf2-namespaces");
    let facts = expected("pdf2-namespaces").struct_outline.unwrap();

    let want: Vec<Shape> = facts.children.iter().map(shape_of_node_facts).collect();
    let got = shapes_of_root(&doc);

    assert_eq!(got, want);
    assert_eq!(count_elems(&got), 10);
    assert_eq!(diagnostics(&doc), []);
}

#[test]
fn pdf2_namespaces_map_chapter_and_aside_through_the_custom_namespace() {
    let doc = fixture("pdf2-namespaces");
    let r = root(&doc);

    assert_eq!(r.find(&elem(11)).unwrap().label, "Chapter → Sect");
    assert_eq!(r.find(&elem(19)).unwrap().label, "Aside → Div");
    assert_eq!(r.find(&elem(14)).unwrap().label, "math");
    let chapter = props(&doc, &elem(11));
    assert_eq!(
        chapter.get("Element", "Namespace"),
        Some("urn:pdfxray:test:chapters")
    );
    assert_eq!(
        chapter.get("Element", "Standard namespace"),
        Some("http://iso.org/pdf2/ssn")
    );
    assert_eq!(chapter.get("Element", "Standard type"), Some("Sect"));
    assert_eq!(
        props(&doc, &elem(19)).get("Element", "Standard type"),
        Some("Div")
    );
    let mi = props(&doc, &elem(15));
    assert_eq!(
        mi.get("Element", "Namespace"),
        Some("http://www.w3.org/1998/Math/MathML")
    );
    assert_eq!(mi.get("Element", "Standard type"), Some("mi"));
    assert_eq!(mi.get("Element", "Standard namespace"), None);
    assert_eq!(
        props(&doc, &elem(10)).get("Element", "Namespace"),
        Some("http://iso.org/pdf2/ssn")
    );
}

#[test]
fn pdf2_namespaces_phoneme_references_and_associated_files() {
    let doc = fixture("pdf2-namespaces");

    let paragraph = props(&doc, &elem(13));
    assert_eq!(paragraph.get("Element", "Phoneme"), Some("kat"));
    assert_eq!(paragraph.get("Element", "Phonetic alphabet"), Some("ipa"));
    let reference = group(&paragraph, "Element")
        .unwrap()
        .rows
        .iter()
        .find(|r| r.key == "Ref")
        .unwrap();
    assert_eq!(
        (reference.value.as_str(), reference.link.clone()),
        ("Aside → Div", Some(elem(19)))
    );
    let figure = props(&doc, &elem(18));
    let file = group(&figure, "Element")
        .unwrap()
        .rows
        .iter()
        .find(|r| r.key == "Associated file")
        .unwrap();
    assert_eq!(file.link, Some(NodeId::Object(obj(20))));
    assert_eq!(figure.get("Element", "Alt"), Some("Pale red box"));
}
