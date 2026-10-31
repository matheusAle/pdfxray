//! The View bar: x-ray, isolate and hide, the paint-order stepper, the clip and the heat map,
//! and the background rendering behind them, end to end.

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
#[path = "common/view_tools.rs"]
mod view_tools;

use common::fake::FakeValidator;
use common::{
    Region, all_label_texts, app, click_at, click_rail, digest, expected_digest, fixture_path, has_label,
    is_selected, open_fixture, texts_in, the, wait_for_render, wait_until,
};
use egui::{Key, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::center::canvas::cards::{COLUMN_W, MARGIN};
use pdfxray_app::center::toolbar;
use pdfxray_app::components::StepTrack;
use pdfxray_app::rulers;
use pdfxray_app::theme::tokens;
use pdfxray_app::{IsolateMode, PdfxrayApp, XRAY_ALPHA, pixel_digest};
use pdfxray_core::{Document, NodeId, TreeKind};
use pdfxray_render::{
    ColorBy, ContentFilter, XrayOptions, alpha_heatmap_overlays, clip_overlays, ops_for, render_page,
    xray_overlays,
};
use pretty_assertions::assert_eq;

const GLYPH_H: NodeId = NodeId::Glyph {
    page: 0,
    op: 3,
    glyph: 0,
};

/// Steps a few frames. `run` would stop at a step limit instead: new widgets ask for a second
/// frame to be styled, and an answer from the render thread adds one more.
fn settle(harness: &mut Harness<'_, PdfxrayApp>) {
    harness.run_steps(4);
}

/// The zoom that leaves room beside every fixture's page for the column of cards: below it the
/// cards are chips.
const ROOMY: f32 = 0.4;

fn opened(fixture: &str) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    harness.state_mut().state.set_zoom(ROOMY);
    settle(&mut harness);
    wait_for_render(&mut harness);
    harness
}

/// Toggles the render mode `label` from the View menu, then waits for the bitmap it asks for.
fn press(harness: &mut Harness<'_, PdfxrayApp>, label: &str) {
    view_tools::toggle(harness, label);
    settle(harness);
    wait_for_render(harness);
}

/// Picks the colouring `label` in the View menu, then waits for the bitmap.
fn colour_by(harness: &mut Harness<'_, PdfxrayApp>, label: &str) {
    view_tools::pick(harness, label);
    settle(harness);
    wait_for_render(harness);
}

/// The x-ray as the app asks for it: the palette at half strength.
fn xray(color_by: ColorBy, include_invisible: bool) -> XrayOptions {
    XrayOptions {
        color_by,
        include_invisible,
        alpha: XRAY_ALPHA,
    }
}

/// Selects the glyph of minimal-one-page the way a user does: click it, pick it in the Hit stack.
fn pick_glyph(harness: &mut Harness<'static, PdfxrayApp>) {
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    let zoom = harness.state().state.canvas.zoom;
    click_at(harness, canvas.min + vec2(30.0 * zoom, (100.0 - 46.0) * zoom));
    the(harness, Region::Drawer, Role::Button, "'H'").click();
    settle(harness);
    assert_eq!(harness.state().state.selection.primary, Some(GLYPH_H));
}

/// The chip that stands for the x-ray legend (`4 kinds`), if it is showing.
fn legend_chip(harness: &Harness<'_, PdfxrayApp>) -> Option<String> {
    texts_in(harness, Region::Center, Role::Label)
        .into_iter()
        .find(|t| t.ends_with(" kinds"))
}

/// Rests the pointer on the legend chip so that it opens, and returns the labels of the card
/// that are legend entries, in order: what follows its title.
fn legend_labels(harness: &mut Harness<'_, PdfxrayApp>) -> Vec<String> {
    if let Some(chip) = legend_chip(harness) {
        let rect = harness.get_by_label(&chip).rect();
        harness.hover_at(egui::pos2(rect.max.x - 4.0, rect.center().y));
        harness.run_steps(4);
    }
    let labels = texts_in(harness, Region::Center, Role::Label);
    let at = labels.iter().position(|t| t == "X-ray").expect("the x-ray card");
    let mut entries = Vec::new();
    for label in &labels[at + 1..] {
        if label == "Heat map" || label.starts_with("step ") {
            break;
        }
        // The counts beside the entries are not entries.
        if label.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        entries.push(label.clone());
    }
    entries
}

