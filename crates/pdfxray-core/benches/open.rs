//! Cost of opening a document: the raw parse, page capture and the whole eager pipeline.
//!
//! `cargo bench -p pdfxray-core --bench open -- --quick` for a fast pass.

// Benchmarks are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them.
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hayro_syntax::Pdf;
use pdfxray_core::Document;
use pdfxray_core::Point;
use pdfxray_core::capture::{PageIndex, capture_document, capture_page};
use pdfxray_fixtures::fixtures_dir;

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

fn open_documents(c: &mut Criterion) {
    let mut group = c.benchmark_group("open");
    group.measurement_time(Duration::from_secs(8));

    let long_doc = std::fs::read(pdfxray_fixtures::pdf_path("long-doc")).unwrap();
    group.bench_function("long-doc (40 pages)", |b| {
        b.iter_batched(
            || long_doc.clone(),
            |bytes| Document::from_bytes("long-doc.pdf", black_box(bytes)).unwrap(),
            BatchSize::LargeInput,
        );
    });

    let root = fixtures_dir()
        .parent()
        .unwrap()
        .join("third_party")
        .join("verapdf");
    let mut files = Vec::new();
    pdfs_below(&root, &mut files);
    files.sort();
    let corpus: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(f).unwrap()).collect();
    group.bench_function(format!("verapdf corpus ({} files)", corpus.len()), |b| {
        b.iter(|| {
            for bytes in &corpus {
                // Many corpus files are deliberately broken; opening may fail and that is fine.
                drop(Document::from_bytes("x.pdf", black_box(bytes.clone())));
            }
        });
    });

    for pages in [50, 200] {
        let bytes = common::synthetic_pdf(pages);
        group.sample_size(10);
        group.bench_function(format!("synthetic {pages} pages"), |b| {
            b.iter_batched(
                || bytes.clone(),
                |bytes| Document::from_bytes("synthetic.pdf", black_box(bytes)).unwrap(),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

/// The pipeline stages that dominate opening, measured on their own.
fn stages(c: &mut Criterion) {
    let mut group = c.benchmark_group("stages");
    group.sample_size(10);
    let bytes = common::synthetic_pdf(50);
    let pdf = Pdf::new(bytes).unwrap();

    group.bench_function("capture_document (50 pages)", |b| {
        b.iter(|| capture_document(black_box(&pdf)));
    });
    group.bench_function("capture_page (1 page)", |b| {
        b.iter(|| capture_page(black_box(&pdf), 25));
    });

    let page = capture_page(&pdf, 25).unwrap();
    group.bench_function(format!("PageIndex::new ({} items)", page.items.len()), |b| {
        b.iter(|| PageIndex::new(black_box(&page)));
    });
    let index = PageIndex::new(&page);
    group.bench_function("PageIndex::hit", |b| {
        b.iter(|| index.hit(black_box(Point::new(40.0, 723.0))));
    });

    // A page with 20 000 painted items: the index must not be quadratic in the item count.
    let dense = Pdf::new(common::dense_page_pdf(20_000)).unwrap();
    let dense_page = capture_page(&dense, 0).unwrap();
    group.bench_function(
        format!("PageIndex::new dense ({} items)", dense_page.items.len()),
        |b| {
            b.iter(|| PageIndex::new(black_box(&dense_page)));
        },
    );
    group.bench_function("capture_page dense (20k MCIDs)", |b| {
        b.iter(|| capture_page(black_box(&dense), 0));
    });
    group.finish();
}

criterion_group!(benches, open_documents, stages);
criterion_main!(benches);
