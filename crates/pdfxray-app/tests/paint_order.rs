//! The paint-order stepper, end to end: the toolbar and the P key, the drawer tab with its
//! buttons, track and cards, the keys, the canvas texture and the inspector's Paint order
//! section, and how the stepper and the scroll of pages leave each other alone or follow each
//! other. The fixtures are `graphics-state` (80 operators, known overlaps) and `text-state`,
//! and `long-doc` (40 pages) for the scroll.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

mod common;
#[path = "common/view_tools.rs"]
mod view_tools;

use common::{
    Region, app, builder, buttons_in, canvas_view, click_at, click_rail, digest, expected_digest,
    fixture_path, has_label, is_selected, open_fixture, open_path, texts_in, the, wait_for_render,
};
use egui::{Event, Key, Modifiers, MouseWheelUnit, Rect, Role, Vec2, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::center::canvas::cards::mini_text;
use pdfxray_app::center::canvas::paint_order::{LEGEND_ENTRIES, LEGEND_TITLE, painting_now};
use pdfxray_app::components::step_track::page_text;
use pdfxray_app::components::{PageMark, StepTrack};
use pdfxray_app::drawer::paint_order::{
    FOLLOW_LABEL, PAGE_END_LABEL, PAGE_START_LABEL, SKIP_LABEL, UNPAINTED_LABEL, at_text, card_label,
    page_edge_hint, position_text,
};
use pdfxray_app::inspector::paint_order as section;
use pdfxray_app::view::ZoomMode;
use pdfxray_app::{DrawerTab, PdfxrayApp, RailItem};
use pdfxray_core::{Document, NodeId, OpPosition};
use pdfxray_fixtures::paint_pages::{
    PAINT_PAGES_BLUE_FILL, PAINT_PAGES_FIRST_OPS, PAINT_PAGES_RED_FILL, two_overlapping_pages,
};
use pdfxray_render::ContentFilter;
use pretty_assertions::assert_eq;

const FIXTURES: [&str; 2] = ["graphics-state", "text-state"];

const TRACK: &str = "Paint order position";

fn settle(harness: &mut Harness<'_, PdfxrayApp>) {
    harness.run_steps(4);
}

/// A fixed zoom that leaves room beside the page for the cards, and does not follow the canvas
/// when the drawer opens.
const ROOMY: f32 = 0.4;

fn opened(fixture: &str) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    harness.state_mut().state.set_zoom(ROOMY);
    settle(&mut harness);
    wait_for_render(&mut harness);
    harness
}

fn doc(fixture: &str) -> Document {
    Document::open(fixture_path(fixture)).unwrap()
}

/// Selects operator `ix` and turns the stepper on, which starts at the selected operator.
fn start_at(harness: &mut Harness<'_, PdfxrayApp>, ix: u32) {
    harness.state_mut().state.select(NodeId::Op { page: 0, ix });
    settle(harness);
    harness.key_press(Key::P);
    settle(harness);
    wait_for_render(harness);
}

/// The operator the stepper is at, on page 1: every test here steps on the first page.
fn step(harness: &Harness<'_, PdfxrayApp>) -> u32 {
    let at = harness.state().state.canvas.view.step;
    assert_eq!(at.page, 0, "the stepper left page 1");
    at.ix
}

fn selected_op(harness: &Harness<'_, PdfxrayApp>) -> Option<NodeId> {
    harness.state().state.selection.primary().cloned()
}

fn press(harness: &mut Harness<'_, PdfxrayApp>, key: Key) {
    harness.key_press(key);
    settle(harness);
}

/// The value after the label `key` in the inspector.
fn inspector_value(harness: &Harness<'_, PdfxrayApp>, key: &str) -> Option<String> {
    let labels = texts_in(harness, Region::Inspector, Role::Label);
    let at = labels.iter().position(|t| t == key)?;
    labels.get(at + 1).cloned()
}

fn drawer_tab(harness: &Harness<'_, PdfxrayApp>) -> DrawerTab {
    harness.state().state.drawer.tab
}

fn op(ix: u32) -> NodeId {
    NodeId::Op { page: 0, ix }
}

#[test]
fn p_switches_the_drawer_to_paint_order_and_back_keeping_the_navigator() {
    for fixture in FIXTURES {
        let mut harness = opened(fixture);
        click_rail(&mut harness, "Content");
        assert_eq!(drawer_tab(&harness), DrawerTab::Operators, "{fixture}");

        press(&mut harness, Key::P);

        assert_eq!(drawer_tab(&harness), DrawerTab::PaintOrder, "{fixture}");
        let tabs = buttons_in(&harness, Region::Drawer);
        assert!(tabs.contains(&"Paint order".to_owned()), "{tabs:?}");
        assert!(!tabs.contains(&"Operators".to_owned()), "{tabs:?}");
        assert!(is_selected(&the(
            &harness,
            Region::Drawer,
            Role::Button,
            "Paint order"
        )));
        assert!(is_selected(&the(
            &harness,
            Region::Center,
            Role::Button,
            "Paint order"
        )));
        // The navigator is the one the user had.
        assert_eq!(harness.state().state.nav.rail, RailItem::Content);
        // The tab counts the operators of the page.
        assert_eq!(
            harness.state().state.op_total(),
            doc(fixture).captured(0).unwrap().ops.len() as u32
        );

        press(&mut harness, Key::P);

        assert_eq!(drawer_tab(&harness), DrawerTab::Operators, "{fixture}");
        assert!(!buttons_in(&harness, Region::Drawer).contains(&"Paint order".to_owned()));
        // With no mode on, the View button says so and is not lit.
        assert!(!is_selected(&the(&harness, Region::Center, Role::Button, "View")));

        // The View menu does the same.
        view_tools::toggle(&mut harness, "Paint order");
        settle(&mut harness);
        assert_eq!(drawer_tab(&harness), DrawerTab::PaintOrder);
        view_tools::toggle(&mut harness, "Paint order");
        settle(&mut harness);
        assert_eq!(drawer_tab(&harness), DrawerTab::Operators);
    }
}