#[test]
fn xray_changes_the_texture_and_the_legend_lists_the_kind_palette() {
    let mut harness = opened("minimal-one-page");
    let plain = digest(&harness);
    assert!(legend_chip(&harness).is_none());

    press(&mut harness, "X-ray");

    // The bitmap is the one the renderer makes with the x-ray overlays (at half strength), and it
    // differs.
    assert!(harness.state().state.canvas.view.xray);
    assert!(is_selected(&the(&harness, Region::Center, Role::Button, "X-ray")));
    let xrayed = digest(&harness);
    assert_ne!(xrayed, plain);
    assert_eq!(
        xrayed,
        expected_digest(&harness, "minimal-one-page", ContentFilter::All, |doc| {
            xray_overlays(doc, 0, xray(ColorBy::Kind, false))
        })
    );
    // The legend is a chip that counts its entries; resting the pointer on it lists exactly the
    // Kind palette.
    assert_eq!(legend_chip(&harness).as_deref(), Some("4 kinds"));
    assert_eq!(legend_labels(&mut harness), ["Glyph", "Path", "Image", "Shading"]);
    // The colouring switch of the View menu shows which one is on.
    view_tools::open(&mut harness);
    assert!(is_selected(&the(&harness, Region::Center, Role::Button, "Kind")));
    view_tools::close(&mut harness);
    // The card names the mode; the status bar does not repeat it.
    assert!(
        texts_in(&harness, Region::Chrome, Role::Label)
            .iter()
            .all(|t| !t.contains("X-ray"))
    );
    // Smoke: no other legend entry leaked into the canvas.
    let canvas_labels = texts_in(&harness, Region::Center, Role::Label);
    for other in ["Tagged", "Untagged", "No tag", "Invisible text"] {
        assert!(
            !canvas_labels.iter().any(|t| t == other),
            "{other} in {canvas_labels:?}"
        );
    }

    // Switching the colouring changes the legend and the switch, not the toggle.
    colour_by(&mut harness, "Artifact");
    assert_eq!(harness.state().state.canvas.view.color_by, ColorBy::Artifact);
    assert_eq!(legend_chip(&harness).as_deref(), Some("3 kinds"));
    assert_eq!(legend_labels(&mut harness), ["Artifact", "Tagged", "Untagged"]);
    view_tools::open(&mut harness);
    assert!(is_selected(&the(
        &harness,
        Region::Center,
        Role::Button,
        "Artifact"
    )));
    assert!(!is_selected(&the(&harness, Region::Center, Role::Button, "Kind")));
    view_tools::close(&mut harness);
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "minimal-one-page", ContentFilter::All, |doc| {
            xray_overlays(doc, 0, xray(ColorBy::Artifact, false))
        })
    );

    // The invisible checkbox adds the hidden-item colours.
    view_tools::check(&mut harness, "Invisible items");
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(
        legend_labels(&mut harness),
        [
            "Artifact",
            "Tagged",
            "Untagged",
            "Outside crop box",
            "Invisible text",
            "White on white"
        ]
    );
    assert!(harness.state().state.canvas.view.invisible);

    // Switching it off restores the plain page.
    press(&mut harness, "X-ray");
    assert_eq!(digest(&harness), plain);
    assert!(legend_chip(&harness).is_none());
    assert!(!harness.state().state.canvas.view.xray);
}

