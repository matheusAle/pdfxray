//! The Pages entry: thumbnails, the five boxes with where they come from, the transform, the
//! jump filter and the page-box diagnostics, end to end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{
    Region, app, click_drawer_tab, click_rail, labels_between, named, open_fixture, texts_in, the,
    wait_for_render, wait_for_thumbnails,
};
use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::PdfxrayApp;
use pdfxray_core::{BoxKind, NodeId};
use pretty_assertions::assert_eq;

fn open_pages(name: &str) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, name);
    click_rail(&mut harness, "Pages");
    wait_for_thumbnails(&mut harness);
    harness
}

/// The navigator's labels of the boxes list of the page on the canvas: name, corners and where
/// the box comes from, per box.
fn box_rows(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    let page = harness.state().state.canvas.page + 1;
    labels_between(
        harness,
        Region::Navigator,
        &format!("PAGE {page} BOXES"),
        &format!("PAGE {page} TRANSFORM"),
    )
}

/// The transform rows of the page on the canvas: key, then value.
fn transform_rows(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    let page = harness.state().state.canvas.page + 1;
    let labels = texts_in(harness, Region::Navigator, Role::Label);
    let at = labels
        .iter()
        .position(|t| *t == format!("PAGE {page} TRANSFORM"))
        .unwrap();
    labels[at + 1..].to_vec()
}

/// The texts of the thumbnail buttons of the navigator, e.g. `Page 2`.
fn thumbnails(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    texts_in(harness, Region::Navigator, Role::Button)
        .into_iter()
        .filter(|t| t.starts_with("Page "))
        .collect()
}

fn strings(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| (*row).to_owned()).collect()
}

#[test]
fn the_pages_rail_lists_the_five_boxes_of_each_page_and_where_they_come_from() {
    let mut harness = open_pages("page-boxes");

    // Header, then the strip: page 1 is /Rotate 90 and says so.
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert_eq!(labels[..5], ["Pages", "3 pages · 3 sizes", "1 ↻90°", "2", "3"]);
    assert_eq!(
        harness.state().state.nav.rail.filter_hint(),
        "Jump to page, size, rotation…"
    );
    // Page 1 sets every box itself.
    assert_eq!(
        box_rows(&harness),
        strings(&[
            "MediaBox",
            "[0 0 400 300]",
            "own",
            "CropBox",
            "[10 10 390 290]",
            "own",
            "BleedBox",
            "[20 20 380 280]",
            "own",
            "TrimBox",
            "[30 30 370 270]",
            "own",
            "ArtBox",
            "[40 40 360 260]",
            "own",
        ])
    );
    assert_eq!(
        transform_rows(&harness),
        strings(&[
            "Rotate",
            "90°",
            "UserUnit",
            "1.0 (1 unit = 1 pt)",
            "Display size",
            "280 × 380 pt"
        ])
    );

    // Page 2 inherits its MediaBox from the /Pages node and defaults everything else.
    harness.state_mut().state.go_to_page(1);
    harness.run();
    assert_eq!(
        box_rows(&harness),
        strings(&[
            "MediaBox",
            "[0 0 300 400]",
            "inherited · /Pages 2 0 R",
            "CropBox",
            "[0 0 300 400]",
            "not set · defaults to MediaBox",
            "BleedBox",
            "[0 0 300 400]",
            "not set · defaults to CropBox",
            "TrimBox",
            "[0 0 300 400]",
            "not set · defaults to CropBox",
            "ArtBox",
            "[0 0 300 400]",
            "not set · defaults to CropBox",
        ])
    );
    assert_eq!(
        transform_rows(&harness),
        strings(&[
            "Rotate",
            "0°",
            "UserUnit",
            "1.0 (1 unit = 1 pt)",
            "Display size",
            "300 × 400 pt"
        ])
    );

    // Page 3 sets Media and Trim; its /UserUnit of 2 makes every length twice its coordinate.
    harness.state_mut().state.go_to_page(2);
    harness.run();
    assert_eq!(
        box_rows(&harness),
        strings(&[
            "MediaBox",
            "[0 0 400 400]",
            "own",
            "CropBox",
            "[0 0 400 400]",
            "not set · defaults to MediaBox",
            "BleedBox",
            "[0 0 400 400]",
            "not set · defaults to CropBox",
            "TrimBox",
            "[300 300 500 500]",
            "own",
            "ArtBox",
            "[0 0 400 400]",
            "not set · defaults to CropBox",
        ])
    );
    assert_eq!(
        transform_rows(&harness),
        strings(&[
            "Rotate",
            "0°",
            "UserUnit",
            "2.0 (1 unit = 2 pt)",
            "Display size",
            "400 × 400 pt"
        ])
    );
}

