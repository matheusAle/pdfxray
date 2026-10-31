//! Selecting on the canvas and in the trees: clicks, the hit stack, hover previews, double-click zoom and the keyboard.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_canvas_click_selects_the_topmost_hit_and_syncs_the_tree_the_crumbs_and_the_hit_stack() {
    let mut harness = opened("struct-rich", "Tags");
    // Inside the Figure: its rectangle at 20..140 x 230..310.
    let inside = screen_of(&harness, Point::new(60.0, 270.0), 400.0);
    click_at(&mut harness, inside);

    let figure = element("struct-rich", "Figure");
    let state = &harness.state().state;
    assert_eq!(state.selection.primary, Some(NodeId::Op { page: 0, ix: 17 }));
    // The tree highlights the nearest tag it has for the selection: the MCID row is only a
    // reference, so it is the Figure that owns it.
    assert_eq!(selected_rows(&harness), ["Figure"]);
    assert_eq!(crumbs(&harness), ["Page 1", "Figure", "MCID 2", "f #17"]);
    // The hit stack lists what is under the point, most specific first, and says where.
    let labels: Vec<String> = buttons_in(&harness, Region::Drawer).split_off(5);
    // Rows are 28 px: four of the five fit in the drawer, the fifth scrolls in.
    assert_eq!(labels[..4], ["f", "Figure MCID 2", "Figure", "Sect"]);
    let note = pdfxray_app::drawer::hit_stack::header_note(state).expect("a click note");
    assert!(note.starts_with("click @ 6") && note.ends_with(" pt"), "{note}");
    let lines = pdfxray_app::drawer::hit_stack::lines(state);
    assert_eq!(lines[0].label, "f");
    assert_eq!(lines[0].detail, "op #17 · stream 6 0 R · bytes 211..212");
    assert_eq!(lines[1].detail, "/Figure <</MCID 2>> BDC … EMC");
    assert_eq!(lines[2].detail, "14 0 R");
    // Smoke: the owner element of the click is the Figure.
    assert_eq!(state.drawer.hit_stack.as_ref().unwrap().entries[2].id, figure);
}

#[test]
fn alt_click_goes_down_the_hit_stack_and_wraps() {
    let mut harness = opened("minimal-one-page", "Tags");
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas");
    canvas.click();
    harness.run();
    let stack = harness.state().state.drawer.hit_stack.clone().unwrap();
    assert!(stack.entries.len() >= 3, "{stack:?}");
    assert_eq!(
        harness.state().state.selection.primary.as_ref(),
        Some(&stack.entries[0].id)
    );

    for expected in [1, 2] {
        harness
            .get_by_role_and_label(Role::Image, "Page 1 canvas")
            .click_modifiers(Modifiers::ALT);
        harness.run();
        assert_eq!(
            harness.state().state.selection.primary.as_ref(),
            Some(&stack.entries[expected].id),
            "alt-click {expected}"
        );
    }
    // Past the last entry it starts over; the stack itself did not change.
    for _ in 2..stack.entries.len() {
        harness
            .get_by_role_and_label(Role::Image, "Page 1 canvas")
            .click_modifiers(Modifiers::ALT);
        harness.run();
    }
    assert_eq!(
        harness.state().state.selection.primary.as_ref(),
        Some(&stack.entries[0].id)
    );
    assert_eq!(harness.state().state.drawer.hit_stack.as_ref(), Some(&stack));
}