#[test]
fn a_long_isolation_title_is_cut_inside_the_card_and_keeps_the_clear_button() {
    let mut harness = opened("minimal-one-page");
    pick_glyph(&mut harness);
    press(&mut harness, "Isolate");
    let short = "Isolated: Glyph \"H\"";
    let room = COLUMN_W - 20.0;
    let short_clear = the(&harness, Region::Center, Role::Button, "Clear isolation").rect();
    // Smoke: a short title is whole and sits left of the button.
    let label = harness.get_by_label(short).rect();
    assert!(label.width() < room - short_clear.width(), "{label:?}");
    assert!(label.max.x <= short_clear.min.x, "{label:?} {short_clear:?}");

    let long = "A very long title of a node that would not fit the card by a long way";
    harness
        .state_mut()
        .state
        .canvas
        .view
        .isolation
        .as_mut()
        .expect("isolated")
        .title = long.to_owned();
    settle(&mut harness);

    let label = harness.get_by_label(&format!("Isolated: {long}")).rect();
    let clear = the(&harness, Region::Center, Role::Button, "Clear isolation").rect();
    assert!(label.width() <= room - clear.width(), "{label:?}");
    assert!(label.max.x <= clear.min.x, "{label:?} {clear:?}");
    // The card keeps its right edge, so the button stays where it was.
    assert!(
        (clear.max.x - short_clear.max.x).abs() < 1.0,
        "{clear:?} {short_clear:?}"
    );
}

#[test]
fn isolating_a_glyph_renders_only_its_operator_and_the_chip_clears_it() {
    let mut harness = opened("minimal-one-page");
    let full = digest(&harness);
    pick_glyph(&mut harness);
    assert!(harness.state().state.can_isolate());

    press(&mut harness, "Isolate");

    // The chip names the node.
    assert_eq!(
        harness
            .state()
            .state
            .canvas
            .view
            .isolation
            .as_ref()
            .map(|i| (i.mode, i.ops.clone())),
        Some((IsolateMode::Only, [3].into_iter().collect()))
    );
    let chips = texts_in(&harness, Region::Center, Role::Label);
    assert!(chips.contains(&"Isolated: Glyph \"H\"".to_owned()), "{chips:?}");
    // The pixels are the filtered render of the operator set core reports.
    let doc = Document::open(fixture_path("minimal-one-page")).unwrap();
    let ops = ops_for(&doc, &GLYPH_H);
    assert_eq!(ops.iter().copied().collect::<Vec<_>>(), [3]);
    let isolated = digest(&harness);
    // The glyph's operator is the only one that paints on this page, so nothing else goes.
    assert_eq!(isolated, full);
    assert_eq!(
        isolated,
        expected_digest(
            &harness,
            "minimal-one-page",
            ContentFilter::Only(ops.clone()),
            |_| Vec::new()
        )
    );

    // Hide is the complement: here the page has nothing else to paint, so it is blank.
    press(&mut harness, "Hide");
    assert!(has_label(&harness, "Hidden: Glyph \"H\""));
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "minimal-one-page", ContentFilter::Hide(ops), |_| {
            Vec::new()
        })
    );
    assert_ne!(digest(&harness), full);

    // The cross on the chip brings everything back.
    the(&harness, Region::Center, Role::Button, "Clear isolation").click();
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(digest(&harness), full);
    assert_eq!(harness.state().state.canvas.view.isolation, None);
    let chips = texts_in(&harness, Region::Center, Role::Label);
    assert!(
        chips
            .iter()
            .all(|t| !t.starts_with("Hidden") && !t.starts_with("Isolated"))
    );
}

#[test]
fn isolate_and_hide_need_content_behind_the_selection() {
    let mut harness = opened("minimal-one-page");
    // The document node has no operators.
    assert!(!harness.state().state.can_isolate());
    for mode in ["Isolate", "Hide"] {
        assert!(
            view_tools::toolbar_button(&harness, mode)
                .accesskit_node()
                .is_disabled(),
            "{mode}"
        );
    }
    view_tools::open(&mut harness);
    // The modes that need no selection are open to everyone.
    assert!(
        !the(&harness, Region::Center, Role::MenuItem, "X-ray")
            .accesskit_node()
            .is_disabled()
    );
    view_tools::close(&mut harness);

    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 3 });
    settle(&mut harness);
    assert!(
        !view_tools::toolbar_button(&harness, "Isolate")
            .accesskit_node()
            .is_disabled()
    );
    // A state-only operator paints nothing but is still an operator of the page.
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 1 });
    settle(&mut harness);
    assert!(harness.state().state.can_isolate());
    // A page box has no content.
    harness.state_mut().state.select(NodeId::Page(0));
    settle(&mut harness);
    assert!(!harness.state().state.can_isolate());
}

