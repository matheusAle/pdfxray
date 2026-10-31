//! The fixed shell: title bar, rail, navigator, drawer, status bar, keys and the global units.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use common::{
    DroppedPath, Region, app, buttons_in, click_at, click_drawer_tab, click_rail, click_title_chip,
    fixture_path, has_label, is_selected, nodes_in, open_fixture, recording_spawner, texts_in, the,
    title_chip, wait_for_open, wait_for_render,
};
use egui::{Key, Modifiers, Role, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::center::pick;
use pdfxray_app::drawer;
use pdfxray_app::{DrawerTab, PdfxrayApp, READY, RailItem, Unit};
use pdfxray_core::{BoxKind, LayerMode, NodeId, ObjRef};
use pdfxray_render::ColorBy;
use pretty_assertions::assert_eq;

type Harness<'a> = egui_kittest::Harness<'a, PdfxrayApp>;

fn media(page: u32) -> NodeId {
    NodeId::PageBox {
        page,
        kind: BoxKind::Media,
    }
}

#[test]
fn the_empty_shell_offers_the_drop_zone_the_rail_and_the_units() {
    let harness = app();

    // The drop zone says what to do and how.
    assert!(has_label(&harness, "Drop a PDF to inspect it"));
    let _open = harness.get_by_label("Open file…");
    assert!(has_label(&harness, "pdfxray"));
    // The rail lists the six entries and the three at the bottom.
    for name in [
        "Tags",
        "Content",
        "Artifacts",
        "Pages",
        "Objects",
        "Revisions",
        "Validation",
        "Diagnostics",
        "Settings",
    ] {
        let _entry = the(&harness, Region::Rail, Role::Button, name);
    }
    // The units switch is there from the start, pt selected.
    for unit in Unit::ALL {
        let button = the(&harness, Region::Chrome, Role::Button, unit.label());
        assert_eq!(is_selected(&button), unit == Unit::Pt, "{}", unit.label());
    }
    // No navigator, inspector or drawer without a file.
    assert!(!has_label(&harness, "Structure"));
    assert!(harness.query_by_label("Hit stack").is_none());
    // Smoke: the defaults of the state.
    let state = &harness.state().state;
    assert!(state.document.is_none());
    assert_eq!(state.selection.primary, None);
    assert_eq!(state.status, "Ready");
    assert_eq!(state.nav.rail, RailItem::Tags);
    assert_eq!(state.drawer.tab, DrawerTab::HitStack);
    assert_eq!(state.drawer.height, 196.0);
    assert_eq!(state.canvas.layer_mode, LayerMode::All);
    assert_eq!(state.unit, Unit::Pt);
}

#[test]
fn dropping_a_file_opens_it_and_names_it_in_the_title_bar() {
    let mut harness = app();
    let path = fixture_path("minimal-one-page");

    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedPath(path.clone())));
    harness.run();
    wait_for_open(&mut harness);

    let state = &harness.state().state;
    assert_eq!(state.document.as_ref().map(|d| d.path.clone()), Some(path));
    assert_eq!(state.page_count(), 1);
    // The title bar names the file and has one chip with its version, tags and revision; the
    // status bar has no news to add.
    assert_eq!(state.status, READY);
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)[0],
        "minimal-one-page.pdf"
    );
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.4 · Untagged · rev 1/1",
    );
    assert!(!has_label(&harness, "Loaded: minimal-one-page.pdf"));
    // The drop zone is gone and the navigator, inspector and drawer are there.
    assert!(!has_label(&harness, "Drop a PDF to inspect it"));
    let _tab = the(&harness, Region::Drawer, Role::Button, "Hit stack");
    assert!(has_label(&harness, "Structure"));
    // Opening selects the document: the inspector shows its file facts.
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Document));
}

