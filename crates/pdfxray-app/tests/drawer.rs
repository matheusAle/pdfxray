//! The drawer: a strip of tabs until the user asks for more, open where it is the main content,
//! and count badges that are coloured only for problems.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use common::fake::{FakeValidator, struct_rich_report};
use common::{
    Region, app, buttons_in, click_at, click_drawer_tab, click_rail, has_label, is_selected, named,
    open_fixture, the, wait_until,
};
use egui::{Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::components::Tone;
use pdfxray_app::drawer::badge_tone;
use pdfxray_app::{Availability, DrawerTab, PdfxrayApp, RailItem};
use pretty_assertions::assert_eq;

type App = Harness<'static, PdfxrayApp>;

fn opened(fixture: &str) -> App {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    harness
}

fn click_page_corner(harness: &mut App) {
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    click_at(harness, canvas.min + vec2(4.0, 4.0));
}

fn open(harness: &App) -> bool {
    harness.state().state.drawer.open
}

#[test]
fn the_drawer_starts_as_a_34_px_strip_of_tabs() {
    let harness = opened("minimal-one-page");

    assert!(!open(&harness));
    for name in ["Hit stack", "Diagnostics", "Validation", "Operators", "Speech"] {
        assert_eq!(
            named(&harness, Region::Drawer, Role::Button, name).len(),
            1,
            "{name}"
        );
    }
    // The tabs sit in a strip just above the status bar: far below where an open drawer's are.
    let tab = the(&harness, Region::Drawer, Role::Button, "Hit stack").rect();
    assert!(tab.max.y - tab.min.y <= 34.0 && tab.min.y > 800.0, "{tab:?}");
    assert!(harness.query_by_label("Resize drawer").is_none());
    // Smoke: no table of any tab is drawn.
    assert!(!has_label(
        &harness,
        pdfxray_app::drawer::hit_stack::HIT_STACK_HINT
    ));
}

#[test]
fn clicking_the_page_opens_it_on_the_hit_stack() {
    let mut harness = opened("minimal-one-page");
    click_page_corner(&mut harness);

    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::HitStack);
    let rows = buttons_in(&harness, Region::Drawer);
    assert_eq!(rows[5..], ["MediaBox", "CropBox", "Page 1"]);
    // Smoke: the panel is as tall as it was dragged to, not a strip.
    let tab = the(&harness, Region::Drawer, Role::Button, "Hit stack").rect();
    assert!(tab.min.y < 900.0 - 28.0 - 100.0, "{tab:?}");
}

#[test]
fn clicking_the_same_point_of_the_page_again_opens_the_hit_stack_again() {
    let mut harness = opened("minimal-one-page");
    click_page_corner(&mut harness);
    click_drawer_tab(&mut harness, "Hit stack");
    assert!(!open(&harness));

    // The stack under the point is the same as before, and it opens all the same.
    click_page_corner(&mut harness);

    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::HitStack);
    // Smoke: it was on another tab when the click came.
    click_drawer_tab(&mut harness, "Diagnostics");
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
    click_page_corner(&mut harness);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::HitStack);
    assert_eq!(
        buttons_in(&harness, Region::Drawer)[5..],
        ["MediaBox", "CropBox", "Page 1"]
    );
}

#[test]
fn the_status_bar_the_rail_and_the_tab_keys_open_a_collapsed_drawer() {
    let mut harness = opened("broken-xref-offsets");
    let fold = |harness: &mut App| {
        harness.state_mut().state.drawer.open = false;
        harness.run_steps(2);
        assert!(!open(harness));
    };

    // The problems count in the status bar.
    fold(&mut harness);
    let count = format!(
        "{} diagnostics",
        harness
            .state()
            .state
            .document
            .as_ref()
            .unwrap()
            .info
            .diagnostics
            .len()
    );
    the(&harness, Region::Chrome, Role::Button, &count).click();
    harness.run_steps(3);
    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);

    // The warning on the rail.
    fold(&mut harness);
    the(&harness, Region::Rail, Role::Button, "Diagnostics").click();
    harness.run_steps(3);
    assert!(open(&harness));

    // ⌘1 to ⌘4 pick the tabs in order.
    fold(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num4);
    harness.run_steps(3);
    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Operators);
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Operators"
    )));
}

