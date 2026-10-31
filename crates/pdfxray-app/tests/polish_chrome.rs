//! The small refinements of the shell chrome: nothing clipped or covered, hit areas a pointer
//! can find, the right cursor, and entries that do nothing not looking selected.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

mod common;

use common::{
    Region, all_label_texts, app, fixture_path, is_selected, nodes_in, open_fixture, open_path, the,
};
use egui::{CursorIcon, Role, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::PdfxrayApp;
use pdfxray_app::center::{pick, toolbar};
use pretty_assertions::assert_eq;

type App = Harness<'static, PdfxrayApp>;

/// Rests the pointer on `at` long enough for a tooltip.
fn rest_on(harness: &mut App, at: egui::Pos2) {
    harness.hover_at(at);
    harness.step();
    harness.remove_cursor();
    harness.hover_at(at);
    harness.step();
    harness.run_steps(3);
}

/// The cursor the app asked the window for in the last frame.
fn cursor_icon(harness: &App) -> CursorIcon {
    harness.output().platform_output.cursor_icon
}

/// Hovers `at` and returns the cursor the app asks for there.
fn cursor_over(harness: &mut App, at: egui::Pos2) -> CursorIcon {
    harness.hover_at(at);
    harness.run_steps(2);
    cursor_icon(harness)
}

fn opened() -> App {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.remove_cursor();
    harness.run();
    harness
}

// ---- title bar ----

#[test]
fn a_long_file_name_is_cut_and_never_runs_under_the_chip_or_validate() {
    let dir = std::env::temp_dir().join(format!("pdfxray-polish-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let name = format!("{}.pdf", "a-very-long-file-name-".repeat(8));
    let path = dir.join(&name);
    std::fs::copy(fixture_path("minimal-one-page"), &path).unwrap();
    let mut harness = app();

    open_path(&mut harness, path);

    let label = nodes_in(&harness, Region::Chrome, Role::Label)
        .into_iter()
        .find(|node| node.value().is_some_and(|v| v.starts_with("a-very-long")))
        .expect("the name in the title bar");
    let name_rect = label.rect();
    let chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        &common::title_chip(&harness),
    )
    .rect();
    let validate = the(&harness, Region::Chrome, Role::Button, "Validate").rect();
    // The name ends before the chip, the chip before Validate, and nothing leaves the window.
    assert!(name_rect.max.x <= chip.min.x, "{name_rect:?} vs {chip:?}");
    assert!(chip.max.x <= validate.min.x, "{chip:?} vs {validate:?}");
    assert!(validate.max.x <= common::WINDOW.x, "{validate:?}");
    // Smoke: it is still a name, not a stub, and the file is the one that was opened.
    assert!(name_rect.width() > 200.0, "{name_rect:?}");
    assert_eq!(harness.state().state.document.as_ref().unwrap().info.name, name);
    std::fs::remove_dir_all(&dir).ok();
}

// ---- rail ----

#[test]
fn rail_entries_that_do_nothing_are_not_selected_and_the_ones_that_work_show_a_hand() {
    let mut harness = app();

    // Nothing is open: the rail remembers Tags, but it does nothing, so it must not look picked.
    for name in ["Tags", "Content", "Validation"] {
        assert!(
            !is_selected(&the(&harness, Region::Rail, Role::Button, name)),
            "{name}"
        );
    }
    let tags = the(&harness, Region::Rail, Role::Button, "Tags").rect();
    assert_eq!(cursor_over(&mut harness, tags.center()), CursorIcon::Default);
    // Settings works without a file.
    let settings = the(&harness, Region::Rail, Role::Button, "Settings").rect();
    assert_eq!(
        cursor_over(&mut harness, settings.center()),
        CursorIcon::PointingHand
    );

    open_fixture(&mut harness, "minimal-one-page");
    harness.remove_cursor();
    harness.run();

    assert!(is_selected(&the(&harness, Region::Rail, Role::Button, "Tags")));
    let tags = the(&harness, Region::Rail, Role::Button, "Tags").rect();
    assert_eq!(cursor_over(&mut harness, tags.center()), CursorIcon::PointingHand);
}

// ---- status bar ----

#[test]
fn the_cursor_readout_over_the_page_is_shown_in_full_and_does_not_push_the_file_info() {
    // A 612 x 792 page.
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    harness.remove_cursor();
    harness.run();
    let info = the(&harness, Region::Chrome, Role::Button, "file info").rect();
    // Over the middle of the page the readout has three digits and two decimals everywhere:
    // `x 306.00  y 396.00 pt · display 306.00, 396.00`, 46 mono characters of 6 points.
    harness.hover_at(pos2(common::WINDOW.x / 2.0, common::WINDOW.y / 2.0));
    harness.run_steps(3);

    let readout = nodes_in(&harness, Region::Chrome, Role::Label)
        .into_iter()
        .find(|node| {
            node.value()
                .is_some_and(|v| v.starts_with("x ") && v.contains("display"))
        })
        .expect("the cursor readout");
    let text = readout.value().unwrap();
    assert_eq!(text.chars().count(), 46, "{text}");
    // Not cut with an ellipsis: the label is as wide as its text.
    assert!(
        readout.rect().width() >= 46.0 * 6.0 - 1.0,
        "{:?} for {text}",
        readout.rect()
    );
    // Smoke: what follows the cursor stays where it is when the cursor leaves the page.
    assert_eq!(
        the(&harness, Region::Chrome, Role::Button, "file info")
            .rect()
            .min
            .x,
        info.min.x
    );
}

#[test]
fn the_diagnostics_and_the_units_are_targets_a_pointer_can_find() {
    let mut harness = opened();
    let bar = tokens_status_h();

    let diagnostics = the(&harness, Region::Chrome, Role::Button, "0 diagnostics").rect();
    assert!(diagnostics.height() >= bar - 6.0, "{diagnostics:?}");
    assert_eq!(
        cursor_over(&mut harness, diagnostics.center()),
        CursorIcon::PointingHand
    );
    for unit in ["pt", "mm", "in", "px"] {
        let segment = the(&harness, Region::Chrome, Role::Button, unit).rect();
        assert!(segment.height() >= 18.0, "{unit}: {segment:?}");
    }
    // Smoke: the five controls still share the bar, in order.
    let pt = the(&harness, Region::Chrome, Role::Button, "pt").rect();
    assert!(pt.max.x < diagnostics.min.x);
}

fn tokens_status_h() -> f32 {
    pdfxray_app::theme::tokens::STATUS_H
}

// ---- toolbar ----

#[test]
fn the_zoom_readout_keeps_its_width_so_the_buttons_beside_it_stay_put() {
    let mut harness = opened();
    let place = |harness: &mut App, zoom: f32| {
        harness.state_mut().state.set_zoom(zoom);
        harness.run_steps(3);
        (
            the(harness, Region::Center, Role::Button, toolbar::ZOOM_OUT_LABEL).rect(),
            the(harness, Region::Center, Role::Button, &toolbar::zoom_text(zoom)).rect(),
        )
    };

    let (out_100, readout_100) = place(&mut harness, 1.0);
    let (out_10, readout_10) = place(&mut harness, 0.1);
    let (out_800, readout_800) = place(&mut harness, 8.0);

    assert_eq!(out_10.min.x, out_100.min.x);
    assert_eq!(out_800.min.x, out_100.min.x);
    assert_eq!(readout_10.width(), readout_100.width());
    assert_eq!(readout_800.width(), readout_100.width());
    // Smoke: the slot is room for three digits and not much more.
    assert!(
        readout_100.width() > 30.0 && readout_100.width() < 70.0,
        "{readout_100:?}"
    );
}

// ---- menus ----

#[test]
fn the_pick_menu_rows_are_the_height_of_every_row_and_only_live_ones_show_a_hand() {
    let mut harness = opened();
    the(&harness, Region::Center, Role::Button, "Pick: All").click();
    harness.run_steps(3);
    let rows: Vec<(String, egui::Rect)> = nodes_in(&harness, Region::Center, Role::Button)
        .into_iter()
        .filter_map(|node| Some((node.accesskit_node().label()?, node.rect())))
        .filter(|(label, rect)| rect.height() < 40.0 && rect.width() > 200.0 && !label.starts_with("Pick"))
        .collect();
    assert_eq!(rows.len(), 6, "{rows:?}");
    for (label, rect) in &rows {
        assert_eq!(rect.height(), pdfxray_app::theme::tokens::ROW_H, "{label}");
    }
    let find = |name: &str| rows.iter().find(|(label, _)| label == name).unwrap().1;
    assert_eq!(
        cursor_over(&mut harness, find("Boxes").center()),
        CursorIcon::PointingHand
    );
    assert_eq!(
        cursor_over(&mut harness, find("Annotations").center()),
        CursorIcon::Default
    );

    // The reason Annotations cannot be picked opens beside the menu, not over its rows.
    rest_on(&mut harness, find("Annotations").center());
    let tip = all_label_texts(&harness)
        .into_iter()
        .find(|t| t.starts_with("Annotations are not hit-tested"))
        .expect("the tooltip of the Annotations row");
    let tip = harness.get_by_label(&tip).rect();
    let menu_x = rows.iter().map(|(_, r)| r.min.x..=r.max.x);
    let (left, right) = menu_x.fold((f32::MAX, f32::MIN), |(l, r), x| {
        (l.min(*x.start()), r.max(*x.end()))
    });
    assert!(
        tip.max.x <= left || tip.min.x >= right,
        "{tip:?} over the menu {left}..{right}"
    );
    for (label, row) in &rows {
        assert!(!tip.intersects(*row), "the tooltip covers {label}");
    }
    assert!(pick::available(pdfxray_core::LayerMode::Boxes));
}

#[test]
fn the_view_menu_rows_show_a_hand_and_the_right_click_menu_rows_fill_the_menu() {
    let mut harness = app();
    open_fixture(&mut harness, "text-state");
    the(&harness, Region::Center, Role::Button, "View options").click();
    harness.run_steps(3);
    let row = the(&harness, Region::Center, Role::MenuItem, "Paint order").rect();
    assert_eq!(cursor_over(&mut harness, row.center()), CursorIcon::PointingHand);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);

    // Right click on an object of the navigator.
    common::click_rail(&mut harness, "Objects");
    the(&harness, Region::Navigator, Role::Button, "3 0 obj").click_secondary();
    harness.run_steps(3);
    let items: Vec<egui::Rect> = harness
        .query_all_by_role(Role::MenuItem)
        .map(|node| node.rect())
        .collect();
    assert!(items.len() >= 2, "{items:?}");
    // Every row is as wide as the menu and as tall as a row, and the rows touch without gaps.
    for item in &items {
        assert_eq!(item.width(), items[0].width());
        assert_eq!(item.height(), pdfxray_app::theme::tokens::ROW_H);
    }
    for pair in items.windows(2) {
        assert_eq!(pair[0].max.y, pair[1].min.y, "{pair:?}");
    }
    // The empty right part of a row is part of it.
    let last_x = items[0].max.x - 4.0;
    assert_eq!(
        cursor_over(&mut harness, pos2(last_x, items[0].center().y)),
        CursorIcon::PointingHand
    );
}

#[test]
fn the_profile_picker_drops_down_exactly_as_wide_as_the_control() {
    let mut harness = opened();
    common::click_rail(&mut harness, "Validation");
    let picker = harness.get_by_label("Profile").rect();

    harness.get_by_label("Profile").click();
    harness.run_steps(4);

    // The frame of the popover is 8 points of margin and a hairline around its rows.
    let row = harness.get_by_role_and_label(Role::Button, "PDF/UA-2").rect();
    let frame_right = row.max.x + 8.0 + 1.0;
    assert!(
        (frame_right - picker.max.x).abs() <= 1.0,
        "popover frame ends at {frame_right}, the picker at {}",
        picker.max.x
    );
    assert_eq!(row.height(), pdfxray_app::theme::tokens::ROW_H);
    // Smoke: the choices are listed under the picker, not over it.
    assert!(row.min.y >= picker.max.y);
}

#[test]
fn a_revision_row_keeps_its_two_lines_inside_the_row() {
    let mut harness = app();
    open_fixture(&mut harness, "incremental-update");
    common::click_title_chip(&harness);
    harness.run_steps(4);

    let row = harness.get_by_role_and_label(Role::Button, "rev 2").rect();
    let name = harness
        .query_all_by_role(Role::Label)
        .find(|node| node.value().as_deref() == Some("rev 2"))
        .expect("the name of the revision")
        .rect();
    let detail = harness
        .query_all_by_role(Role::Label)
        .find(|node| node.value().is_some_and(|v| v.starts_with("xref table @")))
        .expect("the detail of the revision")
        .rect();
    // The name sits in the top half, the detail under it, and both have air to the row's edges.
    assert!(name.min.y >= row.min.y + 4.0, "{name:?} in {row:?}");
    assert!(detail.min.y >= name.max.y, "{detail:?} under {name:?}");
    assert!(detail.max.y <= row.max.y - 3.0, "{detail:?} in {row:?}");
    // Smoke: it is the row of the newest revision, and its text stays inside it sideways too.
    assert!(detail.max.x <= row.max.x && name.min.x >= row.min.x);
}

#[test]
fn a_switch_can_be_hit_anywhere_in_a_row_as_tall_as_a_button_and_shows_a_hand() {
    let mut on = false;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(300.0, 80.0))
        .build_ui(move |ui| {
            pdfxray_app::components::switch(ui, &mut on, "Explain");
        });
    harness.run();

    let switch = harness.get_by_role_and_label(Role::Switch, "Explain").rect();

    // The track is 14 points; the target is not.
    assert!(switch.height() >= 20.0, "{switch:?}");
    assert_eq!(
        cursor_over_plain(&mut harness, switch.center()),
        CursorIcon::PointingHand
    );
    // Smoke: it still holds the track and the label side by side.
    assert!(switch.width() > 26.0 + 6.0 + 20.0, "{switch:?}");
}