#[test]
fn dropping_something_that_is_not_a_pdf_reports_the_error_and_keeps_the_document() {
    let mut harness = app();
    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedPath(fixture_path("minimal-one-page"))));
    harness.run();
    wait_for_open(&mut harness);
    let not_a_pdf = std::env::temp_dir().join(format!("pdfxray-not-a-pdf-{}.txt", std::process::id()));
    std::fs::write(&not_a_pdf, "hello").unwrap();

    harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedPath(not_a_pdf.clone())));
    harness.run();
    wait_for_open(&mut harness);
    std::fs::remove_file(&not_a_pdf).unwrap();

    let name = not_a_pdf.file_name().unwrap().to_string_lossy().into_owned();
    let state = &harness.state().state;
    assert!(
        state
            .status
            .starts_with(&format!("Cannot open {name}: not a PDF")),
        "{}",
        state.status
    );
    assert_eq!(state.open_error.as_deref(), Some(state.status.as_str()));
    // The earlier document is still there.
    assert_eq!(
        state.document.as_ref().map(|d| d.doc.info().name),
        Some("minimal-one-page.pdf".to_owned())
    );
    // Shown in the status bar, and in the Diagnostics tab of the drawer.
    let message = state.status.clone();
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)
            .iter()
            .filter(|t| **t == message)
            .count(),
        1
    );
    click_drawer_tab(&mut harness, "Diagnostics");
    assert_eq!(
        texts_in(&harness, Region::Drawer, Role::Label)
            .iter()
            .filter(|t| **t == message)
            .count(),
        1
    );
}

#[test]
fn the_open_button_and_command_o_ask_the_picker_and_open_its_choice() {
    let mut harness = app();
    let path = fixture_path("page-boxes");
    let asked = Rc::new(Cell::new(0));
    let counter = Rc::clone(&asked);
    harness.state_mut().state.picker = Box::new(move || {
        counter.set(counter.get() + 1);
        Some(path.clone())
    });

    // From the empty state, the drop zone's button.
    harness.get_by_label("Open file…").click();
    harness.run();
    wait_for_open(&mut harness);

    let state = &harness.state().state;
    assert_eq!(asked.get(), 1);
    assert_eq!(state.status, READY);
    assert_eq!(state.page_count(), 3);
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)
            .first()
            .map(String::as_str),
        Some("page-boxes.pdf")
    );

    // With a file open, ⌘O asks again and opens the choice in a new window; this one keeps its
    // file. The title bar has no button for it.
    assert!(harness.query_by_label("Open…").is_none());
    let (spawner, started) = recording_spawner();
    harness.state_mut().state.spawner = spawner;
    harness.state_mut().state.picker = Box::new({
        let counter = Rc::clone(&asked);
        let other = fixture_path("struct-rich");
        move || {
            counter.set(counter.get() + 1);
            Some(other.clone())
        }
    });
    harness.key_press_modifiers(Modifiers::COMMAND, Key::O);
    harness.run();
    assert_eq!(asked.get(), 2);
    assert_eq!(*started.borrow(), [Some(fixture_path("struct-rich"))]);
    assert!(harness.state().state.opening.job().is_none());
    assert_eq!(harness.state().state.page_count(), 3);
}

#[test]
fn cancelling_the_picker_changes_nothing() {
    let mut harness = app();
    harness.state_mut().state.picker = Box::new(|| None);

    harness.get_by_label("Open file…").click();
    harness.run();

    assert_eq!(harness.state().state.status, "Ready");
    assert!(harness.state().state.document.is_none());
}

