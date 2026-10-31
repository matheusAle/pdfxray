//! The `tagged-simple` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn tagged_simple_is_document_h1_p() {
    let doc = fixture("tagged-simple");

    assert_eq!(
        outline_of(&doc),
        [
            "StructTreeRoot [group:100]",
            "  Document [struct:7:0]",
            "    H1 [struct:8:0]",
            "      MCID 0 [mc:0:0]",
            "    P [struct:9:0]",
            "      MCID 1 [mc:0:1]",
        ]
    );
    let r = root(&doc);
    assert_eq!(r.detail.as_deref(), Some("3 elements, /Marked true"));
    let h1 = r.find(&elem(8)).unwrap();
    assert_eq!(h1.detail.as_deref(), Some("\"Heading\" · 1 MCID"));
    assert_eq!(
        r.find(&elem(7)).unwrap().detail.as_deref(),
        Some("\"Heading Body paragraph\" · 2 elements")
    );
    assert_eq!(r.find(&mc(0, 1)).unwrap().detail.as_deref(), Some("Page 1"));
    // Smoke: nothing is wrong with this file and the tree is the whole thing.
    assert_eq!(diagnostics(&doc), []);
    assert_eq!(r.len(), 6);
    assert_eq!(r.badges, []);
}

#[test]
fn tagged_simple_matches_its_expected_structure() {
    let doc = fixture("tagged-simple");
    let facts = expected("tagged-simple").struct_tree.unwrap();

    let want: Vec<Shape> = facts.children.iter().map(shape_of_elem_facts).collect();

    assert_eq!(shapes_of_root(&doc), want);
    assert_eq!(count_elems(&want), 3);
}

#[test]
fn tagged_simple_element_props_cover_every_group() {
    let doc = fixture("tagged-simple");

    let h1 = props(&doc, &elem(8));

    assert_eq!(h1.title, "H1");
    assert_eq!(h1.kind, "struct-elem");
    assert_eq!(h1.get("Element", "Type"), Some("H1"));
    assert_eq!(h1.get("Element", "Standard type"), Some("H1"));
    assert_eq!(h1.get("Element", "Role chain"), None);
    assert_eq!(
        h1.get("Element", "Effective lang"),
        Some("en-US (from the catalog)")
    );
    assert_eq!(h1.get("Content", "Page"), Some("Page 1"));
    assert_eq!(h1.get("Content", "MCID"), Some("0 · Page 1"));
    assert_eq!(h1.get("Content", "Text"), Some("Heading"));
    assert_eq!(h1.get("Structure", "Parent"), Some("Document"));
    assert_eq!(h1.get("Structure", "Depth"), Some("1"));
    assert_eq!(h1.get("Structure", "Children"), Some("1 MCID"));
    assert_eq!(h1.get("Structure", "ParentTree"), Some("key 0 · MCID 0 · Page 1"));
    assert_eq!(h1.get("Object", "Reference"), Some("8 0 R"));
    assert_eq!(h1.get("Object", "Pg"), Some("Page 1"));
    let links: Vec<_> = group(&h1, "Content")
        .unwrap()
        .rows
        .iter()
        .filter_map(|r| r.link.clone())
        .collect();
    assert_eq!(links, [NodeId::Page(0), mc(0, 0)]);
    let parent = group(&h1, "Structure").unwrap().rows[0].link.clone();
    assert_eq!(parent, Some(elem(7)));
    // No attributes, no diagnostics: those groups are not shown at all.
    assert!(group(&h1, "Attributes").is_none());
    assert!(group(&h1, "Diagnostics").is_none());

    let document = props(&doc, &elem(7));
    assert_eq!(document.get("Structure", "Parent"), Some("StructTreeRoot"));
    assert_eq!(document.get("Structure", "Child 2"), Some("P"));
    assert!(doc.node(&elem(999)).is_none());
}

#[test]
fn tagged_simple_hit_stack_runs_from_content_to_the_document_element() {
    let doc = fixture("tagged-simple");
    let (page, heading) = doc.bbox(&mc(0, 0)).unwrap();
    assert_eq!(page, 0);

    let hits = doc.hit(0, center(heading), LayerMode::Tags);

    assert_eq!(hits, vec![mc(0, 0), elem(8), elem(7), NodeId::Page(0)]);
    // The same point with all layers still has the tags in order, ending at the page.
    let all = doc.hit(0, center(heading), LayerMode::All);
    let tag_part: Vec<_> = all
        .iter()
        .filter(|h| matches!(h, NodeId::StructElem(_)))
        .cloned()
        .collect();
    assert_eq!(tag_part, vec![elem(8), elem(7)]);
    assert_eq!(all.last(), Some(&NodeId::Page(0)));
}

#[test]
fn tagged_simple_breadcrumb_and_bbox() {
    let doc = fixture("tagged-simple");

    assert_eq!(doc.breadcrumb(&elem(9)), vec![NodeId::Document, elem(7), elem(9)]);
    assert_eq!(doc.breadcrumb(&elem(7)), vec![NodeId::Document, elem(7)]);
    let (p_page, p_box) = doc.bbox(&elem(9)).unwrap();
    assert_eq!((p_page, doc.bbox(&mc(0, 1))), (0, Some((0, p_box))));
    let (_, h1_box) = doc.bbox(&elem(8)).unwrap();
    assert_eq!(doc.bbox(&mc(0, 0)).map(|(_, b)| b), Some(h1_box));
    let (doc_page, document_box) = doc.bbox(&elem(7)).unwrap();
    assert_eq!(doc_page, 0);
    assert_eq!(
        document_box,
        Rect::new(
            h1_box.x0.min(p_box.x0),
            h1_box.y0.min(p_box.y0),
            h1_box.x1.max(p_box.x1),
            h1_box.y1.max(p_box.y1)
        )
    );
    assert!(h1_box.y0 > p_box.y1, "the heading is above the paragraph");
    assert_eq!(doc.bbox(&elem(999)), None);
}