#[test]
fn isolating_a_tag_keeps_only_its_marked_content() {
    let mut harness = opened("tagged-simple");
    let full = digest(&harness);
    let doc = Document::open(fixture_path("tagged-simple")).unwrap();
    let heading = doc
        .tree(TreeKind::Tags)
        .into_iter()
        .flat_map(flatten)
        .find(|n| n.label == "H1")
        .expect("an H1 element")
        .id;
    harness.state_mut().state.select(heading.clone());
    settle(&mut harness);

    press(&mut harness, "Isolate");

    let ops = ops_for(&doc, &heading);
    assert_eq!(ops.iter().copied().collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(
        harness
            .state()
            .state
            .canvas
            .view
            .isolation
            .as_ref()
            .map(|i| i.node.clone()),
        Some(heading.clone())
    );
    assert_ne!(digest(&harness), full);
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "tagged-simple", ContentFilter::Only(ops), |_| Vec::new(
        ))
    );
}

fn flatten(node: pdfxray_core::TreeNode) -> Vec<pdfxray_core::TreeNode> {
    let mut all = vec![node.clone()];
    all.extend(node.children.into_iter().flat_map(flatten));
    all
}

#[test]
fn a_validation_finding_isolates_its_target() {
    let fake = FakeValidator::ready();
    let mut harness = app();
    harness.state_mut().state.validator = fake;
    open_fixture(&mut harness, "orphan-mcid");
    click_rail(&mut harness, "Validation");
    wait_until(&mut harness, "veraPDF to be found", |s| {
        matches!(s.validation.environment, pdfxray_app::Environment::Known(_))
    });
    the(&harness, Region::Navigator, Role::Button, "Validate").click();
    wait_until(&mut harness, "the report", |s| {
        s.document.as_ref().is_some_and(|d| d.doc.validation().is_some()) && !s.validation.is_running()
    });
    let finding = NodeId::Validation(3);
    harness.state_mut().state.select(finding);
    settle(&mut harness);
    let target = NodeId::MarkedContent { page: 0, mcid: 1 };
    assert_eq!(harness.state().state.view_target(), Some(target.clone()));
    assert!(harness.state().state.can_isolate());

    press(&mut harness, "Isolate");

    let isolation = harness.state().state.canvas.view.isolation.clone().unwrap();
    assert_eq!(isolation.node, target);
    let doc = Document::open(fixture_path("orphan-mcid")).unwrap();
    assert_eq!(isolation.ops, ops_for(&doc, &target));
    assert!(!isolation.ops.is_empty());
    assert_eq!(
        digest(&harness),
        expected_digest(
            &harness,
            "orphan-mcid",
            ContentFilter::Only(isolation.ops.clone()),
            |_| Vec::new()
        )
    );
}

#[test]
fn isolation_and_the_stepper_belong_to_their_page() {
    let mut harness = opened("long-doc");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 4 });
    settle(&mut harness);
    harness.state_mut().state.isolate_selection(IsolateMode::Only);
    assert_eq!(
        harness
            .state()
            .state
            .canvas
            .view
            .isolation
            .as_ref()
            .map(|i| i.page),
        Some(0)
    );
    harness.state_mut().state.set_stepper(true);
    harness.state_mut().state.step_to(2);
    settle(&mut harness);
    // The isolation card and the stepper both describe page 1.
    let chips = texts_in(&harness, Region::Center, Role::Label);
    assert!(chips.contains(&"Isolated: Tj".to_owned()), "{chips:?}");
    assert!(has_label(&harness, "step 3 / 28"));

    harness.state_mut().state.go_to_page(1);
    settle(&mut harness);
    wait_for_render(&mut harness);

    // The isolation is dropped; the stepper stays on and is kept inside the new page.
    assert_eq!(harness.state().state.canvas.view.isolation, None);
    assert!(harness.state().state.canvas.view.stepper);
    let last = harness.state().state.last_op(1).unwrap();
    assert!(harness.state().state.canvas.view.step <= last);
    assert_eq!(
        harness.state().state.shown_request().unwrap().content,
        ContentFilter::UpTo(harness.state().state.canvas.view.step)
    );
}