#[test]
fn rail_entries_switch_the_navigator_the_centre_and_the_drawer() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");

    // Tags: structure tree, canvas, Hit stack.
    assert!(is_selected(&the(&harness, Region::Rail, Role::Button, "Tags")));
    assert!(has_label(&harness, "Structure"));
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Hit stack"
    )));

    // Content: the content tree of the page, Operators.
    click_rail(&mut harness, "Content");
    assert_eq!(harness.state().state.nav.rail, RailItem::Content);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Operators);
    assert!(is_selected(&the(&harness, Region::Rail, Role::Button, "Content")));
    assert!(!is_selected(&the(&harness, Region::Rail, Role::Button, "Tags")));
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Operators"
    )));
    assert!(texts_in(&harness, Region::Navigator, Role::Label).contains(&"Content".to_owned()));

    // Artifacts: the same tree filtered to artifacts, with the artifact x-ray on the canvas.
    click_rail(&mut harness, "Artifacts");
    let view = &harness.state().state.canvas.view;
    assert!(view.xray);
    assert_eq!(view.color_by, ColorBy::Artifact);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Operators);

    // Pages: thumbnails and boxes, boxes overlay on, Diagnostics.
    click_rail(&mut harness, "Pages");
    assert!(harness.state().state.canvas.show_boxes);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Diagnostics"
    )));
    assert!(texts_in(&harness, Region::Navigator, Role::Label).contains(&"Pages".to_owned()));

    // Objects: the object view replaces the canvas; the drawer shows the References.
    click_rail(&mut harness, "Objects");
    assert_eq!(harness.state().state.nav.rail, RailItem::Objects);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::References);
    assert!(harness.query_by_label("Page 1 canvas").is_none());
    assert!(harness.query_by_role_and_label(Role::Button, "X-ray").is_none());

    // Validation: the profile picker and Validate; the Validation tab.
    click_rail(&mut harness, "Validation");
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Validation);
    let _validate = the(&harness, Region::Navigator, Role::Button, "Validate");
    assert!(harness.query_by_label("Page 1 canvas").is_some());

    // Back to Tags: the canvas is back and so is the Hit stack.
    click_rail(&mut harness, "Tags");
    wait_for_render(&mut harness);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::HitStack);
    let _canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas");
}

#[test]
fn switching_the_rail_starts_every_entry_with_an_empty_filter() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    harness.state_mut().state.nav.filter = "Document".to_owned();

    click_rail(&mut harness, "Content");

    assert_eq!(harness.state().state.nav.filter, "");
}

#[test]
fn the_number_keys_set_the_pick_mode_shown_in_the_canvas_toolbar() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    let keys = [
        (Key::Num2, LayerMode::Tags),
        (Key::Num3, LayerMode::Content),
        (Key::Num5, LayerMode::Boxes),
        (Key::Num6, LayerMode::Artifacts),
        (Key::Num1, LayerMode::All),
    ];

    for (key, mode) in keys {
        harness.key_press(key);
        harness.run_steps(2);

        assert_eq!(harness.state().state.canvas.layer_mode, mode);
        // The toolbar's Pick button names the mode and stands out while it filters.
        let button = the(&harness, Region::Center, Role::Button, &pick::label(mode));
        assert_eq!(is_selected(&button), mode != LayerMode::All, "{mode:?}");
    }

    // 4 would pick annotations, which are not hit-tested yet: the mode stays.
    harness.key_press(Key::Num3);
    harness.key_press(Key::Num4);
    harness.run_steps(2);
    assert_eq!(harness.state().state.canvas.layer_mode, LayerMode::Content);
}

#[test]
fn the_pick_menu_sets_the_mode_filters_clicks_and_names_itself_in_the_hit_stack() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    // The title bar no longer has the control.
    assert!(
        buttons_in(&harness, Region::Chrome).iter().all(|b| ![
            "All",
            "Tags",
            "Content",
            "Boxes",
            "Artifacts"
        ]
        .contains(&b.as_str())),
    );

    the(&harness, Region::Center, Role::Button, "Pick: All").click();
    harness.run();
    // The menu lists every mode, the current one selected; annotations wait for v1.1.
    assert!(is_selected(&the(&harness, Region::Center, Role::Button, "All")));
    assert!(
        the(&harness, Region::Center, Role::Button, "Annotations")
            .accesskit_node()
            .is_disabled()
    );
    the(&harness, Region::Center, Role::Button, "Tags").click();
    harness.run();

    assert_eq!(harness.state().state.canvas.layer_mode, LayerMode::Tags);
    // The menu closed and the button says what it picks.
    assert!(harness.query_by_label("Annotations").is_none());
    let _button = the(&harness, Region::Center, Role::Button, "Pick: Tags");

    // The mode decides what a canvas click finds: only the page is left in Tags mode here.
    harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .click();
    harness.run();
    let stack = harness.state().state.drawer.hit_stack.clone().unwrap();
    assert_eq!(
        stack.entries.iter().map(|e| e.label.clone()).collect::<Vec<_>>(),
        ["Page 1"]
    );
    // The Hit stack says why it is short.
    let note = drawer::hit_stack::header_note(&harness.state().state).unwrap();
    assert!(note.starts_with("picking Tags · click @ "), "{note}");

    // The Objects view has no page to pick on, so no Pick button.
    click_rail(&mut harness, "Objects");
    assert!(harness.query_by_label("Pick: Tags").is_none());
}

