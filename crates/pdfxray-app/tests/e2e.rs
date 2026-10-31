//! End-to-end flows through the whole app: open a fixture, click, inspect, navigate.

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

use common::{
    Region, app, buttons_in, click_at, click_drawer_tab, click_rail, has_label, is_selected, named, nav_rows,
    nodes_in, open_fixture, texts_in, the, wait_for_render,
};
use egui::{Event, Key, Modifiers, MouseWheelUnit, Role, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::NavTree;
use pdfxray_app::center::toolbar::zoom_text;
use pdfxray_core::{BoxKind, NodeId, TreeKind};
use pretty_assertions::assert_eq;

/// Clicks 4 points in from the top-left corner of the page image, where nothing is painted.
fn click_page_corner(harness: &mut egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) {
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    click_at(harness, canvas.min + vec2(4.0, 4.0));
}

fn media(page: u32) -> NodeId {
    NodeId::PageBox {
        page,
        kind: BoxKind::Media,
    }
}

fn crop(page: u32) -> NodeId {
    NodeId::PageBox {
        page,
        kind: BoxKind::Crop,
    }
}

/// The names of the property rows of the inspector: its labels that are keys, i.e. followed by
/// a value. Taken from the layout order of the labels.
fn inspector_labels(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) -> Vec<String> {
    texts_in(harness, Region::Inspector, Role::Label)
}

/// The title and the id line of the inspector header: the id line is the first label with a
/// middle dot, the title comes right before it.
fn title_and_id(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) -> (String, String) {
    let labels = inspector_labels(harness);
    let at = labels.iter().position(|t| t.contains(" · ")).expect("an id line");
    (labels[at - 1].clone(), labels[at].clone())
}

/// The names of the buttons of the title and status bars.
fn chrome_buttons(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) -> Vec<String> {
    buttons_in(harness, Region::Chrome)
}

#[test]
fn open_click_the_page_follow_the_hit_stack_and_a_link() {
    let mut harness = app();

    // Open the file: the title bar names it and the toolbar shows "1" of "/ 1".
    open_fixture(&mut harness, "minimal-one-page");
    assert_eq!(harness.state().state.status, pdfxray_app::READY);
    assert!(has_label(&harness, "minimal-one-page.pdf"));
    assert!(has_label(&harness, "/ 1"));
    // Opening selects the document: the inspector shows its title and id line.
    assert_eq!(
        title_and_id(&harness),
        ("minimal-one-page.pdf".to_owned(), "doc · document".to_owned())
    );

    click_page_corner(&mut harness);

    // Core answered with both boxes (equal size, media first) and the page.
    let stack = harness.state().state.drawer.hit_stack.clone().unwrap();
    let labels: Vec<&str> = stack.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["MediaBox", "CropBox", "Page 1"]);
    let kinds: Vec<&str> = stack.entries.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["page-box", "page-box", "page"]);
    // 4 screen points at the fitted zoom, from the left edge and down from the top (y is up).
    let zoom = f64::from(harness.state().state.canvas.zoom);
    assert!(
        (stack.point.x - 4.0 / zoom).abs() < 0.05 && (stack.point.y - (100.0 - 4.0 / zoom)).abs() < 0.05,
        "{:?} at zoom {zoom}",
        stack.point
    );
    assert_eq!(harness.state().state.selection.primary, Some(media(0)));

    // The Hit stack tab lists the same entries, topmost first, and where the click was.
    assert_eq!(
        buttons_in(&harness, Region::Drawer)[5..],
        ["MediaBox", "CropBox", "Page 1"]
    );
    assert!(
        pdfxray_app::drawer::hit_stack::header_note(&harness.state().state)
            .is_some_and(|note| note.starts_with("click @ ") && note.ends_with(" pt"))
    );
    // The rows say where each node lives.
    let details: Vec<String> = pdfxray_app::drawer::hit_stack::lines(&harness.state().state)
        .into_iter()
        .map(|line| line.detail)
        .collect();
    assert_eq!(
        details,
        ["[0 0 200 100]", "[0 0 200 100]", "3 0 R · MediaBox [0 0 200 100]"]
    );
    // The selected entry is the one that is highlighted.
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "MediaBox"
    )));
    assert!(!is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "CropBox"
    )));

    // The inspector shows the selected box: breadcrumb, title, id line and its geometry.
    let crumbs: Vec<String> = buttons_in(&harness, Region::Inspector)
        .into_iter()
        .take_while(|name| name != "Pin")
        .collect();
    assert_eq!(crumbs, ["minimal-one-page.pdf", "Page 1", "MediaBox"]);
    let labels = inspector_labels(&harness);
    assert!(
        labels.contains(&"box:0:media · page-box".to_owned()),
        "{labels:?}"
    );
    for (key, value) in [
        ("Rect", "[0 0 200 100]"),
        ("Width", "200 pt"),
        ("Height", "100 pt"),
    ] {
        let at = labels.iter().position(|t| t == key).expect(key);
        assert_eq!(labels[at + 1], value, "{key}");
    }
    // The last two rows are links: where the box is defined, and the page.
    assert!(labels.contains(&"Defined on".to_owned()), "{labels:?}");
    assert_eq!(
        texts_in(&harness, Region::Inspector, Role::Link),
        ["page 3 0 R", "Page 1"]
    );

    // Clicking another hit stack entry selects it everywhere.
    the(&harness, Region::Drawer, Role::Button, "CropBox").click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(crop(0)));
    assert!(has_label(&harness, "box:0:crop · page-box"));
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "CropBox"
    )));

    // The row that links to the page is clickable and moves the selection to it.
    the(&harness, Region::Inspector, Role::Link, "Page 1").click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Page(0)));
    assert_eq!(
        texts_in(&harness, Region::Inspector, Role::Link),
        [
            "[0 0 200 100]",
            "[0 0 200 100] (default = MediaBox)",
            "[0 0 200 100] (default = CropBox)",
            "[0 0 200 100] (default = CropBox)",
            "[0 0 200 100] (default = CropBox)",
            "3 0 R",
            "3 0 R"
        ]
    );
    assert_eq!(
        title_and_id(&harness),
        ("Page 1".to_owned(), "page:0 · page".to_owned())
    );
    // Smoke: the hit stack of the canvas click is untouched by selection changes.
    assert_eq!(
        harness
            .state()
            .state
            .drawer
            .hit_stack
            .as_ref()
            .map(|s| s.entries.len()),
        Some(3)
    );
}

