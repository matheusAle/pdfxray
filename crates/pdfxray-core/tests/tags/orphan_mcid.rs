//! The `orphan-mcid` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn orphan_mcid_reports_exactly_its_three_defects() {
    let doc = fixture("orphan-mcid");

    assert_eq!(
        diagnostics(&doc),
        [
            ("parent-tree-mismatch", Severity::Error, Some(10)),
            ("mcid-not-in-content", Severity::Error, Some(9)),
            ("mcid-not-in-structure", Severity::Error, Some(3)),
        ]
    );
    // Each one is a deliberate defect of the fixture, by object.
    let mut from_facts: Vec<(&str, u32)> = expected("orphan-mcid")
        .defects
        .iter()
        .map(|d| match d {
            Defect::OrphanMcid { .. } => ("mcid-not-in-structure", 3),
            Defect::MissingMcid { elem, .. } => ("mcid-not-in-content", *elem),
            Defect::ParentTreeMismatch { k_parent, .. } => ("parent-tree-mismatch", *k_parent),
            other => panic!("unexpected defect {other:?}"),
        })
        .collect();
    from_facts.sort_unstable();
    let mut got: Vec<(&str, u32)> = diagnostics(&doc).iter().map(|d| (d.0, d.2.unwrap())).collect();
    got.sort_unstable();
    assert_eq!(got, from_facts);
    assert_eq!(doc.info().diagnostics.len(), 3);
}

#[test]
fn orphan_mcid_messages_name_both_sides() {
    let doc = fixture("orphan-mcid");
    let messages: Vec<_> = doc.info().diagnostics.iter().map(|d| d.message.clone()).collect();

    assert_eq!(
        messages,
        [
            "ParentTree key 0 names 8 0 R for MCID 2 on page 1, but 10 0 R references it in /K",
            "9 0 R references MCID 5 on page 1, but that page's content has no such sequence",
            "MCID 1 on page 1 is not referenced by any structure element (orphan content)",
        ]
    );
    assert!(doc.info().diagnostics.iter().all(|d| d.span[1] > d.span[0]));
}

#[test]
fn orphan_mcid_badges_sit_on_the_offending_elements() {
    let doc = fixture("orphan-mcid");
    let r = root(&doc);

    assert_eq!(r.find(&elem(8)).unwrap().badges, []);
    assert_eq!(badge_codes(r.find(&elem(9)).unwrap()), ["mcid-not-in-content"]);
    assert_eq!(badge_codes(r.find(&elem(10)).unwrap()), ["parent-tree-mismatch"]);
    assert!(r.badges.is_empty(), "the orphan MCID belongs to no element");
    let looking_at_10 = props(&doc, &elem(10));
    assert_eq!(
        group(&looking_at_10, "Diagnostics").unwrap().rows[0].link,
        Some(NodeId::Diagnostic(0))
    );
    let on_9 = props(&doc, &elem(9));
    assert_eq!(
        group(&on_9, "Diagnostics").unwrap().rows[0].link,
        Some(NodeId::Diagnostic(1))
    );
}

#[test]
fn orphan_mcid_parent_tree_entries_show_on_the_element_they_name() {
    let doc = fixture("orphan-mcid");

    // The ParentTree claims element 8 for MCID 0 and MCID 2, although MCID 2 is in element 10's /K.
    assert_eq!(
        rows(&props(&doc, &elem(8)), "Structure", "ParentTree"),
        ["key 0 · MCID 0 · Page 1", "key 0 · MCID 2 · Page 1"]
    );
    assert_eq!(
        rows(&props(&doc, &elem(10)), "Structure", "ParentTree"),
        Vec::<&str>::new()
    );
}

#[test]
fn orphan_mcid_has_no_owner_in_a_hit_stack() {
    let doc = fixture("orphan-mcid");
    let (_, orphan) = doc.bbox(&mc(0, 1)).unwrap();

    let hits = doc.hit(0, center(orphan), LayerMode::Tags);

    assert_eq!(hits, vec![mc(0, 1), NodeId::Page(0)]);
    // Content that /K does name: the owner follows /K, not the ParentTree.
    let (_, parent_tree_disagrees) = doc.bbox(&mc(0, 2)).unwrap();
    assert_eq!(
        doc.hit(0, center(parent_tree_disagrees), LayerMode::Tags),
        vec![mc(0, 2), elem(10), elem(7), NodeId::Page(0)]
    );
}