#[test]
fn the_stepper_follows_the_track_the_buttons_and_the_arrow_keys() {
    let mut harness = opened("graphics-state");
    let last = harness.state().state.last_op(0).unwrap();
    assert_eq!(last, 79);

    press(&mut harness, "Paint order");

    // It starts after the last operator, which is the full page.
    assert_eq!(harness.state().state.canvas.view.step, last);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: last })
    );
    assert!(has_label(&harness, "step 80 / 80"));

    // A click on the floating stepper's track picks the operator under it.
    let track = the(&harness, Region::Center, Role::Slider, "Paint order position").rect();
    click_at(
        &mut harness,
        egui::pos2(StepTrack::x_of(track, 37, last), track.center().y),
    );
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 37 })
    );
    assert_eq!(
        harness.state().state.shown_request().unwrap().content,
        ContentFilter::UpTo(37)
    );
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "graphics-state", ContentFilter::UpTo(37), |_| Vec::new(
        ))
    );
    // The inspector shows that operator and its graphics state.
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(
        inspector.contains(&"op:0:37 · op #37".to_owned()),
        "{inspector:?}"
    );
    for (key, value) in [
        ("Operator", "re"),
        ("Operands", "10 10 20 20"),
        ("Fill", "#008000 DeviceRGB\u{a0}·\u{a0}rg"),
    ] {
        let at = inspector.iter().position(|t| t == key).expect(key);
        assert_eq!(inspector[at + 1], value, "{key}");
    }
    assert!(has_label(&harness, "step 38 / 80"));

    // Arrow keys with the pointer over the canvas.
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    harness.hover_at(canvas.center());
    harness.key_press(Key::ArrowLeft);
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 36 })
    );
    assert_eq!(
        harness.state().state.shown_request().unwrap().content,
        ContentFilter::UpTo(36)
    );
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    let at = inspector.iter().position(|t| t == "Operator").unwrap();
    assert_eq!(inspector[at + 1], "rg");
    harness.key_press(Key::ArrowRight);
    settle(&mut harness);
    harness.key_press(Key::ArrowRight);
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 38 })
    );
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "graphics-state", ContentFilter::UpTo(38), |_| Vec::new(
        ))
    );

    // The buttons move one step too, and stop at the ends.
    the(&harness, Region::Center, Role::Button, "Previous operator").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 37 })
    );
    the(&harness, Region::Center, Role::Button, "Next operator").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 38 })
    );
    harness.state_mut().state.step_to(0);
    settle(&mut harness);
    assert!(
        the(&harness, Region::Center, Role::Button, "Previous operator")
            .accesskit_node()
            .is_disabled()
    );
    harness.state_mut().state.step_by(-5);
    assert_eq!(harness.state().state.canvas.view.step, 0);
    harness.state_mut().state.step_by(1000);
    assert_eq!(harness.state().state.canvas.view.step, last);

    // Turning it off shows the whole page again but keeps the selection.
    harness.state_mut().state.step_to(10);
    wait_for_render_after_step(&mut harness);
    press(&mut harness, "Paint order");
    assert!(!harness.state().state.canvas.view.stepper);
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "graphics-state", ContentFilter::All, |_| Vec::new())
    );
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 10 })
    );
    assert!(!has_label(&harness, "step 11 / 80"));
}

fn wait_for_render_after_step(harness: &mut Harness<'_, PdfxrayApp>) {
    settle(harness);
    wait_for_render(harness);
}