#[test]
fn clicking_the_active_tab_collapses_it_and_clicking_another_opens_it() {
    let mut harness = opened("minimal-one-page");
    click_page_corner(&mut harness);
    assert!(open(&harness));

    click_drawer_tab(&mut harness, "Hit stack");
    assert!(!open(&harness));
    assert!(named(&harness, Region::Drawer, Role::Button, "MediaBox").is_empty());

    click_drawer_tab(&mut harness, "Diagnostics");
    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Diagnostics"
    )));

    click_drawer_tab(&mut harness, "Diagnostics");
    assert!(!open(&harness));
    // Smoke: collapsing keeps the tab, so a click on it opens the same list again.
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
}

#[test]
fn the_drawer_starts_open_in_content_objects_and_validation_and_as_a_strip_elsewhere() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report = struct_rich_report();
    let mut harness = app();
    harness.state_mut().state.validator = Arc::new(fake);
    open_fixture(&mut harness, "struct-rich");

    for (rail, expected) in [
        ("Content", true),
        ("Tags", false),
        ("Objects", true),
        ("Pages", false),
        ("Artifacts", false),
        ("Revisions", false),
    ] {
        click_rail(&mut harness, rail);
        assert_eq!(open(&harness), expected, "{rail}");
    }
    // Validation: the shield on the rail.
    the(&harness, Region::Rail, Role::Button, "Validation").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.nav.rail, RailItem::Validation);
    assert!(open(&harness));
    // Smoke: each entry opens on its own tab.
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Validation);
    click_rail(&mut harness, "Content");
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Operators);
}

#[test]
fn a_choice_survives_until_the_entry_changes() {
    let mut harness = opened("struct-rich");
    click_rail(&mut harness, "Content");
    click_drawer_tab(&mut harness, "Operators");
    assert!(!open(&harness), "the active tab of an open drawer collapses it");
    // The same entry again does not reset what the user chose.
    click_rail(&mut harness, "Content");
    assert!(!open(&harness));
    click_rail(&mut harness, "Tags");
    click_rail(&mut harness, "Content");
    assert!(open(&harness));
}

#[test]
fn the_paint_order_stepper_opens_the_drawer_on_its_tab() {
    let mut harness = opened("minimal-one-page");
    assert!(!open(&harness));
    harness.state_mut().state.set_stepper(true);
    harness.run_steps(3);

    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::PaintOrder);
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Paint order"
    )));

    // Turning it off brings back the old tab and leaves the drawer as the user has it.
    harness.state_mut().state.set_stepper(false);
    harness.run_steps(3);
    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::HitStack);
}

#[test]
fn a_damaged_file_opens_with_the_drawer_on_its_problems() {
    let harness = opened("broken-xref-offsets");
    assert!(open(&harness));
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
    // Smoke: a sound file does not.
    assert!(!open(&opened("minimal-one-page")));
}

#[test]
fn count_badges_are_neutral_except_for_warnings_and_errors() {
    // A sound file: a count of 0 diagnostics and the hit stack are neutral.
    let harness = opened("minimal-one-page");
    let state = &harness.state().state;
    assert_eq!(badge_tone(state, DrawerTab::HitStack), Tone::Neutral);
    assert_eq!(badge_tone(state, DrawerTab::Diagnostics), Tone::Neutral);
    assert_eq!(badge_tone(state, DrawerTab::Operators), Tone::Neutral);

    // A file the reader had to repair: the diagnostics count carries the worst severity.
    let harness = opened("broken-xref-offsets");
    let state = &harness.state().state;
    let tone = badge_tone(state, DrawerTab::Diagnostics);
    assert!(matches!(tone, Tone::Warn | Tone::Err), "{tone:?}");
    // The operators are a count of content, never a problem.
    assert_eq!(badge_tone(state, DrawerTab::Operators), Tone::Neutral);
    assert_eq!(badge_tone(state, DrawerTab::Validation), Tone::Neutral);
}

