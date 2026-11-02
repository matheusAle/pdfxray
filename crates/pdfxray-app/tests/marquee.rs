//! Marquee multi-selection on the canvas (board "2.12 Canvas · Marquee multi-selection"): the
//! gestures, what a marquee touches per Pick target, the count chip, auto-scroll, the action bar
//! of a multi-selection, the inspector summary and stepping, and the keyboard path of the trees.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation
)]

mod common;

use common::{
    Region, app, buttons_in, click_rail, has_label, is_selected, nodes_in, open_fixture, texts_in, the,
    wait_for_render,
};
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::{IsolateMode, PdfxrayApp, RailItem};
use pdfxray_core::{BoxKind, Document, LayerMode, NodeId, ObjRef, Point};
use pdfxray_render::{ContentFilter, ops_for, ops_for_all};
use pretty_assertions::assert_eq;

type App = Harness<'static, PdfxrayApp>;

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

/// The elements of `struct-rich` that own content: `H1` 12, `P` 13, `Figure` 14, `TH` 17,
/// `Cell` 18, `Lbl` 21, `LBody` 22.
const H1: u32 = 12;
const P: u32 = 13;
const FIGURE: u32 = 14;
const CELL: u32 = 18;

/// Empty space between the paragraph and the heading of `struct-rich`, right of the text.
const EMPTY: Point = Point::new(160.0, 345.0);
/// Down to the second cell: a marquee from [`EMPTY`] to here touches P, the Figure and the cell.
const ACROSS: Point = Point::new(100.0, 200.0);

fn opened(fixture: &str, rail: &str, pick: LayerMode) -> App {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    click_rail(&mut harness, rail);
    harness.state_mut().state.canvas.layer_mode = pick;
    harness.run_steps(4);
    wait_for_render(&mut harness);
    harness
}

fn settle(harness: &mut App) {
    harness.run_steps(4);
}

/// Screen position of page-space point `p` of page 1.
fn at(harness: &App, p: Point) -> Pos2 {
    let state = &harness.state().state;
    let crop = state.document.as_ref().unwrap().doc.page(0).unwrap().crop_box;
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    let zoom = f64::from(state.canvas.zoom);
    canvas.min + vec2(((p.x - crop.x0) * zoom) as f32, ((crop.y1 - p.y) * zoom) as f32)
}

/// Presses at `from` with `modifiers` held and moves to `to` in four steps, without letting go.
fn begin_drag_screen(harness: &mut App, from: Pos2, to: Pos2, modifiers: Modifiers) {
    // The modifiers are held before the press, as a hand would.
    harness.event_modifiers(Event::PointerMoved(from), modifiers);
    harness.run();
    harness.event_modifiers(Event::PointerMoved(from), modifiers);
    harness.run();
    harness.event_modifiers(
        Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        },
        modifiers,
    );
    harness.run();
    for step in 1..=4u8 {
        harness.event_modifiers(
            Event::PointerMoved(from.lerp(to, f32::from(step) / 4.0)),
            modifiers,
        );
        // Not `run`: a marquee past the edge keeps the canvas scrolling.
        harness.run_steps(3);
    }
}

fn end_drag(harness: &mut App, modifiers: Modifiers) {
    let pos = harness.state().state.canvas.cursor.map_or(Pos2::ZERO, |_| {
        harness
            .ctx
            .input(|i| i.pointer.latest_pos().unwrap_or(Pos2::ZERO))
    });
    harness.event_modifiers(
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers,
        },
        modifiers,
    );
    harness.run();
    settle(harness);
}

fn begin_drag(harness: &mut App, from: Point, to: Point, modifiers: Modifiers) {
    let (from, to) = (at(harness, from), at(harness, to));
    begin_drag_screen(harness, from, to, modifiers);
}

fn drag(harness: &mut App, from: Point, to: Point, modifiers: Modifiers) {
    begin_drag(harness, from, to, modifiers);
    end_drag(harness, modifiers);
}

