//! The findings: listed by rule in the navigator, marked on the canvas, tabled in the drawer.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_title_bar_validates_and_the_navigator_lists_the_findings_by_rule() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    assert_eq!(harness.state().state.nav.rail, RailItem::Tags);

    // Pressing Validate before veraPDF was looked for waits for the search, then runs.
    title_bar_validate(&mut harness);

    assert_eq!(harness.state().state.nav.rail, RailItem::Validation);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Validation);
    assert_eq!(
        fake.calls(),
        [
            Call::Discover(None),
            Call::Validate(fixture_path("orphan-mcid"), Profile::PdfUa1)
        ]
    );
    assert_eq!(
        harness.state().state.status,
        "Validated against PDF/UA-1: not compliant"
    );
    // The picker shows the profile and its standard.
    assert_eq!(
        harness.get_by_label("Profile").value().as_deref(),
        Some("PDF/UA-1 · ISO 14289-1")
    );
    // Counters: failed checks, failed rules, passed checks.
    let labels = labels_in(&harness, Region::Navigator);
    assert_eq!(
        labels[labels.len() - 6..],
        ["4", "failed", "4", "rules", "225", "passed"]
    );
    // One group per rule, titled in plain words, each with its node.
    assert_eq!(
        findings_list(&harness),
        [
            "Font is not embedded · 7.21.4.1-1",
            "Font /F1",
            "Window does not show the document title · 7.1-10",
            "Document",
            "Document has no XMP metadata · 7.1-8",
            "Document",
            "Content is neither tagged nor an artifact · 7.1-3",
            "MCID 1 p1",
        ]
    );
    // Smoke: the run is over and Re-run replaced Validate.
    assert!(!harness.state().state.validation.is_running());
    assert!(named(&harness, Region::Navigator, Role::Button, "Validate").is_empty());
    // The title bar chip says the file is not compliant with UA-1, and its popover how many checks failed.
    let chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.7 · UA-1 failed · rev 1/1",
    );
    assert!(
        !texts_in(&harness, Region::Chrome, Role::Button)
            .iter()
            .any(|l| l.contains("Tagged"))
    );
    chip.click();
    harness.run();
    let labels = common::all_label_texts(&harness);
    let at = labels.iter().position(|t| t == "PDF/UA").unwrap();
    assert_eq!(labels[at + 1], "UA-1 · not compliant · 4 failed checks");
}

#[test]
fn clicking_a_node_selects_it_and_the_canvas_marks_the_failure() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);
    // Every finding with a box on the page is badged, whichever node is selected.
    wait_for_render(&mut harness);
    let badge = "× 7.1-3 · Content is neither tagged nor an artifact";
    assert_eq!(
        texts_in(&harness, Region::Center, Role::Button)
            .into_iter()
            .filter(|name| name.starts_with("× "))
            .collect::<Vec<_>>(),
        [badge]
    );

    the(&harness, Region::Navigator, Role::Button, "MCID 1 p1").click();
    harness.run();

    let state = &harness.state().state;
    assert_eq!(
        state.selection.primary,
        Some(NodeId::MarkedContent { page: 0, mcid: 1 })
    );
    assert_eq!(state.validation.selected_finding, Some(3));
    assert_eq!(state.canvas.page, 0);
    // The badge sits on that node's box on the page.
    let badges = state.page_badges(0);
    assert_eq!(badges.len(), 1);
    assert!(badges[0].current);
    assert_eq!(
        Some(badges[0].rect),
        pdfxray_core::Document::open(fixture_path("orphan-mcid"))
            .unwrap()
            .bbox(&NodeId::MarkedContent { page: 0, mcid: 1 })
            .map(|(_, rect)| rect)
    );
    // The card is a chip beside the page until the pointer rests on it; then it counts the
    // failures on this page, and says why it cannot count the passes.
    assert_eq!(
        labels_in(&harness, Region::Center),
        ["/ 1", "0 pt", "On this page"]
    );
    open_page_card(&mut harness);
    let center = labels_in(&harness, Region::Center);
    assert!(center.contains(&"On this page".to_owned()), "{center:?}");
    assert!(center.contains(&"1 failed".to_owned()), "{center:?}");
    assert!(
        center.contains(&"225 passed in the file".to_owned()),
        "{center:?}"
    );

    // Clicking the badge itself selects its finding too: start from another one.
    the(&harness, Region::Navigator, Role::Button, "Font /F1").click();
    harness.run();
    assert_eq!(harness.state().state.validation.selected_finding, Some(0));
    the(&harness, Region::Center, Role::Button, badge).click();
    harness.run();
    assert_eq!(harness.state().state.validation.selected_finding, Some(3));
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::MarkedContent { page: 0, mcid: 1 })
    );
}

