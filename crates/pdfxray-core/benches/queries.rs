//! Per-frame and per-click queries on a 200-page tagged document (~20k structure elements).
//!
//! `cargo bench -p pdfxray-core --bench queries -- --quick` for a fast pass.

// Benchmarks are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use pdfxray_core::{Document, LayerMode, NodeId, ObjRef, Point, TreeKind};

const PAGES: u32 = 200;
const MID_PAGE: u32 = 100;

fn queries(c: &mut Criterion) {
    let doc = Document::from_bytes("synthetic.pdf", common::synthetic_pdf(PAGES)).unwrap();
    // Page 100's Sect and first paragraph (see `common`: 8 objects up front, 102 per page).
    let sect_obj = 8 + 102 * MID_PAGE + 2;
    let sect = NodeId::StructElem(ObjRef::new(sect_obj, 0));
    let paragraph = NodeId::StructElem(ObjRef::new(sect_obj + 2, 0));
    let point = Point::new(40.0, 723.0);
    let mcid = NodeId::MarkedContent {
        page: MID_PAGE,
        mcid: 5,
    };
    assert!(
        doc.hit(MID_PAGE, point, LayerMode::All).len() > 4,
        "probe must hit content"
    );

    let mut group = c.benchmark_group("tree");
    for (name, kind) in [
        ("pages", TreeKind::Pages),
        ("objects", TreeKind::Objects),
        ("tags", TreeKind::Tags),
        ("content (1 page)", TreeKind::Content(MID_PAGE)),
        ("artifacts", TreeKind::Artifacts),
        ("resources", TreeKind::Resources),
        ("diagnostics", TreeKind::Diagnostics),
    ] {
        group.bench_function(name, |b| b.iter(|| doc.tree(black_box(kind))));
    }
    group.finish();

    let mut group = c.benchmark_group("node");
    for (name, id) in [
        ("document", NodeId::Document),
        ("page", NodeId::Page(MID_PAGE)),
        ("object", NodeId::Object(ObjRef::new(7, 0))),
        ("struct elem (paragraph)", paragraph.clone()),
        ("struct elem (sect)", sect.clone()),
        ("marked content", mcid),
    ] {
        group.bench_function(name, |b| b.iter(|| doc.node(black_box(&id))));
    }
    group.finish();

    let mut group = c.benchmark_group("hit");
    for (name, layer) in [("all", LayerMode::All), ("tags", LayerMode::Tags)] {
        group.bench_function(name, |b| b.iter(|| doc.hit(MID_PAGE, black_box(point), layer)));
    }
    group.finish();

    let mut group = c.benchmark_group("misc");
    group.bench_function("info", |b| b.iter(|| doc.info()));
    group.bench_function("bbox (sect)", |b| b.iter(|| doc.bbox(black_box(&sect))));
    group.bench_function("breadcrumb (paragraph)", |b| {
        b.iter(|| doc.breadcrumb(black_box(&paragraph)));
    });
    group.finish();
}

criterion_group!(benches, queries);
criterion_main!(benches);