#[test]
fn the_units_switch_changes_every_length_of_the_pages_rail() {
    let mut harness = open_pages("box-matrix");

    // Page 1: a MediaBox with a negative origin.
    assert_eq!(
        box_rows(&harness)[..3],
        strings(&["MediaBox", "[-50 -50 650 842]", "own"])[..]
    );
    harness.state_mut().state.go_to_page(3);
    harness.run();
    // Page 4: UserUnit 3 on a 250 x 300 sheet, its CropBox inherited from the /Pages node.
    assert_eq!(
        transform_rows(&harness),
        strings(&[
            "Rotate",
            "0°",
            "UserUnit",
            "3.0 (1 unit = 3 pt)",
            "Display size",
            "690 × 840 pt"
        ])
    );

    harness.state_mut().state.unit = pdfxray_app::Unit::Mm;
    harness.run();

    assert_eq!(
        transform_rows(&harness),
        strings(&[
            "Rotate",
            "0°",
            "UserUnit",
            "3.0 (1 unit = 1.06 mm)",
            "Display size",
            "243.42 × 296.33 mm",
        ])
    );
}

#[test]
fn a_single_page_file_says_one_page_and_one_size() {
    let harness = open_pages("graphics-state");

    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert_eq!(labels[..3], ["Pages", "1 page · 1 size", "1"]);
    assert_eq!(
        box_rows(&harness),
        strings(&[
            "MediaBox",
            "[0 0 400 400]",
            "own",
            "CropBox",
            "[0 0 400 400]",
            "not set · defaults to MediaBox",
            "BleedBox",
            "[0 0 400 400]",
            "not set · defaults to CropBox",
            "TrimBox",
            "[0 0 400 400]",
            "not set · defaults to CropBox",
            "ArtBox",
            "[0 0 400 400]",
            "not set · defaults to CropBox",
        ])
    );
    assert_eq!(
        transform_rows(&harness),
        strings(&[
            "Rotate",
            "0°",
            "UserUnit",
            "1.0 (1 unit = 1 pt)",
            "Display size",
            "400 × 400 pt"
        ])
    );
}

