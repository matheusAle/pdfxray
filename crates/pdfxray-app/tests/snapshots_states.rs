//! Pixel snapshots of the four states of design frames 07 to 10: empty, opening, encrypted and
//! broken file.
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test snapshots_states`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use common::{builder, fixture_path, open_fixture, wait_for_open, wait_for_render, wait_until};
use egui_kittest::kittest::Queryable;
use pdfxray_app::open::Observer;
use pdfxray_app::{PdfxrayApp, Recents};
use pdfxray_core::OpenStage;

/// 2026-10-29 12:00:00 UTC: the clock of the pictures, so the times of the recent files read the
/// same on every run.
const NOW: u64 = 1_793_275_200;

fn pinned() -> u64 {
    NOW
}

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfxray-snap-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A copy of the `struct-rich` fixture at `home/relative`, so the file is there.
fn put(home: &Path, relative: &str) -> PathBuf {
    let path = home.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::copy(fixture_path("struct-rich"), &path).unwrap();
    path
}

#[test]
fn snapshot_state_empty() {
    let _gpu = common::gpu_lock();
    let data = scratch("empty-data");
    let home = scratch("empty-home");
    let files = [
        (
            "Documents/reports/annual-report-2025.pdf",
            "annual-report-2025.pdf",
            48,
            true,
            120,
        ),
        ("Downloads/invoice-4471.pdf", "invoice-4471.pdf", 2, false, 86_400),
        (
            "pdfxray/fixtures/generated/struct-rich.pdf",
            "struct-rich.pdf",
            3,
            true,
            2 * 86_400,
        ),
    ];
    let saved: Vec<serde_json::Value> = files
        .iter()
        .map(|(relative, name, pages, tagged, ago)| {
            serde_json::json!({
                "path": put(&home, relative),
                "name": name,
                "pages": pages,
                "tagged": tagged,
                "opened_at": NOW - ago,
            })
        })
        .collect();
    std::fs::write(
        data.join("recents.json"),
        serde_json::json!({ "files": saved }).to_string(),
    )
    .unwrap();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    {
        let state = &mut harness.state_mut().state;
        state.recents = Recents::at(Some(&data)).with_home(Some(home.clone()));
        state.clock = pinned;
    }
    harness.run();

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_state_empty");
    std::fs::remove_dir_all(&data).ok();
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn snapshot_state_opening() {
    let _gpu = common::gpu_lock();
    let held = Arc::new(AtomicBool::new(true));
    let gate = Arc::clone(&held);
    // Held with 30 of the 40 pages captured.
    let observer: Observer = Arc::new(move |stage, done, _| {
        if stage == OpenStage::CaptureContent && done >= 30 {
            while gate.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    let path = fixture_path("long-doc");
    {
        let state = &mut harness.state_mut().state;
        state.picker = Box::new(move || Some(path.clone()));
        state.opening.card_delay = Duration::ZERO;
        state.opening.spinner_turns = false;
        state.opening.observer = Some(observer);
    }
    harness.get_by_label("Open file…").click();
    harness.run();
    wait_until(&mut harness, "30 pages", |state| {
        state
            .open_progress()
            .and_then(|p| p.detail(OpenStage::CaptureContent))
            .is_some_and(|detail| detail.starts_with("30 /"))
    });

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_state_opening");
    held.store(false, Ordering::SeqCst);
    wait_for_open(&mut harness);
}

#[test]
fn snapshot_state_encrypted() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "encrypted-stub");

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_state_encrypted");
}

#[test]
fn snapshot_state_broken() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "broken-xref-offsets");
    harness.get_by_label("View 2 problems").click();
    harness.run();
    wait_for_render(&mut harness);

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_state_broken");
}