#[test]
fn the_drawer_lists_the_same_findings_as_a_table() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);
    harness.state_mut().state.drawer.height = 300.0;
    harness.run_steps(2);

    let rows: Vec<String> = texts_in(&harness, Region::Drawer, Role::Button)
        .into_iter()
        .skip(5)
        .collect();
    assert_eq!(
        rows,
        [
            "7.21.4.1-1 Font /F1",
            "7.1-10 Document",
            "7.1-8 Document",
            "7.1-3 MCID 1"
        ]
    );
    // The tab counts the findings; the rows say what failed, where and on which page.
    let labels = labels_in(&harness, Region::Drawer);
    assert!(labels.contains(&"4".to_owned()), "{labels:?}");

    // The note at the right of the tab bar: version, profile, how long ago and how long it took.
    {
        let last = harness.state_mut().state.validation.last.as_mut().unwrap();
        last.finished = Instant::now().checked_sub(Duration::from_secs(12)).unwrap();
        last.took = Duration::from_millis(800);
    }
    assert_eq!(
        pdfxray_app::drawer::validation::header_note(&harness.state().state).as_deref(),
        Some("veraPDF 1.30.3 · PDF/UA-1 · ran 12 s ago · 0.8 s")
    );

    // Clicking a row selects its node and highlights the row.
    the(&harness, Region::Drawer, Role::Button, "7.21.4.1-1 Font /F1").click();
    harness.run();
    assert_eq!(harness.state().state.validation.selected_finding, Some(0));
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Resource {
            owner: pdfxray_core::ObjRef::new(3, 0),
            kind: pdfxray_core::ResKind::Font,
            name: "F1".to_owned(),
        })
    );
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "7.21.4.1-1 Font /F1"
    )));
    assert!(!is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "7.1-3 MCID 1"
    )));
}

#[test]
fn the_nodes_of_a_rule_are_tree_rows_and_every_row_of_the_list_shows_a_hand() {
    let fake = FakeValidator::ready();
    let mut harness = app_with(&fake);
    open_fixture(&mut harness, "orphan-mcid");
    title_bar_validate(&mut harness);
    harness.run_steps(3);

    let nodes = named(&harness, Region::Navigator, Role::Button, "Document");
    assert_eq!(nodes.len(), 2);
    let (first, second) = (nodes[0].rect(), nodes[1].rect());
    let font = named(&harness, Region::Navigator, Role::Button, "Font /F1")[0].rect();
    let group = the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Font is not embedded · 7.21.4.1-1",
    )
    .rect();

    // A node is as tall as a tree row, and the next group starts well below it.
    assert_eq!(font.height(), pdfxray_app::theme::tokens::ROW_H);
    assert_eq!(first.height(), pdfxray_app::theme::tokens::ROW_H);
    assert!(font.min.y >= group.max.y, "{font:?} under {group:?}");
    assert!(second.min.y > first.max.y);
    // The node, the group and the "more" rows all take the hand.
    let cursor_at = |harness: &mut Harness<'static, PdfxrayApp>, at: egui::Pos2| {
        harness.hover_at(at);
        harness.run_steps(2);
        harness.output().platform_output.cursor_icon
    };
    assert_eq!(
        cursor_at(&mut harness, font.center()),
        egui::CursorIcon::PointingHand
    );
    assert_eq!(
        cursor_at(&mut harness, group.center()),
        egui::CursorIcon::PointingHand
    );
    // Smoke: the pointer between the two lists shows none.
    let beside = egui::pos2(common::WINDOW.x - 400.0, font.center().y);
    assert_ne!(cursor_at(&mut harness, beside), egui::CursorIcon::PointingHand);
}