#[test]
fn the_view_toggle_names_its_mode_with_its_keycap_in_the_tooltip() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    let toggle = the(&harness, Region::Center, Role::Button, "View").rect();

    harness.hover_at(toggle.center());
    harness.run_steps(4);

    // Nothing is on, so the toggle is the x-ray's: its name, its key and what it does.
    let labels = texts_in(&harness, Region::Center, Role::Label);
    let at = labels
        .iter()
        .position(|t| t == "X-ray")
        .unwrap_or_else(|| panic!("no tooltip name in {labels:?}"));
    assert_eq!(
        labels[at..at + 3],
        [
            "X-ray",
            "X",
            "Outline every painted item: glyphs, paths, images, shadings"
        ]
    );
    // Smoke: hovering showed a tooltip, it did not toggle the mode.
    assert!(!harness.state().state.canvas.view.xray);

    // With another mode on, the toggle names that one instead.
    harness.key_press(Key::C);
    harness.run_steps(3);
    let toggle = the(&harness, Region::Center, Role::Button, "Clip").rect();
    harness.hover_at(toggle.center() + vec2(0.0, 1.0));
    harness.run_steps(2);
    harness.hover_at(toggle.center());
    harness.run_steps(4);
    let labels = texts_in(&harness, Region::Center, Role::Label);
    let at = labels
        .iter()
        .position(|t| t == "Clip")
        .unwrap_or_else(|| panic!("no tooltip name in {labels:?}"));
    assert_eq!(labels[at + 1], "C");

    // The arrow names the menu it opens and has no key.
    let arrow = the(&harness, Region::Center, Role::Button, "View options").rect();
    harness.hover_at(arrow.center());
    harness.run_steps(4);
    let labels = texts_in(&harness, Region::Center, Role::Label);
    assert!(labels.contains(&"View options".to_owned()), "{labels:?}");
    assert!(
        labels.contains(&"Render modes: x-ray, paint order, clip, alpha".to_owned()),
        "{labels:?}"
    );
}

#[test]
fn keys_are_ignored_while_a_text_field_has_the_keyboard() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");

    // The page number field of the toolbar.
    nodes_in(&harness, Region::Center, Role::TextInput)
        .remove(0)
        .focus();
    harness.run();
    harness.key_press(Key::Num3);
    harness.run();

    assert_eq!(harness.state().state.canvas.layer_mode, LayerMode::All);
}

#[test]
fn the_units_switch_converts_the_values_in_the_inspector() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.state_mut().state.select(media(0));
    harness.run();
    // The MediaBox is 200 x 100 pt.
    assert!(texts_in(&harness, Region::Inspector, Role::Label).contains(&"200 pt".to_owned()));

    for (unit, width, height) in [
        (Unit::Mm, "70.56 mm", "35.28 mm"),
        (Unit::In, "2.778 in", "1.389 in"),
        (Unit::Px, "266.67 px", "133.33 px"),
        (Unit::Pt, "200 pt", "100 pt"),
    ] {
        the(&harness, Region::Chrome, Role::Button, unit.label()).click();
        harness.run();

        assert_eq!(harness.state().state.unit, unit);
        let shown = texts_in(&harness, Region::Inspector, Role::Label);
        assert!(shown.contains(&width.to_owned()), "{width} in {shown:?}");
        assert!(shown.contains(&height.to_owned()), "{height} in {shown:?}");
        // Rects are coordinates, not lengths: they never change.
        assert!(shown.contains(&"[0 0 200 100]".to_owned()), "{shown:?}");
        let status = the(&harness, Region::Chrome, Role::Button, unit.label());
        assert!(is_selected(&status));
    }
}

