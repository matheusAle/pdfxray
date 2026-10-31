//! Pixel snapshots of the Objects entry: the decoded view, the raw bytes and the revision timeline.
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test objects_snapshots`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Region, builder, click_rail, click_title_chip, named, open_fixture, the};
use egui::Role;
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;

fn open_objects(fixture: &str, object: u32) -> egui_kittest::Harness<'static, PdfxrayApp> {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, fixture);
    click_rail(&mut harness, "Objects");
    the(
        &harness,
        Region::Navigator,
        Role::Button,
        &format!("{object} 0 obj"),
    )
    .click();
    harness.run_steps(4);
    harness
}

#[test]
fn snapshot_objects_stream_decoded() {
    let mut harness = open_objects("text-state", 4);
    harness.remove_cursor();
    harness.run_steps(2);

    harness.snapshot("app_objects_stream_decoded");
}

#[test]
fn snapshot_objects_raw_bytes() {
    let mut harness = open_objects("text-state", 4);
    // The stream's bytes are on the right-click menu of the title.
    named(&harness, Region::Inspector, Role::Label, "Content stream")[0].click_secondary();
    harness.run_steps(3);
    harness
        .get_by_role_and_label(Role::MenuItem, "Stream bytes")
        .click();
    harness.run_steps(4);
    harness.remove_cursor();
    harness.run_steps(2);

    harness.snapshot("app_objects_raw_bytes");
}

#[test]
fn snapshot_rev_popover() {
    let mut harness = open_objects("incremental-update", 4);
    click_title_chip(&harness);
    harness.run_steps(4);
    harness.remove_cursor();
    harness.run_steps(2);

    harness.snapshot("app_rev_popover");
}
