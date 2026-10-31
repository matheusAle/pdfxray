//! Marked content rows of the Tags tree: shown, but not buttons; selected elsewhere, their tag row lights up.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_marked_content_rows_are_shown_with_their_label_and_page_but_are_no_buttons() {
    let harness = expanded_tags();

    // The row is still in the tree, with its label and its page, in the Figure's subtree.
    let _row = mcid_row(&harness, "MCID 2");
    // Nine marked content references are in the structure tree (both pages' MCIDs but one);
    // rows are 28 px, so the eight that fit the navigator (there is no footer under it) are on
    // screen and the rest scroll in.
    assert_eq!(mcid_rows(&harness).len(), 8);
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(labels.contains(&"p1".to_owned()) && labels.contains(&"p2".to_owned()));
    // None of them is a button: nothing to focus, press or select.
    assert!(named(&harness, Region::Navigator, Role::Button, "MCID 2").is_empty());
    let buttons = buttons_in(&harness, Region::Navigator);
    assert!(
        buttons.iter().all(|name| !name.starts_with("MCID")),
        "{buttons:?}"
    );
    // Smoke: the tree lists the Figure and then, under it, its MCID.
    let mut harness = harness;
    let labels = harness
        .state_mut()
        .state
        .tree_model(pdfxray_app::NavTree::Core(TreeKind::Tags))
        .unwrap()
        .visible_labels();
    let figure = labels.iter().position(|l| l.trim() == "Figure").unwrap();
    assert_eq!(labels[figure + 1].trim(), "MCID 2");
    // Indented under its tag.
    let indent = |label: &str| label.len() - label.trim_start().len();
    assert!(indent(&labels[figure + 1]) > indent(&labels[figure]));
}

#[test]
fn clicking_a_marked_content_row_does_nothing() {
    let mut harness = expanded_tags();
    let figure = element("struct-rich", "Figure");
    named(&harness, Region::Navigator, Role::Button, "Figure")[0].click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(figure.clone()));

    // A click and a double click on the row of the Figure's MCID, then a click on another's.
    let at = mcid_row(&harness, "MCID 2").rect().center();
    click_at(&mut harness, at);
    click_at(&mut harness, at);
    let other = mcid_row(&harness, "MCID 5").rect().center();
    click_at(&mut harness, other);

    let state = &harness.state().state;
    assert_eq!(state.selection.primary, Some(figure.clone()));
    assert_eq!(selected_rows(&harness), ["Figure"]);
    // It neither zoomed the canvas nor was the second selection set.
    assert_eq!(state.canvas.zoom_mode, ZoomMode::Fit);
    assert_eq!(state.selection.secondary, None);
    // Smoke: the Figure is still the one selected and the MCID rows are still listed.
    assert!(mcid_rows(&harness).contains(&"MCID 2".to_owned()));
    assert_eq!(state.selection.primary, Some(figure));
}

#[test]
fn hovering_a_marked_content_row_previews_nothing_and_explains_itself() {
    let mut harness = expanded_tags();
    let before = harness.state().state.selection.primary.clone();

    let at = mcid_row(&harness, "MCID 2").rect().center();
    harness.hover_at(at);
    harness.run();
    assert_eq!(harness.state().state.canvas.drawn_preview, None);
    assert_eq!(harness.state().state.selection.primary, before);
    // Resting on it for 400 ms says what the row is.
    harness.run_steps(3);
    assert!(has_label(
        &harness,
        pdfxray_app::components::tree_row::CONTENT_REF_TIP
    ));
    // Smoke: a tag row still previews its box.
    let figure = named(&harness, Region::Navigator, Role::Button, "Figure")[0]
        .rect()
        .center();
    harness.hover_at(figure);
    harness.run();
    assert_eq!(
        harness.state().state.canvas.drawn_preview,
        Some(element("struct-rich", "Figure"))
    );
}

#[test]
fn arrow_down_from_a_tag_steps_over_its_marked_content_to_the_next_tag() {
    let mut harness = expanded_tags();
    named(&harness, Region::Navigator, Role::Button, "Figure")[0].click();
    harness.run();

    harness.key_press(Key::ArrowDown);
    harness.run();

    // The Figure's MCID 2 row sits between the two, and is skipped.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(element("struct-rich", "Table"))
    );
    assert_eq!(selected_rows(&harness), ["Table"]);
    // Up goes back over it.
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(selected_rows(&harness), ["Figure"]);
    // Smoke: the arrow keys never selected marked content.
    assert!(!matches!(
        harness.state().state.selection.primary,
        Some(NodeId::MarkedContent { .. })
    ));
}

#[test]
fn arrow_right_on_a_tag_goes_to_its_first_child_tag_not_to_marked_content() {
    let mut harness = expanded_tags();
    named(&harness, Region::Navigator, Role::Button, "Table")[0].click();
    harness.run();
    harness.key_press(Key::ArrowRight);
    harness.run();

    assert_eq!(
        harness.state().state.selection.primary,
        Some(element("struct-rich", "TR"))
    );
    // On a tag that has only marked content below it, Right (open already) stays put.
    named(&harness, Region::Navigator, Role::Button, "Figure")[0].click();
    harness.run();
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(selected_rows(&harness), ["Figure"]);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(element("struct-rich", "Figure"))
    );
}

#[test]
fn selecting_marked_content_elsewhere_highlights_and_reveals_its_tag_row() {
    let mut harness = opened("struct-rich", "Tags");
    // Collapsed: the list and its label are hidden, and so are their MCID rows.
    assert!(named(&harness, Region::Navigator, Role::Button, "Lbl").is_empty());

    harness
        .state_mut()
        .state
        .select(NodeId::MarkedContent { page: 0, mcid: 5 });
    harness.run_steps(3);

    assert_eq!(selected_rows(&harness), [OWNER_OF_MCID_5]);
    // The inspector still shows the marked content itself.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::MarkedContent { page: 0, mcid: 5 })
    );
    assert!(
        crumbs(&harness).contains(&"MCID 5".to_owned()),
        "{:?}",
        crumbs(&harness)
    );
    // Smoke: no row of marked content is among the buttons, selected or not.
    assert!(
        buttons_in(&harness, Region::Navigator)
            .iter()
            .all(|b| !b.starts_with("MCID"))
    );
}

#[test]
fn a_hit_stack_row_of_marked_content_selects_it_and_the_tree_highlights_its_tag() {
    let mut harness = opened("struct-rich", "Tags");
    let inside = screen_of(&harness, Point::new(60.0, 270.0), 400.0);
    click_at(&mut harness, inside);

    the(&harness, Region::Drawer, Role::Button, "Figure MCID 2").click();
    harness.run_steps(3);

    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::MarkedContent { page: 0, mcid: 2 })
    );
    assert_eq!(selected_rows(&harness), ["Figure"]);
    // Smoke: the crumbs end on the MCID, as the inspector shows it.
    assert_eq!(crumbs(&harness), ["Page 1", "Figure", "MCID 2"]);
}

#[test]
fn a_breadcrumb_of_marked_content_selects_it_and_the_tree_highlights_its_tag() {
    let mut harness = opened("struct-rich", "Tags");
    let inside = screen_of(&harness, Point::new(60.0, 270.0), 400.0);
    click_at(&mut harness, inside);

    the(&harness, Region::Inspector, Role::Button, "MCID 2").click();
    harness.run_steps(3);

    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::MarkedContent { page: 0, mcid: 2 })
    );
    assert_eq!(selected_rows(&harness), ["Figure"]);
    assert_eq!(crumbs(&harness), ["Page 1", "Figure", "MCID 2"]);
}