fn cursor_over_plain<S>(harness: &mut Harness<'_, S>, at: egui::Pos2) -> CursorIcon {
    harness.hover_at(at);
    harness.run_steps(2);
    harness.output().platform_output.cursor_icon
}

// ---- round 2: live findings ----

/// The accessible names of the toolbar's controls, left to right.
const TOOLBAR: [&str; 9] = [
    "Prev",
    "Next",
    "Zoom in",
    "Zoom out",
    "Fit page",
    "Page boxes",
    "Measure",
    "View options",
    "Pick: All",
];

/// The rectangles of the toolbar's controls, the View toggle (named after its mode) included.
fn toolbar_rects(harness: &App) -> Vec<(String, egui::Rect)> {
    let mut rects: Vec<(String, egui::Rect)> = TOOLBAR
        .iter()
        .map(|name| {
            let node = harness.get_by_role_and_label(Role::Button, name);
            ((*name).to_owned(), node.rect())
        })
        .collect();
    let view = harness
        .query_all_by_role(Role::Button)
        .find(|node| node.accesskit_node().label().as_deref() == Some("View"))
        .expect("the View toggle");
    rects.push(("View".to_owned(), view.rect()));
    rects.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));
    rects
}

#[test]
fn the_toolbar_drops_names_before_two_controls_overlap_at_the_narrowest_window() {
    let mut wide = opened();
    let roomy = toolbar_rects(&wide);
    let pick_wide = roomy.iter().find(|(name, _)| name == "Pick: All").unwrap().1;
    wide.run_steps(1);

    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    harness.set_size(egui::vec2(1100.0, 700.0));
    harness.remove_cursor();
    harness.run_steps(4);
    let tight = toolbar_rects(&harness);

    // No two controls overlap, and all of them sit inside the centre column.
    for pair in tight.windows(2) {
        assert!(
            pair[0].1.max.x <= pair[1].1.min.x,
            "{} {:?} overlaps {} {:?}",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1
        );
    }
    let left = pdfxray_app::theme::tokens::RAIL_W + pdfxray_app::theme::tokens::NAVIGATOR_W;
    let right = 1100.0 - pdfxray_app::theme::tokens::INSPECTOR_W;
    assert!(tight[0].1.min.x >= left, "{tight:?}");
    assert!(tight[tight.len() - 1].1.max.x <= right, "{tight:?} past {right}");
    // The Pick menu is its icon alone here, and the whole name where there is room. It still
    // opens: the click lands on the control, not on its neighbour.
    let pick = tight.iter().find(|(name, _)| name == "Pick: All").unwrap().1;
    assert!(
        pick.width() < pick_wide.width() - 40.0,
        "{pick:?} vs {pick_wide:?}"
    );
    assert!(pick_wide.width() > 80.0, "{pick_wide:?}");
    harness.get_by_role_and_label(Role::Button, "Pick: All").click();
    harness.run_steps(3);
    assert!(harness.query_by_label("Boxes").is_some());
    // Smoke: pressing the control did not turn the Measure tool on.
    assert!(!harness.state().state.canvas.measure.active);
}

