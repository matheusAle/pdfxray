//! Performance budgets of the eager loading path (plan decision 13).
//!
//! Timing in a debug build is meaningless, so every test is `#[ignore]`d. Run with
//! `cargo test --release -p pdfxray-core --test perf -- --ignored --nocapture`.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr)]

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pdfxray_core::{Document, LayerMode, NodeId, Point, TreeKind};
use pdfxray_fixtures::fixtures_dir;

const LONG_DOC_OPEN: Duration = Duration::from_millis(150);
const SYNTH_OPEN: Duration = Duration::from_secs(2);
const SYNTH_RSS_MB: u64 = 500;
const CORPUS_FILE_OPEN: Duration = Duration::from_millis(200);
const QUERY: Duration = Duration::from_millis(5);
const SYNTH_PAGES: u32 = 200;

/// Records the best of five runs of `f` under `name`.
fn time<T>(timings: &mut Vec<(String, Duration)>, name: &str, mut f: impl FnMut() -> T) {
    let (took, _) = best_of(5, || std::hint::black_box(f()));
    timings.push((name.to_owned(), took));
}

/// Best of `runs` executions of `f`, so a noisy neighbour does not fail the budget.
fn best_of<T>(runs: u32, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best: Option<(Duration, T)> = None;
    for _ in 0..runs {
        let start = Instant::now();
        let out = f();
        let took = start.elapsed();
        if best.as_ref().is_none_or(|(b, _)| took < *b) {
            best = Some((took, out));
        }
    }
    best.expect("runs > 0")
}

/// Peak resident set size of this process in MiB: `VmHWM` where `/proc` has it, else the current
/// size from `ps` (no `unsafe`, no extra dependency).
fn rss_mb() -> u64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(kb) = status
            .lines()
            .find_map(|l| l.strip_prefix("VmHWM:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
    {
        return kb / 1024;
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps");
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0);
    kb / 1024
}

fn pdfs_below(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pdfs_below(&path, out);
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "timing budget; run with --release -- --ignored"]
fn long_doc_opens_within_budget() {
    let bytes = std::fs::read(pdfxray_fixtures::pdf_path("long-doc")).unwrap();
    let (took, doc) = best_of(5, || Document::from_bytes("long-doc.pdf", bytes.clone()).unwrap());
    eprintln!("long-doc.pdf ({} pages): open {took:?}", doc.page_count());

    assert_eq!(doc.page_count(), 40);
    assert!(
        took < LONG_DOC_OPEN,
        "open took {took:?}, budget {LONG_DOC_OPEN:?}"
    );
}

#[test]
#[ignore = "timing budget; run with --release -- --ignored"]
fn every_vendored_verapdf_file_opens_within_budget() {
    let root = fixtures_dir()
        .parent()
        .unwrap()
        .join("third_party")
        .join("verapdf");
    let mut files = Vec::new();
    pdfs_below(&root, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "no vendored veraPDF files under {}",
        root.display()
    );

    let mut slow = Vec::new();
    let mut total = Duration::ZERO;
    let mut slowest = Duration::ZERO;
    for file in &files {
        let bytes = std::fs::read(file).unwrap();
        let (took, _) = best_of(3, || Document::from_bytes("x.pdf", bytes.clone()));
        total += took;
        slowest = slowest.max(took);
        if took >= CORPUS_FILE_OPEN {
            slow.push(format!("{} {took:?}", file.display()));
        }
    }
    eprintln!(
        "{} veraPDF files: total {total:?}, slowest {slowest:?}",
        files.len()
    );

    assert!(slow.is_empty(), "over {CORPUS_FILE_OPEN:?}: {slow:#?}");
}

#[test]
#[ignore = "timing budget; run with --release -- --ignored"]
fn synthetic_200_pages_open_within_budget() {
    let bytes = common::synthetic_pdf(SYNTH_PAGES);
    eprintln!("synthetic document: {} bytes", bytes.len());
    let (took, doc) = best_of(3, || {
        Document::from_bytes("synthetic.pdf", bytes.clone()).unwrap()
    });
    eprintln!("synthetic open: {took:?}");

    assert_eq!(doc.page_count(), SYNTH_PAGES as usize);
    assert!(doc.info().diagnostics.is_empty(), "{:#?}", doc.info().diagnostics);
    assert!(took < SYNTH_OPEN, "open took {took:?}, budget {SYNTH_OPEN:?}");
}

/// Memory is measured in a child process running [`memory_probe`]: the allocator keeps what the
/// other tests freed, so the size of this process says nothing about one open.
#[test]
#[ignore = "memory budget; run with --release -- --ignored"]
fn synthetic_200_pages_memory_within_budget() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "memory_probe", "--nocapture"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let rss: u64 = stdout
        .lines()
        .find_map(|l| l.strip_prefix("rss_mb="))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| panic!("no rss_mb line in the probe's output:\n{stdout}"));
    eprintln!("synthetic document, {SYNTH_PAGES} pages: {rss} MiB resident");

    assert!(rss < SYNTH_RSS_MB, "rss {rss} MiB, budget {SYNTH_RSS_MB} MiB");
}

