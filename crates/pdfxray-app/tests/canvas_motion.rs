//! Motion on the canvas: zoom and jump-to-page ease over 220 ms and still arrive exactly where
//! the instant version goes, an eased zoom keeps the centre of the view in every frame, the
//! wheel zooms without easing; and the panels' lists, which do not bounce past their ends. This
//! file is its own test binary because the reduce-motion flag is process-wide.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

mod common;

use std::sync::{Mutex, MutexGuard, PoisonError};

use common::{builder, canvas_view, click_rail, open_fixture, wait_for_render};
use egui::style::{KineticScrollStyle, ScrollAnimation};
use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, Pos2, Rangef, Rect, Role, TouchPhase, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_app::theme::{motion, tokens};
use pdfxray_core::{NodeId, Point};
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
    // Near the top of page 1 (the zoom from fitting kept the centre of the view).
    let start = offset_y(&harness);
    assert!(start < arrival / 2.0, "{start} of {arrival}");
    harness.state_mut().state.go_to_page(2);
    harness.run_steps(2);

    // On its way: moved, but not there yet.
    let midway = offset_y(&harness);
    assert!(
        midway > start + 1.0 && midway < arrival - 1.0,
        "{midway} from {start} to {arrival}"
    );
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

fn wheel(
    harness: &mut Harness<'_, PdfxrayApp>,
    phase: TouchPhase,
    source: MouseWheelSource,
    dy: f32,
    modifiers: Modifiers,
) {
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase,
        source,
        modifiers,
    });
}

/// The point of the page under the pointer, as the canvas read it last frame.
fn under_pointer(harness: &Harness<'_, PdfxrayApp>) -> Point {
    harness
        .state()
        .state
        .canvas
        .cursor
        .expect("the pointer is on a page")
        .page
}

/// The two points are the same spot of the page, within a point.
fn assert_same_spot(actual: Point, expected: Point, when: &str) {
    assert!(
        (actual.x - expected.x).abs() < 1.0 && (actual.y - expected.y).abs() < 1.0,
        "{when}: {actual:?} is not {expected:?}"
    );
}

/// `long-doc` at 100 %, animated, scrolled down a little, with the pointer `at` the given point
/// of the canvas (from its centre) and at rest.
fn pointing(from_centre: egui::Vec2) -> (Harness<'static, PdfxrayApp>, Pos2) {
    let mut harness = opened("long-doc", 1.0, true);
    let at = canvas_view(&harness).center() + from_centre;
    harness.hover_at(at);
    wheel(
        &mut harness,
        TouchPhase::Move,
        MouseWheelSource::Wheel,
        -300.0,
        Modifiers::NONE,
    );
    harness.run_steps(15);
    wait_for_render(&mut harness);
    harness.hover_at(at);
    harness.step();
    (harness, at)
}

#[test]
fn an_eased_zoom_keeps_the_centre_of_the_view_in_every_frame() {
    let _guard = lock();
    let (mut harness, _) = pointing(egui::Vec2::ZERO);
    let spot = under_pointer(&harness);
    let start = page_rect(&harness, 0).width();

    // ⌘= (the button and the menu do the same).
    harness.state_mut().state.zoom_in();
    let mut widths = Vec::new();
    for frame in 0..10 {
        harness.step();
        widths.push(page_rect(&harness, 0).width());
        assert_same_spot(under_pointer(&harness), spot, &format!("frame {frame}"));
    }

    // It eased: some frames were on their way from 100 % to 125 %.
    assert!(
        widths.iter().any(|w| *w > start + 1.0 && *w < start * 1.25 - 1.0),
        "{start} -> {widths:?}"
    );
    assert!((widths[9] - start * 1.25).abs() < 0.5, "{start} -> {widths:?}");
    motion::set_reduce_motion(true);
}

#[test]
fn the_wheel_zooms_with_the_hand_without_easing_and_keeps_the_spot_under_the_pointer() {
    let _guard = lock();
    let (mut harness, _) = pointing(vec2(150.0, -120.0));
    let spot = under_pointer(&harness);
    // Page width on screen per unit of zoom.
    let per_zoom = page_rect(&harness, 0).width() / harness.state().state.canvas.zoom;

    wheel(
        &mut harness,
        TouchPhase::Move,
        MouseWheelSource::Wheel,
        120.0,
        Modifiers::COMMAND,
    );
    for frame in 0..10 {
        harness.step();
        // Each frame shows the zoom the wheel has reached, not an easing towards it.
        let zoom = harness.state().state.canvas.zoom;
        let width = page_rect(&harness, 0).width();
        assert!(
            (width - per_zoom * zoom).abs() < 0.5,
            "frame {frame}: {width} at {zoom}"
        );
        assert_same_spot(under_pointer(&harness), spot, &format!("frame {frame}"));
    }

    // e^(120 / 200): the zoom is exponential in the wheel's travel.
    let zoom = harness.state().state.canvas.zoom;
    assert!((zoom - 0.6_f32.exp()).abs() < 1e-3, "{zoom}");
    motion::set_reduce_motion(true);
}

/// A side panel's list, left to right on screen.
#[derive(Debug, Clone, Copy)]
struct Panel {
    x: Rangef,
}