#[test]
fn the_stepper_starts_at_the_selected_operator_and_outlines_what_it_painted() {
    let mut harness = opened("graphics-state");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 3 });
    settle(&mut harness);

    press(&mut harness, "Paint order");

    assert_eq!(harness.state().state.canvas.view.step, 3);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Op { page: 0, ix: 3 })
    );
    // The red square (op 3, the first fill) is outlined as the selection.
    let doc = Document::open(fixture_path("graphics-state")).unwrap();
    assert_eq!(
        harness.state().state.selection_rect().map(|s| s.rect),
        doc.bbox(&NodeId::Op { page: 0, ix: 3 }).map(|(_, rect)| rect)
    );
    assert!(harness.state().state.selection_rect().is_some());
}

#[test]
fn the_clip_toggle_outlines_the_clip_of_the_selected_operator() {
    let mut harness = opened("graphics-state");
    let plain = digest(&harness);
    // Op 59 (`sh`) runs under the clip set up by `re W n` at ops 56 to 58.
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 59 });
    settle(&mut harness);

    press(&mut harness, "Clip");

    let state = &harness.state().state;
    assert!(state.canvas.view.clip);
    assert_eq!(state.clip_op(), Some(59));
    let request = state.shown_request().unwrap().clone();
    assert_eq!(request.clip_op, Some(59));
    let doc = Document::open(fixture_path("graphics-state")).unwrap();
    assert_eq!(request.spec(&doc).overlays.len(), 1);
    assert_eq!(request.spec(&doc).overlays, clip_overlays(&doc, 0, 59));
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "graphics-state", ContentFilter::All, |doc| {
            clip_overlays(doc, 0, 59)
        })
    );
    assert_ne!(digest(&harness), plain);
    assert!(has_label(&harness, "Clip"));

    // An operator outside any clip has nothing to outline: the page looks plain again.
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 3 });
    settle(&mut harness);
    wait_for_render(&mut harness);
    assert_eq!(
        harness
            .state()
            .state
            .shown_request()
            .unwrap()
            .spec(&doc)
            .overlays
            .len(),
        0
    );
    assert_eq!(digest(&harness), plain);

    // Off again: the request carries no clip.
    press(&mut harness, "Clip");
    assert_eq!(harness.state().state.shown_request().unwrap().clip_op, None);
}

#[test]
fn a_selected_glyph_shows_the_clip_of_its_operator() {
    let mut harness = opened("minimal-one-page");

    harness.state_mut().state.select(GLYPH_H);
    assert_eq!(harness.state().state.clip_op(), Some(3));
    harness.state_mut().state.select(NodeId::Page(0));
    assert_eq!(harness.state().state.clip_op(), None);
    // An operator of another page is not drawn here.
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 2 });
    assert_eq!(harness.state().state.clip_op(), Some(2));
}

#[test]
fn the_alpha_heatmap_tints_items_that_are_not_opaque() {
    let mut harness = opened("graphics-state");
    let plain = digest(&harness);

    press(&mut harness, "Alpha");

    assert!(harness.state().state.canvas.view.heatmap);
    assert!(is_selected(&the(&harness, Region::Center, Role::Button, "Alpha")));
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "graphics-state", ContentFilter::All, |doc| {
            alpha_heatmap_overlays(doc, 0)
        })
    );
    assert_ne!(digest(&harness), plain);
    let labels = texts_in(&harness, Region::Center, Role::Label);
    let at = labels
        .iter()
        .position(|t| t == "Heat map")
        .expect("a heat map legend");
    assert_eq!(
        labels[at + 1..at + 4],
        ["Soft mask", "Blend mode", "Alpha below 1"]
    );
    // The x-ray legend is not there.
    assert!(!labels.iter().any(|t| t == "Legend"));
}