#[test]
fn the_arrow_keys_step_one_operator_and_the_selection_and_inspector_follow() {
    for fixture in FIXTURES {
        let mut harness = opened(fixture);
        let total = harness.state().state.op_total();
        start_at(&mut harness, 5);
        assert_eq!(step(&harness), 5, "{fixture}");
        assert_eq!(inspector_value(&harness, "Step"), Some(format!("6 of {total}")));

        press(&mut harness, Key::ArrowRight);
        press(&mut harness, Key::ArrowRight);

        assert_eq!(step(&harness), 7, "{fixture}");
        assert_eq!(selected_op(&harness), Some(op(7)));
        assert_eq!(selected_op(&harness).unwrap().to_string(), "op:0:7");
        assert_eq!(inspector_value(&harness, "Step"), Some(format!("8 of {total}")));
        assert!(
            texts_in(&harness, Region::Inspector, Role::Label).contains(&"op:0:7 · op #7".to_owned()),
            "{fixture}"
        );

        press(&mut harness, Key::ArrowLeft);

        assert_eq!(step(&harness), 6, "{fixture}");
        assert_eq!(selected_op(&harness), Some(op(6)));
        assert_eq!(inspector_value(&harness, "Step"), Some(format!("7 of {total}")));
        // The drawer says the same.
        assert!(has_label(&harness, &position_text(7, total)), "{fixture}");
    }
}

#[test]
fn the_arrows_stop_at_the_first_and_last_operator() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 79);

    press(&mut harness, Key::ArrowRight);
    assert_eq!(step(&harness), 79);
    harness.state_mut().state.step_to(OpPosition::new(0, 0));
    settle(&mut harness);
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(step(&harness), 0);
    assert_eq!(selected_op(&harness), Some(op(0)));
}

#[test]
fn the_arrows_belong_to_the_navigator_tree_while_the_pointer_is_over_it() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 10);

    harness.hover_at(pos2(200.0, 400.0));
    press(&mut harness, Key::ArrowRight);
    assert_eq!(step(&harness), 10);

    harness.hover_at(pos2(800.0, 300.0));
    press(&mut harness, Key::ArrowRight);
    assert_eq!(step(&harness), 11);
}

#[test]
fn shift_and_an_arrow_jump_to_the_next_visible_change() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 49);
    let core = doc("graphics-state");
    assert_eq!(core.next_visible_change(0, 49, true), Some(53));

    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    settle(&mut harness);

    assert_eq!(step(&harness), 53);
    assert_eq!(selected_op(&harness), Some(op(53)));

    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
    settle(&mut harness);
    assert_eq!(step(&harness), 49);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
    settle(&mut harness);
    assert_eq!(
        step(&harness),
        43,
        "back over the state operators between two paints"
    );
}

#[test]
fn a_jump_is_exactly_what_core_calls_the_next_visible_change_on_both_fixtures() {
    for fixture in FIXTURES {
        let core = doc(fixture);
        let last = core.captured(0).unwrap().ops.len() as u32 - 1;
        // A start whose next operator paints nothing, so a jump differs from a step.
        let start = (0..last)
            .find(|ix| core.is_state_only(0, ix + 1) && core.next_visible_change(0, *ix, true).is_some())
            .expect("a state-only operator followed by a painting one");
        let mut harness = opened(fixture);
        start_at(&mut harness, start);

        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        settle(&mut harness);

        assert_eq!(
            Some(step(&harness)),
            core.next_visible_change(0, start, true),
            "{fixture}"
        );
        assert!(
            step(&harness) > start + 1,
            "{fixture}: it jumped over a state operator"
        );
        assert_eq!(selected_op(&harness), Some(op(step(&harness))));
    }
}

#[test]
fn skipping_state_only_ops_makes_the_arrows_land_only_on_painting_operators() {
    for fixture in FIXTURES {
        let core = doc(fixture);
        let last = core.captured(0).unwrap().ops.len() as u32 - 1;
        let painting: Vec<u32> = (1..=last).filter(|ix| !core.is_state_only(0, *ix)).collect();
        let mut harness = opened(fixture);
        start_at(&mut harness, 0);
        let switch = the(&harness, Region::Drawer, Role::Switch, "Skip state-only ops");
        assert!(!is_selected(&switch), "off by default");
        switch.click();
        settle(&mut harness);
        assert!(harness.state().state.canvas.view.skip_state_only);

        let mut landed = Vec::new();
        for _ in 0..=last {
            harness.key_press(Key::ArrowRight);
            harness.run_steps(2);
            if landed.last() == Some(&step(&harness)) {
                break;
            }
            landed.push(step(&harness));
        }

        assert_eq!(landed, painting, "{fixture}");
        // And back again, ending on the first operator that paints.
        for _ in 0..=last {
            harness.key_press(Key::ArrowLeft);
            harness.run_steps(2);
        }
        assert_eq!(Some(step(&harness)), painting.first().copied(), "{fixture}");
    }
    // The known head of graphics-state: the first fill, the first stroke, the first fill and stroke.
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 0);
    the(&harness, Region::Drawer, Role::Switch, "Skip state-only ops").click();
    settle(&mut harness);
    let mut head = Vec::new();
    for _ in 0..3 {
        press(&mut harness, Key::ArrowRight);
        head.push(step(&harness));
    }
    assert_eq!(head, [3, 12, 19]);

    // Switching it off steps one operator again.
    the(&harness, Region::Drawer, Role::Switch, "Skip state-only ops").click();
    settle(&mut harness);
    press(&mut harness, Key::ArrowRight);
    assert_eq!(step(&harness), 20);
}