#[test]
fn escape_closes_the_popover_then_the_line_then_the_second_selection_then_the_selection() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.select(media(0));
    harness.state_mut().state.select_additional(NodeId::PageBox {
        page: 0,
        kind: BoxKind::Crop,
    });
    harness.key_press(Key::M);
    harness.run();
    let origin = harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .rect()
        .min;
    // Away from the cards in the top left corner of the canvas.
    click_at(&mut harness, origin + vec2(300.0, 400.0));
    click_at(&mut harness, origin + vec2(380.0, 400.0));
    assert!(harness.state().state.canvas.measure.line().is_some());
    // Open the revisions popover.
    click_title_chip(&harness);
    harness.run();
    assert!(harness.query_by_role_and_label(Role::Button, "rev 1").is_some());

    // 1. The popover goes first; nothing else changes.
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.query_by_role_and_label(Role::Button, "rev 1").is_none());
    assert!(harness.state().state.canvas.measure.line().is_some());
    assert!(harness.state().state.selection.secondary.is_some());

    // 2. The measured line (the tool stays on).
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().state.canvas.measure.line().is_none());
    assert!(harness.state().state.canvas.measure.active);
    assert!(harness.state().state.selection.secondary.is_some());

    // 3. The second selection.
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(harness.state().state.selection.secondary, None);
    assert_eq!(harness.state().state.selection.primary, Some(media(0)));

    // 4. The selection.
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(harness.state().state.selection.primary, None);
    assert!(has_label(&harness, "Nothing selected."));
}

#[test]
fn the_drawer_tabs_follow_cmd_1_to_5_and_show_live_counts() {
    let mut harness = app();
    open_fixture(&mut harness, "orphan-mcid");
    let keys = [
        (Key::Num1, DrawerTab::HitStack),
        (Key::Num2, DrawerTab::Diagnostics),
        (Key::Num3, DrawerTab::Validation),
        (Key::Num4, DrawerTab::Operators),
        (Key::Num5, DrawerTab::Speech),
    ];

    for (key, tab) in keys {
        harness.key_press_modifiers(Modifiers::COMMAND, key);
        harness.run_steps(2);

        assert_eq!(harness.state().state.drawer.tab, tab);
        assert!(is_selected(&the(
            &harness,
            Region::Drawer,
            Role::Button,
            tab.title()
        )));
    }

    // Speech counts what is announced, once the tab has been looked at (it just was).
    let spoken = drawer::count(&harness.state().state, DrawerTab::Speech);
    assert!(drawer::has_count(&harness.state().state, DrawerTab::Speech));
    assert!(texts_in(&harness, Region::Drawer, Role::Label).contains(&spoken.to_string()));
    // The count badges are the sizes of their lists.
    let ops = drawer::count(&harness.state().state, DrawerTab::Operators);
    assert!(ops > 0);
    let badges = texts_in(&harness, Region::Drawer, Role::Label);
    assert!(badges.contains(&ops.to_string()), "{badges:?}");
    // The hit stack count updates with a click on the page.
    assert_eq!(drawer::count(&harness.state().state, DrawerTab::HitStack), 0);
    harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .click();
    harness.run();
    let hits = drawer::count(&harness.state().state, DrawerTab::HitStack);
    assert!(hits > 0);
    assert!(texts_in(&harness, Region::Drawer, Role::Label).contains(&hits.to_string()));
}