fn selected(harness: &App) -> Vec<NodeId> {
    harness.state().state.selection.nodes().to_vec()
}

fn primary(harness: &App) -> Option<NodeId> {
    harness.state().state.selection.primary().cloned()
}

/// The names of the buttons of the bar under the selection.
fn bar(harness: &App) -> Vec<String> {
    let names: Vec<&str> = pdfxray_app::actions::SelectionAction::ALL
        .iter()
        .map(|a| a.label())
        .collect();
    let toolbar_bottom = pdfxray_app::theme::tokens::TITLE_H + pdfxray_app::center::toolbar::HEIGHT;
    nodes_in(harness, Region::Center, Role::Button)
        .into_iter()
        .filter(|node| node.rect().min.y > toolbar_bottom)
        .filter_map(|node| node.accesskit_node().label())
        .filter(|label| names.contains(&label.as_str()))
        .collect()
}

/// The lowest button named `name`: the bar's, not the toolbar's.
fn bar_button<'h>(harness: &'h App, name: &'h str) -> egui_kittest::Node<'h> {
    harness
        .query_all_by_role_and_label(Role::Button, name)
        .max_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .expect("a bar button")
}

fn painted(harness: &App) -> Vec<Shape> {
    fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
        if let Shape::Vec(shapes) = shape {
            for shape in shapes {
                flatten(shape, out);
            }
        } else {
            out.push(shape.clone());
        }
    }
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut out);
    }
    out
}

/// How many outlines in `color` are painted.
fn outlines_in(harness: &App, color: egui::Color32) -> usize {
    painted(harness)
        .iter()
        .filter(|shape| matches!(shape, Shape::Rect(rect) if rect.stroke.color == color))
        .count()
}

/// The text the last frame copied.
fn copied(harness: &App) -> Option<String> {
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

fn doc(name: &str) -> Document {
    Document::open(common::fixture_path(name)).unwrap()
}

#[test]
fn a_drag_from_empty_space_selects_every_element_it_touches_with_the_tags_target() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);

    begin_drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    // While it is drawn: the count chip, the pre-highlight of what it will catch, nothing selected
    // yet.
    let marquee = harness.state().state.canvas.marquee.clone().expect("a marquee");
    assert_eq!(marquee.catch, [elem(P), elem(FIGURE), elem(CELL)]);
    assert!(has_label(&harness, "3 · Tags"));
    assert_eq!(
        outlines_in(&harness, pdfxray_app::theme::tokens::MARQUEE_CATCH),
        3
    );
    assert_eq!(primary(&harness), Some(NodeId::Document));
    end_drag(&mut harness, Modifiers::NONE);

    assert_eq!(selected(&harness), [elem(P), elem(FIGURE), elem(CELL)]);
    // The first in reading order is the primary.
    assert_eq!(primary(&harness), Some(elem(P)));
    assert!(harness.state().state.canvas.marquee.is_none());
    // Smoke: the chip is gone, the group badge is up.
    assert!(!has_label(&harness, "3 · Tags"));
    assert!(has_label(&harness, "3 selected"));
}

