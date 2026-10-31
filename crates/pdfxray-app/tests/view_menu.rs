//! The View control of the canvas toolbar: the button that names the active mode, the menu with
//! every mode and its key, the X-ray options it stores and the keys `⇧T`, `L` and `R`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/view_tools.rs"]
mod view_tools;

use common::{Region, app, is_selected, nodes_in, open_fixture, texts_in, the};
use egui::{Key, Modifiers, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::theme::tokens;
use pdfxray_app::{OrderSource, PdfxrayApp, ViewTool};
use pdfxray_core::NodeId;
use pdfxray_render::ColorBy;
use pretty_assertions::assert_eq;

fn opened() -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, "graphics-state");
    harness
}

fn settle(harness: &mut Harness<'_, PdfxrayApp>) {
    harness.run_steps(4);
}

fn on(harness: &Harness<'_, PdfxrayApp>, tool: ViewTool) -> bool {
    tool.active(&harness.state().state)
}

#[test]
fn the_button_says_view_until_a_mode_is_on_and_then_names_it() {
    let mut harness = opened();
    // The button sits in the toolbar and is not lit while nothing is on.
    assert!(!is_selected(&the(&harness, Region::Center, Role::Button, "View")));
    assert_eq!(view_tools::trigger(&harness), "View");

    for tool in [
        ViewTool::Xray,
        ViewTool::PaintOrder,
        ViewTool::Clip,
        ViewTool::Alpha,
    ] {
        view_tools::toggle(&mut harness, tool.label());

        // The button now reads the mode, in the neutral active look.
        assert_eq!(view_tools::trigger(&harness), tool.label());
        assert!(is_selected(&the(
            &harness,
            Region::Center,
            Role::Button,
            tool.label()
        )));
        assert!(on(&harness, tool), "{tool:?}");

        view_tools::toggle(&mut harness, tool.label());

        assert_eq!(view_tools::trigger(&harness), "View");
        assert!(!on(&harness, tool), "{tool:?}");
    }
    // Smoke: nothing is left on; the toolbar has View, Isolate and Hide, and no button for the
    // menu's modes.
    assert!(ViewTool::ALL.iter().all(|tool| !on(&harness, *tool)));
    let toolbar = tokens::TITLE_H + 40.0;
    let buttons: Vec<String> = nodes_in(&harness, Region::Center, Role::Button)
        .into_iter()
        .filter(|node| node.rect().max.y < toolbar)
        .filter_map(|node| node.accesskit_node().label())
        .collect();
    for present in ["View", "Isolate", "Hide"] {
        assert!(buttons.contains(&present.to_owned()), "{present} in {buttons:?}");
    }
    for in_menu in ["X-ray", "Paint order", "Clip", "Alpha"] {
        assert!(!buttons.contains(&in_menu.to_owned()), "{in_menu} in {buttons:?}");
    }
}

#[test]
fn the_icon_toggles_the_active_mode_and_only_the_arrow_opens_the_menu() {
    let mut harness = opened();
    assert!(!on(&harness, ViewTool::Xray));

    // With no mode on, the toggle turns the x-ray on; the menu stays shut.
    the(&harness, Region::Center, Role::Button, "View").click();
    settle(&mut harness);
    assert!(on(&harness, ViewTool::Xray));
    assert_eq!(view_tools::trigger(&harness), "X-ray");
    assert!(!view_tools::is_open(&harness));

    // With a mode on, it turns that mode off again.
    the(&harness, Region::Center, Role::Button, "X-ray").click();
    settle(&mut harness);
    assert!(!on(&harness, ViewTool::Xray));
    assert_eq!(view_tools::trigger(&harness), "View");

    // The toggle follows the first mode that is on when there are several.
    view_tools::toggle(&mut harness, "Clip");
    view_tools::toggle(&mut harness, "Alpha");
    the(&harness, Region::Center, Role::Button, "Clip +1").click();
    settle(&mut harness);
    assert!(!on(&harness, ViewTool::Clip) && on(&harness, ViewTool::Alpha));

    // The arrow is a button of its own that opens the menu, and does not change a mode.
    the(&harness, Region::Center, Role::Button, "View options").click();
    settle(&mut harness);
    assert!(view_tools::is_open(&harness));
    assert!(on(&harness, ViewTool::Alpha) && !on(&harness, ViewTool::Xray));
    // Smoke: the menu is anchored under the whole control, starting at the toggle.
    let toggle = the(&harness, Region::Center, Role::Button, "Alpha").rect();
    let first = the(&harness, Region::Center, Role::MenuItem, "X-ray").rect();
    assert!(
        (first.min.x - toggle.min.x).abs() < 12.0,
        "{first:?} vs {toggle:?}"
    );
    assert!(first.min.y > toggle.max.y);
}

