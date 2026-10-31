//! The rich outline, annotations, namespaces and embedded files, re-derived from the raw objects.

use std::collections::{BTreeMap, BTreeSet};

use pdfxray_fixtures::{
    AttributeSource, Expected, KidFacts, NodeFacts, ObjectParentFacts, OutlineFacts, all, find,
};
use pretty_assertions::assert_eq;

use crate::outline::{observe_annotations, observe_embedded_files, observe_outline};

fn built(name: &str) -> (Vec<u8>, Expected) {
    let b = find(name).unwrap().build().unwrap();
    (b.pdf, b.expected)
}

fn outline(expected: &Expected) -> &OutlineFacts {
    expected.struct_outline.as_ref().unwrap()
}

/// Depth-first walk over every element.
fn walk<'a>(nodes: &'a [NodeFacts], out: &mut Vec<&'a NodeFacts>) {
    for node in nodes {
        out.push(node);
        for kid in &node.kids {
            if let KidFacts::Elem(child) = kid {
                walk(std::slice::from_ref(child), out);
            }
        }
    }
}

fn elements(outline: &OutlineFacts) -> Vec<&NodeFacts> {
    let mut out = Vec::new();
    walk(&outline.children, &mut out);
    out
}

fn node(outline: &OutlineFacts, object: u32) -> &NodeFacts {
    elements(outline)
        .into_iter()
        .find(|n| n.object == object)
        .unwrap()
}

#[test]
fn rich_outlines_equal_what_the_objects_say() {
    let mut checked = Vec::new();
    for fixture in all() {
        let built = fixture.build().unwrap();
        let Some(declared) = &built.expected.struct_outline else {
            continue;
        };

        assert_eq!(
            observe_outline(&built.pdf).as_ref(),
            Some(declared),
            "{}",
            fixture.name
        );
        // A fixture describes its structure either as a plain tree or as a rich outline.
        assert_eq!(built.expected.struct_tree, None, "{}", fixture.name);
        checked.push(fixture.name);
    }

    assert_eq!(
        checked,
        [
            "struct-rich",
            "pdf2-namespaces",
            "annotations",
            "speech-footnote",
            "table-simple-pdf17",
            "table-scope-pdf2",
            "table-headers-ids",
            "speech-lang",
            "speech-link-objr",
        ]
    );
}

#[test]
fn struct_rich_preorder_lists_roles_and_the_roles_they_resolve_to() {
    let (_, expected) = built("struct-rich");

    let roles: Vec<(u32, &str, Option<&str>)> = elements(outline(&expected))
        .iter()
        .map(|n| (n.object, n.role.as_str(), n.standard_role.as_deref()))
        .collect();

    assert_eq!(
        roles,
        vec![
            (10, "Document", Some("Document")),
            (11, "Sect", Some("Sect")),
            (12, "H1", Some("H1")),
            (13, "P", Some("P")),
            (14, "Figure", Some("Figure")),
            (15, "Table", Some("Table")),
            (16, "TR", Some("TR")),
            (17, "TH", Some("TH")),
            // RoleMap: Cell -> TD, and Callout -> Remark -> Span.
            (18, "Cell", Some("TD")),
            (19, "L", Some("L")),
            (20, "LI", Some("LI")),
            (21, "Lbl", Some("Lbl")),
            (22, "LBody", Some("LBody")),
            (23, "Link", Some("Link")),
            (24, "Callout", Some("Span")),
        ]
    );
    assert_eq!(roles.len(), 15);
}

#[test]
fn struct_rich_attributes_alternate_text_and_ids_are_exact() {
    let (_, expected) = built("struct-rich");
    let outline = outline(&expected);

    let figure = node(outline, 14);
    assert_eq!(figure.alt.as_deref(), Some("A pale blue rectangle"));
    assert_eq!(figure.id.as_deref(), Some("fig-1"));
    assert_eq!(figure.classes, ["Wide"]);
    let figure_attrs: Vec<(String, String, Vec<(String, String)>)> = figure
        .attributes
        .iter()
        .map(|a| {
            (
                match &a.source {
                    AttributeSource::Direct => "direct".to_owned(),
                    AttributeSource::Class { name } => format!("class {name}"),
                },
                a.set.owner.clone(),
                a.set.entries.clone().into_iter().collect(),
            )
        })
        .collect();
    assert_eq!(
        figure_attrs,
        vec![
            (
                "direct".to_owned(),
                "Layout".to_owned(),
                vec![("BBox".to_owned(), "[20 230 140 310]".to_owned())]
            ),
            (
                "class Wide".to_owned(),
                "Layout".to_owned(),
                vec![("Placement".to_owned(), "/Block".to_owned())]
            ),
            (
                "class Wide".to_owned(),
                "Layout".to_owned(),
                vec![("Width".to_owned(), "120".to_owned())]
            ),
        ]
    );

    let header = node(outline, 17);
    assert_eq!(header.attributes[0].set.owner, "Table");
    assert_eq!(
        header.attributes[0].set.entries.get("Scope").map(String::as_str),
        Some("/Column")
    );
    assert_eq!(node(outline, 13).lang.as_deref(), Some("fr-FR"));
    assert_eq!(
        node(outline, 24).actual_text.as_deref(),
        Some("World Wide Web Consortium")
    );
    assert_eq!(
        outline
            .id_tree
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect::<Vec<_>>(),
        [("fig-1", 14), ("tbl-1", 15), ("th-1", 17)]
    );
    // Smoke: every /ID in the tree is reachable through the IDTree and vice versa.
    let ids: BTreeMap<String, u32> = elements(outline)
        .iter()
        .filter_map(|n| n.id.clone().map(|id| (id, n.object)))
        .collect();
    assert_eq!(ids, outline.id_tree);
}

