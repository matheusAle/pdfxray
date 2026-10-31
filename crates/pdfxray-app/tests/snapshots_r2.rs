//! Pixel snapshots of the Tags and Content screens (design frames 01 and 02).
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test snapshots_r2`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Region, builder, click_rail, fixture_path, open_fixture, the, wait_for_render};
use egui::Role;
use egui_kittest::Harness;
use pdfxray_app::PdfxrayApp;
use pdfxray_core::{Document, NodeId, Point, TreeKind, TreeNode};

fn opened(name: &str) -> Harness<'static, PdfxrayApp> {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, name);
    harness
}

fn settle(harness: &mut Harness<'_, PdfxrayApp>) {
    harness.run_steps(4);
    wait_for_render(harness);
    harness.remove_cursor();
    harness.run_steps(2);
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

#[test]
fn snapshot_tags_with_a_figure_selected() {
    let mut harness = opened("struct-rich");
    click_rail(&mut harness, "Tags");
    harness.key_press(egui::Key::X);
    harness.run_steps(2);
    // Click the figure on the canvas for the hit stack, then select its element.
    harness
        .state_mut()
        .state
        .click_page_point(Point::new(60.0, 270.0));
    let doc = Document::open(fixture_path("struct-rich")).unwrap();
    let figure = find_tag(doc.tree(TreeKind::Tags), "Figure").expect("a Figure element");
    harness.state_mut().state.select(figure);
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    settle(&mut harness);
    harness.snapshot("app_tags_struct_rich");
}

#[test]
fn snapshot_content_with_a_glyph_selected() {
    let mut harness = opened("text-state");
    click_rail(&mut harness, "Content");
    harness.key_press(egui::Key::X);
    let state = &mut harness.state_mut().state;
    let op = state
        .document
        .as_ref()
        .and_then(|l| l.doc.captured(0))
        .and_then(|c| c.ops.iter().find(|o| o.operator == "Tj" || o.operator == "TJ"))
        .map(|o| o.ix)
        .expect("a text operator");
    state.select(NodeId::Glyph {
        page: 0,
        op,
        glyph: 0,
    });
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    settle(&mut harness);
    harness.snapshot("app_content_glyph_text_state");
}

#[test]
fn snapshot_artifacts_filter() {
    let mut harness = opened("artifacts");
    click_rail(&mut harness, "Artifacts");
    settle(&mut harness);
    harness.snapshot("app_artifacts_filter");
}
