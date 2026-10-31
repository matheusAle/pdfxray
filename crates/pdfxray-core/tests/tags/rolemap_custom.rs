//! The `rolemap-custom` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn rolemap_custom_shows_mapped_roles_and_the_cycle() {
    let doc = fixture("rolemap-custom");

    let r = root(&doc);
    let labels: Vec<_> = r.children[0].children.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Heading1 → H1", "MyPara → P", "LoopA"]);
    assert_eq!(diagnostics(&doc), [("rolemap-cycle", Severity::Error, Some(6))]);
    assert_eq!(badge_codes(&r), ["rolemap-cycle"]);
    assert_eq!(badge_codes(r.find(&elem(10)).unwrap()), ["rolemap-cycle"]);
    assert_eq!(r.find(&elem(8)).unwrap().badges, []);
    assert_eq!(r.find(&elem(10)).unwrap().badges[0].severity, Severity::Error);
    assert_eq!(
        doc.info().diagnostics[0].message,
        "the role map contains a cycle: LoopA → LoopB → LoopA"
    );
}

#[test]
fn rolemap_custom_props_record_the_chain_and_link_the_diagnostic() {
    let doc = fixture("rolemap-custom");

    let heading = props(&doc, &elem(8));
    assert_eq!(heading.title, "Heading1 → H1");
    assert_eq!(heading.get("Element", "Type"), Some("Heading1"));
    assert_eq!(heading.get("Element", "Standard type"), Some("H1"));
    assert_eq!(heading.get("Element", "Role chain"), Some("Heading1 → H1"));
    let looping = props(&doc, &elem(10));
    assert_eq!(
        looping.get("Element", "Standard type"),
        Some("none (role map cycle: LoopA → LoopB)")
    );
    assert_eq!(
        looping.get("Element", "Role chain"),
        Some("LoopA → LoopB → LoopA")
    );
    let diag = group(&looping, "Diagnostics").unwrap();
    assert_eq!(diag.rows.len(), 1);
    assert_eq!(diag.rows[0].key, "rolemap-cycle");
    assert_eq!(diag.rows[0].link, Some(NodeId::Diagnostic(0)));
    assert_eq!(
        props(&doc, &NodeId::Diagnostic(0)).get("Diagnostic", "Code"),
        Some("rolemap-cycle")
    );
    assert!(group(&heading, "Diagnostics").is_none());
}

#[test]
fn rolemap_custom_matches_its_expected_structure() {
    let doc = fixture("rolemap-custom");
    let facts = expected("rolemap-custom").struct_tree.unwrap();

    let want: Vec<Shape> = facts.children.iter().map(shape_of_elem_facts).collect();

    assert_eq!(shapes_of_root(&doc), want);
    assert!(matches!(
        expected("rolemap-custom").defects.as_slice(),
        [Defect::RoleMapCycle { cycle }] if cycle == &["LoopA".to_owned(), "LoopB".to_owned()]
    ));
}