#[test]
fn the_eye_toggle_hides_the_outline_of_one_box_and_keeps_it_in_the_legend() {
    let mut harness = open_pages("page-boxes");
    wait_for_render(&mut harness);
    // The Pages entry switches the overlay on, with every box outlined.
    assert!(harness.state().state.canvas.show_boxes);
    assert_eq!(harness.state().state.canvas.shown_boxes(), BoxKind::ALL);

    the(&harness, Region::Navigator, Role::Button, "Hide CropBox").click();
    harness.run();

    assert_eq!(
        harness.state().state.canvas.shown_boxes(),
        [BoxKind::Media, BoxKind::Bleed, BoxKind::Trim, BoxKind::Art]
    );
    // The toggle says what it does next, for that box only.
    let buttons = texts_in(&harness, Region::Navigator, Role::Button);
    assert!(buttons.contains(&"Show CropBox".to_owned()), "{buttons:?}");
    assert!(!buttons.contains(&"Hide CropBox".to_owned()));
    assert!(buttons.contains(&"Hide MediaBox".to_owned()));
    // Hiding an outline does not select anything and the legend still lists all five boxes.
    assert_ne!(
        harness.state().state.selection.primary,
        Some(NodeId::PageBox {
            page: 0,
            kind: BoxKind::Crop
        })
    );
    // At the fitted zoom the legend is a chip; with room beside the page it is the full card.
    harness.state_mut().state.set_zoom(0.4);
    harness.run();
    let legend: Vec<String> = texts_in(&harness, Region::Center, Role::Label)
        .into_iter()
        .filter(|t| t.contains('×'))
        .collect();
    assert_eq!(legend, ["400×300", "380×280", "360×260", "340×240", "320×220"]);

    // The choice holds for every page and survives the overlay being switched off and on.
    harness.state_mut().state.go_to_page(1);
    harness.run();
    assert_eq!(harness.state().state.canvas.shown_boxes().len(), 4);
    harness.key_press(egui::Key::B);
    harness.run();
    assert_eq!(harness.state().state.canvas.shown_boxes(), []);
    harness.key_press(egui::Key::B);
    harness.run();
    assert_eq!(harness.state().state.canvas.shown_boxes().len(), 4);

    // Showing it again brings the outline back.
    the(&harness, Region::Navigator, Role::Button, "Show CropBox").click();
    harness.run();
    assert_eq!(harness.state().state.canvas.shown_boxes(), BoxKind::ALL);
}

#[test]
fn clicking_a_box_row_selects_it_and_shift_click_adds_it_to_the_selection() {
    let mut harness = open_pages("page-boxes");
    let page_box = |kind| NodeId::PageBox { page: 0, kind };

    the(&harness, Region::Navigator, Role::Button, "ArtBox").click();
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(page_box(BoxKind::Art))
    );
    assert_eq!(harness.state().state.selection.secondary, None);
    assert!(common::is_selected(&the(
        &harness,
        Region::Navigator,
        Role::Button,
        "ArtBox"
    )));

    the(&harness, Region::Navigator, Role::Button, "MediaBox").click_modifiers(egui::Modifiers::SHIFT);
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(page_box(BoxKind::Art))
    );
    assert_eq!(
        harness.state().state.selection.secondary,
        Some(page_box(BoxKind::Media))
    );
    // The inspector switched to the two-node view.
    assert!(common::has_label(&harness, "2 selected · gap"));

    // A plain click drops the second selection.
    the(&harness, Region::Navigator, Role::Button, "TrimBox").click();
    harness.run();
    assert_eq!(harness.state().state.selection.secondary, None);
    assert!(!common::has_label(&harness, "2 selected · gap"));
}

#[test]
fn thumbnails_are_rendered_and_clicking_one_goes_to_that_page() {
    let mut harness = open_pages("page-boxes");

    // Every page of the strip has a picture.
    assert_eq!(harness.state().state.thumbnails_rendered(), 3);
    assert_eq!(thumbnails(&harness), ["Page 1", "Page 2", "Page 3"]);
    assert!(common::is_selected(&the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Page 1"
    )));
    assert!(!common::is_selected(&the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Page 3"
    )));

    the(&harness, Region::Navigator, Role::Button, "Page 3").click();
    harness.run();
    wait_for_render(&mut harness);

    let state = &harness.state().state;
    assert_eq!(state.canvas.page, 2);
    assert_eq!(state.selection.primary, Some(NodeId::Page(2)));
    assert_eq!(state.canvas.page_field, "3");
    // The strip shows which page is on the canvas, and the lists below follow it.
    assert!(common::is_selected(&the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Page 3"
    )));
    assert!(!common::is_selected(&the(
        &harness,
        Region::Navigator,
        Role::Button,
        "Page 1"
    )));
    assert_eq!(
        labels_between(&harness, Region::Navigator, "PAGE 3 BOXES", "PAGE 3 TRANSFORM")[..3],
        ["MediaBox", "[0 0 400 400]", "own"]
    );
}