#[test]
fn dragging_the_top_edge_resizes_the_drawer() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    // The drawer rests as a strip; the top edge is there once a tab opens it.
    assert!(
        harness
            .query_by_role_and_label(Role::Splitter, "Resize drawer")
            .is_none()
    );
    click_drawer_tab(&mut harness, "Hit stack");
    let handle = the(&harness, Region::Drawer, Role::Splitter, "Resize drawer").rect();
    assert_eq!(harness.state().state.drawer.height, 196.0);

    // Up by 80 points: taller.
    let from = handle.center();
    harness.hover_at(from);
    harness.drag_at(from);
    harness.hover_at(from - vec2(0.0, 40.0));
    harness.hover_at(from - vec2(0.0, 80.0));
    harness.drop_at(from - vec2(0.0, 80.0));
    harness.run_steps(3);

    let height = harness.state().state.drawer.height;
    assert!((height - 276.0).abs() < 1.5, "{height}");
    // The panel really got taller: the tab row moved up by as much.
    let tab = the(&harness, Region::Drawer, Role::Button, "Hit stack").rect();
    assert!(tab.min.y < 900.0 - 26.0 - 250.0, "{tab:?}");
    // It never gets smaller than the minimum.
    harness.state_mut().state.drawer.height = 10.0;
    harness.run_steps(2);
    assert_eq!(harness.state().state.drawer.height, 96.0);
    let handle = the_handle(&harness);
    harness.hover_at(handle);
    harness.drag_at(handle);
    harness.hover_at(handle + vec2(0.0, 300.0));
    harness.drop_at(handle + vec2(0.0, 300.0));
    harness.run_steps(3);
    assert!(harness.state().state.drawer.height >= 96.0);
}

fn the_handle(harness: &egui_kittest::Harness<'_, PdfxrayApp>) -> egui::Pos2 {
    harness
        .get_by_role_and_label(Role::Splitter, "Resize drawer")
        .rect()
        .center()
}

#[test]
fn the_tool_keys_toggle_the_tools_and_the_buttons_show_it() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    // The view modes live behind one control: it reads `View` until a mode is on, then names it.
    let lit = |h: &egui_kittest::Harness<'_, PdfxrayApp>, name: &str| {
        is_selected(&the(h, Region::Center, Role::Button, name))
    };
    assert!(!lit(&harness, "View"));

    harness.key_press(Key::X);
    harness.run_steps(3);
    assert!(harness.state().state.canvas.view.xray);
    assert!(lit(&harness, "X-ray"));

    // Clip and Alpha are modes of the same control, which counts the ones that are on.
    for (key, label) in [(Key::C, "X-ray +1"), (Key::A, "X-ray +2")] {
        harness.key_press(key);
        harness.run_steps(3);
        assert!(lit(&harness, label), "{label} after its key");
    }
    for (key, label) in [(Key::C, "X-ray +1"), (Key::A, "X-ray")] {
        harness.key_press(key);
        harness.run_steps(3);
        assert!(lit(&harness, label), "{label} after its key again");
    }

    for (key, check) in [(Key::B, "Page boxes"), (Key::M, "Measure")] {
        harness.key_press(key);
        harness.run_steps(3);
        assert!(
            is_selected(&the(&harness, Region::Center, Role::Button, check)),
            "{check} after its key"
        );
        harness.key_press(key);
        harness.run_steps(3);
        assert!(
            !is_selected(&the(&harness, Region::Center, Role::Button, check)),
            "{check} again"
        );
    }

    harness.key_press(Key::X);
    harness.run_steps(3);
    assert!(!harness.state().state.canvas.view.xray);
    assert!(!lit(&harness, "View"));
}

#[test]
fn cmd_f_focuses_the_navigator_filter_and_typing_narrows_the_tree() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    let all = harness
        .state_mut()
        .state
        .tree_model(pdfxray_app::NavTree::Core(pdfxray_core::TreeKind::Tags))
        .unwrap()
        .visible_len();

    harness.key_press_modifiers(Modifiers::COMMAND, Key::F);
    harness.run_steps(3);
    nodes_in(&harness, Region::Navigator, Role::TextInput)
        .remove(0)
        .type_text("H1");
    harness.run_steps(3);

    assert_eq!(harness.state().state.nav.filter, "H1");
    let shown = harness
        .state_mut()
        .state
        .tree_model(pdfxray_app::NavTree::Core(pdfxray_core::TreeKind::Tags))
        .unwrap()
        .visible_labels();
    // The match, its ancestors and what is under it; nothing else.
    assert_eq!(
        shown,
        [
            "StructTreeRoot",
            "  Document",
            "    Sect",
            "      H1",
            "        MCID 0"
        ]
    );
    let total = harness
        .state_mut()
        .state
        .tree_model(pdfxray_app::NavTree::Core(pdfxray_core::TreeKind::Tags))
        .unwrap()
        .len();
    assert!(shown.len() < total, "{} of {total}", shown.len());
    assert!(!has_label(&harness, "Table"));
    assert!(has_label(&harness, "H1"));
    // Clearing the text brings every row back.
    harness.state_mut().state.nav.filter.clear();
    harness.run_steps(3);
    let back = harness
        .state_mut()
        .state
        .tree_model(pdfxray_app::NavTree::Core(pdfxray_core::TreeKind::Tags))
        .unwrap()
        .visible_len();
    assert_eq!(back, all);
}