#[test]
fn rail_tooltips_open_beside_the_button_and_do_not_hide_the_next_one() {
    let mut harness = opened();
    let content = the(&harness, Region::Rail, Role::Button, "Content").rect();
    let artifacts = the(&harness, Region::Rail, Role::Button, "Artifacts").rect();

    rest_on(&mut harness, content.center());

    let tip = harness
        .query_all_by_role(Role::Label)
        .find(|node| node.value().as_deref() == Some("Content"))
        .expect("the tooltip of the Content entry")
        .rect();
    assert!(tip.min.x >= content.max.x, "{tip:?} beside {content:?}");
    assert!(!tip.intersects(artifacts), "{tip:?} covers {artifacts:?}");
    // Level with the button it names.
    assert!(
        (tip.center().y - content.center().y).abs() < 6.0,
        "{tip:?} vs {content:?}"
    );
    // Smoke: the next entry is still there to be hit.
    assert_eq!(
        cursor_over(&mut harness, artifacts.center()),
        CursorIcon::PointingHand
    );
}

#[test]
fn the_context_menu_rows_with_a_key_are_as_tall_as_the_rest() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    the(&harness, Region::Navigator, Role::Button, "Document").click_secondary();
    harness.run_steps(3);

    let items: Vec<(String, egui::Rect)> = harness
        .query_all_by_role(Role::MenuItem)
        .map(|node| (node.accesskit_node().label().unwrap_or_default(), node.rect()))
        .collect();

    let names: Vec<&str> = items.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"Isolate") && names.contains(&"Hide"), "{names:?}");
    for (name, rect) in &items {
        assert_eq!(rect.height(), pdfxray_app::theme::tokens::ROW_H, "{name}");
    }
    // Every row starts where the one above it ends: the keycap rows do not pull the next up.
    for pair in items.windows(2) {
        assert_eq!(
            pair[0].1.max.y, pair[1].1.min.y,
            "{} then {}",
            pair[0].0, pair[1].0
        );
    }
}