impl Panel {
    const NAVIGATOR: Self = Self {
        x: Rangef {
            min: tokens::RAIL_W,
            max: tokens::RAIL_W + tokens::NAVIGATOR_W,
        },
    };
    const INSPECTOR: Self = Self {
        x: Rangef {
            min: common::WINDOW.x - tokens::INSPECTOR_W,
            max: common::WINDOW.x,
        },
    };

    /// A point over the list.
    fn inside(self) -> Pos2 {
        Pos2::new(self.x.center(), 500.0)
    }

    /// Where the panel's rows stop on screen: the bottom of the lowest row that starts above the
    /// status bar (a row the panel's edge cuts counts). The scroll bar and the resize handle run
    /// the panel's whole height, so they are not rows.
    fn lowest(self, harness: &Harness<'_, PdfxrayApp>) -> f32 {
        let status_bar = common::WINDOW.y - tokens::STATUS_H;
        harness
            .query_all_by(|node| {
                node.bounding_box().is_some() && !matches!(node.role(), Role::ScrollBar | Role::Splitter)
            })
            .map(|node| node.rect())
            .filter(|rect| self.x.contains(rect.center().x) && rect.top() < status_bar)
            .map(|rect| rect.bottom())
            .fold(f32::NEG_INFINITY, f32::max)
    }
}

/// `long-doc` with the scroll physics the app has when motion is on, which are egui's own: a
/// list can be pulled past its end and thrown back. 60 frames a second, as on screen.
fn physical() -> Harness<'static, PdfxrayApp> {
    let mut harness = builder()
        .with_step_dt(1.0 / 60.0)
        .build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "long-doc");
    motion::set_reduce_motion(false);
    harness.ctx.all_styles_mut(|style| {
        style.spacing.scroll.kinetic = KineticScrollStyle::default();
        style.scroll_animation = ScrollAnimation::default();
    });
    harness
}

/// Scrolls `panel`'s list to its end with a mouse wheel, which stops at the end, and back to its
/// start; returns where the list ends on screen once it is at its end.
fn end_of_list(harness: &mut Harness<'_, PdfxrayApp>, panel: Panel) -> f32 {
    harness.hover_at(panel.inside());
    wheel(
        harness,
        TouchPhase::Move,
        MouseWheelSource::Wheel,
        -100_000.0,
        Modifiers::NONE,
    );
    harness.run_steps(60);
    let end = panel.lowest(harness);
    wheel(
        harness,
        TouchPhase::Move,
        MouseWheelSource::Wheel,
        100_000.0,
        Modifiers::NONE,
    );
    harness.run_steps(60);
    end
}

/// A fast two-finger flick down `panel`'s list on the trackpad: the fingers, then the momentum
/// the system keeps sending after they lift. Returns the largest empty band seen under the
/// rows in any frame, and where the list ends once at rest.
fn fling(harness: &mut Harness<'_, PdfxrayApp>, panel: Panel, end: f32) -> (f32, f32) {
    harness.hover_at(panel.inside());
    let mut events = vec![(TouchPhase::Start, MouseWheelSource::Trackpad, 0.0)];
    events.extend([(TouchPhase::Move, MouseWheelSource::Trackpad, -600.0); 14]);
    events.push((TouchPhase::End, MouseWheelSource::Trackpad, 0.0));
    let mut momentum = -600.0;
    for _ in 0..20 {
        events.push((TouchPhase::Move, MouseWheelSource::Momentum, momentum));
        momentum *= 0.85;
    }
    let mut band = 0.0_f32;
    for (phase, source, dy) in events {
        wheel(harness, phase, source, dy, Modifiers::NONE);
        harness.step();
        band = band.max(end - panel.lowest(harness));
    }
    for _ in 0..60 {
        harness.step();
        band = band.max(end - panel.lowest(harness));
    }
    (band, panel.lowest(harness))
}

#[test]
fn a_fast_flick_to_the_end_of_a_panel_list_shows_no_empty_band_under_its_last_row() {
    let _guard = lock();
    let mut harness = physical();
    let mut lists = Vec::new();

    // The Objects list (rows of its own), the structure tree (the shared tree view) and the
    // inspector on a page (boxes, resources and content run past the panel).
    click_rail(&mut harness, "Objects");
    let end = end_of_list(&mut harness, Panel::NAVIGATOR);
    lists.push(("Objects", end, fling(&mut harness, Panel::NAVIGATOR, end)));
    click_rail(&mut harness, "Tags");
    harness.get_by_label("Expand all").click();
    harness.run_steps(3);
    let end = end_of_list(&mut harness, Panel::NAVIGATOR);
    lists.push(("Tags", end, fling(&mut harness, Panel::NAVIGATOR, end)));
    harness.state_mut().state.select(NodeId::Page(0));
    harness.run_steps(3);
    let end = end_of_list(&mut harness, Panel::INSPECTOR);
    lists.push(("Inspector", end, fling(&mut harness, Panel::INSPECTOR, end)));
    motion::set_reduce_motion(true);

    for (list, end, (band, rest)) in lists {
        // While the list scrolls, the lowest row that starts above the status bar ends at most a
        // row's gap short of the panel's end. A list thrown past its end leaves far more.
        assert!(
            band < tokens::ROW_H,
            "{list}: an empty band of {band} pt under the rows"
        );
        // Smoke: the flick reached the end of the list and the list stays there.
        assert!(
            (rest - end).abs() < 0.5,
            "{list}: at rest at {rest}, its end is {end}"
        );
    }
}