#[test]
fn cmd_brackets_walk_the_object_history() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    click_rail(&mut harness, "Objects");
    let object = |n| NodeId::Object(ObjRef::new(n, 0));
    for n in [1, 2, 3] {
        harness.state_mut().state.select(object(n));
    }

    harness.key_press_modifiers(Modifiers::COMMAND, Key::OpenBracket);
    harness.run_steps(2);
    assert_eq!(harness.state().state.selection.primary, Some(object(2)));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::OpenBracket);
    harness.run_steps(2);
    assert_eq!(harness.state().state.selection.primary, Some(object(1)));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::CloseBracket);
    harness.run_steps(2);
    assert_eq!(harness.state().state.selection.primary, Some(object(2)));
}

/// The labels between the `FILE` header and the `REVISIONS` header of the open popover.
fn summary_rows(harness: &Harness<'_>) -> Vec<String> {
    let labels = common::all_label_texts(harness);
    let from = labels
        .iter()
        .position(|t| t == "FILE")
        .expect("the summary is open");
    let to = labels
        .iter()
        .position(|t| t == "REVISIONS")
        .expect("the revisions are listed");
    labels[from + 1..to].to_vec()
}

#[test]
fn the_title_bar_has_one_chip_with_the_version_the_tags_and_the_revision() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");

    // Name, then the chip: the old version, tagged and rev chips are gone.
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)[0],
        "struct-rich.pdf"
    );
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.7 · Tagged · rev 1/1",
    );
    let labels = texts_in(&harness, Region::Chrome, Role::Label);
    for old in ["PDF 1.7", "Tagged", "rev 1 of 1"] {
        assert!(!labels.contains(&old.to_owned()), "{old} in {labels:?}");
    }
    assert!(harness.query_by_label("damaged xref").is_none());

    open_fixture(&mut harness, "minimal-one-page");
    assert_eq!(title_chip(&harness), "PDF 1.4 · Untagged · rev 1/1");

    // Tags and PDF/UA are hidden by encryption: the chip says so instead.
    open_fixture(&mut harness, "encrypted-stub");
    assert_eq!(title_chip(&harness), "PDF 1.7 · Encrypted · rev 1/1");
    // Smoke: it is still the only chip.
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Button)
            .iter()
            .filter(|label| label.starts_with("PDF "))
            .count(),
        1
    );
}

#[test]
fn the_chip_opens_the_file_summary_above_the_revisions_and_picking_one_reads_it_only() {
    let mut harness = app();
    open_fixture(&mut harness, "incremental-update");
    let chip = title_chip(&harness);
    assert!(chip.ends_with(" · rev 2/2"), "{chip}");
    assert!(harness.query_by_label("FILE").is_none());

    click_title_chip(&harness);
    harness.run();

    let info = harness.state().state.document.as_ref().unwrap().info.clone();
    assert_eq!(
        summary_rows(&harness),
        [
            "Name",
            "incremental-update.pdf",
            "Size",
            &pdfxray_app::object_facts::human_bytes(info.byte_len),
            "Header",
            &format!("PDF {}", info.header_version),
            "Pages",
            "1",
            "Tagged",
            "no",
            "PDF/UA",
            "not validated",
            "Encryption",
            "none",
            "Damage",
            "none",
        ]
    );
    // The timeline follows: newest first, the newest marked.
    let labels = common::all_label_texts(&harness);
    for text in [
        "latest",
        "xref table @ 826 · 2 objects",
        "xref table @ 410 · 5 objects",
    ] {
        assert!(labels.contains(&text.to_owned()), "{text} in {labels:?}");
    }
    assert_eq!(
        texts_in(&harness, Region::Center, Role::Button)
            .into_iter()
            .filter(|label| label.starts_with("rev "))
            .collect::<Vec<_>>(),
        ["rev 2", "rev 1"]
    );

    harness.get_by_role_and_label(Role::Button, "rev 1").click();
    harness.run();

    // The popover closed, the chip says it reads revision 1 of 2 and it is read-only.
    assert!(harness.query_by_label("FILE").is_none());
    assert_eq!(
        title_chip(&harness),
        chip.replace("rev 2/2", "rev 1/2 · read-only")
    );
    // Smoke: the state agrees.
    assert_eq!(harness.state().state.viewed_revision(), Some(0));

    // Back to the newest from the same popover.
    click_title_chip(&harness);
    harness.run();
    harness.get_by_role_and_label(Role::Button, "rev 2").click();
    harness.run();
    assert_eq!(title_chip(&harness), chip);
    assert_eq!(harness.state().state.viewed_revision(), None);
}