#[test]
fn struct_rich_paragraph_continues_on_the_next_page_and_the_link_has_an_object_reference() {
    let (_, expected) = built("struct-rich");
    let outline = outline(&expected);

    let paragraph = node(outline, 13);
    let link = node(outline, 23);

    assert_eq!(
        paragraph.kids,
        vec![
            KidFacts::Mcr {
                page: 0,
                mcid: 1,
                stream: None
            },
            KidFacts::Mcr {
                page: 1,
                mcid: 0,
                stream: None
            },
        ]
    );
    assert_eq!(
        link.kids,
        vec![
            KidFacts::Mcr {
                page: 1,
                mcid: 1,
                stream: None
            },
            KidFacts::Objr {
                page: Some(1),
                object: 9
            },
        ]
    );
    assert_eq!(outline.parent_tree_nodes, [26, 27, 28]);
    assert_eq!(
        outline.parent_tree_objects,
        [ObjectParentFacts { key: 2, parent: 23 }]
    );
}

#[test]
fn parent_trees_agree_with_the_content_references_in_the_outline() {
    for name in ["struct-rich", "pdf2-namespaces", "annotations"] {
        let (pdf, expected) = built(name);
        let outline = outline(&expected);

        // Owners per (page, mcid) according to the marked-content references.
        let mut owners: BTreeMap<u32, BTreeMap<i32, u32>> = BTreeMap::new();
        for n in elements(outline) {
            for kid in &n.kids {
                if let KidFacts::Mcr { page, mcid, .. } = kid {
                    owners.entry(*page).or_default().insert(*mcid, n.object);
                }
            }
        }
        for key in &outline.page_keys {
            let by_mcid = &owners[&key.page];
            let expected_array: Vec<Option<u32>> = (0..=*by_mcid.keys().max().unwrap())
                .map(|mcid| by_mcid.get(&mcid).copied())
                .collect();
            let entry = outline.parent_tree.iter().find(|e| e.key == key.key).unwrap();

            assert_eq!(entry.parents, expected_array, "{name} page {}", key.page);
            // The MCIDs the page content uses are exactly those some element references.
            let used: BTreeSet<i32> = expected.pages[key.page as usize].mcids.iter().copied().collect();
            assert_eq!(
                used,
                by_mcid.keys().copied().collect::<BTreeSet<_>>(),
                "{name} page {}",
                key.page
            );
        }

        // Annotation keys: the owner named by the parent tree holds an OBJR to that annotation.
        let annots = observe_annotations(&pdf);
        for annot in annots.iter().filter(|a| a.struct_parent.is_some()) {
            let key = annot.struct_parent.unwrap();
            let entry = outline.parent_tree_objects.iter().find(|e| e.key == key).unwrap();
            let owner = node(outline, entry.parent);

            assert!(
                owner
                    .kids
                    .iter()
                    .any(|k| matches!(k, KidFacts::Objr { object, .. } if *object == annot.object)),
                "{name}: annotation {} is not referenced by element {}",
                annot.object,
                owner.object
            );
        }
    }
}