#[test]
fn space_plays_over_frames_pauses_and_stops_at_the_end() {
    // One frame lasts as long as one operator does while playing.
    let mut harness = builder()
        .with_step_dt(0.125)
        .build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "graphics-state");
    start_at(&mut harness, 70);
    assert!(!harness.state().state.canvas.view.playing);

    harness.key_press(Key::Space);
    harness.step();
    assert!(harness.state().state.canvas.view.playing);
    assert!((70..=71).contains(&step(&harness)), "it starts from where it is");
    harness.run_steps(2);
    assert!(
        the(&harness, Region::Drawer, Role::Button, "Pause")
            .accesskit_node()
            .label()
            .is_some()
    );
    let mut seen = vec![step(&harness)];
    for _ in 0..4 {
        harness.step();
        seen.push(step(&harness));
    }
    // One operator a frame, in order, and the selection and the drawer follow it.
    let first = seen[0];
    assert!(first > 70);
    assert_eq!(seen, (first..first + 5).collect::<Vec<_>>());
    assert_eq!(selected_op(&harness), Some(op(step(&harness))));
    assert!(has_label(&harness, &position_text(step(&harness) + 1, 80)));

    // Space again pauses where it is.
    press(&mut harness, Key::Space);
    let paused = step(&harness);
    assert!(!harness.state().state.canvas.view.playing);
    harness.run_steps(6);
    assert_eq!(step(&harness), paused);
    assert!(
        the(&harness, Region::Drawer, Role::Button, "Play")
            .accesskit_node()
            .label()
            .is_some()
    );

    // Playing on reaches the last operator and stops there.
    press(&mut harness, Key::Space);
    for _ in 0..40 {
        harness.step();
        if !harness.state().state.canvas.view.playing {
            break;
        }
    }
    assert!(
        !harness.state().state.canvas.view.playing,
        "playing stops by itself"
    );
    assert_eq!(step(&harness), 79);

    // Space at the end plays from the beginning.
    press(&mut harness, Key::Space);
    assert!(harness.state().state.canvas.view.playing);
    assert!(step(&harness) < 5, "{}", step(&harness));
    // Turning the stepper off stops it.
    press(&mut harness, Key::P);
    assert!(!harness.state().state.canvas.view.playing);
}

#[test]
fn a_click_or_drag_on_the_track_scrubs_to_the_operator_under_it() {
    for fixture in FIXTURES {
        let mut harness = opened(fixture);
        let last = harness.state().state.last_op(0).unwrap();
        start_at(&mut harness, 0);
        let track = the(&harness, Region::Drawer, Role::Slider, TRACK).rect();
        let at = |fraction: f32| {
            let start = StepTrack::x_of(track, 0, last);
            let end = StepTrack::x_of(track, last, last);
            pos2(start + (end - start) * fraction, track.center().y)
        };

        click_at(&mut harness, at(0.25));

        let quarter = (f64::from(last) * 0.25).round() as u32;
        assert_eq!(step(&harness), quarter, "{fixture}");
        assert_eq!(selected_op(&harness), Some(op(quarter)), "{fixture}");

        click_at(&mut harness, at(1.0));
        assert_eq!(step(&harness), last, "{fixture}");
        click_at(&mut harness, at(0.0));
        assert_eq!(step(&harness), 0, "{fixture}");

        // Dragging follows the pointer and lets go where it stops.
        harness.drag_at(at(0.25));
        harness.step();
        harness.hover_at(at(0.75));
        harness.step();
        harness.drop_at(at(0.75));
        harness.run();
        assert_eq!(
            step(&harness),
            (f64::from(last) * 0.75).round() as u32,
            "{fixture}"
        );
    }
    // The known case: a quarter of the way along graphics-state's 80 operators is operator 20.
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 0);
    let track = the(&harness, Region::Drawer, Role::Slider, TRACK).rect();
    let x = StepTrack::x_of(track, 0, 79)
        + (StepTrack::x_of(track, 79, 79) - StepTrack::x_of(track, 0, 79)) * 0.25;
    click_at(&mut harness, pos2(x, track.center().y));
    assert_eq!(step(&harness), 20);
}

#[test]
fn the_cards_show_the_operators_around_the_current_one_and_a_click_steps_there() {
    let core = doc("graphics-state");
    let ops = &core.captured(0).unwrap().ops;
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 49);

    let names = |harness: &Harness<'_, PdfxrayApp>| -> Vec<String> {
        buttons_in(harness, Region::Drawer)
            .into_iter()
            .filter(|name| name.starts_with("op #"))
            .collect()
    };
    let expected: Vec<String> = (45..=51)
        .map(|ix| card_label(ix, &ops[ix as usize].operator))
        .collect();
    assert_eq!(names(&harness), expected);
    let current = card_label(49, &ops[49].operator);
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        &current
    )));
    for name in expected.iter().filter(|name| **name != current) {
        assert!(
            !is_selected(&the(&harness, Region::Drawer, Role::Button, name)),
            "{name}"
        );
    }

    // A future card steps forward; a past one steps back.
    the(
        &harness,
        Region::Drawer,
        Role::Button,
        &card_label(51, &ops[51].operator),
    )
    .click();
    settle(&mut harness);
    assert_eq!(step(&harness), 51);
    assert_eq!(selected_op(&harness), Some(op(51)));
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        &card_label(51, &ops[51].operator)
    )));
    the(
        &harness,
        Region::Drawer,
        Role::Button,
        &card_label(47, &ops[47].operator),
    )
    .click();
    settle(&mut harness);
    assert_eq!(step(&harness), 47);

    // At the ends there are fewer cards: nothing before the first operator.
    harness.state_mut().state.step_to(OpPosition::new(0, 0));
    settle(&mut harness);
    assert_eq!(names(&harness).len(), 3);
    harness.state_mut().state.step_to(OpPosition::new(0, 79));
    settle(&mut harness);
    assert_eq!(names(&harness).len(), 5);
}