#[test]
fn shift_click_on_the_canvas_adds_the_second_selection() {
    let mut harness = opened("minimal-one-page", "Tags");
    harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .click();
    harness.run();
    let first = harness.state().state.selection.primary.clone().unwrap();

    harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .click_modifiers(Modifiers::SHIFT);
    harness.run();

    let state = &harness.state().state;
    assert_eq!(state.selection.primary, Some(first.clone()));
    // The topmost node under the point is the one already selected, so nothing was added.
    assert_eq!(state.selection.secondary, None);
    // Smoke: a shift-click elsewhere on the page adds one.
    let elsewhere = screen_of(&harness, Point::new(2.0, 2.0), 100.0);
    harness.hover_at(elsewhere);
    harness.event_modifiers(
        Event::PointerButton {
            pos: elsewhere,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
    harness.event_modifiers(
        Event::PointerButton {
            pos: elsewhere,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::SHIFT,
        },
        Modifiers::SHIFT,
    );
    harness.run();
    assert!(harness.state().state.selection.secondary.is_some());
    assert_eq!(harness.state().state.selection.primary, Some(first));
}

#[test]
fn hovering_a_tree_row_previews_its_box_on_the_canvas_without_selecting() {
    let mut harness = opened("struct-rich", "Tags");
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run();
    let before = harness.state().state.selection.primary.clone();
    let figure = element("struct-rich", "Figure");

    let row = named(&harness, Region::Navigator, Role::Button, "Figure")[0]
        .rect()
        .center();
    harness.hover_at(row);
    harness.run();

    assert_eq!(harness.state().state.canvas.drawn_preview, Some(figure));
    assert_eq!(harness.state().state.selection.primary, before);
    // Moving off the tree takes the preview away.
    harness.hover_at(egui::pos2(700.0, 800.0));
    harness.run();
    assert_eq!(harness.state().state.canvas.drawn_preview, None);
}

#[test]
fn hovering_the_canvas_outlines_the_node_under_the_pointer_and_clicking_does_not_need_it() {
    let mut harness = opened("struct-rich", "Tags");
    harness.hover_at(screen_of(&harness, Point::new(60.0, 270.0), 400.0));
    harness.run();

    assert_eq!(
        harness.state().state.canvas.hover,
        Some(NodeId::Op { page: 0, ix: 17 })
    );
    assert_ne!(
        harness.state().state.selection.primary,
        harness.state().state.canvas.hover
    );
    // The cursor readout in the status bar follows the pointer, in the unit of the switch.
    let readout = texts_in(&harness, Region::Chrome, Role::Label)
        .into_iter()
        .find(|t| t.starts_with("x 60"))
        .expect("a cursor readout");
    assert!(readout.contains("y 270"), "{readout}");
    // Leaving the page clears it.
    harness.hover_at(egui::pos2(700.0, 30.0));
    harness.run();
    assert_eq!(harness.state().state.canvas.hover, None);
}

#[test]
fn double_clicking_a_tree_row_zooms_the_canvas_to_its_box() {
    let mut harness = opened("struct-rich", "Tags");
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run();
    let fitted = harness.state().state.canvas.zoom;
    assert_eq!(harness.state().state.canvas.zoom_mode, ZoomMode::Fit);

    named(&harness, Region::Navigator, Role::Button, "Figure")[0].click();
    harness.run();
    let at = named(&harness, Region::Navigator, Role::Button, "Figure")[0]
        .rect()
        .center();
    // A step is 250 ms; allow a double click that long between its two clicks.
    harness
        .ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 0.6);
    harness.hover_at(at);
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        harness.step();
    }
    harness.run();
    wait_for_render(&mut harness);

    let state = &harness.state().state;
    assert_eq!(state.selection.primary, Some(element("struct-rich", "Figure")));
    assert_eq!(state.canvas.zoom_mode, ZoomMode::Manual);
    // The Figure is 120 x 80 pt on a 400 pt page: the zoom rises well above the fitted one.
    assert!(
        state.canvas.zoom > fitted * 1.5,
        "{} vs {fitted}",
        state.canvas.zoom
    );
    // Smoke: the selection was scrolled into view, and the box fits the view.
    assert!(!state.canvas.reveal_pending());
    assert!(120.0 * state.canvas.zoom <= state.canvas.viewport.x);
}

#[test]
fn arrow_keys_collapse_expand_and_move_the_selection_in_the_tree() {
    let mut harness = opened("struct-rich", "Tags");
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run();
    let table = element("struct-rich", "Table");
    named(&harness, Region::Navigator, Role::Button, "Table")[0].click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(table.clone()));
    let open_rows = tags_rows(&mut harness);

    // Left collapses the open row: the rows under it go away, the selection stays.
    harness.key_press(Key::ArrowLeft);
    harness.run();
    let collapsed = tags_rows(&mut harness);
    assert!(collapsed < open_rows, "{collapsed} < {open_rows}");
    assert_eq!(harness.state().state.selection.primary, Some(table.clone()));

    // Right opens it again, a second Right goes to the first child.
    harness.key_press(Key::ArrowRight);
    harness.run();
    let reopened = tags_rows(&mut harness);
    assert_eq!(reopened, open_rows);
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(element("struct-rich", "TR"))
    );
    assert_eq!(selected_rows(&harness), ["TR"]);

    // Down moves to the next row, Up back; Left on a leaf-or-closed row goes to the parent.
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(element("struct-rich", "TH"))
    );
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(element("struct-rich", "TR"))
    );
    harness.key_press(Key::ArrowLeft);
    harness.run();
    harness.key_press(Key::ArrowLeft);
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(table));
}

#[test]
fn selecting_on_the_canvas_opens_and_scrolls_the_tree_to_the_row() {
    let mut harness = opened("struct-rich", "Tags");
    // Nothing is expanded but the root; the Lbl of the list is hidden.
    assert!(named(&harness, Region::Navigator, Role::Button, "Lbl").is_empty());

    harness
        .state_mut()
        .state
        .click_page_point(Point::new(25.0, 172.0));
    harness.run_steps(3);

    assert_eq!(selected_rows(&harness), [OWNER_OF_MCID_5]);
    assert!(!named(&harness, Region::Navigator, Role::Button, "Lbl").is_empty());
}