#[test]
fn each_pick_target_selects_its_own_kind_of_node() {
    // Content: the operators that paint there, the Figure's `f` among them.
    let mut harness = opened("struct-rich", "Content", LayerMode::Content);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    let ops = selected(&harness);
    assert!(ops.iter().all(|n| matches!(n, NodeId::Op { .. })), "{ops:?}");
    assert!(ops.contains(&NodeId::Op { page: 0, ix: 17 }), "{ops:?}");
    assert_eq!(
        ops,
        doc("struct-rich").hit_rect(
            0,
            pdfxray_core::Rect::new(100.0, 200.0, 160.0, 345.0),
            LayerMode::Content
        )
    );

    // All: everything is tagged, so the elements.
    let mut harness = opened("struct-rich", "Content", LayerMode::All);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    assert_eq!(selected(&harness), [elem(P), elem(FIGURE), elem(CELL)]);

    // Artifacts: the rule and the border, not the header or the footer.
    let mut harness = opened("artifacts", "Artifacts", LayerMode::Artifacts);
    // From the margin right of the border.
    drag(
        &mut harness,
        Point::new(198.0, 150.0),
        Point::new(30.0, 85.0),
        Modifiers::NONE,
    );
    let artifact = |seq| NodeId::Artifact { page: 0, seq };
    assert_eq!(selected(&harness), [artifact(1), artifact(3)]);

    // Boxes: the outlines it crosses; the whole page is empty space for a box drag.
    let mut harness = opened("page-boxes", "Pages", LayerMode::Boxes);
    drag(
        &mut harness,
        Point::new(100.0, 100.0),
        Point::new(25.0, 120.0),
        Modifiers::NONE,
    );
    let page_box = |kind| NodeId::PageBox { page: 0, kind };
    assert_eq!(
        selected(&harness),
        [page_box(BoxKind::Art), page_box(BoxKind::Trim)]
    );
    // Smoke: two boxes are the compare view.
    assert!(has_label(&harness, "2 selected · gap"));
}

#[test]
fn shift_drag_adds_and_alt_drag_subtracts() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);

    // Around the heading, from the empty space right of it.
    drag(
        &mut harness,
        Point::new(185.0, 390.0),
        Point::new(150.0, 360.0),
        Modifiers::SHIFT,
    );
    assert_eq!(selected(&harness), [elem(P), elem(FIGURE), elem(CELL), elem(H1)]);
    assert_eq!(primary(&harness), Some(elem(P)));

    // Across the Figure's right edge, from the empty space right of it.
    drag(
        &mut harness,
        Point::new(155.0, 300.0),
        Point::new(130.0, 290.0),
        Modifiers::ALT,
    );
    assert_eq!(selected(&harness), [elem(P), elem(CELL), elem(H1)]);
    // Smoke: a plain marquee over nothing clears the selection.
    drag(
        &mut harness,
        Point::new(185.0, 390.0),
        Point::new(175.0, 380.0),
        Modifiers::NONE,
    );
    assert_eq!(selected(&harness), Vec::<NodeId>::new());
}

#[test]
fn shift_click_adds_or_removes_one_and_alt_click_subtracts_one() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    let heading = at(&harness, Point::new(60.0, 365.0));
    let figure = at(&harness, Point::new(60.0, 270.0));

    // The heading's marked content joins the selection.
    harness.event_modifiers(Event::PointerMoved(heading), Modifiers::SHIFT);
    harness.run();
    click_with(&mut harness, heading, Modifiers::SHIFT);
    let mc_heading = NodeId::MarkedContent { page: 0, mcid: 0 };
    assert_eq!(selected(&harness).len(), 4);
    assert_eq!(selected(&harness)[3], mc_heading);
    // Shift-click on it again takes it out.
    click_with(&mut harness, heading, Modifiers::SHIFT);
    assert_eq!(selected(&harness), [elem(P), elem(FIGURE), elem(CELL)]);

    // ⌥-click on the Figure takes the selected element under the pointer out.
    click_with(&mut harness, figure, Modifiers::ALT);
    assert_eq!(selected(&harness), [elem(P), elem(CELL)]);
    // Shift-click on the Figure: the element under the pointer that is selected is none, so its
    // marked content (the topmost) joins.
    click_with(&mut harness, figure, Modifiers::SHIFT);
    assert_eq!(selected(&harness).len(), 3);
    // Smoke: the primary stayed the first one throughout.
    assert_eq!(primary(&harness), Some(elem(P)));
}

fn click_with(harness: &mut App, pos: Pos2, modifiers: Modifiers) {
    harness.event_modifiers(Event::PointerMoved(pos), modifiers);
    harness.run();
    for pressed in [true, false] {
        harness.event_modifiers(
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers,
            },
            modifiers,
        );
    }
    harness.run();
    settle(harness);
}