#[test]
fn failed_validation_checks_make_the_validation_badge_an_error() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report = struct_rich_report();
    fake.run_gate.open();
    let mut harness = app();
    harness.state_mut().state.validator = Arc::new(fake);
    open_fixture(&mut harness, "struct-rich");
    the(&harness, Region::Chrome, Role::Button, "Validate").click();
    wait_until(&mut harness, "the validation result", |s| {
        s.document.as_ref().unwrap().doc.validation().is_some() && !s.validation.is_running()
    });

    let state = &harness.state().state;
    assert_eq!(badge_tone(state, DrawerTab::Validation), Tone::Err);
    // The finished run leaves the drawer open on the findings.
    assert!(state.drawer.open);
    assert_eq!(state.drawer.tab, DrawerTab::Validation);
}

#[test]
fn making_a_tab_active_moves_none_of_the_tabs() {
    let mut harness = opened("minimal-one-page");
    // Speech is the last: its badge appears once it has been read, which is not the title's doing.
    let names = ["Hit stack", "Diagnostics", "Validation", "Operators", "Speech"];
    let spans = |harness: &App| -> Vec<(f32, f32)> {
        names
            .iter()
            .map(|name| {
                let rect = the(harness, Region::Drawer, Role::Button, name).rect();
                (rect.min.x, rect.max.x)
            })
            .collect()
    };
    click_drawer_tab(&mut harness, "Hit stack");
    let first = spans(&harness);

    for name in &names[1..] {
        click_drawer_tab(&mut harness, name);
        assert_eq!(
            harness.state().state.drawer.tab.title(),
            *name,
            "the tab became active"
        );
        let now = spans(&harness);
        for ((before, after), tab) in first.iter().zip(&now).zip(names).take(names.len() - 1) {
            assert!(
                (before.0 - after.0).abs() < 0.5 && (before.1 - after.1).abs() < 0.5,
                "{tab} moved when {name} became active: {before:?} -> {after:?}"
            );
        }
    }
    // Smoke: the tabs are laid out left to right, apart from each other.
    assert!(first.windows(2).all(|pair| pair[0].1 < pair[1].0), "{first:?}");
}

#[test]
fn a_drawer_tab_is_a_hand_and_a_resting_pointer_lights_it() {
    let mut harness = opened("minimal-one-page");
    let tab = the(&harness, Region::Drawer, Role::Button, "Diagnostics").rect();
    let off = tab.center() - vec2(0.0, 60.0);
    let cursor_at = |harness: &mut App, at: egui::Pos2| {
        harness.hover_at(at);
        harness.run_steps(2);
        harness.output().platform_output.cursor_icon
    };

    assert_eq!(
        cursor_at(&mut harness, tab.center()),
        egui::CursorIcon::PointingHand
    );
    // The wash behind it is a rounded rectangle of the hover token, not there with the pointer
    // away.
    let washes = |harness: &App| {
        harness
            .output()
            .shapes
            .iter()
            .flat_map(|clipped| match &clipped.shape {
                egui::Shape::Vec(shapes) => shapes.clone(),
                other => vec![other.clone()],
            })
            .filter(|shape| match shape {
                egui::Shape::Rect(rect) => rect.fill == pdfxray_app::theme::tokens::NEUTRAL_HOVER,
                _ => false,
            })
            .count()
    };
    let lit = washes(&harness);
    assert_eq!(cursor_at(&mut harness, off), egui::CursorIcon::Default);
    assert_eq!(washes(&harness), lit - 1);
}

#[test]
fn a_clickable_row_of_the_drawer_is_a_hand_with_the_trees_hover_wash() {
    let mut harness = opened("minimal-one-page");
    click_page_corner(&mut harness);
    let row = the(&harness, Region::Drawer, Role::Button, "CropBox").rect();
    harness.hover_at(row.center());
    harness.run_steps(2);

    assert_eq!(
        harness.output().platform_output.cursor_icon,
        egui::CursorIcon::PointingHand
    );
    let tokens = pdfxray_app::theme::tokens::NEUTRAL_HOVER;
    let washes: Vec<egui::Rect> = harness
        .output()
        .shapes
        .iter()
        .flat_map(|clipped| match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.clone(),
            other => vec![other.clone()],
        })
        .filter_map(|shape| match shape {
            egui::Shape::Rect(rect) if rect.fill == tokens => Some(rect.rect),
            _ => None,
        })
        .collect();
    // The wash lies inside the row, inset like the tree's, and is as tall as the row less a hair.
    assert!(
        washes
            .iter()
            .any(|wash| row.contains_rect(*wash) && (wash.height() - (row.height() - 1.0)).abs() < 0.6),
        "{washes:?} in {row:?}"
    );
    // Smoke: pointer away, no wash on that row and the arrow back.
    harness.hover_at(row.center() - vec2(0.0, 200.0));
    harness.run_steps(2);
    assert_ne!(
        harness.output().platform_output.cursor_icon,
        egui::CursorIcon::PointingHand
    );
}

