//! The `untagged-content` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn untagged_content_reports_runs_duplicates_and_nesting_inside_artifacts() {
    let doc = fixture("untagged-content");
    let facts = expected("untagged-content");

    let got: Vec<_> = doc
        .info()
        .diagnostics
        .iter()
        .map(|d| (d.code, d.severity, d.object.map(|o| o.num), d.message.clone()))
        .collect();

    let page = Some(3);
    assert_eq!(
        got,
        [
            (
                "mcid-duplicate-in-content",
                Severity::Warning,
                page,
                "MCID 1 is used by 2 marked-content sequences in the content of page 1".to_owned()
            ),
            (
                "marked-content-in-artifact",
                Severity::Warning,
                page,
                "operator 37 on page 1 opens /Span inside an artifact; content inside an artifact is not part of the structure"
                    .to_owned()
            ),
            (
                "untagged-content",
                Severity::Warning,
                page,
                "operators 17..18 on page 1 paint outside any tagged or artifact marked-content sequence"
                    .to_owned()
            ),
            (
                "untagged-content",
                Severity::Warning,
                page,
                "operators 28..29 on page 1 paint outside any tagged or artifact marked-content sequence"
                    .to_owned()
            ),
        ]
    );
    // Four defects in the facts, four diagnostics.
    assert_eq!(facts.defects.len(), got.len());
    for defect in &facts.defects {
        let found = match defect {
            Defect::UntaggedContent {
                op_index_range: [a, b],
                ..
            } => got
                .iter()
                .any(|g| g.0 == "untagged-content" && g.3.contains(&format!("operators {a}..{b} "))),
            Defect::DuplicateMcid { mcid, count, .. } => got.iter().any(|g| {
                g.0 == "mcid-duplicate-in-content"
                    && g.3.contains(&format!("MCID {mcid} is used by {count} "))
            }),
            Defect::MarkedContentInsideArtifact { op_index, tag, .. } => got.iter().any(|g| {
                g.0 == "marked-content-in-artifact"
                    && g.3
                        .contains(&format!("operator {op_index} on page 1 opens /{tag} "))
            }),
            other => panic!("unexpected defect {other:?}"),
        };
        assert!(found, "{defect:?}");
    }
}

#[test]
fn untagged_content_structure_itself_is_sound() {
    let doc = fixture("untagged-content");
    let facts = expected("untagged-content").struct_tree.unwrap();

    let want: Vec<Shape> = facts.children.iter().map(shape_of_elem_facts).collect();

    assert_eq!(shapes_of_root(&doc), want);
    // The element for the content nested in an artifact is still reachable by hit-testing.
    let (_, nested) = doc.bbox(&mc(0, 2)).unwrap();
    assert_eq!(
        doc.hit(0, center(nested), LayerMode::Tags),
        vec![mc(0, 2), elem(10), elem(7), NodeId::Page(0)]
    );
    assert_eq!(root(&doc).find(&elem(9)).unwrap().badges, []);
}