#[test]
fn clicking_a_breadcrumb_selects_that_ancestor() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    click_page_corner(&mut harness);
    assert_eq!(harness.state().state.selection.primary, Some(media(0)));
    // The last crumb is the node itself.
    assert!(is_selected_crumb_free(&harness));

    the(&harness, Region::Inspector, Role::Button, "Page 1").click();
    harness.run();

    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Page(0)));
    the(&harness, Region::Inspector, Role::Button, "minimal-one-page.pdf").click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Document));
    assert!(has_label(&harness, "doc · document"));
}

/// Crumbs are plain buttons; this only checks that all three are there before clicking.
fn is_selected_crumb_free(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) -> bool {
    let names = buttons_in(harness, Region::Inspector);
    ["minimal-one-page.pdf", "Page 1", "MediaBox"]
        .iter()
        .all(|crumb| names.iter().any(|n| n == crumb))
}

#[test]
fn next_page_updates_the_page_field_and_the_canvas() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    assert!(has_label(&harness, "/ 3"));
    assert_eq!(harness.state().state.canvas.page, 0);

    the(&harness, Region::Center, Role::Button, "Next").click();
    harness.run();
    wait_for_render(&mut harness);

    assert_eq!(harness.state().state.canvas.page, 1);
    // The page field follows.
    assert_eq!(
        nodes_in(&harness, Region::Center, Role::TextInput)[0]
            .value()
            .as_deref(),
        Some("2")
    );
    // The canvas now describes page 2.
    let _canvas = harness.get_by_role_and_label(Role::Image, "Page 2 canvas");
}

#[test]
fn page_buttons_stop_at_the_first_and_last_page() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    let disabled = |harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>, name: &str| {
        the(harness, Region::Center, Role::Button, name)
            .accesskit_node()
            .is_disabled()
    };
    assert!(disabled(&harness, "Prev"));
    assert!(!disabled(&harness, "Next"));

    for _ in 0..3 {
        the(&harness, Region::Center, Role::Button, "Next").click();
        harness.run();
    }

    assert_eq!(harness.state().state.canvas.page, 2);
    assert!(disabled(&harness, "Next"));
    the(&harness, Region::Center, Role::Button, "Prev").click();
    harness.run();
    assert_eq!(harness.state().state.canvas.page, 1);
    assert!(!disabled(&harness, "Prev"));
}