#[test]
fn isolate_and_hide_are_toolbar_buttons_that_wait_for_a_selection() {
    let mut harness = opened();
    harness.state_mut().state.clear_selection();
    settle(&mut harness);
    // Nothing to isolate yet: both are disabled, and say why.
    for name in ["Isolate", "Hide"] {
        assert!(
            view_tools::toolbar_button(&harness, name)
                .accesskit_node()
                .is_disabled(),
            "{name}"
        );
    }
    let isolate = view_tools::toolbar_button(&harness, "Isolate").rect();
    rest_on(&mut harness, isolate.center());
    assert!(common::has_label(
        &harness,
        pdfxray_app::center::toolbar::NEEDS_SELECTION
    ));

    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 3 });
    settle(&mut harness);
    view_tools::toggle(&mut harness, "Isolate");

    // The button is lit; the View button still names the page-wide modes only.
    assert!(on(&harness, ViewTool::Isolate));
    assert!(!on(&harness, ViewTool::Hide));
    assert!(is_selected(&view_tools::toolbar_button(&harness, "Isolate")));
    assert_eq!(view_tools::trigger(&harness), "View");

    view_tools::toggle(&mut harness, "Hide");

    // Hide replaces isolate: one mode of the two is on, and it is the one that was asked for.
    assert!(on(&harness, ViewTool::Hide));
    assert!(!on(&harness, ViewTool::Isolate));
    assert!(is_selected(&view_tools::toolbar_button(&harness, "Hide")));
    assert!(!is_selected(&view_tools::toolbar_button(&harness, "Isolate")));
    assert_eq!(
        harness
            .state()
            .state
            .canvas
            .view
            .isolation
            .as_ref()
            .map(|i| i.node.clone()),
        Some(NodeId::Op { page: 0, ix: 3 })
    );
}

#[test]
fn more_than_one_mode_on_shows_the_first_and_how_many_more() {
    let mut harness = opened();

    view_tools::toggle(&mut harness, "Alpha");
    view_tools::toggle(&mut harness, "X-ray");
    view_tools::toggle(&mut harness, "Clip");

    // Menu order, not the order they were turned on in.
    assert_eq!(view_tools::trigger(&harness), "X-ray +2");
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::Button,
        "X-ray +2"
    )));
    assert_eq!(
        ViewTool::ALL.iter().filter(|tool| on(&harness, **tool)).count(),
        3
    );
}

#[test]
fn the_menu_lists_every_mode_with_its_icon_row_and_key_and_marks_the_ones_that_are_on() {
    let mut harness = opened();
    view_tools::toggle(&mut harness, "Alpha");

    view_tools::open(&mut harness);

    // The page-wide modes; isolate and hide are toolbar buttons.
    let items = texts_in(&harness, Region::Center, Role::MenuItem);
    assert_eq!(items, ["X-ray", "Paint order", "Clip", "Alpha"]);
    // Each name has its key beside it as a keycap.
    let labels = texts_in(&harness, Region::Center, Role::Label);
    for key in ["X", "P", "C", "A", "⇧T", "L", "R"] {
        assert!(labels.contains(&key.to_owned()), "{key} in {labels:?}");
    }
    // Only Alpha is lit.
    for tool in ViewTool::PAGE {
        let item = the(&harness, Region::Center, Role::MenuItem, tool.label());
        assert_eq!(is_selected(&item), tool == ViewTool::Alpha, "{tool:?}");
    }
    // Smoke: the menu stays open while the user toggles, so a mode and its options go together.
    the(&harness, Region::Center, Role::MenuItem, "X-ray").click();
    settle(&mut harness);
    assert!(view_tools::is_open(&harness));
    assert!(on(&harness, ViewTool::Xray));
}

#[test]
fn the_xray_options_are_stored_in_the_view_state() {
    let mut harness = opened();
    assert_eq!(harness.state().state.canvas.view.color_by, ColorBy::Kind);

    // Colour by: Kind · Tag role · Artifact.
    view_tools::open(&mut harness);
    assert!(is_selected(&the(&harness, Region::Center, Role::Button, "Kind")));
    the(&harness, Region::Center, Role::Button, "Tag role").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.canvas.view.color_by, ColorBy::TagRole);
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::Button,
        "Tag role"
    )));
    assert!(!is_selected(&the(&harness, Region::Center, Role::Button, "Kind")));
    the(&harness, Region::Center, Role::Button, "Artifact").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.canvas.view.color_by, ColorBy::Artifact);

    // Show: the five switches start as the design has them, and each flips its own field.
    let show = harness.state().state.canvas.view.show;
    assert_eq!(
        (
            show.all_boxes,
            show.labels,
            show.reading_order,
            show.nesting,
            show.artifacts
        ),
        (false, true, false, true, true)
    );
    for label in [
        "All tag boxes",
        "Tag labels",
        "Reading order",
        "Nesting",
        "Artifacts",
    ] {
        the(&harness, Region::Center, Role::CheckBox, label).click();
        settle(&mut harness);
    }
    let show = harness.state().state.canvas.view.show;
    assert_eq!(
        (
            show.all_boxes,
            show.labels,
            show.reading_order,
            show.nesting,
            show.artifacts
        ),
        (true, false, true, false, false)
    );
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::CheckBox,
        "All tag boxes"
    )));
    assert!(!is_selected(&the(
        &harness,
        Region::Center,
        Role::CheckBox,
        "Tag labels"
    )));

    // Reading order from: Structure tree · Paint order.
    assert_eq!(
        harness.state().state.canvas.view.order_from,
        OrderSource::Structure
    );
    the(&harness, Region::Center, Role::Button, "Paint order").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.canvas.view.order_from, OrderSource::Paint);
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::Button,
        "Paint order"
    )));
    // That is the option, not the mode: the stepper stays off.
    assert!(!on(&harness, ViewTool::PaintOrder));

    // Tag colours are fixed: the menu offers no way to edit them.
    assert!(harness.query_by_label_contains("Edit tag colours").is_none());
    // Smoke: choosing options did not turn the x-ray on.
    assert!(!harness.state().state.canvas.view.xray);
}