#[test]
fn the_checkbox_options_of_the_view_menu_are_rows_that_can_be_hit_anywhere() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    the(&harness, Region::Center, Role::Button, "View options").click();
    harness.run_steps(3);
    let nesting = harness.get_by_role_and_label(Role::CheckBox, "Nesting").rect();
    let menu = the(&harness, Region::Center, Role::MenuItem, "Paint order").rect();
    let before = harness.state().state.canvas.view.show.nesting;
    // The far right of the menu, well past the label.
    let far = pos2(menu.max.x - 6.0, nesting.center().y);

    assert_eq!(cursor_over(&mut harness, far), CursorIcon::PointingHand);
    common::click_at(&mut harness, far);
    harness.run_steps(3);

    assert_eq!(harness.state().state.canvas.view.show.nesting, !before);
    // Smoke: another option did not move, and the menu is still open.
    assert!(
        harness
            .query_by_role_and_label(Role::MenuItem, "Paint order")
            .is_some()
    );
}

#[test]
fn the_zoom_tooltips_name_their_key_like_the_other_tools() {
    let mut harness = opened();
    let zoom = toolbar::zoom_text(harness.state().state.canvas.zoom);
    let readout = the(&harness, Region::Center, Role::Button, &zoom).rect();

    rest_on(&mut harness, readout.center());

    let labels = all_label_texts(&harness);
    for text in ["Actual size", "Reset the zoom to 100%", "⌘0"] {
        assert!(labels.iter().any(|t| t == text), "{text} in {labels:?}");
    }
    let zoom_in = the(&harness, Region::Center, Role::Button, "Zoom in").rect();
    rest_on(&mut harness, zoom_in.center());
    assert!(all_label_texts(&harness).iter().any(|t| t == "⌘="));
}