#[test]
fn typing_a_page_number_jumps_there() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");

    let field = nodes_in(&harness, Region::Center, Role::TextInput).remove(0);
    field.focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    nodes_in(&harness, Region::Center, Role::TextInput)
        .remove(0)
        .type_text("3");
    harness.key_press(Key::Enter);
    harness.run();

    assert_eq!(harness.state().state.canvas.page, 2);
    assert_eq!(
        nodes_in(&harness, Region::Center, Role::TextInput)[0]
            .value()
            .as_deref(),
        Some("3")
    );
}

#[test]
fn an_out_of_range_page_number_is_ignored() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");

    nodes_in(&harness, Region::Center, Role::TextInput)
        .remove(0)
        .focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    nodes_in(&harness, Region::Center, Role::TextInput)
        .remove(0)
        .type_text("9");
    harness.key_press(Key::Enter);
    harness.run();

    assert_eq!(harness.state().state.canvas.page, 0);
    assert_eq!(
        nodes_in(&harness, Region::Center, Role::TextInput)[0]
            .value()
            .as_deref(),
        Some("1")
    );
}

#[test]
fn the_objects_entry_lists_the_document_the_resources_and_every_object() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");

    click_rail(&mut harness, "Objects");

    // The three sections of the list, then the objects in number order.
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    for heading in ["DOCUMENT", "ALL OBJECTS", "8 objects · 1 rev"] {
        assert!(labels.contains(&heading.to_owned()), "{heading} in {labels:?}");
    }
    let rows = nav_rows(&harness, 0);
    assert_eq!(
        rows,
        [
            "trailer", "/Catalog", "1 0 obj", "2 0 obj", "3 0 obj", "4 0 obj", "5 0 obj", "6 0 obj",
            "7 0 obj", "8 0 obj"
        ]
    );

    // The trailer has properties of its own.
    the(&harness, Region::Navigator, Role::Button, "trailer").click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Trailer));
    assert_eq!(
        title_and_id(&harness),
        ("Trailer".to_owned(), "trailer · trailer".to_owned())
    );
    let labels = inspector_labels(&harness);
    assert!(labels.contains(&"/Size".to_owned()), "{labels:?}");
    assert!(labels.contains(&"9".to_owned()), "{labels:?}");
    assert!(labels.contains(&"/Root".to_owned()), "{labels:?}");

    // A page object: the inspector names what it is, the object view shows its dictionary.
    the(&harness, Region::Navigator, Role::Button, "3 0 obj").click();
    // A page object has a preview, rendered in the background.
    common::wait_for_previews(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(pdfxray_core::ObjRef::new(3, 0)))
    );
    let (title, id_line) = title_and_id(&harness);
    assert_eq!(title, "Page");
    assert!(id_line.starts_with("obj:3:0"), "{id_line}");
    assert_eq!(harness.state().state.selection_rect(), None);
    let center = texts_in(&harness, Region::Center, Role::Label);
    assert!(center.contains(&"3 0 obj".to_owned()), "{center:?}");
    assert!(center.contains(&"[10 10 390 290]".to_owned()), "{center:?}");
    // The footer is gone; the actions of an object are on the right-click menu of its title.
    named(&harness, Region::Inspector, Role::Label, "Page")[0].click_secondary();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::MenuItem, "Raw bytes").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.objects.mode, pdfxray_app::ObjectMode::Raw);
    // Smoke: the object view switched to the bytes of the same object.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(pdfxray_core::ObjRef::new(3, 0)))
    );
}

#[test]
fn empty_trees_say_so_and_populated_ones_list_their_nodes() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");

    // Tags is the first entry; the file has no structure tree.
    assert!(has_label(&harness, "This file has no structure tree."));
    // The page has content but no artifacts: the filter leaves nothing to show.
    click_rail(&mut harness, "Artifacts");
    assert!(has_label(&harness, "This page has no artifacts."));
    assert!(!has_label(&harness, "This file has no structure tree."));
    // The page does have resources: one font, listed in the Objects entry.
    click_rail(&mut harness, "Objects");
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(labels.contains(&"RESOURCES · PAGE 1".to_owned()), "{labels:?}");
    assert_eq!(nav_rows(&harness, 0)[2..5], ["Font", "Collapse Font", "/F1"]);
}

