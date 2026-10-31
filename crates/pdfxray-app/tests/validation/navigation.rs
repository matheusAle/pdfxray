//! Walking the findings: next and previous, folded rules, findings on other pages or without a node.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn next_and_previous_walk_the_findings_in_the_lists_order_and_wrap() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);
    the(&harness, Region::Navigator, Role::Button, "Font /F1").click();
    harness.run();
    let shown = |harness: &Harness<'_, PdfxrayApp>| harness.state().state.current_finding();
    assert_eq!(shown(&harness), Some(0));

    let mut seen = Vec::new();
    for _ in 0..4 {
        the(&harness, Region::Inspector, Role::Button, "Next finding").click();
        harness.run();
        seen.push(shown(&harness));
    }
    // Report order is list order here; after the last one it starts over.
    assert_eq!(seen, [Some(1), Some(2), Some(3), Some(0)]);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Resource {
            owner: pdfxray_core::ObjRef::new(3, 0),
            kind: pdfxray_core::ResKind::Font,
            name: "F1".to_owned(),
        })
    );

    // Previous from the first goes to the last.
    the(&harness, Region::Inspector, Role::Button, "Previous").click();
    harness.run();
    assert_eq!(shown(&harness), Some(3));
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::MarkedContent { page: 0, mcid: 1 })
    );

    // ⌘[ and ⌘] do the same on this entry, and the lists follow the selection.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::OpenBracket);
    harness.run();
    assert_eq!(shown(&harness), Some(2));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::CloseBracket);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::CloseBracket);
    harness.run();
    assert_eq!(shown(&harness), Some(0));
    let drawer_rows = texts_in(&harness, Region::Drawer, Role::Button);
    assert!(
        is_selected(&the(
            &harness,
            Region::Drawer,
            Role::Button,
            "7.21.4.1-1 Font /F1"
        )),
        "{drawer_rows:?}"
    );

    // On the Objects entry the same keys are the object history.
    click_rail(&mut harness, "Objects");
    let before = harness.state().state.selection.primary.clone();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::CloseBracket);
    harness.run();
    assert_eq!(harness.state().state.selection.primary, before);
}

#[test]
fn a_rule_with_more_nodes_than_fit_folds_into_more_and_shows_the_page_of_the_rest() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report = struct_rich_report();
    fake.run_gate.open();
    let mut harness = app_with(&Arc::new(fake));
    open_fixture(&mut harness, "struct-rich");
    title_bar_validate(&mut harness);

    let list = findings_list(&harness);
    assert_eq!(
        list,
        [
            "Figure has no alternative text · 7.3-1",
            "H1 12 0 R p1",
            "P 13 0 R p1",
            "Figure 14 0 R p1",
            "+2 more p2",
            "Content is neither tagged nor an artifact · 7.1-3",
            "MCID 2 p2",
            "Window does not show the document title · 7.1-10",
            "Document",
        ]
    );
    let labels = labels_in(&harness, Region::Navigator);
    assert_eq!(
        labels[labels.len() - 6..],
        ["7", "failed", "3", "rules", "300", "passed"]
    );

    // Opening the rest lists the other two, on page 2.
    the(&harness, Region::Navigator, Role::Button, "+2 more p2").click();
    harness.run();
    let list = findings_list(&harness);
    assert_eq!(list[4..6], ["Link 23 0 R p2", "Callout → Span 24 0 R p2"]);
    assert!(named(&harness, Region::Navigator, Role::Button, "+2 more p2").is_empty());

    // Clicking one on another page brings that page to the canvas, with its badges and card.
    the(&harness, Region::Navigator, Role::Button, "Link 23 0 R p2").click();
    harness.run();
    assert_eq!(harness.state().state.canvas.page, 1);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::StructElem(pdfxray_core::ObjRef::new(23, 0)))
    );
    wait_for_render(&mut harness);
    let on_page = |harness: &Harness<'_, PdfxrayApp>| {
        texts_in(harness, Region::Center, Role::Button)
            .into_iter()
            .filter(|name| name.starts_with("× "))
            .collect::<Vec<_>>()
    };
    // The selected finding's badge is drawn last, so compare without the order.
    let mut badges = on_page(&harness);
    badges.sort();
    assert_eq!(
        badges,
        [
            "× 7.1-3 · Content is neither tagged nor an artifact",
            "× 7.3-1 · Figure has no alternative text",
            "× 7.3-1 · Figure has no alternative text",
        ]
    );
    open_page_card(&mut harness);
    assert!(labels_in(&harness, Region::Center).contains(&"3 failed".to_owned()));
    // The header says which of the rule's findings this is.
    assert!(labels_in(&harness, Region::Inspector).contains(&"PDF/UA-1 · rule 7.3-1 · 4 of 5".to_owned()));
}

#[test]
fn a_finding_on_another_page_brings_that_page_to_the_canvas() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report.findings[0].locator = Locator::Page { index: 2 };
    fake.run_gate.open();
    let fake = Arc::new(fake);
    let mut harness = validation_tab(&fake, "page-boxes");
    run_validation(&mut harness);
    assert_eq!(harness.state().state.canvas.page, 0);

    the(&harness, Region::Navigator, Role::Button, "Page 3 p3").click();
    harness.run();

    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Page(2)));
    assert_eq!(harness.state().state.canvas.page, 2);
    assert!(has_label(&harness, "/ 3"));
    // The page is rendered in the background; its image is there once it arrives.
    wait_for_render(&mut harness);
    let _canvas = harness.get_by_role_and_label(Role::Image, "Page 3 canvas");
}

#[test]
fn a_finding_without_a_node_selects_the_finding_itself() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report.findings[1].locator = Locator::Unknown;
    fake.run_gate.open();
    let mut harness = validation_tab(&Arc::new(fake), "orphan-mcid");
    run_validation(&mut harness);
    let _row = the(&harness, Region::Navigator, Role::Button, "not resolved");

    harness.state_mut().state.select_finding(1);
    harness.run();

    // The failed check list only keeps resolved-or-not findings in report order; the second
    // finding is the one that lost its target.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Validation(1))
    );
}