/// Drags from `from` by `dx` points sideways, the way a hand does it.
fn drag_sideways(harness: &mut App, from: egui::Pos2, dx: f32) {
    harness.hover_at(from);
    harness.drag_at(from);
    harness.hover_at(from + egui::vec2(dx / 2.0, 0.0));
    harness.hover_at(from + egui::vec2(dx, 0.0));
    harness.drop_at(from + egui::vec2(dx, 0.0));
    harness.run_steps(3);
}

/// The navigator's filter field: its right edge follows the navigator's.
fn navigator_filter(harness: &App) -> egui::Rect {
    harness
        .get_all_by_role(Role::TextInput)
        .map(|node| node.rect())
        .find(|rect| rect.min.x < pdfxray_app::theme::tokens::RAIL_W + 60.0)
        .expect("the navigator filter")
}

/// The inspector's title: its left edge follows the inspector's.
fn inspector_title(harness: &App) -> egui::Rect {
    harness
        .get_all_by_label("minimal-one-page.pdf")
        .map(|node| node.rect())
        .find(|rect| rect.min.y > pdfxray_app::theme::tokens::TITLE_H && rect.min.x > 700.0)
        .expect("the inspector title")
}

#[test]
fn both_side_panels_can_be_dragged_wider_or_narrower_within_their_limits() {
    use pdfxray_app::theme::tokens::{
        CANVAS_MIN_W, INSPECTOR_MIN_W, INSPECTOR_W, NAVIGATOR_MIN_W, NAVIGATOR_W, RAIL_W,
    };
    let mut harness = opened();
    let navigator_edge = RAIL_W + NAVIGATOR_W;
    let inspector_edge = common::WINDOW.x - INSPECTOR_W;
    let resizing = [
        CursorIcon::ResizeHorizontal,
        CursorIcon::ResizeEast,
        CursorIcon::ResizeWest,
    ];
    // Both edges offer the two-way arrow.
    for edge in [navigator_edge, inspector_edge] {
        let cursor = cursor_over(&mut harness, pos2(edge, 400.0));
        assert!(resizing.contains(&cursor), "{cursor:?} at edge {edge}");
    }
    let filter = navigator_filter(&harness);
    let title = inspector_title(&harness);

    // The navigator 80 wider, the inspector 60 wider: their contents follow the edges.
    drag_sideways(&mut harness, pos2(navigator_edge, 400.0), 80.0);
    drag_sideways(&mut harness, pos2(inspector_edge, 400.0), -60.0);
    let wider = navigator_filter(&harness);
    assert!(
        (wider.max.x - filter.max.x - 80.0).abs() < 2.0,
        "{filter:?} -> {wider:?}"
    );
    let moved = inspector_title(&harness);
    assert!(
        (title.min.x - moved.min.x - 60.0).abs() < 2.0,
        "{title:?} -> {moved:?}"
    );

    // Far to the left the navigator stops at its minimum width.
    drag_sideways(&mut harness, pos2(navigator_edge + 80.0, 400.0), -400.0);
    let narrowest = navigator_filter(&harness).max.x;
    assert!(narrowest > RAIL_W + NAVIGATOR_MIN_W - 40.0, "{narrowest}");
    assert!(narrowest <= RAIL_W + NAVIGATOR_MIN_W, "{narrowest}");
    // Far to the right the inspector stops at its minimum too.
    drag_sideways(&mut harness, pos2(inspector_edge - 60.0, 400.0), 400.0);
    let inspector_left = inspector_title(&harness).min.x;
    assert!(
        inspector_left >= common::WINDOW.x - INSPECTOR_MIN_W,
        "{inspector_left}"
    );

    // Smoke: dragged as wide as it goes, the navigator leaves the canvas its minimum width.
    drag_sideways(&mut harness, pos2(RAIL_W + NAVIGATOR_MIN_W, 400.0), 1200.0);
    let widest = navigator_filter(&harness).max.x;
    assert!(
        widest < common::WINDOW.x - INSPECTOR_MIN_W - CANVAS_MIN_W,
        "{widest}"
    );
    assert!(widest > RAIL_W + NAVIGATOR_W + 100.0, "{widest}");
}