#[test]
fn shift_t_l_and_r_toggle_the_tag_overlay_options() {
    let mut harness = opened();
    let show = |harness: &Harness<'_, PdfxrayApp>| {
        let show = harness.state().state.canvas.view.show;
        (show.all_boxes, show.labels, show.reading_order)
    };
    assert_eq!(show(&harness), (false, true, false));

    harness.key_press_modifiers(Modifiers::SHIFT, Key::T);
    settle(&mut harness);
    assert_eq!(show(&harness), (true, true, false));

    harness.key_press(Key::L);
    settle(&mut harness);
    assert_eq!(show(&harness), (true, false, false));

    harness.key_press(Key::R);
    settle(&mut harness);
    assert_eq!(show(&harness), (true, false, true));

    // The menu shows what the keys did.
    view_tools::open(&mut harness);
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::CheckBox,
        "All tag boxes"
    )));
    assert!(!is_selected(&the(
        &harness,
        Region::Center,
        Role::CheckBox,
        "Tag labels"
    )));
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::CheckBox,
        "Reading order"
    )));
    view_tools::close(&mut harness);

    // Smoke: a plain T does nothing, and the old mode keys still work.
    harness.key_press(Key::T);
    settle(&mut harness);
    assert_eq!(show(&harness), (true, false, true));
    harness.key_press(Key::X);
    settle(&mut harness);
    assert!(on(&harness, ViewTool::Xray));
    assert_eq!(view_tools::trigger(&harness), "X-ray");
}

/// Rests the pointer on `at` long enough for a tooltip.
fn rest_on(harness: &mut Harness<'_, PdfxrayApp>, at: egui::Pos2) {
    harness.hover_at(at);
    harness.step();
    harness.remove_cursor();
    harness.hover_at(at);
    harness.step();
    harness.run_steps(3);
}

#[test]
fn the_hint_of_a_menu_row_opens_beside_the_menu_and_covers_none_of_its_rows() {
    let mut harness = opened();
    view_tools::open(&mut harness);
    let rows: Vec<egui::Rect> = ViewTool::PAGE
        .iter()
        .map(|tool| the(&harness, Region::Center, Role::MenuItem, tool.label()).rect())
        .collect();
    let menu_right = rows.iter().map(|row| row.max.x).fold(f32::MIN, f32::max);
    let menu_left = rows.iter().map(|row| row.min.x).fold(f32::MAX, f32::min);

    for (tool, row) in ViewTool::PAGE.iter().zip(&rows) {
        rest_on(&mut harness, row.center());

        let hint = harness.get_by_label(tool.hint()).rect();
        // Smoke: the menu is still open under the pointer.
        assert!(view_tools::is_open(&harness), "{tool:?}");
        // Beside the menu, to its right, and level with the row it describes ...
        assert!(
            hint.min.x >= menu_right,
            "{tool:?}: hint {hint:?} starts left of the menu edge {menu_right}"
        );
        assert!(
            hint.min.y < row.max.y && hint.max.y > row.min.y,
            "{tool:?}: hint {hint:?} is not level with {row:?}"
        );
        // ... so it covers no row of the menu.
        for other in &rows {
            assert!(
                !hint.intersects(*other),
                "{tool:?}: hint {hint:?} covers the row {other:?}"
            );
        }
        harness.remove_cursor();
        harness.run_steps(2);
    }
    assert!(menu_left < menu_right);
}

#[test]
fn the_trigger_of_an_open_menu_shows_no_tooltip_over_the_menu() {
    let mut harness = opened();
    view_tools::open(&mut harness);
    let arrow = the(&harness, Region::Center, Role::Button, "View options").rect();

    rest_on(&mut harness, arrow.center());

    // egui keeps the tooltip of a widget shut while its popup is open: it would sit on the
    // first rows of the menu.
    assert!(
        harness
            .query_by_label("Render modes: x-ray, isolate, hide, paint order, clip, alpha")
            .is_none()
    );
    // Smoke: the menu is still open, and a row of it still has its hint on hover.
    assert!(view_tools::is_open(&harness));
    let row = the(&harness, Region::Center, Role::MenuItem, "Clip").rect();
    rest_on(&mut harness, row.center());
    assert!(harness.query_by_label(ViewTool::Clip.hint()).is_some());
}