#[test]
fn the_buttons_of_the_drawer_step_and_jump() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 49);

    the(&harness, Region::Drawer, Role::Button, "Next operator").click();
    settle(&mut harness);
    assert_eq!(step(&harness), 50);
    the(&harness, Region::Drawer, Role::Button, "Previous operator").click();
    settle(&mut harness);
    assert_eq!(step(&harness), 49);
    // One page: Page end and Page start are its last and first operator, and stop there.
    the(&harness, Region::Drawer, Role::Button, PAGE_END_LABEL).click();
    settle(&mut harness);
    assert_eq!(step(&harness), 79);
    for button in ["Next operator", PAGE_END_LABEL] {
        assert!(
            the(&harness, Region::Drawer, Role::Button, button)
                .accesskit_node()
                .is_disabled(),
            "{button}"
        );
    }
    the(&harness, Region::Drawer, Role::Button, PAGE_START_LABEL).click();
    settle(&mut harness);
    assert_eq!(step(&harness), 0);
    assert!(
        the(&harness, Region::Drawer, Role::Button, PAGE_START_LABEL)
            .accesskit_node()
            .is_disabled()
    );
    the(&harness, Region::Drawer, Role::Button, "Play").click();
    settle(&mut harness);
    assert!(harness.state().state.canvas.view.playing);
}

#[test]
fn the_canvas_shows_the_page_painted_up_to_the_step() {
    for fixture in FIXTURES {
        let mut harness = opened(fixture);
        let all = digest(&harness);
        start_at(&mut harness, 5);

        assert_eq!(
            harness.state().state.shown_request().unwrap().content,
            ContentFilter::UpTo(5)
        );
        let at_five = expected_digest(&harness, fixture, ContentFilter::UpTo(5), |_, _| Vec::new());
        assert_eq!(digest(&harness), at_five, "{fixture}");

        press(&mut harness, Key::ArrowRight);
        wait_for_render(&mut harness);
        assert_eq!(
            digest(&harness),
            expected_digest(&harness, fixture, ContentFilter::UpTo(6), |_, _| Vec::new()),
            "{fixture}"
        );

        press(&mut harness, Key::P);
        wait_for_render(&mut harness);
        assert_eq!(digest(&harness), all, "{fixture}: the whole page again");
    }
    // Smoke: stepping really changes pixels where something paints.
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 3);
    let before = digest(&harness);
    harness.state_mut().state.step_to(OpPosition::new(0, 12));
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_ne!(digest(&harness), before);
}

#[test]
fn the_inspector_lists_what_covers_the_operator_and_how_much_stays_visible() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 49);

    // Op 49 fills the orange square; op 53 strokes a square over most of its corner.
    assert_eq!(inspector_value(&harness, "Step"), Some("50 of 80".to_owned()));
    assert_eq!(
        inspector_value(&harness, "Paints over"),
        Some(section::BARE_PAGE.to_owned())
    );
    assert_eq!(
        inspector_value(&harness, "Covered later by"),
        Some("op #53 S".to_owned())
    );
    assert_eq!(
        inspector_value(&harness, "Visible"),
        Some("47% · 53% under later ops".to_owned())
    );
    assert!(texts_in(&harness, Region::Inspector, Role::Label).contains(&section::ESTIMATE_NOTE.to_owned()));
    assert!(texts_in(&harness, Region::Inspector, Role::Label).contains(&section::STACK_TITLE.to_owned()));
    // The section comes before GEOMETRY.
    let buttons = buttons_in(&harness, Region::Inspector);
    let at = |name: &str| {
        buttons
            .iter()
            .position(|b| b == name)
            .unwrap_or_else(|| panic!("{name} in {buttons:?}"))
    };
    assert!(at("Paint order") < at("Geometry"));
    // The stack, top first: the later stroke, this operator.
    let stack: Vec<&String> = buttons.iter().filter(|b| b.starts_with('#')).collect();
    assert_eq!(stack, ["#53 S", "#49 f"]);

    // The link moves the stepper to the covering operator, whose own facts then show.
    the(&harness, Region::Inspector, Role::Label, "op #53 S").click();
    settle(&mut harness);
    assert_eq!(step(&harness), 53);
    assert_eq!(selected_op(&harness), Some(op(53)));
    assert_eq!(inspector_value(&harness, "Step"), Some("54 of 80".to_owned()));
    assert_eq!(inspector_value(&harness, "Paints over"), Some("#49 f".to_owned()));
    assert_eq!(
        inspector_value(&harness, "Covered later by"),
        Some("op #59 sh /Sh1".to_owned())
    );
    assert_eq!(
        inspector_value(&harness, "Visible"),
        Some("88% · 12% under later ops".to_owned())
    );

    // A stack row steps there too.
    the(&harness, Region::Inspector, Role::Button, "#49 f").click();
    settle(&mut harness);
    assert_eq!(step(&harness), 49);

    // Without the stepper the section is gone.
    press(&mut harness, Key::P);
    assert!(!buttons_in(&harness, Region::Inspector).contains(&"Paint order".to_owned()));
    assert_eq!(inspector_value(&harness, "Step"), None);
}