#[test]
fn the_summary_of_an_encrypted_file_names_the_cipher_and_does_not_guess_the_tags() {
    let mut harness = app();
    open_fixture(&mut harness, "encrypted-stub");

    click_title_chip(&harness);
    harness.run();

    let rows = summary_rows(&harness);
    assert_eq!(
        rows[8..12],
        ["Tagged", "unknown (encrypted)", "PDF/UA", "not validated"]
    );
    assert_eq!(rows[12..14], ["Encryption", "RC4-40"]);
    // Smoke: its revision is listed like any other.
    assert!(has_label(&harness, "REVISIONS"));
}

#[test]
fn file_info_in_the_status_bar_shows_the_counts_the_open_time_and_verapdf_on_hover() {
    let mut harness = app();
    open_fixture(&mut harness, "untagged-content");
    let doc = pdfxray_core::Document::open(fixture_path("untagged-content")).unwrap();
    let coverage = doc.coverage(0).unwrap();
    let counts = format!(
        "{} tagged · {} artifacts · {} untagged",
        coverage.tagged, coverage.artifact, coverage.untagged
    );

    // At rest the bar only says "file info": nothing of the card is shown.
    let _info = the(&harness, Region::Chrome, Role::Button, "file info");
    assert!(!has_label(&harness, &counts));
    assert!(!has_label(&harness, "opened in 0.31 s"));

    the(&harness, Region::Chrome, Role::Button, "file info").hover();
    harness.run_steps(12);

    for text in [counts.as_str(), "opened in 0.31 s", "veraPDF · not checked yet"] {
        assert!(
            has_label(&harness, text),
            "{text} in {:?}",
            common::all_label_texts(&harness)
        );
    }
    // Moving away hides it again.
    harness.remove_cursor();
    harness.run();
    assert!(!has_label(&harness, "opened in 0.31 s"));
}

#[test]
fn the_status_bar_keeps_the_cursor_file_info_units_and_problems_and_nothing_else() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.remove_cursor();
    harness.run();

    let bar = texts_in(&harness, Region::Chrome, Role::Label)
        .into_iter()
        .skip(1)
        .collect::<Vec<_>>();
    // The cursor over no page, then the item with its hover; the counts, the open time and
    // veraPDF moved into the hover.
    assert_eq!(bar[0], "x –  y –");
    assert_eq!(bar[1], "file info");
    // Left to right on screen: file info, the units, then the problems.
    let mut buttons = common::nodes_in(&harness, Region::Chrome, Role::Button);
    buttons.sort_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x));
    let buttons: Vec<String> = buttons
        .into_iter()
        .filter_map(|node| node.accesskit_node().label())
        .skip_while(|label| label.starts_with("PDF "))
        .collect();
    assert_eq!(
        buttons,
        ["file info", "pt", "mm", "in", "px", "0 diagnostics", "Validate"]
    );
    assert!(
        !bar.iter()
            .any(|t| t.starts_with("opened in") || t.starts_with("veraPDF"))
    );
}