#[test]
fn rendering_never_blocks_the_frame_and_keeps_the_old_bitmap_meanwhile() {
    let mut harness = opened("graphics-state");
    let plain = digest(&harness);
    let renders = harness.state().state.render_count();

    // One frame submits the request; the thread cannot have answered before the next frame polls.
    view_tools::open(&mut harness);
    the(&harness, Region::Center, Role::MenuItem, "X-ray").click();
    harness.step();
    assert!(harness.state().state.canvas.view.xray);
    assert!(harness.state().state.render_pending());
    assert_eq!(harness.state().state.render_count(), renders + 1);
    // The page is still drawn from the previous bitmap while the new one is pending.
    assert_eq!(harness.state().state.shown_digest(), Some(plain));
    harness.get_by_role_and_label(Role::Image, "Page 1 canvas");

    // Bounded wait: frames keep coming until the thread answers.
    let mut frames = 0;
    while harness.state().state.render_pending() {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
        frames += 1;
        assert!(frames < 2000, "the render never arrived");
    }
    harness.run_steps(2);
    assert_ne!(harness.state().state.shown_digest(), Some(plain));
    // Smoke: the status bar stops saying so once the bitmap is in.
    assert!(!has_label(&harness, "Rendering..."));
    // No further render was needed for the answer.
    assert_eq!(harness.state().state.render_count(), renders + 1);
}

#[test]
fn only_the_newest_request_matters_and_the_cache_serves_old_ones() {
    let mut harness = opened("graphics-state");
    let plain = digest(&harness);
    let renders = harness.state().state.render_count();

    // Three changes, one frame each, without waiting for any answer.
    view_tools::open(&mut harness);
    for label in ["X-ray", "Alpha", "Paint order"] {
        the(&harness, Region::Center, Role::MenuItem, label).click();
        harness.step();
        harness.step();
    }
    view_tools::close(&mut harness);
    wait_for_render(&mut harness);

    // The page shows the state the user ended up in.
    let request = harness.state().state.shown_request().unwrap().clone();
    assert!(request.xray.is_some() && request.heatmap);
    assert_eq!(request.content, ContentFilter::UpTo(79));
    let doc = Document::open(fixture_path("graphics-state")).unwrap();
    assert_eq!(
        digest(&harness),
        pixel_digest(&render_page(&doc, 0, &request.spec(&doc)).unwrap().rgba)
    );
    assert!(harness.state().state.render_count() >= renders + 3);

    // Undoing the changes finds the earlier bitmaps in the cache: no new request.
    let after = harness.state().state.render_count();
    for label in ["Paint order", "Alpha", "X-ray"] {
        press(&mut harness, label);
    }
    assert_eq!(digest(&harness), plain);
    assert!(harness.state().state.render_count() <= after + 2);
}

#[test]
fn opening_another_document_starts_a_fresh_render_thread() {
    let mut harness = opened("graphics-state");
    press(&mut harness, "X-ray");
    let xrayed = digest(&harness);

    open_fixture(&mut harness, "minimal-one-page");

    // Toggles survive, the stepper and isolation (per document) do not.
    assert!(harness.state().state.canvas.view.xray);
    assert!(!harness.state().state.canvas.view.stepper);
    assert_eq!(
        digest(&harness),
        expected_digest(&harness, "minimal-one-page", ContentFilter::All, |doc| {
            xray_overlays(doc, 0, xray(ColorBy::Kind, false))
        })
    );
    assert_ne!(digest(&harness), xrayed);
}

#[test]
fn validation_badges_are_a_red_marker_and_other_badges_stay_text() {
    let mut harness = app();
    harness.state_mut().state.validator = FakeValidator::ready();
    open_fixture(&mut harness, "orphan-mcid");
    click_rail(&mut harness, "Objects");
    let markers = |harness: &Harness<'_, PdfxrayApp>| {
        harness
            .query_all_by_role_and_label(Role::Image, "Validation finding")
            .count()
    };
    assert_eq!(markers(&harness), 0);

    click_rail(&mut harness, "Validation");
    wait_until(&mut harness, "veraPDF to be found", |s| {
        matches!(s.validation.environment, pdfxray_app::Environment::Known(_))
    });
    the(&harness, Region::Navigator, Role::Button, "Validate").click();
    wait_until(&mut harness, "the report", |s| {
        s.document.as_ref().is_some_and(|d| d.doc.validation().is_some()) && !s.validation.is_running()
    });
    click_rail(&mut harness, "Objects");

    // Core marks the nodes findings point at; the tree draws a marker, not "[validation]".
    let doc = &harness.state().state.document.as_ref().unwrap().doc;
    let flagged: Vec<String> = doc
        .tree(TreeKind::Resources)
        .into_iter()
        .flat_map(flatten)
        .filter(|n| n.badges.iter().any(|b| b.code == "validation"))
        .map(|n| n.id.to_string())
        .collect();
    assert_eq!(flagged, ["res:3:0:Font:F1"]);
    assert!(!all_label_texts(&harness).iter().any(|t| t == "[validation]"));
    // The Font group is open, so the flagged font shows its marker.
    assert_eq!(markers(&harness), flagged.len());
}