#[test]
fn pdf2_namespaces_resolve_roles_through_role_map_ns() {
    let (pdf, expected) = built("pdf2-namespaces");
    let outline = outline(&expected);

    let resolved: Vec<(&str, Option<&str>, &str)> = elements(outline)
        .iter()
        .map(|n| {
            (
                n.role.as_str(),
                n.standard_role.as_deref(),
                n.namespace.as_deref().unwrap_or("-"),
            )
        })
        .collect();

    assert_eq!(
        resolved,
        vec![
            ("Document", Some("Document"), "http://iso.org/pdf2/ssn"),
            // Chapter lives in the custom namespace; RoleMapNS sends it to Sect in PDF 2.0.
            ("Chapter", Some("Sect"), "urn:pdfxray:test:chapters"),
            ("H1", Some("H1"), "http://iso.org/pdf2/ssn"),
            ("P", Some("P"), "http://iso.org/pdf2/ssn"),
            ("math", Some("math"), "http://www.w3.org/1998/Math/MathML"),
            ("mi", Some("mi"), "http://www.w3.org/1998/Math/MathML"),
            ("mo", Some("mo"), "http://www.w3.org/1998/Math/MathML"),
            ("mn", Some("mn"), "http://www.w3.org/1998/Math/MathML"),
            ("Figure", Some("Figure"), "http://iso.org/pdf2/ssn"),
            ("Aside", Some("Div"), "urn:pdfxray:test:chapters"),
        ]
    );
    let chapter = node(outline, 11);
    let aside = node(outline, 19);
    assert_eq!(
        chapter.standard_namespace.as_deref(),
        Some("http://iso.org/pdf2/ssn")
    );
    assert_eq!(
        aside.standard_namespace.as_deref(),
        Some("http://iso.org/pdf2/ssn")
    );
    assert_eq!(
        outline.namespaces.iter().map(|n| n.object).collect::<Vec<_>>(),
        [7, 8, 9]
    );

    let paragraph = node(outline, 13);
    assert_eq!(
        (
            paragraph.phoneme.as_deref(),
            paragraph.phonetic_alphabet.as_deref(),
            paragraph.refs.clone()
        ),
        (Some("kat"), Some("ipa"), vec![19])
    );
    let files = observe_embedded_files(&pdf, outline);
    assert_eq!(files, expected.embedded_files);
    assert_eq!(
        files
            .iter()
            .map(|f| (f.name.as_str(), f.relationship.as_str(), f.size))
            .collect::<Vec<_>>(),
        [("data.txt", "Data", 12)]
    );
    assert_eq!(node(outline, 18).associated_files, [20]);
    assert_eq!(expected.header_version, "2.0");
}

#[test]
fn annotations_equal_what_the_page_annots_arrays_say() {
    let mut checked = Vec::new();
    for fixture in all() {
        let built = fixture.build().unwrap();
        if built.expected.annotations.is_empty() {
            assert_eq!(observe_annotations(&built.pdf), [], "{}", fixture.name);
            continue;
        }

        assert_eq!(
            observe_annotations(&built.pdf),
            built.expected.annotations,
            "{}",
            fixture.name
        );
        checked.push((fixture.name, built.expected.annotations.len()));
    }

    assert_eq!(
        checked,
        [("struct-rich", 1), ("annotations", 5), ("speech-link-objr", 3)]
    );
}

#[test]
fn annotations_fixture_flags_tagging_and_widget_details() {
    let (pdf, expected) = built("annotations");

    let rows: Vec<(u32, &str, u32, Option<i32>)> = expected
        .annotations
        .iter()
        .map(|a| (a.object, a.subtype.as_str(), a.flags, a.struct_parent))
        .collect();

    assert_eq!(
        rows,
        vec![
            (17, "Link", 4, Some(1)),
            (18, "Link", 4, Some(2)),
            (19, "Widget", 4, Some(3)),
            (20, "Text", 4, Some(4)),
            // Flag value 2 is the Hidden bit; the hidden annotation is not tagged.
            (21, "Square", 2, None),
        ]
    );
    let hidden: Vec<u32> = expected
        .annotations
        .iter()
        .filter(|a| a.flags & 2 != 0)
        .map(|a| a.object)
        .collect();
    assert_eq!(hidden, [21]);
    let widget = &expected.annotations[2];
    assert_eq!(
        (
            widget.field_type.as_deref(),
            widget.field_name.as_deref(),
            widget.has_appearance
        ),
        (Some("Tx"), Some("name"), true)
    );
    assert_eq!(
        expected.annotations[0].uri.as_deref(),
        Some("https://example.com/a")
    );
    // The GoTo link targets the second page object (7), i.e. page index 1.
    assert_eq!(expected.annotations[1].goto_page, Some(1));
    // AcroForm lists exactly the widget, and the widget's appearance stream is a form XObject.
    let text = crate::common::latin1(&pdf);
    assert!(text.contains("/Fields [19 0 R]"));
    assert!(crate::common::text_of(&pdf, 22).contains("/Subtype /Form"));
    // Struct elements Link, Link, Form, Annot each hold one OBJR.
    let outline = outline(&expected);
    let objrs: Vec<(&str, u32)> = elements(outline)
        .iter()
        .flat_map(|n| {
            n.kids.iter().filter_map(move |k| match k {
                KidFacts::Objr { object, .. } => Some((n.role.as_str(), *object)),
                _ => None,
            })
        })
        .collect();
    assert_eq!(objrs, [("Link", 17), ("Link", 18), ("Form", 19), ("Annot", 20)]);
}