/// Opens the synthetic document once and prints the resident size; see the test above.
#[test]
#[ignore = "helper of synthetic_200_pages_memory_within_budget"]
fn memory_probe() {
    let bytes = common::synthetic_pdf(SYNTH_PAGES);
    let doc = Document::from_bytes("synthetic.pdf", bytes).unwrap();
    assert_eq!(doc.page_count(), SYNTH_PAGES as usize);
    println!("rss_mb={}", rss_mb());
}

#[test]
#[ignore = "timing budget; run with --release -- --ignored"]
fn synthetic_200_pages_queries_within_budget() {
    let doc = Document::from_bytes("synthetic.pdf", common::synthetic_pdf(SYNTH_PAGES)).unwrap();

    let tags = doc.tree(TreeKind::Tags);
    assert!(tags[0].len() > 20_000, "tag tree has {} nodes", tags[0].len());
    let mid_page = 100;
    // Page 100's Sect and first paragraph (see `common`: 8 objects up front, 102 per page).
    let sect_obj = 8 + 102 * mid_page + 2;
    let sect = NodeId::StructElem(pdfxray_core::ObjRef::new(sect_obj, 0));
    let first_p = NodeId::StructElem(pdfxray_core::ObjRef::new(sect_obj + 2, 0));
    // Inside the first paragraph's text on that page.
    let point = Point::new(40.0, 723.0);
    assert!(
        doc.hit(mid_page, point, LayerMode::All).len() > 4,
        "the probe point must hit content"
    );

    let mut timings: Vec<(String, Duration)> = Vec::new();
    // The first hit on a page builds that page's spatial index; later pages are still cold here.
    let cold = Instant::now();
    let hits = doc.hit(150, point, LayerMode::All);
    timings.push(("hit(All) cold page".to_owned(), cold.elapsed()));
    assert!(hits.len() > 4, "the probe point must hit content on page 150 too");
    time(&mut timings, "tree(Pages)", || doc.tree(TreeKind::Pages));
    time(&mut timings, "tree(Tags)", || doc.tree(TreeKind::Tags));
    time(&mut timings, "tree(Objects)", || doc.tree(TreeKind::Objects));
    time(&mut timings, "tree(Content)", || {
        doc.tree(TreeKind::Content(mid_page))
    });
    time(&mut timings, "tree(Artifacts)", || doc.tree(TreeKind::Artifacts));
    time(&mut timings, "tree(Resources)", || doc.tree(TreeKind::Resources));
    time(&mut timings, "tree(Diagnostics)", || {
        doc.tree(TreeKind::Diagnostics)
    });
    time(&mut timings, "node(StructElem P)", || doc.node(&first_p));
    time(&mut timings, "node(StructElem Sect)", || doc.node(&sect));
    let object = NodeId::Object(pdfxray_core::ObjRef::new(7, 0));
    time(&mut timings, "node(Object)", || doc.node(&object));
    let marked = NodeId::MarkedContent {
        page: mid_page,
        mcid: 5,
    };
    time(&mut timings, "node(MarkedContent)", || doc.node(&marked));
    time(&mut timings, "node(Document)", || doc.node(&NodeId::Document));
    time(&mut timings, "node(Page)", || doc.node(&NodeId::Page(mid_page)));
    time(&mut timings, "hit(All)", || {
        doc.hit(mid_page, point, LayerMode::All)
    });
    time(&mut timings, "hit(Tags)", || {
        doc.hit(mid_page, point, LayerMode::Tags)
    });
    time(&mut timings, "info", || doc.info());
    time(&mut timings, "bbox(StructElem Sect)", || doc.bbox(&sect));
    time(&mut timings, "breadcrumb(StructElem P)", || {
        doc.breadcrumb(&first_p)
    });
    for (name, took) in &timings {
        eprintln!("{name:<24} {took:?}");
    }

    let over: Vec<_> = timings.iter().filter(|(_, t)| *t >= QUERY).collect();
    assert!(over.is_empty(), "queries over {QUERY:?}: {over:#?}");
}

/// A page with 20 000 marked-content sequences, one structure element each under one parent:
/// the shape that exposes anything quadratic in MCIDs per page or children per element.
#[test]
#[ignore = "timing budget; run with --release -- --ignored"]
fn dense_page_stays_linear() {
    let bytes = common::dense_page_pdf(20_000);
    let (took, doc) = best_of(3, || Document::from_bytes("dense.pdf", bytes.clone()).unwrap());
    eprintln!("dense page open: {took:?}");
    assert_eq!(doc.page_count(), 1);
    assert!(doc.info().diagnostics.is_empty(), "{:#?}", doc.info().diagnostics);
    assert!(took < Duration::from_secs(1), "open took {took:?}");

    let root = NodeId::StructElem(pdfxray_core::ObjRef::new(5, 0));
    let (marked, items) = best_of(5, || doc.marked_content(&root));
    let (bbox, _) = best_of(5, || doc.bbox(&root));
    let (props, _) = best_of(5, || doc.node(&root));
    let (tags, _) = best_of(5, || doc.tree(TreeKind::Tags));
    eprintln!("marked_content {marked:?}, bbox {bbox:?}, node {props:?}, tree(Tags) {tags:?}");

    assert_eq!(items.len(), 20_000);
    for (name, took) in [
        ("marked_content", marked),
        ("bbox", bbox),
        ("node", props),
        ("tree(Tags)", tags),
    ] {
        assert!(took < QUERY, "{name} took {took:?}, budget {QUERY:?}");
    }
}