#[test]
fn command_drag_starts_a_marquee_over_an_element_and_a_plain_drag_there_does_not() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    let inside_figure = Point::new(60.0, 270.0);
    let to = Point::new(130.0, 200.0);

    begin_drag(&mut harness, inside_figure, to, Modifiers::NONE);
    assert!(harness.state().state.canvas.marquee.is_none());
    end_drag(&mut harness, Modifiers::NONE);
    assert_eq!(selected(&harness), [NodeId::Document]);

    drag(&mut harness, inside_figure, to, Modifiers::COMMAND);
    // The Figure and the second cell: two, so the compare view, measured from the first in
    // reading order.
    assert_eq!(selected(&harness), [elem(FIGURE), elem(CELL)]);
    assert_eq!(primary(&harness), Some(elem(FIGURE)));
    assert!(has_label(&harness, "2 selected · gap"));
    // Smoke: the crumbs name the Figure, then the cell after a plus.
    assert!(texts_in(&harness, Region::Inspector, Role::Label).contains(&"+".to_owned()));
}

#[test]
fn space_drag_pans_and_the_measure_tool_keeps_plain_drag() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    harness.state_mut().state.set_zoom(3.0);
    settle(&mut harness);
    wait_for_render(&mut harness);
    let offset = harness.state().state.canvas.scroll_offset;
    let from = common::canvas_view(&harness).center();

    harness.event(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    begin_drag_screen(&mut harness, from, from + vec2(-80.0, -120.0), Modifiers::NONE);
    assert!(harness.state().state.canvas.marquee.is_none());
    end_drag(&mut harness, Modifiers::NONE);
    harness.event(Event::Key {
        key: Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();
    assert_ne!(harness.state().state.canvas.scroll_offset, offset);
    assert_eq!(selected(&harness), [NodeId::Document]);

    // Measuring: a drag draws the measured line, not a marquee.
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    harness.key_press(Key::M);
    settle(&mut harness);
    begin_drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    assert!(harness.state().state.canvas.marquee.is_none());
    end_drag(&mut harness, Modifiers::NONE);
    assert!(harness.state().state.canvas.measure.line().is_some());
    // Smoke: the selection did not change.
    assert_eq!(selected(&harness), [NodeId::Document]);
}

#[test]
fn command_a_selects_all_of_the_pick_target_and_escape_steps_back() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);

    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    settle(&mut harness);
    let all = [H1, P, FIGURE, 17, CELL, 21, 22].map(elem);
    assert_eq!(selected(&harness), all);
    assert!(has_label(&harness, "7 selected"));

    // Esc keeps the primary alone, then clears.
    harness.key_press(Key::Escape);
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(H1)]);
    harness.key_press(Key::Escape);
    settle(&mut harness);
    assert_eq!(selected(&harness), Vec::<NodeId>::new());
    // Smoke: with the Content target ⌘A selects every operator that paints.
    harness.state_mut().state.canvas.layer_mode = LayerMode::Content;
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    settle(&mut harness);
    assert!(selected(&harness).iter().all(|n| matches!(n, NodeId::Op { .. })));
    assert!(selected(&harness).contains(&NodeId::Op { page: 0, ix: 17 }));
}

#[test]
fn escape_while_drawing_drops_the_marquee_and_keeps_the_selection() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    begin_drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().state.canvas.marquee.is_none());
    end_drag(&mut harness, Modifiers::NONE);
    assert_eq!(selected(&harness), [NodeId::Document]);
}