#[test]
fn the_more_row_of_the_stack_unfolds_in_place_and_show_fewer_folds_it() {
    // box-matrix: op 18 paints over five operators, so the stack lists three and "+2 more under".
    let mut harness = opened("box-matrix");
    start_at(&mut harness, 18);
    let stack = |harness: &Harness<'_, PdfxrayApp>| {
        buttons_in(harness, Region::Inspector)
            .into_iter()
            .filter(|b| b.starts_with('#'))
            .count()
    };
    let more = section::more_text(2, "under");
    assert_eq!(stack(&harness), 4, "this op and three under it");

    the(&harness, Region::Inspector, Role::Button, &more).click();
    settle(&mut harness);

    assert_eq!(stack(&harness), 6, "this op and all five under it");
    assert!(harness.query_by_label(&more).is_none());
    // Unfolding is not stepping.
    assert_eq!(step(&harness), 18);

    the(&harness, Region::Inspector, Role::Button, section::SHOW_FEWER).click();
    settle(&mut harness);
    assert_eq!(stack(&harness), 4);

    // What is unfolded belongs to the operator: stepping away and back finds it as it was.
    the(&harness, Region::Inspector, Role::Button, &more).click();
    settle(&mut harness);
    harness.state_mut().state.step_to(OpPosition::new(0, 17));
    settle(&mut harness);
    harness.state_mut().state.step_to(OpPosition::new(0, 18));
    settle(&mut harness);
    // Smoke: back on op 18, its part is still unfolded.
    assert_eq!(stack(&harness), 6);
}

#[test]
fn an_operator_nothing_covers_is_fully_visible() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 3);

    assert_eq!(
        inspector_value(&harness, "Covered later by"),
        Some("nothing".to_owned())
    );
    assert_eq!(inspector_value(&harness, "Visible"), Some("100%".to_owned()));
}

#[test]
fn the_canvas_has_the_legend_the_label_and_a_stepper_that_follows_the_drawer() {
    let mut harness = opened("graphics-state");
    assert!(!has_label(&harness, LEGEND_TITLE) || texts_in(&harness, Region::Center, Role::Label).is_empty());
    start_at(&mut harness, 49);

    let labels = texts_in(&harness, Region::Center, Role::Label);
    let at = labels
        .iter()
        .position(|t| t == LEGEND_TITLE)
        .expect("the legend card");
    assert_eq!(labels[at + 1..at + 4], LEGEND_ENTRIES);
    assert_eq!(
        painting_now(&harness.state().state).as_deref(),
        Some("▶ op #49 · f · painting now")
    );
    // The floating stepper reads the same position as the drawer and is usable.
    assert!(has_label(&harness, "step 50 / 80"));
    the(&harness, Region::Center, Role::Button, "Next operator").click();
    settle(&mut harness);
    assert_eq!(step(&harness), 50);
    assert!(has_label(&harness, "step 51 / 80"));
    assert!(has_label(&harness, &position_text(51, 80)));

    press(&mut harness, Key::P);
    assert!(!texts_in(&harness, Region::Center, Role::Label).contains(&LEGEND_TITLE.to_owned()));
    assert_eq!(painting_now(&harness.state().state), None);
}

/// A plain scroll of the canvas by `dy` points (negative scrolls down), with the pointer in the
/// middle of the canvas.
fn scroll_canvas(harness: &mut Harness<'_, PdfxrayApp>, dy: f32) {
    harness.hover_at(canvas_view(harness).center());
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    });
    settle(harness);
    wait_for_render(harness);
}

fn offset(harness: &Harness<'_, PdfxrayApp>) -> Vec2 {
    harness.state().state.canvas.scroll_offset
}

/// The first two operators of page 1 that paint something: a jump goes from one to the other.
fn two_painting_ops(doc: &Document) -> (u32, u32) {
    let last = doc.captured(0).unwrap().ops.last().unwrap().ix;
    let first = (0..=last).find(|ix| !doc.is_state_only(0, *ix)).unwrap();
    (first, doc.next_visible_change(0, first, true).unwrap())
}

/// Where operator `at` is drawn on screen, from where its page is drawn and the operator's box.
/// `None` while its page is not drawn.
fn op_on_screen(harness: &Harness<'_, PdfxrayApp>, doc: &Document, at: OpPosition) -> Option<Rect> {
    let page = harness
        .query_by_role_and_label(Role::Image, &format!("Page {} canvas", at.page + 1))?
        .rect();
    let info = doc.page(at.page).unwrap();
    assert_eq!(info.rotate, 0, "the mapping below is for an upright page");
    let (_, bbox) = doc.bbox(&NodeId::from(at)).unwrap();
    let crop = info.crop_box;
    // Page space is y-up from the crop box's corner; the screen is y-down from the page's.
    let at = |x: f64, y: f64| {
        let fx = ((x - crop.x0) / crop.width()) as f32;
        let fy = ((crop.y1 - y) / crop.height()) as f32;
        page.min + vec2(fx * page.width(), fy * page.height())
    };
    Some(Rect::from_two_pos(at(bbox.x0, bbox.y0), at(bbox.x1, bbox.y1)))
}

#[test]
fn scrolling_the_canvas_leaves_the_stepper_on_its_page_and_step() {
    let mut harness = opened("long-doc");
    start_at(&mut harness, 5);
    let total = harness.state().state.op_total();
    assert!(has_label(&harness, &mini_text(6, total)));

    // Far enough down that page 1 is out of view and another page fills it.
    scroll_canvas(&mut harness, -3000.0);

    let state = &harness.state().state;
    assert!(
        state.canvas.scroll_offset.y > 2000.0,
        "{:?}",
        state.canvas.scroll_offset
    );
    assert!(
        harness
            .query_by_role_and_label(Role::Image, "Page 1 canvas")
            .is_none()
    );
    // The stepper stays where it was: on page 1, at operator 5, painting page 1 up to it.
    assert_eq!(state.canvas.page, 0);
    assert_eq!(step(&harness), 5);
    assert_eq!(selected_op(&harness), Some(op(5)));
    let doc = doc("long-doc");
    let filter = |page| state.canvas.view.content_filter(&doc, page);
    assert_eq!(filter(0), ContentFilter::UpTo(5));
    assert_eq!(filter(5), ContentFilter::All);
    assert!(has_label(&harness, &mini_text(6, total)));
    assert!(has_label(&harness, &position_text(6, total)));
    // Smoke: the stepper is still on and still steps.
    the(&harness, Region::Center, Role::Button, "Next operator").click();
    settle(&mut harness);
    assert_eq!(step(&harness), 6);
}