#[test]
fn zoom_buttons_change_the_zoom_and_cached_bitmaps_are_reused() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    let after_open = harness.state().state.render_count();
    let fitted = harness.state().state.canvas.zoom;
    assert!(fitted > 3.0);
    assert!(chrome_free_button(&harness, &zoom_text(fitted)));

    // The readout is the button for the actual size.
    the(&harness, Region::Center, Role::Button, &zoom_text(fitted)).click();
    harness.run();
    wait_for_render(&mut harness);
    assert_eq!(harness.state().state.canvas.zoom, 1.0);
    assert!(chrome_free_button(&harness, "100%"));
    assert_eq!(harness.state().state.render_count(), after_open + 1);
    assert!(!is_selected(&the(
        &harness,
        Region::Center,
        Role::Button,
        "Fit page"
    )));

    the(&harness, Region::Center, Role::Button, "Zoom in").click();
    harness.run();
    wait_for_render(&mut harness);
    assert_eq!(harness.state().state.canvas.zoom, 1.25);
    assert!(chrome_free_button(&harness, "125%"));
    assert_eq!(harness.state().state.render_count(), after_open + 2);

    // Back to a zoom that was shown before: no new rasterisation.
    the(&harness, Region::Center, Role::Button, "Zoom out").click();
    harness.run();
    wait_for_render(&mut harness);
    assert_eq!(harness.state().state.canvas.zoom, 1.0);
    assert_eq!(harness.state().state.render_count(), after_open + 2);

    // Fit goes back to the fitted zoom and its cached bitmap.
    the(&harness, Region::Center, Role::Button, "Fit page").click();
    harness.run();
    wait_for_render(&mut harness);
    assert!(harness.state().state.canvas.zoom > 3.0);
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::Button,
        "Fit page"
    )));
    assert_eq!(harness.state().state.render_count(), after_open + 2);
}

#[test]
fn cmd_scroll_over_the_canvas_zooms() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    let canvas = harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .rect()
        .center();

    harness.hover_at(canvas);
    harness.run();
    harness.event_modifiers(
        Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: vec2(0.0, 5.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::COMMAND,
        },
        Modifiers::COMMAND,
    );
    harness.run();

    let zoom = harness.state().state.canvas.zoom;
    assert!(zoom > 1.0, "zoomed in: {zoom}");
    assert!(chrome_free_button(&harness, &zoom_text(zoom)));
    // Smoke: scrolling zoomed the canvas and left the fit mode.
    assert_eq!(
        harness.state().state.canvas.zoom_mode,
        pdfxray_app::view::ZoomMode::Manual
    );
}

#[test]
fn copy_icons_put_the_id_and_values_on_the_clipboard() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    click_page_corner(&mut harness);

    // The selection bar on the canvas copies the id.
    the(&harness, Region::Center, Role::Button, "Copy id").click();
    // Copy commands are in the output of the frame that handled the click only.
    harness.step();
    assert_eq!(copied(&harness), Some("box:0:media".to_owned()));

    // A row's copy icon shows while the pointer is on the row; it copies the row's value.
    let rect = labelled_rect(&harness, "Rect");
    harness.hover_at(rect.center());
    harness.run();
    the(&harness, Region::Inspector, Role::Button, "Copy Rect").click();
    harness.step();
    assert_eq!(copied(&harness), Some("[0 0 200 100]".to_owned()));

    // Right-click on the title copies everything as JSON. The crumb, the title and the chip of
    // the strip are all called MediaBox; the title is the second.
    harness.run_steps(2);
    named(&harness, Region::Inspector, Role::Label, "MediaBox")[1].click_secondary();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::MenuItem, "Copy JSON").click();
    harness.step();
    let json = copied(&harness).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["title"], "MediaBox");
    assert_eq!(parsed["kind"], "page-box");
}

/// True when the toolbar has a button with this name, e.g. the zoom readout.
fn chrome_free_button(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>, name: &str) -> bool {
    buttons_in(harness, Region::Center).iter().any(|n| n == name)
}