/// The tab buttons of the strip, in order, whatever the size of the window.
fn tab_rects(harness: &App, names: &[&str]) -> Vec<egui::Rect> {
    names
        .iter()
        .map(|name| {
            // The rail has a Diagnostics button of its own, at the window's left edge.
            harness
                .query_all_by_role_and_label(Role::Button, name)
                .map(|node| node.rect())
                .find(|rect| rect.min.x > 300.0)
                .unwrap_or_else(|| panic!("the {name} tab"))
        })
        .collect()
}

#[test]
fn at_the_narrowest_window_every_tab_fits_the_strip_and_none_is_cut() {
    const TABS: [&str; 5] = ["Hit stack", "Diagnostics", "Validation", "Operators", "Speech"];
    let mut harness = opened("struct-rich");
    harness.set_size(vec2(1100.0, 700.0));
    harness.run_steps(4);
    click_page_corner(&mut harness);
    harness.run_steps(4);
    let wide = {
        let mut roomy = opened("struct-rich");
        roomy.run_steps(2);
        tab_rects(&roomy, &TABS)
    };

    let tabs = tab_rects(&harness, &TABS);

    // The strip is as wide as the centre: the window less the rail, the navigator and the
    // inspector. Every tab ends inside it, with its inset to spare, and in order.
    let right = 1100.0 - pdfxray_app::theme::tokens::INSPECTOR_W;
    for (name, tab) in TABS.iter().zip(&tabs) {
        assert!(
            tab.max.x <= right - 10.0,
            "{name} reaches {tab:?}, the strip ends at {right}"
        );
    }
    for pair in tabs.windows(2) {
        assert!(pair[0].max.x < pair[1].min.x, "{pair:?}");
    }
    // Smoke: in the roomy window the tabs keep their wide spacing, so it is the narrow one that
    // tightened them.
    let gap = |rects: &[egui::Rect]| rects[1].min.x - rects[0].max.x;
    assert!(gap(&wide) > gap(&tabs), "{} vs {}", gap(&wide), gap(&tabs));
}

#[test]
fn at_the_narrowest_window_the_diagnostics_keep_room_for_their_message() {
    let mut harness = opened("broken-xref-offsets");
    // A damaged file opens the drawer on its own; a click on the open tab would close it.
    if !open(&harness) {
        click_drawer_tab(&mut harness, "Diagnostics");
    }
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
    harness.set_size(vec2(1100.0, 700.0));
    harness.run_steps(4);
    assert!(open(&harness));

    let rows = harness
        .state()
        .state
        .document
        .as_ref()
        .unwrap()
        .info
        .diagnostics
        .clone();
    let message = rows[0].message.clone();
    let code = harness
        .query_all_by_role(Role::Label)
        .find(|node| node.value().as_deref() == Some("xref-offset-wrong"))
        .expect("the code of the first row")
        .rect();
    let text = harness
        .query_all_by_role(Role::Label)
        .find(|node| {
            node.value()
                .is_some_and(|t| t.starts_with(&message[..8.min(message.len())]))
                && node.rect().min.x > code.max.x
        })
        .expect("the message of the first row")
        .rect();

    // The message is more than a few letters wide: the code and the place gave way.
    assert!(text.width() >= 140.0, "{text:?} next to {code:?}");
    // Smoke: the code is still its own column, left of the message.
    assert!(code.max.x <= text.min.x, "{code:?} {text:?}");
}
