//! The `struct-rich` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn struct_rich_matches_its_expected_outline() {
    let doc = fixture("struct-rich");
    let facts = expected("struct-rich").struct_outline.unwrap();

    let want: Vec<Shape> = facts.children.iter().map(shape_of_node_facts).collect();
    let got = shapes_of_root(&doc);

    assert_eq!(got, want);
    assert_eq!(
        diagnostics(&doc),
        [
            // `Callout` reaches `Span` through `Remark`: two role map hops.
            ("rolemap-multi-hop", Severity::Warning, Some(24)),
            // The data cell beside a `Scope /Column` header has no header of its own.
            ("table-header-unresolved", Severity::Info, Some(18)),
        ]
    );
    // Smoke: all 15 elements, none missing and none extra.
    assert_eq!(count_elems(&got), 15);
    assert_eq!(root(&doc).detail.as_deref(), Some("15 elements, /Marked true"));
}

#[test]
fn struct_rich_attributes_come_from_the_element_and_from_classes() {
    let doc = fixture("struct-rich");

    let figure = props(&doc, &elem(14));
    assert_eq!(figure.get("Attributes", "Classes"), Some("Wide"));
    assert_eq!(figure.get("Attributes", "Layout: BBox"), Some("[20 230 140 310]"));
    assert_eq!(
        rows(&figure, "Attributes", "Layout: Placement"),
        ["/Block (from class Wide)"]
    );
    assert_eq!(
        rows(&figure, "Attributes", "Layout: Width"),
        ["120 (from class Wide)"]
    );
    assert_eq!(figure.get("Element", "Alt"), Some("A pale blue rectangle"));
    assert_eq!(figure.get("Element", "ID"), Some("fig-1"));

    let paragraph = props(&doc, &elem(13));
    assert_eq!(paragraph.get("Element", "Lang"), Some("fr-FR"));
    assert_eq!(paragraph.get("Element", "Effective lang"), None);
    assert_eq!(
        paragraph.get("Attributes", "Layout: TextAlign"),
        Some("/Justify (from class Body)")
    );

    let th = props(&doc, &elem(17));
    assert_eq!(th.get("Attributes", "Table: Scope"), Some("/Column"));
    assert_eq!(th.get("Attributes", "Classes"), None);
    // A child of the French paragraph's parent inherits the catalog language.
    assert_eq!(
        props(&doc, &elem(12)).get("Element", "Effective lang"),
        Some("en-US (from the catalog)")
    );
}

#[test]
fn struct_rich_role_map_chain_reaches_span_through_remark() {
    let doc = fixture("struct-rich");

    let callout = props(&doc, &elem(24));

    assert_eq!(callout.title, "Callout → Span");
    assert_eq!(callout.get("Element", "Standard type"), Some("Span"));
    assert_eq!(
        callout.get("Element", "Role chain"),
        Some("Callout → Remark → Span")
    );
    assert_eq!(
        callout.get("Element", "ActualText"),
        Some("World Wide Web Consortium")
    );
    assert_eq!(
        props(&doc, &elem(18)).get("Element", "Role chain"),
        Some("Cell → TD")
    );
    let table = root(&doc);
    assert_eq!(
        table.find(&elem(24)).unwrap().detail.as_deref(),
        Some("ActualText: \"World Wide Web Consortium\" · 1 MCID")
    );
    assert_eq!(
        table.find(&elem(14)).unwrap().detail.as_deref(),
        Some("Alt: \"A pale blue rectangle\" · 1 MCID")
    );
}

#[test]
fn struct_rich_content_spans_pages_and_references_the_annotation() {
    let doc = fixture("struct-rich");
    let r = root(&doc);

    // One paragraph, two pages.
    let paragraph = r.find(&elem(13)).unwrap();
    let kids: Vec<_> = paragraph.children.iter().map(|c| c.id.clone()).collect();
    assert_eq!(kids, [mc(0, 1), mc(1, 0)]);
    assert_eq!(paragraph.children[1].detail.as_deref(), Some("Page 2"));
    // The link: a bare MCID that takes its page from /Pg, then the OBJR.
    let link = r.find(&elem(23)).unwrap();
    let link_kids: Vec<_> = link
        .children
        .iter()
        .map(|c| (c.id.clone(), c.label.as_str(), c.detail.as_deref()))
        .collect();
    assert_eq!(
        link_kids,
        [
            (mc(1, 1), "MCID 1", Some("Page 2")),
            (NodeId::Object(obj(9)), "OBJR", Some("Link")),
        ]
    );
    let props = props(&doc, &elem(23));
    assert_eq!(rows(&props, "Content", "OBJR"), ["9 0 R · Link · Page 2"]);
    // The paragraph's text is the concatenation of both pages' content.
    assert_eq!(
        self::props(&doc, &elem(13)).get("Content", "Text"),
        Some("Un paragraphe qui continue sur la page suivante")
    );
}

#[test]
fn struct_rich_parent_tree_with_kids_maps_every_slot_and_object() {
    let doc = fixture("struct-rich");
    let facts = expected("struct-rich").struct_outline.unwrap();

    // Number tree nodes /Kids -> /Limits -> /Nums were all read.
    let mut checked = 0;
    for entry in &facts.parent_tree {
        let page = facts.page_keys.iter().find(|k| k.key == entry.key).unwrap().page;
        for (mcid, parent) in (0..).zip(&entry.parents) {
            let want = format!("key {} · MCID {mcid} · Page {}", entry.key, page + 1);
            let owner = props(&doc, &elem(parent.unwrap()));
            assert!(
                rows(&owner, "Structure", "ParentTree").contains(&want.as_str()),
                "{want}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 10);
    for object in &facts.parent_tree_objects {
        let owner = props(&doc, &elem(object.parent));
        assert!(rows(&owner, "Structure", "ParentTree").contains(&"key 2 · object entry"));
    }
    assert_eq!(facts.parent_tree_objects.len(), 1);
}

#[test]
fn struct_rich_hit_on_the_second_page_finds_the_link_elements() {
    let doc = fixture("struct-rich");
    let (page, link_text) = doc.bbox(&mc(1, 1)).unwrap();
    assert_eq!(page, 1);

    let hits = doc.hit(1, center(link_text), LayerMode::Tags);

    assert_eq!(
        hits,
        vec![mc(1, 1), elem(23), elem(11), elem(10), NodeId::Page(1)]
    );
    assert_eq!(
        doc.breadcrumb(&elem(23)),
        vec![NodeId::Document, elem(10), elem(11), elem(23)]
    );
    // The element's box is its marked content, not the annotation.
    assert_eq!(doc.bbox(&elem(23)), Some((1, link_text)));
}

#[test]
fn struct_rich_paragraph_bbox_is_on_the_first_page_it_touches() {
    let doc = fixture("struct-rich");
    let (_, first) = doc.bbox(&mc(0, 1)).unwrap();

    let (page, paragraph) = doc.bbox(&elem(13)).unwrap();

    assert_eq!(page, 0);
    assert_eq!(paragraph, first);
    assert_eq!(doc.bbox(&elem(15)).unwrap().0, 0);
}