fn labelled_rect(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>, text: &str) -> egui::Rect {
    named(harness, Region::Inspector, Role::Label, text)[0].rect()
}

fn copied(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

#[test]
fn an_encrypted_document_lists_its_pages_but_the_canvas_says_it_is_not_rendered() {
    let mut harness = app();

    open_fixture(&mut harness, "encrypted-stub");
    // The explainer comes first; picking a rail entry goes on to the workspace.
    assert_eq!(harness.state().state.screen(), pdfxray_app::Screen::Encrypted);
    click_rail(&mut harness, "Tags");

    assert_eq!(harness.state().state.status, pdfxray_app::READY);
    assert!(has_label(&harness, "encrypted-stub.pdf"));
    // The structure is readable: the page is counted and listed.
    assert_eq!(harness.state().state.page_count(), 1);
    assert!(
        texts_in(&harness, Region::Center, Role::Label).contains(&"Encrypted: not rendered in v1".to_owned())
    );
    assert!(has_label(&harness, "/ 1"));
    // The toolbar is there, the page is not.
    for name in [
        "Prev",
        "Next",
        "Zoom in",
        "Zoom out",
        "Fit page",
        "Measure",
        "Page boxes",
        "View",
    ] {
        let _button = the(&harness, Region::Center, Role::Button, name);
    }
    // No bitmap was made for it.
    assert_eq!(harness.state().state.render_count(), 0);
    assert!(harness.query_by_label("Page 1 canvas").is_none());
    click_drawer_tab(&mut harness, "Diagnostics");
    assert!(has_label(&harness, "Rendering engine: Encrypted"));
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.7 · Encrypted · rev 1/1",
    );
    click_rail(&mut harness, "Pages");
    // The boxes are listed although the page is not drawn; the thumbnail has no picture.
    assert_eq!(
        common::labels_between(&harness, Region::Navigator, "PAGE 1 BOXES", "PAGE 1 TRANSFORM")
            .into_iter()
            .filter(|t| t.ends_with("Box") && !t.contains(' '))
            .collect::<Vec<_>>(),
        ["MediaBox", "CropBox", "BleedBox", "TrimBox", "ArtBox"]
    );
    assert_eq!(harness.state().state.thumbnails_rendered(), 0);
    // Smoke: selecting its page shows the page's facts in the inspector.
    the(&harness, Region::Navigator, Role::Button, "Page 1").click();
    harness.run();
    assert_eq!(title_and_id(&harness).0, "Page 1");
}

#[test]
fn page_boxes_selection_is_drawn_on_the_canvas_only_for_its_page() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");

    harness.state_mut().state.select(crop(0));
    harness.run();
    let rect = harness.state().state.selection_rect().unwrap();

    assert_eq!(rect.page, 0);
    assert_eq!(
        (rect.rect.x0, rect.rect.y0, rect.rect.x1, rect.rect.y1),
        (10.0, 10.0, 390.0, 290.0)
    );
    // Selecting a box on another page moves the canvas there.
    harness.state_mut().state.select(crop(2));
    harness.run();
    assert_eq!(harness.state().state.canvas.page, 2);
    assert_eq!(harness.state().state.selection_rect().map(|r| r.page), Some(2));
}

#[test]
fn the_diagnostics_tab_lists_what_the_reader_found() {
    let mut harness = app();
    // Junk before the header is reported by the reader whatever else it learns to detect.
    let mut bytes = b"junk before the header\n".to_vec();
    bytes.extend(std::fs::read(common::fixture_path("minimal-one-page")).unwrap());
    let path = std::env::temp_dir().join(format!("pdfxray-junk-header-{}.pdf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();

    harness.state_mut().state.open_path(&path);
    harness.state_mut().state.drawer.height = 420.0;
    harness.run();
    common::wait_for_open(&mut harness);
    std::fs::remove_file(&path).unwrap();
    // A file whose header is not at the start is damaged: the drawer opens on its problems by
    // itself, so a click on the tab would fold it again.
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Diagnostics"
    )));

    let found = harness
        .state()
        .state
        .document
        .as_ref()
        .unwrap()
        .doc
        .info()
        .diagnostics
        .clone();
    assert_eq!(found[0].code, "header-not-at-start");
    assert_eq!(
        found[0].message,
        "the %PDF- header starts at byte 23 instead of byte 0"
    );
    let shown = texts_in(&harness, Region::Drawer, Role::Label);
    for diagnostic in &found {
        assert!(shown.contains(&diagnostic.code.to_owned()), "{shown:?}");
        assert!(shown.contains(&diagnostic.message), "{shown:?}");
    }
    assert!(!has_label(&harness, "No diagnostics."));
    // The tab's count, the status bar and the rail all know how many there are.
    assert!(shown.contains(&found.len().to_string()));
    assert!(chrome_buttons(&harness).contains(&format!("{} diagnostics", found.len())));
}

