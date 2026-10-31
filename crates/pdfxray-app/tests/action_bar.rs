//! The selection action bar under the outline on the canvas, and the right-click menu of the
//! canvas: the same actions as the tree rows, for the node the user is pointing at.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation
)]

mod common;
#[path = "common/view_tools.rs"]
mod view_tools;

use common::{Region, app, click_at, is_selected, open_fixture, texts_in, the, wait_for_render};
use egui::{PointerButton, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::center::canvas::action_bar::BAR_ACTIONS;
use pdfxray_app::{IsolateMode, PdfxrayApp, RailItem};
use pdfxray_core::{NodeId, ObjRef, TreeKind};
use pretty_assertions::assert_eq;

const ROOMY: f32 = 0.4;

fn opened(fixture: &str) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    harness.state_mut().state.set_zoom(ROOMY);
    harness.run_steps(4);
    wait_for_render(&mut harness);
    harness
}

fn settle(harness: &mut Harness<'_, PdfxrayApp>) {
    harness.run_steps(4);
}

/// The names of the buttons of the bar: the ones under the page's top edge that are not in the
/// toolbar.
fn bar(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    let names: Vec<&str> = BAR_ACTIONS.iter().map(|a| a.label()).collect();
    // Isolate and Hide are on the toolbar as well.
    let toolbar_bottom = pdfxray_app::theme::tokens::TITLE_H + pdfxray_app::center::toolbar::HEIGHT;
    common::nodes_in(harness, Region::Center, Role::Button)
        .into_iter()
        .filter(|node| node.rect().min.y > toolbar_bottom)
        .filter_map(|node| node.accesskit_node().label())
        .filter(|label| names.contains(&label.as_str()))
        .collect()
}

/// The button of the bar named `name`: the lowest one, as the View button can have the same name
/// while the mode is on.
fn bar_button<'h>(harness: &'h Harness<'_, PdfxrayApp>, name: &'h str) -> egui_kittest::Node<'h> {
    harness
        .query_all_by_role_and_label(Role::Button, name)
        .max_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .expect("a bar button")
}

fn page_rect(harness: &Harness<'_, PdfxrayApp>) -> egui::Rect {
    harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect()
}

fn find_heading(nodes: Vec<pdfxray_core::TreeNode>) -> Option<NodeId> {
    nodes.into_iter().find_map(|n| {
        if n.label == "H1" {
            Some(n.id)
        } else {
            find_heading(n.children)
        }
    })
}

fn heading(fixture: &str) -> NodeId {
    let doc = pdfxray_core::Document::open(common::fixture_path(fixture)).unwrap();
    find_heading(doc.tree(TreeKind::Tags)).expect("an H1")
}

/// Where the selection's outline ends, on screen: the page rectangle's bottom edge is page
/// `y0`, the zoom maps points to pixels.
fn outline_bottom(harness: &Harness<'_, PdfxrayApp>) -> f32 {
    let state = &harness.state().state;
    let rect = state.selection_rect().expect("a selection with a box").rect;
    let page = page_rect(harness);
    let zoom = state.canvas.zoom;
    page.max.y - rect.y0 as f32 * zoom
}

#[test]
fn the_selection_gets_a_bar_under_its_outline_with_the_five_actions() {
    let fixture = "tagged-simple";
    let mut harness = opened(fixture);
    // Nothing is selected: no bar. (The document node is selected until the user picks one.)
    harness.state_mut().state.clear_selection();
    settle(&mut harness);
    assert_eq!(bar(&harness), Vec::<String>::new());

    harness.state_mut().state.select(heading(fixture));
    settle(&mut harness);

    assert_eq!(
        bar(&harness),
        ["Isolate", "Hide", "Show in tree", "Object", "Copy id"]
    );
    // Under the outline, not over it.
    let under = outline_bottom(&harness);
    for name in bar(&harness) {
        let top = bar_button(&harness, &name).rect().min.y;
        assert!(top >= under - 1.0, "{name} at {top}, the outline ends at {under}");
    }
    // Isolate shows its name as in the design; the others are icons with the name as tooltip.
    assert!(!is_selected(&bar_button(&harness, "Isolate")));
    // Smoke: the selection is still the heading, the bar changed nothing.
    assert_eq!(harness.state().state.selection.primary, Some(heading(fixture)));
}