/// Types `text` into the navigator filter.
fn filter_with(harness: &mut Harness<'_, PdfxrayApp>, text: &str) {
    common::nodes_in(harness, Region::Navigator, Role::TextInput)
        .remove(0)
        .focus();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    if text.is_empty() {
        harness.key_press(egui::Key::Backspace);
    } else {
        common::nodes_in(harness, Region::Navigator, Role::TextInput)
            .remove(0)
            .type_text(text);
    }
    harness.run();
}

#[test]
fn the_filter_jumps_to_a_page_by_number_rotation_or_size() {
    let mut harness = open_pages("box-matrix");
    assert_eq!(thumbnails(&harness), ["Page 1", "Page 2", "Page 3", "Page 4"]);

    // `rot 180` is page 2 only; the canvas goes there and the strip narrows to it.
    filter_with(&mut harness, "rot 180");
    assert_eq!(harness.state().state.canvas.page, 1);
    assert_eq!(thumbnails(&harness), ["Page 2"]);
    assert!(common::has_label(&harness, "2 ↻180°"));
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Page(1)));

    // A number is a page.
    filter_with(&mut harness, "4");
    assert_eq!(harness.state().state.canvas.page, 3);
    assert_eq!(thumbnails(&harness), ["Page 4"]);

    // A size: page 3 is 280 x 230 pt shown turned by 270 degrees.
    filter_with(&mut harness, "230x280");
    assert_eq!(harness.state().state.canvas.page, 2);
    assert_eq!(thumbnails(&harness), ["Page 3"]);

    // Nothing matches: it says so, and the canvas stays where it was.
    filter_with(&mut harness, "rot 90");
    assert_eq!(harness.state().state.canvas.page, 2);
    assert!(common::has_label(&harness, "No pages match “rot 90”."));
    assert_eq!(thumbnails(&harness), Vec::<String>::new());

    // Clearing the filter lists every page again, on the same page.
    filter_with(&mut harness, "");
    assert_eq!(thumbnails(&harness), ["Page 1", "Page 2", "Page 3", "Page 4"]);
    assert_eq!(harness.state().state.canvas.page, 2);
}

#[test]
fn the_diagnostics_drawer_is_filtered_to_page_boxes_on_this_entry() {
    let mut harness = open_pages("page-boxes");
    harness.state_mut().state.drawer.height = 260.0;
    harness.run();
    // The drawer rests as a strip on this entry; a click on its tab opens it.
    click_drawer_tab(&mut harness, "Diagnostics");
    let shown = |harness: &Harness<'_, PdfxrayApp>| {
        let labels = texts_in(harness, Region::Drawer, Role::Label);
        let at = labels
            .iter()
            .position(|t| t.starts_with("box-"))
            .unwrap_or(labels.len());
        labels[at..].to_vec()
    };

    assert!(harness.state().state.drawer.page_boxes_only);
    assert_eq!(
        shown(&harness),
        strings(&[
            "box-outside-mediabox",
            "TrimBox of Page 3 [150 150 250 250] extends outside its MediaBox [0 0 200 200]",
            "page 3 · 5 0 R",
            "box-inherited",
            "MediaBox for page 2 comes from /Pages 2 0 R, not the page itself",
            "1 page",
            "rotate-nonzero",
            "Page 1 is rotated 90°: shown portrait, coordinates stay landscape",
            "page 1",
        ])
    );
    // The filter is named and can be lifted.
    let _link = the(&harness, Region::Drawer, Role::Link, "Clear filter");

    // A row selects what it is about: the box, the box that is inherited, the page.
    the(&harness, Region::Drawer, Role::Button, "box-outside-mediabox").click();
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::PageBox {
            page: 2,
            kind: BoxKind::Trim
        })
    );
    assert_eq!(harness.state().state.canvas.page, 2);
    the(&harness, Region::Drawer, Role::Button, "box-inherited").click();
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::PageBox {
            page: 1,
            kind: BoxKind::Media
        })
    );
    assert_eq!(harness.state().state.canvas.page, 1);
    the(&harness, Region::Drawer, Role::Button, "rotate-nonzero").click();
    harness.run();
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Page(0)));
    assert_eq!(harness.state().state.canvas.page, 0);

    // Clearing the filter shows what the reader found, nothing more.
    the(&harness, Region::Drawer, Role::Link, "Clear filter").click();
    harness.run();
    assert!(!harness.state().state.drawer.page_boxes_only);
    assert_eq!(
        shown(&harness),
        strings(&[
            "box-outside-mediabox",
            "TrimBox of Page 3 [150 150 250 250] extends outside its MediaBox [0 0 200 200]",
            "5 0 R",
        ])
    );
    // The tab is not filtered on the other entries.
    click_rail(&mut harness, "Content");
    click_drawer_tab(&mut harness, "Diagnostics");
    assert!(!harness.state().state.drawer.page_boxes_only);
    click_rail(&mut harness, "Pages");
    assert!(harness.state().state.drawer.page_boxes_only);
}