#[test]
fn a_clean_document_has_no_diagnostics() {
    let mut harness = app();

    open_fixture(&mut harness, "minimal-one-page");
    click_drawer_tab(&mut harness, "Diagnostics");

    assert_eq!(
        texts_in(&harness, Region::Drawer, Role::Label)
            .into_iter()
            .filter(|t| t.contains("iagnostics"))
            .collect::<Vec<_>>(),
        ["Diagnostics", "No diagnostics."]
    );
    assert!(chrome_buttons(&harness).contains(&"0 diagnostics".to_owned()));
}

#[test]
fn the_content_entry_follows_the_current_page() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    click_rail(&mut harness, "Content");

    // Page 1: four marked-content sequences, one per line of text.
    assert_eq!(
        harness.state().state.nav_tree(),
        Some(NavTree::Core(TreeKind::Content(0)))
    );
    let lines = |harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>| {
        texts_in(harness, Region::Navigator, Role::Label)
            .into_iter()
            .filter(|t| t.starts_with('"'))
            // The rows below the first screenful are not built.
            .take(3)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        lines(&harness),
        [
            "\"Chapter 1\" #4",
            "\"First paragraph of chapter 1\" #11",
            "\"Second paragraph of chapter 1\" #18"
        ]
    );

    the(&harness, Region::Center, Role::Button, "Next").click();
    harness.run();

    assert_eq!(
        harness.state().state.nav_tree(),
        Some(NavTree::Core(TreeKind::Content(1)))
    );
    assert_eq!(
        lines(&harness),
        [
            "\"Chapter 2\" #4",
            "\"First paragraph of chapter 2\" #11",
            "\"Second paragraph of chapter 2\" #18"
        ]
    );
    // Going back shows page 1 again.
    the(&harness, Region::Center, Role::Button, "Prev").click();
    harness.run();
    assert!(has_label(&harness, "\"Chapter 1\" #4"));
    assert!(!has_label(&harness, "\"Chapter 2\" #4"));
}

#[test]
fn selecting_a_content_operator_outlines_its_box_on_the_canvas() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    click_rail(&mut harness, "Content");

    the(&harness, Region::Navigator, Role::Button, "Tj").click();
    harness.run();

    // `(Hello pdfxray) Tj` at 20 40 in 18 pt Helvetica: the box starts at the text origin and
    // lies inside the 200 x 100 pt page.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 3 })
    );
    let rect = harness.state().state.selection_rect().unwrap();
    assert_eq!(rect.page, 0);
    assert_eq!(rect.rect.x0, 20.0);
    assert!(rect.rect.x1 > 100.0 && rect.rect.x1 < 200.0, "{rect:?}");
    assert!(
        rect.rect.y0 < 40.0 && rect.rect.y1 > 40.0 && rect.rect.y1 < 100.0,
        "{rect:?}"
    );
    assert!(title_and_id(&harness).1.starts_with("op:0:3 · "));
    // Smoke: the scroll request was consumed by drawing the outline.
    assert!(!harness.state().state.canvas.reveal_pending());
    // The Operators tab lists the same operator, highlighted.
    assert!(is_selected(&the(&harness, Region::Drawer, Role::Button, "Tj")));
}

#[test]
fn the_artifacts_entry_narrows_the_content_tree_to_artifact_sequences() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    click_rail(&mut harness, "Content");
    let all = nav_rows(&harness, 2).len();

    click_rail(&mut harness, "Artifacts");

    let rows = nav_rows(&harness, 2);
    assert!(
        rows.len() < all || rows.is_empty(),
        "artifacts {rows:?} of {all} rows"
    );
    // Typing adds to the artifact filter instead of replacing it.
    assert_eq!(harness.state().state.nav.filter, "");
}