#[test]
fn isolate_from_the_bar_isolates_the_selection_and_a_second_click_brings_it_back() {
    let fixture = "tagged-simple";
    let mut harness = opened(fixture);
    let node = heading(fixture);
    harness.state_mut().state.select(node.clone());
    settle(&mut harness);

    bar_button(&harness, "Isolate").click();
    settle(&mut harness);
    wait_for_render(&mut harness);

    let isolation = harness.state().state.canvas.view.isolation.clone().unwrap();
    assert_eq!((isolation.mode, isolation.node), (IsolateMode::Only, node));
    // The bar and the toolbar button show it is on.
    assert!(is_selected(&bar_button(&harness, "Isolate")));
    assert!(is_selected(&view_tools::toolbar_button(&harness, "Isolate")));

    bar_button(&harness, "Isolate").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.canvas.view.isolation, None);
    assert_eq!(view_tools::trigger(&harness), "View");
}

#[test]
fn the_other_bar_actions_show_the_node_in_its_tree_open_its_object_and_copy_its_id() {
    let fixture = "tagged-simple";
    let mut harness = opened(fixture);
    let node = heading(fixture);
    harness.state_mut().state.select(node.clone());
    settle(&mut harness);

    the(&harness, Region::Center, Role::Button, "Copy id").click();
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
    assert_eq!(copied, Some(node.to_string()));

    the(&harness, Region::Center, Role::Button, "Object").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.nav.rail, RailItem::Objects);
    let NodeId::StructElem(ObjRef { .. }) = node else {
        panic!("a structure element");
    };
    assert!(matches!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(_))
    ));

    // Back on the page, a node that has no object of its own does not offer one.
    harness.state_mut().state.set_rail(RailItem::Tags);
    harness
        .state_mut()
        .state
        .select(NodeId::MarkedContent { page: 0, mcid: 0 });
    settle(&mut harness);
    assert_eq!(bar(&harness), ["Isolate", "Hide", "Show in tree", "Copy id"]);
    the(&harness, Region::Center, Role::Button, "Show in tree").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.nav.rail, RailItem::Tags);
}

/// Right-clicks at `pos` (screen).
fn right_click(harness: &mut Harness<'_, PdfxrayApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(3);
    }
}

#[test]
fn right_click_on_the_canvas_selects_the_node_under_the_pointer_and_opens_its_actions() {
    let mut harness = opened("minimal-one-page");
    let page = page_rect(&harness);
    let zoom = harness.state().state.canvas.zoom;
    // The "H" of the page: where the glyph is, 46 points above the bottom of a 100-point page.
    let on_glyph = page.min + vec2(30.0 * zoom, (100.0 - 46.0) * zoom);

    right_click(&mut harness, on_glyph);

    // The topmost thing under the pointer is selected, as for a left click.
    let top = harness
        .state()
        .state
        .drawer
        .hit_stack
        .as_ref()
        .and_then(|stack| stack.entries.first())
        .map(|entry| entry.id.clone())
        .expect("something under the pointer");
    assert_eq!(harness.state().state.selection.primary, Some(top.clone()));
    // Its actions are in the menu, in the order of the bar, with the ones the bar leaves out.
    let items = texts_in(&harness, Region::Center, Role::MenuItem);
    assert_eq!(items.first().map(String::as_str), Some("Isolate"));
    assert!(items.contains(&"Copy id".to_owned()), "{items:?}");
    assert!(items.contains(&"Copy JSON".to_owned()), "{items:?}");

    the(&harness, Region::Center, Role::MenuItem, "Isolate").click();
    settle(&mut harness);

    assert_eq!(
        harness
            .state()
            .state
            .canvas
            .view
            .isolation
            .as_ref()
            .map(|i| (i.mode, i.node.clone())),
        Some((IsolateMode::Only, top))
    );
    // Smoke: the menu closed with the click, and a left click elsewhere does not reopen it.
    assert_eq!(
        texts_in(&harness, Region::Center, Role::MenuItem),
        Vec::<String>::new()
    );
    click_at(&mut harness, page.min + vec2(2.0, 2.0));
    assert_eq!(
        texts_in(&harness, Region::Center, Role::MenuItem),
        Vec::<String>::new()
    );
}