#[test]
fn the_diagnostics_drawer_groups_inherited_boxes_and_lists_every_turned_page() {
    let mut harness = open_pages("box-matrix");
    harness.state_mut().state.drawer.height = 300.0;
    harness.run();
    click_drawer_tab(&mut harness, "Diagnostics");

    let labels = texts_in(&harness, Region::Drawer, Role::Label);
    let at = labels.iter().position(|t| t.starts_with("box-")).unwrap();
    assert_eq!(
        labels[at..],
        strings(&[
            "box-inherited",
            "MediaBox for page 3 comes from /Pages 2 0 R, not the page itself",
            "1 page",
            "box-inherited",
            "CropBox for pages 3, 4 comes from /Pages 2 0 R, not the page itself",
            "2 pages",
            "rotate-nonzero",
            "Page 2 is rotated 180°: shown upside down, coordinates stay landscape",
            "page 2",
            "rotate-nonzero",
            "Page 3 is rotated 270°: shown landscape, coordinates stay portrait",
            "page 3",
        ])[..]
    );
    // The CropBox row selects the CropBox of the first page that inherits it.
    named(&harness, Region::Drawer, Role::Button, "box-inherited")[1].click();
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::PageBox {
            page: 2,
            kind: BoxKind::Crop
        })
    );
}

#[test]
fn a_file_without_page_box_notes_says_so() {
    let mut harness = open_pages("minimal-one-page");
    click_drawer_tab(&mut harness, "Diagnostics");

    assert!(common::has_label(
        &harness,
        "Nothing to note about the page boxes."
    ));
    harness.run();
}

/// The names of the items of the context menu that is open.
fn menu_items(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

#[test]
fn right_click_on_a_thumbnail_and_on_a_box_row_lists_the_actions_of_the_page_and_the_box() {
    let mut harness = open_pages("page-boxes");
    let page_object = harness
        .state()
        .state
        .document
        .as_ref()
        .unwrap()
        .doc
        .page(1)
        .unwrap()
        .obj
        .expect("page 2 is an indirect object");

    the(&harness, Region::Navigator, Role::Button, "Page 2").click_secondary();
    harness.run_steps(3);
    assert_eq!(
        menu_items(&harness),
        ["Show in tree", "Object", "Raw bytes", "Copy id", "Copy JSON"]
    );
    harness.get_by_role_and_label(Role::MenuItem, "Object").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(page_object))
    );
    // Smoke: the Objects entry is showing.
    assert_eq!(harness.state().state.nav.rail, pdfxray_app::RailItem::Objects);

    click_rail(&mut harness, "Pages");
    the(&harness, Region::Navigator, Role::Button, "CropBox").click_secondary();
    harness.run_steps(3);
    assert_eq!(
        menu_items(&harness),
        ["Show in tree", "Object", "Raw bytes", "Copy id", "Copy JSON"]
    );
    harness
        .get_by_role_and_label(Role::MenuItem, "Show in tree")
        .click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::PageBox {
            page: harness.state().state.canvas.page,
            kind: BoxKind::Crop
        })
    );
}