#[test]
fn a_marquee_past_the_edge_of_the_view_scrolls_the_canvas() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    harness.state_mut().state.set_zoom(3.0);
    settle(&mut harness);
    // The top of the page in view.
    harness.state_mut().state.go_to_page(0);
    settle(&mut harness);
    wait_for_render(&mut harness);
    let view = common::canvas_view(&harness);
    let from = at(&harness, Point::new(185.0, 390.0));
    assert!(view.contains(from), "{from:?} in {view:?}");
    // Down past the bottom of the view and left across the text.
    let below = egui::pos2(from.x - 150.0, view.max.y + 40.0);

    begin_drag_screen(&mut harness, from, below, Modifiers::NONE);
    let (offset, end) = {
        let state = &harness.state().state;
        (
            state.canvas.scroll_offset.y,
            state.canvas.marquee.as_ref().unwrap().end.y,
        )
    };
    harness.run_steps(10);

    let state = &harness.state().state;
    assert!(
        state.canvas.scroll_offset.y > offset,
        "{} > {offset}",
        state.canvas.scroll_offset.y
    );
    // The pointer stayed put while the page moved up under it: the marquee reaches further down
    // the page (lower y in PDF space).
    let marquee = state.canvas.marquee.as_ref().unwrap();
    assert!(marquee.end.y < end, "{} < {end}", marquee.end.y);
    // Smoke: it still catches what it touches.
    assert_ne!(marquee.catch, Vec::<NodeId>::new());
    end_drag(&mut harness, Modifiers::NONE);
}

#[test]
fn the_bar_of_a_multi_selection_copies_ids_shows_them_in_the_tree_and_clears() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);

    assert_eq!(
        bar(&harness),
        [
            "Isolate",
            "Hide",
            "Show in tree",
            "Copy ids",
            "Clear selection (Esc)"
        ]
    );
    assert!(has_label(&harness, "3 elements"));
    // Object and the crumbs' actions are for one node only.
    assert!(!bar(&harness).contains(&"Object".to_owned()));

    bar_button(&harness, "Copy ids").click();
    harness.step();
    assert_eq!(
        copied(&harness).as_deref(),
        Some("struct:13:0\nstruct:14:0\nstruct:18:0")
    );

    bar_button(&harness, "Show in tree").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.nav.rail, RailItem::Tags);
    let rows: Vec<String> = nodes_in(&harness, Region::Navigator, Role::Button)
        .into_iter()
        .filter(is_selected)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(rows, ["P", "Figure", "Cell"]);
    assert_eq!(selected(&harness).len(), 3);

    bar_button(&harness, "Clear selection (Esc)").click();
    settle(&mut harness);
    assert_eq!(selected(&harness), Vec::<NodeId>::new());
    // Smoke: no bar without a selection.
    assert_eq!(bar(&harness), Vec::<String>::new());
}

#[test]
fn isolate_and_hide_act_on_every_selected_node() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    let nodes = vec![elem(P), elem(FIGURE), elem(CELL)];
    let document = doc("struct-rich");
    let ops = ops_for_all(&document, &nodes);
    for node in &nodes {
        assert!(ops_for(&document, node).is_subset(&ops));
    }

    bar_button(&harness, "Isolate").click();
    settle(&mut harness);
    wait_for_render(&mut harness);
    let isolation = harness.state().state.canvas.view.isolation.clone().unwrap();
    assert_eq!(
        (isolation.mode, isolation.nodes.clone()),
        (IsolateMode::Only, nodes.clone())
    );
    assert_eq!(isolation.title, "3 elements");
    assert_eq!(
        harness.state().state.shown_request().map(|r| r.content.clone()),
        Some(ContentFilter::Only(ops.clone()))
    );

    // H hides the whole selection.
    harness.key_press(Key::H);
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(
        harness.state().state.shown_request().map(|r| r.content.clone()),
        Some(ContentFilter::Hide(ops.clone()))
    );
    // Smoke: I toggles back to isolating all three.
    harness.key_press(Key::I);
    settle(&mut harness);
    let isolation = harness.state().state.canvas.view.isolation.clone().unwrap();
    assert_eq!((isolation.mode, isolation.nodes), (IsolateMode::Only, nodes));
}