#[test]
fn with_follow_step_a_step_out_of_view_scrolls_to_what_it_paints() {
    let doc = doc("long-doc");
    let (first, second) = two_painting_ops(&doc);
    let mut harness = opened("long-doc");
    start_at(&mut harness, first);
    // The switch sits next to "Skip state-only ops" in the Paint order tab, on from the start.
    let follow = the(&harness, Region::Drawer, Role::Switch, FOLLOW_LABEL);
    let skip = the(&harness, Region::Drawer, Role::Switch, SKIP_LABEL);
    assert!(is_selected(&follow));
    assert!((follow.rect().center().y - skip.rect().center().y).abs() < 1.0);
    assert!(harness.state().state.canvas.view.follow_step);

    // A step to what is already in view leaves the view alone, wherever it is in the view.
    scroll_canvas(&mut harness, -40.0);
    let still = offset(&harness);
    assert!(still.y > 0.0);
    assert!(
        canvas_view(&harness)
            .contains_rect(op_on_screen(&harness, &doc, OpPosition::new(0, second)).unwrap())
    );
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    settle(&mut harness);
    assert_eq!(step(&harness), second);
    assert_eq!(offset(&harness), still);

    // Scrolled away, a step back brings it into view.
    scroll_canvas(&mut harness, -3000.0);
    assert!(op_on_screen(&harness, &doc, OpPosition::new(0, first)).is_none());
    let away = offset(&harness);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowLeft);
    settle(&mut harness);
    wait_for_render(&mut harness);
    settle(&mut harness);

    assert_eq!(step(&harness), first);
    let shown = op_on_screen(&harness, &doc, OpPosition::new(0, first)).expect("page 1 is in view again");
    let view = canvas_view(&harness);
    assert!(view.contains_rect(shown), "{shown:?} is not inside {view:?}");
    // Smoke: the stepper's page stayed the current one, and the canvas scrolled back up to it.
    assert_eq!(harness.state().state.canvas.page, 0);
    assert!(
        offset(&harness).y < away.y - 2000.0,
        "{away:?} -> {:?}",
        offset(&harness)
    );
}

#[test]
fn without_follow_step_stepping_leaves_the_view_where_it_is() {
    let doc = doc("long-doc");
    let (first, second) = two_painting_ops(&doc);
    let mut harness = opened("long-doc");
    start_at(&mut harness, first);
    the(&harness, Region::Drawer, Role::Switch, FOLLOW_LABEL).click();
    settle(&mut harness);
    assert!(!harness.state().state.canvas.view.follow_step);
    assert!(!is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Switch,
        FOLLOW_LABEL
    )));
    scroll_canvas(&mut harness, -3000.0);
    let away = offset(&harness);

    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    settle(&mut harness);
    wait_for_render(&mut harness);

    assert_eq!(step(&harness), second);
    assert_eq!(selected_op(&harness), Some(op(second)));
    assert_eq!(offset(&harness), away);
    assert!(op_on_screen(&harness, &doc, OpPosition::new(0, second)).is_none());
    // Smoke: playing (another way to change the step) does not move the view either.
    harness.key_press(Key::Space);
    settle(&mut harness);
    assert!(step(&harness) > second);
    assert_eq!(offset(&harness), away);
}

/// Zooms the canvas to `zoom` by hand, as the zoom buttons do, and lets it redraw.
fn zoom_to(harness: &mut Harness<'_, PdfxrayApp>, zoom: f32) {
    harness.state_mut().state.set_zoom(zoom);
    settle(harness);
    wait_for_render(harness);
}

/// Steps until the canvas has scrolled to the step and drawn its page.
fn settle_on_step(harness: &mut Harness<'_, PdfxrayApp>) {
    settle(harness);
    wait_for_render(harness);
    settle(harness);
}

#[test]
fn a_step_onto_the_next_page_scrolls_what_it_paints_into_view_at_the_same_zoom() {
    let doc = doc("long-doc");
    let mut harness = opened("long-doc");
    zoom_to(&mut harness, 2.0);
    // Page 1's last Tj; long-doc runs 28 operators a page, the Tjs at 4, 11, 18 and 25.
    start_at(&mut harness, 25);
    the(&harness, Region::Drawer, Role::Switch, SKIP_LABEL).click();
    settle(&mut harness);
    let zoom = harness.state().state.canvas.zoom;

    press(&mut harness, Key::ArrowRight);
    settle_on_step(&mut harness);

    let state = &harness.state().state;
    let at = state.canvas.view.step;
    let shown = op_on_screen(&harness, &doc, at).expect("page 2 is drawn");
    let view = canvas_view(&harness);
    assert_eq!(
        at,
        OpPosition::new(1, 4),
        "past page 1's end to page 2's first Tj"
    );
    assert!(view.contains_rect(shown), "{shown:?} is not inside {view:?}");
    assert!(
        (state.canvas.zoom - zoom).abs() < f32::EPSILON,
        "the zoom is kept: {zoom} -> {}",
        state.canvas.zoom
    );
    assert_eq!(state.canvas.zoom_mode, ZoomMode::Manual);
    assert_eq!(state.canvas.page, 1);
    assert_eq!(selected_op(&harness), Some(NodeId::Op { page: 1, ix: 4 }));
    // smoke: the drawer counts the whole document and says where the step is
    assert!(has_label(&harness, &position_text(28 + 4 + 1, 40 * 28)));
    assert!(has_label(&harness, &at_text(at)));
    assert!(has_label(&harness, &mini_text(28 + 4 + 1, 40 * 28)));
}

