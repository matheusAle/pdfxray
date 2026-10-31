//! The `long-doc` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn long_doc_builds_201_elements_quickly() {
    let path = pdfxray_fixtures::pdf_path("long-doc");

    let started = Instant::now();
    let doc = Document::open(path).unwrap();
    let opened = started.elapsed();
    let tree_started = Instant::now();
    let r = root(&doc);
    let rendered = tree_started.elapsed();

    println!("long-doc: open {opened:?} (structure build included), tags tree {rendered:?}");
    assert_eq!(r.detail.as_deref(), Some("201 elements, /Marked true"));
    // 1 root + 201 elements + 40 * 4 marked-content leaves.
    assert_eq!(r.len(), 1 + 201 + 160);
    assert_eq!(diagnostics(&doc), []);
    let want: Vec<Shape> = expected("long-doc")
        .struct_tree
        .unwrap()
        .children
        .iter()
        .map(shape_of_elem_facts)
        .collect();
    assert_eq!(shapes_of_root(&doc), want);
    assert_eq!(count_elems(&want), 201);
    // Soft check: only meaningful for optimised builds.
    if !cfg!(debug_assertions) {
        assert!(opened.as_millis() < 200, "open took {opened:?}");
    }
}

#[test]
fn long_doc_hit_on_the_last_page_climbs_to_its_section() {
    let doc = fixture("long-doc");
    let (page, third) = doc.bbox(&mc(39, 3)).unwrap();
    assert_eq!(page, 39);

    let hits = doc.hit(39, center(third), LayerMode::Tags);

    // Last page: Sect = 7 + 7 * 39 + 2, its third paragraph = Sect + 4.
    assert_eq!(
        hits,
        vec![
            mc(39, 3),
            elem(7 + 7 * 39 + 6),
            elem(7 + 7 * 39 + 2),
            elem(5),
            NodeId::Page(39)
        ]
    );
    assert_eq!(doc.breadcrumb(&elem(7 + 7 * 39 + 6)).len(), 4);
}
