//! Pixel snapshots of the app with a document open. The four states without a workspace are in `snapshots_states.rs`, the
//! components in `components/snapshot.rs`.
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test snapshots`.

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
use common::{Region, builder, click_at, fixture_path, open_fixture, the, wait_for_render, wait_until};
use egui::{Role, vec2};
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_core::{Document, NodeId, TreeKind, TreeNode};

#[test]
fn snapshot_minimal_opened() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "minimal-one-page");

    wait_for_render(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_minimal_opened");
}

/// The canvas toolbar's Pick menu, open with Tags picked: the modes with their keys, the current
/// one highlighted, annotations greyed out until v1.1.
#[test]
fn snapshot_pick_menu() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "struct-rich");
    harness.state_mut().state.canvas.layer_mode = pdfxray_core::LayerMode::Tags;
    harness.run();
    the(&harness, Region::Center, Role::Button, "Pick: Tags").click();
    harness.run();
    assert!(harness.query_by_label("Annotations").is_some());

    wait_for_render(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_pick_menu");
}

#[test]
fn snapshot_inspector_with_a_page_selected() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.select(NodeId::Page(0));
    harness.run();

    wait_for_render(&mut harness);
    harness.snapshot("app_inspector_page");
}

/// The Validation entry of design frame 05, on the two-page `struct-rich` fixture with the
/// hand-edited report of `tests/data/struct-rich-ua1.xml`: findings by rule, the failure badges
/// and the "On this page" card, the drawer table and the inspector of the first Figure finding.
#[test]
fn snapshot_validation_findings() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    let mut fake = FakeValidator::new(pdfxray_app::Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report = common::fake::struct_rich_report();
    fake.run_gate.open();
    harness.state_mut().state.validator = std::sync::Arc::new(fake);
    open_fixture(&mut harness, "struct-rich");
    the(&harness, Region::Chrome, Role::Button, "Validate").click();
    wait_until(&mut harness, "the report", |s| {
        s.document.as_ref().is_some_and(|d| d.doc.validation().is_some()) && !s.validation.is_running()
    });
    the(&harness, Region::Navigator, Role::Button, "H1 12 0 R p1").click();
    harness.run();
    wait_for_render(&mut harness);
    // The drawer's note says when the run was and how long it took: fixed for the picture.
    if let Some(last) = harness.state_mut().state.validation.last.as_mut() {
        last.finished = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(12))
            .unwrap();
        last.took = std::time::Duration::from_millis(800);
    }
    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_validation_findings");
}

#[test]
fn snapshot_measure_tool_with_a_line() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "minimal-one-page");
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    the(&harness, Region::Center, Role::Button, "Measure").click();
    harness.run();
    let origin = harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .rect()
        .min;
    click_at(&mut harness, origin + vec2(20.0, 20.0));
    click_at(&mut harness, origin + vec2(50.0, 60.0));
    harness.remove_cursor();
    harness.run();

    wait_for_render(&mut harness);
    harness.snapshot("app_measure_active");
}

#[test]
fn snapshot_page_boxes_overlay_on_a_rotated_page() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "page-boxes");
    the(&harness, Region::Center, Role::Button, "Page boxes").click();
    harness.run();

    wait_for_render(&mut harness);
    harness.snapshot("app_page_boxes_overlay");
}

#[test]
fn snapshot_page_boxes_overlay_with_defaulted_boxes() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.go_to_page(2);
    harness.state_mut().state.canvas.show_boxes = true;
    harness.run();

    wait_for_render(&mut harness);
    harness.snapshot("app_page_boxes_defaults");
}

/// Turns a mode on from the View menu and waits until its bitmap is on the canvas.
fn press(harness: &mut egui_kittest::Harness<'_, PdfxrayApp>, label: &str) {
    view_tools::toggle(harness, label);
    wait_for_render(harness);
}

#[test]
fn snapshot_xray_with_legend() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "graphics-state");
    press(&mut harness, "X-ray");

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_xray_graphics_state");
}

/// Design frame 06, on `graphics-state` at op 49 (the orange fill the stroked square at op 53
/// covers): the Paint order tab with its cards, the ghosts of what is not painted yet, the
/// legend and the inspector's Paint order section. "Skip state-only ops" is on, as in the frame.
#[test]
fn snapshot_paint_order() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "graphics-state");
    common::click_rail(&mut harness, "Content");
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 49 });
    harness.run_steps(4);
    press(&mut harness, "Paint order");
    the(&harness, Region::Drawer, Role::Switch, "Skip state-only ops").click();
    harness.run_steps(4);
    wait_for_render(&mut harness);

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_paint_order");
}

#[test]
fn snapshot_isolating_a_tag() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "tagged-simple");
    let doc = Document::open(fixture_path("tagged-simple")).unwrap();
    let heading = find_tag(doc.tree(TreeKind::Tags), "H1").expect("an H1 element");
    harness.state_mut().state.select(heading);
    harness.run_steps(4);
    press(&mut harness, "Isolate");

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_isolate_tag");
}

fn find_tag(nodes: Vec<TreeNode>, label: &str) -> Option<NodeId> {
    nodes.into_iter().find_map(|node| {
        if node.label == label {
            Some(node.id)
        } else {
            find_tag(node.children, label)
        }
    })
}