#[test]
fn a_node_the_inspector_cannot_show_does_not_offer_copy_json_and_a_stream_offers_its_bytes() {
    use pdfxray_app::actions::Action;
    let mut harness = opened("minimal-one-page");
    let offered =
        |harness: &Harness<'_, PdfxrayApp>, id: &NodeId| Action::for_node(&harness.state().state, id);

    // A page that does not exist, a diagnostic that was never reported and a group have no
    // properties to copy; a real page has.
    assert!(!offered(&harness, &NodeId::Page(9)).contains(&Action::CopyJson));
    assert!(!offered(&harness, &NodeId::Diagnostic(99)).contains(&Action::CopyJson));
    assert!(!offered(&harness, &NodeId::Group(0)).contains(&Action::CopyJson));
    assert!(offered(&harness, &NodeId::Page(0)).contains(&Action::CopyJson));

    // Stream bytes are offered for the stream the object view shows, and for nothing else.
    let stream = NodeId::Object(ObjRef::new(4, 0));
    assert!(!offered(&harness, &stream).contains(&Action::StreamBytes));
    harness.state_mut().state.set_rail(RailItem::Objects);
    harness.state_mut().state.select(stream.clone());
    settle(&mut harness);
    assert!(offered(&harness, &stream).contains(&Action::StreamBytes));
    // Smoke: the dictionary object next to it is no stream.
    harness
        .state_mut()
        .state
        .select(NodeId::Object(ObjRef::new(3, 0)));
    settle(&mut harness);
    assert!(!offered(&harness, &NodeId::Object(ObjRef::new(3, 0))).contains(&Action::StreamBytes));
}

#[test]
fn the_validation_entry_keeps_the_bar_off_its_failure_badges() {
    let mut harness = common::app();
    common::open_fixture(&mut harness, "struct-rich");
    harness
        .state_mut()
        .state
        .select(pdfxray_core::NodeId::StructElem(pdfxray_core::ObjRef::new(14, 0)));
    harness.run_steps(3);
    assert!(
        bar(&harness).contains(&"Isolate".to_owned()),
        "the bar shows in Tags"
    );

    common::click_rail(&mut harness, "Validation");

    // The same selection, no bar: the badges stay clickable.
    assert_eq!(bar(&harness), Vec::<String>::new());
    assert!(harness.state().state.selection.primary.is_some());
}

#[test]
fn the_bar_keeps_clear_of_the_paint_order_stepper_at_the_bottom_of_the_canvas() {
    let mut harness = opened("graphics-state");
    // A short canvas: the bar under the page's outline lands where the floating stepper is.
    harness.state_mut().state.drawer.height = 567.0;
    harness.state_mut().state.drawer.open = true;
    harness.state_mut().state.select(NodeId::Page(0));
    harness.state_mut().state.set_stepper(true);
    settle(&mut harness);
    wait_for_render(&mut harness);
    settle(&mut harness);

    let stepper = the(&harness, Region::Center, Role::Button, "Previous operator").rect();
    let buttons = bar(&harness);
    assert_eq!(buttons, ["Isolate", "Hide", "Show in tree", "Copy id"]);
    for name in &buttons {
        let rect = bar_button(&harness, name).rect();
        // The stepper's card has 10 px around its buttons; the bar sits above all of it.
        assert!(
            rect.max.y + 4.0 <= stepper.min.y - 10.0,
            "{name} at {rect:?} over the stepper at {stepper:?}"
        );
    }
    // Smoke: the bar still works from there.
    bar_button(&harness, "Isolate").click();
    settle(&mut harness);
    assert!(harness.state().state.canvas.view.isolation.is_some());
}

#[test]
fn a_selected_glyph_gets_its_bar_under_the_baseline_tag_not_over_it() {
    let mut harness = opened("minimal-one-page");
    harness.state_mut().state.select(NodeId::Glyph {
        page: 0,
        op: 3,
        glyph: 0,
    });
    settle(&mut harness);

    // The pink `baseline y …` tag hangs 4 px under the glyph's baseline and is about 16 px tall:
    // the bar starts under it (its own gap and padding are 8 and 4 px more).
    let under = outline_bottom(&harness);
    let isolate = bar_button(&harness, "Isolate").rect();
    assert!(
        isolate.min.y >= under + 8.0 + 4.0 + 16.0,
        "the bar starts at {}, the glyph ends at {under}",
        isolate.min.y
    );
    // Smoke: it is the full bar, under the glyph and not far from it.
    assert_eq!(bar(&harness), ["Isolate", "Hide", "Show in tree", "Copy id"]);
    assert!(isolate.min.y < under + 60.0);
}