/// The labels of the cards that the heat map and the page boxes make, and of their chips.
const CARD_LABELS: [&str; 7] = [
    "Heat map",
    "Soft mask",
    "Blend mode",
    "Alpha below 1",
    "Page boxes",
    "Media",
    "Crop",
];

fn page_rect(harness: &Harness<'_, PdfxrayApp>) -> egui::Rect {
    harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect()
}

#[test]
fn cards_sit_in_the_top_right_corner_beside_the_page_and_never_over_it() {
    let mut harness = opened("graphics-state");
    press(&mut harness, "Alpha");
    harness.key_press(Key::B);
    settle(&mut harness);

    let page = page_rect(&harness);
    let canvas_right = common::WINDOW.x - tokens::INSPECTOR_W;
    let canvas_top = tokens::TITLE_H + toolbar::HEIGHT + rulers::THICKNESS;
    for label in CARD_LABELS {
        let rect = harness.get_by_role_and_label(Role::Label, label).rect();
        // To the right of the page, inside the canvas, 14 points from its edge and below the ruler.
        assert!(!rect.intersects(page), "{label} {rect:?} over the page {page:?}");
        assert!(rect.min.x >= page.max.x, "{label} {rect:?} not right of {page:?}");
        assert!(rect.max.x <= canvas_right - MARGIN, "{label} {rect:?}");
        assert!(rect.min.y >= canvas_top + MARGIN, "{label} {rect:?}");
    }
    // The cards are stacked in one column: the boxes card is under the heat map card.
    assert!(
        harness
            .get_by_role_and_label(Role::Label, "Page boxes")
            .rect()
            .min
            .y
            > harness
                .get_by_role_and_label(Role::Label, "Alpha below 1")
                .rect()
                .max
                .y
    );
    // Smoke: it is the full card, not the chip.
    assert!(has_label(&harness, "Soft mask"));
    assert!(COLUMN_W + MARGIN < canvas_right - page.max.x);
}

#[test]
fn cards_collapse_to_chips_when_the_page_leaves_no_room_and_open_while_the_pointer_is_on_them() {
    let mut harness = opened("graphics-state");
    press(&mut harness, "Alpha");
    harness.key_press(Key::B);
    harness.state_mut().state.set_zoom(0.9);
    settle(&mut harness);
    wait_for_render(&mut harness);

    // Only the names are left.
    assert!(has_label(&harness, "Heat map"));
    assert!(has_label(&harness, "Page boxes"));
    for hidden in ["Soft mask", "Blend mode", "Alpha below 1", "Media", "Crop"] {
        assert!(!has_label(&harness, hidden), "{hidden} still showing");
    }
    let chip = harness.get_by_role_and_label(Role::Label, "Heat map").rect();
    assert!(chip.max.x <= common::WINDOW.x - tokens::INSPECTOR_W - MARGIN);

    // The pointer on the chip opens it; leaving closes it again.
    harness.hover_at(egui::pos2(chip.max.x - 4.0, chip.center().y));
    harness.run_steps(4);
    assert!(has_label(&harness, "Soft mask"));
    assert!(has_label(&harness, "Alpha below 1"));
    harness.hover_at(page_rect(&harness).center());
    harness.run_steps(4);
    assert!(!has_label(&harness, "Soft mask"));

    // Smoke: with room again they are full without any hover.
    harness.state_mut().state.set_zoom(ROOMY);
    settle(&mut harness);
    assert!(has_label(&harness, "Soft mask"));
}