#[test]
fn the_inspector_summarises_the_selection_and_only_narrows_it_to_a_role() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);

    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    let from = labels.iter().position(|t| t == "3 elements").expect("the title");
    assert_eq!(labels[from + 1], "Pick: Tags · page 1");
    assert!(labels.contains(&"3 roles".to_owned()), "{labels:?}");
    assert!(labels.contains(&"page 1".to_owned()), "{labels:?}");
    let buttons = buttons_in(&harness, Region::Inspector);
    for name in [
        "Only P",
        "Only Figure",
        "Only Cell",
        "Inspect P 13 0 R",
        "Inspect Figure 14 0 R",
        "Inspect Cell 18 0 R",
    ] {
        assert!(buttons.contains(&name.to_owned()), "{name} in {buttons:?}");
    }

    the(&harness, Region::Inspector, Role::Button, "Only Figure").click();
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(FIGURE)]);
    // Smoke: one node is the normal inspector again.
    assert!(!has_label(&harness, "3 elements"));

    // Operators narrow by their name: every `Tj` of the page.
    let mut harness = opened("struct-rich", "Content", LayerMode::Content);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    settle(&mut harness);
    let all = selected(&harness).len();
    the(&harness, Region::Inspector, Role::Button, "Only Tj").click();
    settle(&mut harness);
    let tj = selected(&harness);
    let document = doc("struct-rich");
    assert!(tj.len() >= 2 && tj.len() < all, "{} of {all}", tj.len());
    assert!(tj.iter().all(|n| document.node(n).unwrap().title == "Tj"));
}

#[test]
fn a_row_steps_into_one_element_and_the_brackets_step_through_the_others() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);

    the(&harness, Region::Inspector, Role::Button, "Inspect Figure 14 0 R").click();
    settle(&mut harness);
    assert_eq!(primary(&harness), Some(elem(FIGURE)));
    assert!(has_label(&harness, "2 of 3"));
    assert!(buttons_in(&harness, Region::Inspector).contains(&"← 3 elements".to_owned()));
    // The canvas keeps all three selected.
    assert_eq!(selected(&harness).len(), 3);

    harness.key_press(Key::CloseBracket);
    settle(&mut harness);
    assert_eq!(primary(&harness), Some(elem(CELL)));
    assert!(has_label(&harness, "3 of 3"));
    harness.key_press(Key::CloseBracket);
    settle(&mut harness);
    assert_eq!(primary(&harness), Some(elem(P)));
    harness.key_press(Key::OpenBracket);
    settle(&mut harness);
    assert_eq!(primary(&harness), Some(elem(CELL)));

    the(&harness, Region::Inspector, Role::Button, "← 3 elements").click();
    settle(&mut harness);
    assert!(has_label(&harness, "3 elements"));
    assert!(!has_label(&harness, "3 of 3"));
    // Smoke: going back keeps the selection and its primary.
    assert_eq!(selected(&harness), [elem(P), elem(FIGURE), elem(CELL)]);
    assert_eq!(primary(&harness), Some(elem(CELL)));
}

#[test]
fn the_tree_extends_with_shift_arrows_toggles_with_command_space_and_the_canvas_mirrors_it() {
    let mut harness = opened("struct-rich", "Tags", LayerMode::Tags);
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    settle(&mut harness);
    the(&harness, Region::Navigator, Role::Button, "H1").click();
    settle(&mut harness);
    the(&harness, Region::Navigator, Role::Button, "H1").focus();
    settle(&mut harness);

    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(H1), elem(P)]);
    assert_eq!(primary(&harness), Some(elem(P)));
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(H1), elem(P), elem(FIGURE)]);
    // Going back shrinks the run.
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(H1), elem(P)]);

    // ⌘+Space toggles the focused row (the heading) out and in again.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Space);
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(P)]);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Space);
    settle(&mut harness);
    assert_eq!(selected(&harness), [elem(P), elem(H1)]);

    // ⌘A: all of the Pick target; the tree and the canvas show all of them.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    settle(&mut harness);
    assert_eq!(selected(&harness).len(), 7);
    assert!(has_label(&harness, "7 selected"));
    let rows: Vec<String> = nodes_in(&harness, Region::Navigator, Role::Button)
        .into_iter()
        .filter(is_selected)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    // Smoke: every element of the page has its row highlighted.
    assert_eq!(rows.len(), 7, "{rows:?}");
}

