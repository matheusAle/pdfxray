//! Memory budget of the app's render threads (lazy-loading plan §4.1): the page, thumbnail and
//! preview threads share the UI's document, so having all three running costs little more than
//! the document itself.
//!
//! Resident size is meaningless in a debug build and noisy next to other tests, so the test is
//! `#[ignore]`d and measures in child processes. Run with
//! `cargo test --release -p pdfxray-app --test perf -- --ignored --nocapture`; CI runs it in its
//! `perf` job.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr)]

mod common;
#[path = "../../pdfxray-core/tests/common/mod.rs"]
mod synthetic;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use pdfxray_app::{AppState, RailItem};
use pdfxray_core::{Document, NodeId, ObjRef};

/// Pages of the synthetic document (the one of the core's memory budget).
const PAGES: u32 = 200;
/// What the app may hold for one open document with every render thread running, as a multiple
/// of what the document alone holds. Measured 1.15–1.17× (the rest is the app's own state of the
/// document); before the threads shared the document each opened its own copy and it measured
/// 4.04× (see `docs/spec/lazy-loading-plan.md` §4.1).
const APP_DOCS_RSS: f64 = 1.25;
/// The synthetic document's image `XObject` (see `synthetic`: object 7), previewed by the preview
/// thread.
const IMAGE: ObjRef = ObjRef::new(7, 0);

/// Resident size of this process in MiB, from `ps` (no `unsafe`, no extra dependency).
fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    let kb: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0.0);
    kb / 1024.0
}

/// The synthetic document, written once to a file the app can open.
fn synthetic_path() -> PathBuf {
    let dir = std::env::temp_dir().join("pdfxray-app-perf");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("synthetic-{PAGES}.pdf"));
    let bytes = synthetic::synthetic_pdf(PAGES);
    if std::fs::read(&path).ok().as_deref() != Some(&bytes[..]) {
        std::fs::write(&path, bytes).unwrap();
    }
    path
}

/// Runs probe `name` of this binary in a child process and reads the number it prints.
fn probe(name: &str) -> f64 {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", name, "--nocapture"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("grew_mb="))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "no grew_mb line in {name}'s output:\n{stdout}\n{}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
}

#[test]
#[ignore = "memory budget; run with --release -- --ignored"]
fn render_threads_share_the_document() {
    let alone = probe("document_alone_probe");
    let in_app = probe("app_with_every_render_thread_probe");
    let ratio = in_app / alone;
    eprintln!(
        "synthetic document, {PAGES} pages: alone {alone:.0} MiB, in the app with every render \
         thread running {in_app:.0} MiB ({ratio:.2}x)"
    );

    assert!(alone > 100.0, "the document alone grew only {alone:.0} MiB");
    assert!(
        ratio <= APP_DOCS_RSS,
        "the app holds {ratio:.2}x one document, budget {APP_DOCS_RSS}x"
    );
}

/// Opens the synthetic document with the core alone and prints how much the process grew.
#[test]
#[ignore = "helper of render_threads_share_the_document"]
fn document_alone_probe() {
    let path = synthetic_path();
    let before = rss_mb();
    let doc = Document::open(&path).unwrap();
    assert_eq!(doc.page_count(), PAGES as usize);
    println!("grew_mb={}", rss_mb() - before);
}

/// Opens the synthetic document in the app, has the page, thumbnail and preview threads each
/// render something, and prints how much the process grew since before the file was opened.
#[test]
#[ignore = "helper of render_threads_share_the_document"]
fn app_with_every_render_thread_probe() {
    let mut harness = common::app();
    let before = rss_mb();
    let path = synthetic_path();
    harness.state_mut().state.picker = Box::new(move || Some(path.clone()));
    harness.get_by_label("Open file…").click();
    step_until(&mut harness, "the file to open", |s| {
        s.opening.job().is_none() && s.document.is_some()
    });
    step_until(&mut harness, "page 1 on the canvas", |s| {
        s.shown_digest().is_some()
    });
    harness.state_mut().state.set_rail(RailItem::Pages);
    step_until(&mut harness, "a thumbnail", |s| s.thumbnails_rendered() > 0);
    harness.state_mut().state.set_rail(RailItem::Objects);
    harness.state_mut().state.select(NodeId::Object(IMAGE));
    step_until(&mut harness, "the image preview", |s| {
        s.preview_digest(IMAGE).is_some()
    });
    println!("grew_mb={}", rss_mb() - before);
}

/// Steps the app until `done` holds, for at most a minute.
fn step_until(
    harness: &mut egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>,
    what: &str,
    done: impl Fn(&AppState) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done(&harness.state().state) {
        assert!(Instant::now() < deadline, "{what} did not happen within a minute");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}
