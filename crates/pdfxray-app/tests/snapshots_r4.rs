//! Pixel snapshots of the Pages and Measure screens (design frame 04).
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test snapshots_r4`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Region, builder, click_rail, open_fixture, the, wait_for_render, wait_for_thumbnails};
use egui::{Role, vec2};
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_core::{BoxKind, NodeId};

#[test]
fn snapshot_pages_and_boxes() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "page-boxes");
    click_rail(&mut harness, "Pages");
    // The ArtBox is selected; the CropBox outline is hidden to show the eye toggle.
    harness.state_mut().state.select(NodeId::PageBox {
        page: 0,
        kind: BoxKind::Art,
    });
    the(&harness, Region::Navigator, Role::Button, "Hide CropBox").click();
    harness.run_steps(4);
    wait_for_render(&mut harness);
    wait_for_thumbnails(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);

    harness.snapshot("app_pages_boxes");
}

#[test]
fn snapshot_measure_with_a_gap_between_two_nodes() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "graphics-state");
    click_rail(&mut harness, "Pages");
    // The orange square and the green one to its left.
    harness.state_mut().state.select(NodeId::Op { page: 0, ix: 49 });
    harness
        .state_mut()
        .state
        .select_additional(NodeId::Op { page: 0, ix: 38 });
    harness.key_press(egui::Key::M);
    harness.run_steps(4);
    wait_for_render(&mut harness);
    wait_for_thumbnails(&mut harness);

    // Drag a line across the lower left, below the cards, and leave the pointer on the page.
    let page = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    let (from, to) = (page.min + vec2(30.0, 330.0), page.min + vec2(150.0, 300.0));
    harness.hover_at(from);
    harness.drag_at(from);
    harness.hover_at(from + vec2(5.0, 2.0));
    harness.hover_at(to);
    harness.drop_at(to);
    harness.run();
    assert!(harness.state().state.canvas.measure.offset().is_some());
    harness.hover_at(page.min + vec2(page.width() * 0.5, page.height() * 0.62));
    harness.run_steps(3);

    harness.snapshot("app_measure_gap");
}