#[test]
fn the_settings_popover_fits_its_sentence_on_one_line() {
    let mut harness = opened();
    the(&harness, Region::Rail, Role::Button, "Settings").click();
    harness.run_steps(4);

    let sentence = harness
        .get_by_label("Reads PDFs only: it never changes your file.")
        .rect();

    // One line of text, not a line and a lone word.
    assert!(sentence.height() < 20.0, "{sentence:?}");
    assert!(harness.query_by_label("pdfxray").is_some());
}

#[test]
fn the_toolbar_does_not_overlap_when_a_view_mode_names_its_button_at_the_narrowest_window() {
    let mut harness = app();
    open_fixture(&mut harness, "rolemap-two-hops");
    harness.state_mut().state.canvas.view.xray = true;
    harness.set_size(egui::vec2(1100.0, 700.0));
    harness.remove_cursor();
    harness.run_steps(4);

    // The View toggle now wears the mode's name.
    let prev = harness.get_by_role_and_label(Role::Button, "Prev").rect();
    let mut row: Vec<(String, egui::Rect)> = common::nodes_in(&harness, Region::Center, Role::Button)
        .into_iter()
        .filter(|node| (node.rect().center().y - prev.center().y).abs() < 2.0)
        .map(|node| (node.accesskit_node().label().unwrap_or_default(), node.rect()))
        .collect();
    row.sort_by(|a, b| a.1.min.x.total_cmp(&b.1.min.x));

    assert!(row.iter().any(|(name, _)| name.starts_with("X-ray")), "{row:?}");
    for pair in row.windows(2) {
        assert!(
            pair[0].1.max.x <= pair[1].1.min.x,
            "{} {:?} overlaps {} {:?}",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1
        );
    }
    // Short of room, Isolate and Hide leave the toolbar (the action bar and the keys keep them)
    // before two controls overlap.
    assert!(
        !row.iter().any(|(name, _)| name == "Isolate" || name == "Hide"),
        "{row:?}"
    );
    // Smoke: every control of the row is inside the centre column.
    let right = 1100.0 - pdfxray_app::theme::tokens::INSPECTOR_W;
    assert!(
        row.iter().all(|(_, rect)| rect.max.x <= right),
        "{row:?} past {right}"
    );
}
