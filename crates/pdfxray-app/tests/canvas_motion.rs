//! Motion on the canvas: zoom and jump-to-page ease over 220 ms and still arrive exactly where
//! the instant version goes. This file is its own test binary because the reduce-motion flag is
//! process-wide.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

mod common;

use std::sync::{Mutex, MutexGuard, PoisonError};

use common::{builder, open_fixture, wait_for_render};
use egui::{Rect, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_app::theme::motion;
use pretty_assertions::assert_eq;

static FLAG: Mutex<()> = Mutex::new(());

/// Held by every test: they flip the process-wide flag, so they run one at a time.
fn lock() -> MutexGuard<'static, ()> {
    FLAG.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An app with `fixture` open at `zoom`, 40 ms per step, at rest. Animations are off while it
/// opens and `animated` says whether they are on afterwards.
fn opened(fixture: &str, zoom: f32, animated: bool) -> Harness<'static, PdfxrayApp> {
    let mut harness = builder()
        .with_step_dt(0.04)
        .build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, fixture);
    motion::set_reduce_motion(!animated);
    harness.state_mut().state.set_zoom(zoom);
    // Long enough for a zoom that is easing to arrive.
    harness.run_steps(15);
    wait_for_render(&mut harness);
    harness
}

fn page_rect(harness: &Harness<'_, PdfxrayApp>, page: u32) -> Rect {
    harness
        .get_by_role_and_label(Role::Image, &format!("Page {} canvas", page + 1))
        .rect()
}

fn offset_y(harness: &Harness<'_, PdfxrayApp>) -> f32 {
    harness.state().state.canvas.scroll_offset.y
}

#[test]
fn zoom_eases_to_its_target_and_the_state_holds_the_target_at_once() {
    let _guard = lock();
    let mut harness = opened("page-boxes", 0.4, true);
    let start = page_rect(&harness, 0).width();

    harness.state_mut().state.set_zoom(0.8);
    harness.step();

    // The state has the target straight away; the page on screen is on its way.
    assert_eq!(harness.state().state.canvas.zoom, 0.8);
    harness.step();
    let midway = page_rect(&harness, 0).width();
    assert!(
        midway > start + 1.0 && midway < start * 2.0 - 1.0,
        "{start} -> {midway}"
    );

    // 220 ms later it has arrived.
    harness.run_steps(10);
    wait_for_render(&mut harness);
    let end = page_rect(&harness, 0).width();
    assert!((end - start * 2.0).abs() < 0.5, "{start} -> {end}");
    assert_eq!(harness.state().state.canvas.zoom, 0.8);
    motion::set_reduce_motion(true);
}

#[test]
fn jumping_to_a_page_eases_and_arrives_where_the_instant_jump_does() {
    let _guard = lock();
    // The instant jump: where page 3 ends up with animations off.
    let mut instant = opened("long-doc", 1.0, false);
    instant.state_mut().state.go_to_page(2);
    instant.run_steps(6);
    let arrival = offset_y(&instant);
    assert!(arrival > 100.0, "{arrival}");

    let mut harness = opened("long-doc", 1.0, true);
    assert_eq!(offset_y(&harness), 0.0);
    harness.state_mut().state.go_to_page(2);
    harness.run_steps(2);

    // On its way: moved, but not there yet.
    let midway = offset_y(&harness);
    assert!(midway > 0.0 && midway < arrival - 1.0, "{midway} of {arrival}");
    assert_eq!(harness.state().state.canvas.page, 2);

    harness.run_steps(12);
    assert!(
        (offset_y(&harness) - arrival).abs() < 1.0,
        "{} vs {arrival}",
        offset_y(&harness)
    );
    // The current page is the one jumped to, not one passed on the way.
    assert_eq!(harness.state().state.canvas.page, 2);
    motion::set_reduce_motion(true);
}

#[test]
fn the_reading_order_path_draws_badge_by_badge_over_300_ms_and_is_instant_without_motion() {
    use egui::{Key, Modifiers};

    let badges = |harness: &Harness<'_, PdfxrayApp>| {
        harness
            .query_all_by_role(Role::Button)
            .filter_map(|node| egui_kittest::kittest::NodeT::accesskit_node(&node).label())
            .filter(|name| name.starts_with("Reading order step"))
            .count()
    };
    let _guard = lock();
    let mut harness = opened("reading-order", 1.0, true);

    harness.key_press_modifiers(Modifiers::NONE, Key::R);
    harness.step();
    harness.step();

    // Badge 1 is there at once; the others come as the path reaches them.
    let started = badges(&harness);
    assert!((1..5).contains(&started), "{started} badges at the start");

    // 300 ms later (40 ms steps) the path is all there.
    harness.run_steps(12);
    assert_eq!(badges(&harness), 5);

    // Under reduce motion it is there on the first frame.
    harness.key_press_modifiers(Modifiers::NONE, Key::R);
    harness.run_steps(3);
    assert_eq!(badges(&harness), 0);
    motion::set_reduce_motion(true);
    harness.key_press_modifiers(Modifiers::NONE, Key::R);
    harness.step();
    harness.step();
    assert_eq!(badges(&harness), 5);
}