#[test]
fn tab_reaches_the_buttons_of_the_bar() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .focus();
    settle(&mut harness);

    let mut reached = false;
    for _ in 0..60 {
        harness.key_press(Key::Tab);
        harness.run();
        if bar_button(&harness, "Isolate").is_focused() {
            reached = true;
            break;
        }
    }
    assert!(reached, "Tab never reached the bar");
    // Smoke: the next Tab goes on along the bar.
    harness.key_press(Key::Tab);
    harness.run();
    assert!(bar_button(&harness, "Hide").is_focused());
}

/// The app on the wgpu renderer for a pixel snapshot, with `struct-rich` open on the Tags entry
/// and the Tags target.
fn snapshot_app() -> App {
    let mut harness = common::builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "struct-rich");
    click_rail(&mut harness, "Tags");
    harness.state_mut().state.canvas.layer_mode = LayerMode::Tags;
    harness.run_steps(4);
    wait_for_render(&mut harness);
    harness
}

#[test]
fn snapshot_a_marquee_being_drawn() {
    let _gpu = common::gpu_lock();
    let mut harness = snapshot_app();
    begin_drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    harness.run_steps(2);
    assert_eq!(
        harness
            .state()
            .state
            .canvas
            .marquee
            .as_ref()
            .map(|m| m.catch.len()),
        Some(3)
    );
    harness.snapshot("app_marquee_drawing");
}

#[test]
fn snapshot_a_multi_selection_with_its_bar_and_summary() {
    let _gpu = common::gpu_lock();
    let mut harness = snapshot_app();
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    drag(
        &mut harness,
        Point::new(185.0, 390.0),
        Point::new(150.0, 360.0),
        Modifiers::SHIFT,
    );
    assert_eq!(selected(&harness).len(), 4);
    harness.remove_cursor();
    harness.run_steps(4);
    harness.snapshot("app_multi_selection");
}

#[test]
fn snapshot_one_element_of_a_multi_selection() {
    let _gpu = common::gpu_lock();
    let mut harness = snapshot_app();
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    drag(
        &mut harness,
        Point::new(185.0, 390.0),
        Point::new(150.0, 360.0),
        Modifiers::SHIFT,
    );
    the(&harness, Region::Inspector, Role::Button, "Inspect Figure 14 0 R").click();
    harness.run_steps(4);
    assert!(has_label(&harness, "2 of 4"));
    harness.remove_cursor();
    harness.run_steps(4);
    harness.snapshot("app_multi_stepped_in");
}

#[test]
fn a_by_role_row_reads_tag_count_and_only_as_one_group_from_the_tag() {
    let mut harness = opened("struct-rich", "Content", LayerMode::Tags);
    drag(&mut harness, EMPTY, ACROSS, Modifiers::NONE);
    let only = the(&harness, Region::Inspector, Role::Button, "Only Figure").rect();
    let count = common::named(&harness, Region::Inspector, Role::Label, "1")
        .into_iter()
        .map(|n| n.rect())
        .find(|r| (r.center().y - only.center().y).abs() < 2.0)
        .expect("the count on the row of Only Figure");
    let tag = painted(&harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r) if (r.rect.height() - 16.0).abs() < 0.5 => Some(r.rect),
            _ => None,
        })
        .find(|r| (r.center().y - only.center().y).abs() < 2.0 && r.max.x <= count.min.x)
        .expect("the Figure tag on the row");

    // Tag · count · Only Figure, 10 px apart, from the tag on.
    assert!((count.min.x - tag.max.x - 10.0).abs() < 1.0, "{tag:?} {count:?}");
    assert!(
        (only.min.x - count.max.x - 10.0).abs() < 1.0,
        "{count:?} {only:?}"
    );
    // Smoke: the group stays by the tag, and the free space is on the right of the panel.
    assert!(only.max.x < tag.min.x + 160.0, "{tag:?} {only:?}");
    assert!(harness.ctx.content_rect().max.x - only.max.x > 100.0, "{only:?}");
}