#[test]
fn the_track_spans_every_page_and_dragging_it_shows_the_page_it_lands_on() {
    let doc = doc("long-doc");
    let mut harness = opened("long-doc");
    zoom_to(&mut harness, 1.5);
    start_at(&mut harness, 0);
    let zoom = harness.state().state.canvas.zoom;
    let last = harness.state().state.last_step_index().unwrap();
    let track = the(&harness, Region::Drawer, Role::Slider, TRACK).rect();
    let at = |fraction: f32| {
        let start = StepTrack::x_of(track, 0, last);
        let end = StepTrack::x_of(track, last, last);
        pos2(start + (end - start) * fraction, track.center().y)
    };

    harness.drag_at(at(0.0));
    harness.step();
    harness.hover_at(at(1.0));
    harness.step();
    harness.drop_at(at(1.0));
    settle_on_step(&mut harness);

    let state = &harness.state().state;
    let end = state.canvas.view.step;
    assert_eq!(last, 40 * 28 - 1);
    assert_eq!(
        end,
        OpPosition::new(39, 27),
        "the end of the track is the last page's last op"
    );
    assert_eq!(state.canvas.page, 39);
    assert!(
        (state.canvas.zoom - zoom).abs() < f32::EPSILON,
        "the zoom is kept: {zoom} -> {}",
        state.canvas.zoom
    );
    // EMC paints nothing, so its box is the empty one at the page's origin: the page holding it
    // is what has to be in view.
    let page = harness
        .query_by_role_and_label(Role::Image, "Page 40 canvas")
        .expect("page 40 is drawn")
        .rect();
    assert!(page.intersects(canvas_view(&harness)));

    // A click a quarter along lands on the operator a quarter through the document.
    click_at(&mut harness, at(0.25));
    settle_on_step(&mut harness);
    let quarter = (f64::from(last) * 0.25).round() as u32;
    let state = &harness.state().state;
    assert_eq!(state.step_index(), Some(quarter));
    assert_eq!(state.canvas.view.step, doc.op_at(quarter).unwrap());
    // smoke: the step's page is the one on the canvas, and it is cut at the step
    let step = state.canvas.view.step;
    assert_eq!(state.canvas.page, step.page);
    assert_eq!(
        state.canvas.view.content_filter(&doc, step.page),
        ContentFilter::UpTo(step.ix)
    );
    assert_eq!(
        state.canvas.view.content_filter(&doc, step.page + 1),
        ContentFilter::All
    );
}

/// Every shape painted last frame, groups flattened.
fn painted(harness: &Harness<'_, PdfxrayApp>) -> Vec<egui::Shape> {
    fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
            other => out.push(other.clone()),
        }
    }
    let mut shapes = Vec::new();
    for clipped in &harness.output().shapes {
        flatten(&clipped.shape, &mut shapes);
    }
    shapes
}

#[test]
fn the_track_marks_every_page_and_home_and_end_go_to_the_edges_of_pages() {
    let mut harness = opened("long-doc");
    harness.state_mut().state.select(NodeId::Op { page: 5, ix: 11 });
    settle(&mut harness);
    press(&mut harness, Key::P);
    settle_on_step(&mut harness);
    let shapes = painted(&harness);

    // One hairline per page, 40 in all.
    let marks = shapes
        .iter()
        .filter(|s| {
            matches!(s, egui::Shape::LineSegment { stroke, .. }
            if stroke.color == pdfxray_app::theme::tokens::TRACK_PAGE_MARK)
        })
        .count();
    assert_eq!(marks, 40);
    // A number shows only where the next mark leaves room for it, as the track says.
    let track = the(&harness, Region::Drawer, Role::Slider, TRACK).rect();
    let last = harness.state().state.last_step_index().unwrap();
    let all: Vec<PageMark> = (0..40).map(|page| PageMark { ix: page * 28, page }).collect();
    let expected: Vec<String> = StepTrack::labelled(track, &all, last)
        .iter()
        .map(|m| page_text(m.page))
        .collect();
    let numbers: Vec<String> = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t)
                if t.galley.text().starts_with('p') && t.galley.text()[1..].parse::<u32>().is_ok() =>
            {
                Some(t.galley.text().to_owned())
            }
            _ => None,
        })
        .collect();
    assert_eq!(numbers, expected);
    assert!(numbers.len() < 40, "{numbers:?}");

    // Home: page 6's first operator, then page 5's; End: page 5's last, then page 6's.
    let mut visited = Vec::new();
    for key in [Key::Home, Key::Home, Key::End, Key::End] {
        press(&mut harness, key);
        settle(&mut harness);
        visited.push(harness.state().state.canvas.view.step);
    }
    assert_eq!(
        visited,
        [
            OpPosition::new(5, 0),
            OpPosition::new(4, 0),
            OpPosition::new(4, 27),
            OpPosition::new(5, 27),
        ]
    );
    // The buttons do the same: at the end of page 6, Page end goes to the end of page 7.
    the(&harness, Region::Drawer, Role::Button, PAGE_END_LABEL).click();
    settle(&mut harness);
    assert_eq!(harness.state().state.canvas.view.step, OpPosition::new(6, 27));
    // Smoke: the hint says when the button would leave the page.
    assert_eq!(page_edge_hint(true, true), "Page end (next page)");
    assert_eq!(page_edge_hint(false, false), "Page start");
}

#[test]
fn the_track_and_the_switches_stay_put_while_the_position_text_grows() {
    let mut harness = opened("long-doc");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 0 });
    settle(&mut harness);
    press(&mut harness, Key::P);
    settle_on_step(&mut harness);
    let place = |harness: &Harness<'_, PdfxrayApp>| {
        (
            the(harness, Region::Drawer, Role::Slider, TRACK).rect(),
            the(harness, Region::Drawer, Role::Switch, UNPAINTED_LABEL).rect(),
        )
    };
    let at_start = place(&harness);

    // From `1 / 1120 ops   page 1 · op #0` to `1120 / 1120 ops   page 40 · op #27`.
    let last = harness.state().state.last_step_index().unwrap();
    harness.state_mut().state.step_to_index(last);
    settle_on_step(&mut harness);

    assert_eq!(place(&harness), at_start);
    // Smoke: the position really is at the end.
    assert!(has_label(&harness, &position_text(1120, 1120)));
}

