//! The `tagged-simple`, `orphan-mcid` and `artifacts` fixtures.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn tagged_simple_has_two_sequences_with_their_text() {
    let page = fixture_page("tagged-simple", 0);

    assert_eq!(
        page.marked.iter().map(seq_summary).collect::<Vec<_>>(),
        vec![
            ("H1", Some(0), 0, Some(6), None),
            ("P", Some(1), 7, Some(13), None)
        ]
    );
    assert_eq!(
        page.marked[0].properties,
        vec![("MCID".to_owned(), "0".to_owned())]
    );
    assert_eq!(
        page.marked[1].properties,
        vec![("MCID".to_owned(), "1".to_owned())]
    );
    let all = glyphs(&page);
    let heading: Vec<_> = all.iter().filter(|g| g.item.mc_stack == [0]).collect();
    let body: Vec<_> = all.iter().filter(|g| g.item.mc_stack == [1]).collect();
    assert_eq!(heading.iter().map(|g| g.unicode).collect::<String>(), "Heading");
    assert_eq!(
        body.iter().map(|g| g.unicode).collect::<String>(),
        "Body paragraph"
    );
    assert_eq!(heading.len() + body.len(), all.len());
    // A sequence's box is the union of its items' boxes.
    let heading_items: Vec<_> = heading.iter().map(|g| g.item).collect();
    assert_rect(page.marked[0].bbox.unwrap(), {
        let u = union(&heading_items);
        [u.x0, u.y0, u.x1, u.y1]
    });
    assert!(close(heading[0].item.gstate.text.font_size, 24.0));
    // mc_depth is the depth after each operator.
    assert_eq!(
        page.ops.iter().map(|o| o.mc_depth).collect::<Vec<_>>(),
        vec![1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0]
    );
    assert_consistent(&page);
}

#[test]
fn page_mcids_match_the_fixture_facts() {
    for name in [
        "tagged-simple",
        "orphan-mcid",
        "artifacts",
        "form-xobject-nesting",
    ] {
        let page = fixture_page(name, 0);
        let facts = expected(name);
        // Only the page's own stream is listed in the facts.
        let own: Vec<i32> = page
            .marked
            .iter()
            .filter(|s| s.stream == StreamRef::PageContent { index: 0 })
            .filter_map(|s| s.mcid)
            .collect();

        assert_eq!(own, facts.pages[0].mcids, "{name}");
        assert_eq!(page.alignment, Alignment::Exact, "{name}");
    }
}

#[test]
fn orphan_mcid_captures_all_three_ids_with_their_text() {
    let page = fixture_page("orphan-mcid", 0);

    assert_eq!(
        page.marked
            .iter()
            .map(|s| (s.tag.as_str(), s.mcid))
            .collect::<Vec<_>>(),
        vec![("P", Some(0)), ("Span", Some(1)), ("P", Some(2))]
    );
    let all = glyphs(&page);
    let by_mcid = |mcid: usize| -> String {
        all.iter()
            .filter(|g| g.item.mc_stack == [u32::try_from(mcid).unwrap()])
            .map(|g| g.unicode)
            .collect()
    };
    assert_eq!(by_mcid(0), "Properly tagged");
    assert_eq!(by_mcid(1), "Orphan content");
    assert_eq!(by_mcid(2), "Parent tree disagrees");
    assert!(all.iter().all(|g| g.item.mc_stack.len() == 1));
    // The orphan sits between the other two on the page (y 120, between 150 and 90).
    assert!(close(page.marked[1].bbox.unwrap().y0, 120.0 - 0.207 * 12.0));
}

#[test]
fn artifacts_report_type_subtype_and_bare_artifacts() {
    let page = fixture_page("artifacts", 0);

    let infos: Vec<_> = page
        .marked
        .iter()
        .map(|s| {
            s.artifact
                .as_ref()
                .map(|a| (a.kind.as_deref(), a.subtype.as_deref()))
        })
        .collect();
    assert_eq!(
        infos,
        vec![
            Some((Some("Pagination"), Some("Header"))),
            None,
            Some((Some("Layout"), None)),
            Some((Some("Pagination"), Some("Footer"))),
            Some((None, None)),
        ]
    );
    assert_eq!(
        page.marked[0].properties,
        vec![
            ("Subtype".to_owned(), "/Header".to_owned()),
            ("Type".to_owned(), "/Pagination".to_owned())
        ]
    );
    // The bare `/Artifact BMC` has no property list at all.
    assert_eq!(page.marked[4].properties, vec![]);
    assert_eq!(page.marked[4].property_resource, None);
    // Same artifacts, same order, as the fixture facts.
    let facts = expected("artifacts");
    let from_facts: Vec<_> = facts
        .artifacts
        .iter()
        .map(|a| (a.artifact_type.clone(), a.subtype.clone()))
        .collect();
    let captured: Vec<_> = page
        .marked
        .iter()
        .filter_map(|s| s.artifact.as_ref())
        .map(|a| (a.kind.clone(), a.subtype.clone()))
        .collect();
    assert_eq!(captured, from_facts);
}

#[test]
fn artifacts_own_the_content_inside_them() {
    let page = fixture_page("artifacts", 0);
    let all = glyphs(&page);

    let header: String = all
        .iter()
        .filter(|g| g.item.mc_stack == [0])
        .map(|g| g.unicode)
        .collect();
    let footer: String = all
        .iter()
        .filter(|g| g.item.mc_stack == [3])
        .map(|g| g.unicode)
        .collect();
    assert_eq!(header, "Page header");
    assert_eq!(footer, "Page footer");
    // The layout artifact holds the grey rule, the bare artifact the grey frame.
    let rule = &page.items[page.marked[2].begin_op as usize..]
        .iter()
        .find(|i| i.mc_stack == [2])
        .unwrap();
    assert!(matches!(rule.kind, PaintKind::Fill));
    assert_eq!(rule.gstate.fill, gray(0.8));
    assert_rect(rule.bbox, [20.0, 90.0, 180.0, 91.0]);
    let frame: Vec<_> = page.items.iter().filter(|i| i.mc_stack == [4]).collect();
    assert_eq!(frame.len(), 1);
    assert!(matches!(frame[0].kind, PaintKind::Stroke));
    assert_eq!(frame[0].gstate.stroke, gray(0.5));
    assert_rect(frame[0].bbox, [4.5, 4.5, 195.5, 195.5]);
}