#[test]
fn a_document_of_few_pages_has_every_page_number_over_the_track() {
    let mut harness = opened_two_pages();
    harness.key_press(Key::P);
    settle_on_step(&mut harness);

    let numbers: Vec<String> = painted(&harness)
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t) if ["p1", "p2"].contains(&t.galley.text()) => {
                Some(t.galley.text().to_owned())
            }
            _ => None,
        })
        .collect();

    assert_eq!(numbers, ["p1", "p2"]);
}

#[test]
fn the_unpainted_boxes_switch_takes_the_ghosts_and_the_legend_row_away() {
    let mut harness = opened("graphics-state");
    start_at(&mut harness, 49);
    let not_yet = LEGEND_ENTRIES[2];
    let label = pdfxray_app::center::canvas::paint_order::not_painted_yet(50, 79);
    assert!(has_label(&harness, not_yet));
    let shown_text = |harness: &Harness<'_, PdfxrayApp>| {
        painted(harness)
            .iter()
            .any(|s| matches!(s, egui::Shape::Text(t) if t.galley.text() == label))
    };
    assert!(shown_text(&harness));

    the(&harness, Region::Drawer, Role::Switch, UNPAINTED_LABEL).click();
    settle(&mut harness);

    assert!(!harness.state().state.canvas.view.unpainted);
    assert!(!shown_text(&harness));
    assert!(!has_label(&harness, not_yet));
    // Smoke: painted and current op stay in the legend.
    assert!(has_label(&harness, LEGEND_ENTRIES[0]));
    assert!(has_label(&harness, LEGEND_ENTRIES[1]));
}

/// [`two_overlapping_pages`], written once to a file the app can open, opened at [`ROOMY`].
fn opened_two_pages() -> Harness<'static, PdfxrayApp> {
    let dir = std::env::temp_dir().join("pdfxray-paint-order");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two-overlapping-pages.pdf");
    let bytes = two_overlapping_pages().unwrap();
    if std::fs::read(&path).ok().as_deref() != Some(&bytes[..]) {
        std::fs::write(&path, bytes).unwrap();
    }
    let mut harness = app();
    open_path(&mut harness, path);
    harness.state_mut().state.set_zoom(ROOMY);
    settle(&mut harness);
    wait_for_render(&mut harness);
    harness
}

#[test]
fn on_a_later_page_the_inspector_links_and_the_cards_step_on_that_page() {
    let (red, blue) = (PAINT_PAGES_RED_FILL, PAINT_PAGES_BLUE_FILL);
    let mut harness = opened_two_pages();
    harness.state_mut().state.select(NodeId::Op { page: 1, ix: red });
    settle(&mut harness);
    harness.key_press(Key::P);
    settle_on_step(&mut harness);
    let started = harness.state().state.canvas.view.step;
    let covered_by = inspector_value(&harness, "Covered later by");

    // The link to the covering operator: page 2's blue fill, not operator 5 of page 1.
    the(&harness, Region::Inspector, Role::Label, &format!("op #{blue} f")).click();
    settle(&mut harness);
    let after_link = harness.state().state.canvas.view.step;
    // A stack row back to the red fill.
    the(&harness, Region::Inspector, Role::Button, &format!("#{red} f")).click();
    settle(&mut harness);
    let after_row = harness.state().state.canvas.view.step;
    // A card: page 2's `re` of the blue square, operator 4.
    the(&harness, Region::Drawer, Role::Button, &card_label(4, "re")).click();
    settle(&mut harness);
    let after_card = harness.state().state.canvas.view.step;

    assert_eq!(started, OpPosition::new(1, red));
    assert_eq!(covered_by, Some(format!("op #{blue} f")));
    assert_eq!(after_link, OpPosition::new(1, blue));
    assert_eq!(after_row, OpPosition::new(1, red));
    assert_eq!(after_card, OpPosition::new(1, 4));
    assert_eq!(selected_op(&harness), Some(NodeId::Op { page: 1, ix: 4 }));
    // smoke: the count is the place in the whole document, after page 1's operators
    assert!(has_label(
        &harness,
        &position_text(PAINT_PAGES_FIRST_OPS + 4 + 1, PAINT_PAGES_FIRST_OPS + 6)
    ));
}

/// A selection made elsewhere (the Content tree, a link) of an operator with no box still shows
/// its page, at the same zoom. The stepper's version of this, which goes through the canvas's
/// reveal of a selection without a box, is covered by
/// `the_track_spans_every_page_and_dragging_it_shows_the_page_it_lands_on`.
#[test]
fn selecting_an_operator_that_paints_nothing_on_a_page_out_of_view_scrolls_to_its_page() {
    let mut harness = opened("long-doc");
    let zoom = harness.state().state.canvas.zoom;
    assert!(
        harness
            .query_by_role_and_label(Role::Image, "Page 40 canvas")
            .is_none(),
        "page 40 starts out of view"
    );

    // Page 40's last operator, an EMC: no box to outline, without the stepper.
    harness.state_mut().state.select(NodeId::Op { page: 39, ix: 27 });
    settle_on_step(&mut harness);

    let page = harness
        .query_by_role_and_label(Role::Image, "Page 40 canvas")
        .expect("page 40 is drawn");
    let state = &harness.state().state;
    assert!(page.rect().intersects(canvas_view(&harness)));
    assert!(
        (state.canvas.zoom - zoom).abs() < f32::EPSILON,
        "the zoom is kept: {zoom} -> {}",
        state.canvas.zoom
    );
    assert!(!state.canvas.view.stepper);
    // smoke: nothing is left waiting to be revealed
    assert!(!state.canvas.reveal_pending());
}
