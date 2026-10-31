//! Capture over the third-party corpus (`fixtures/third_party`, `fixtures/cache`).
//!
//! The corpus is optional: directories that are absent are skipped, and the test passes
//! trivially when none exists.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do. Float comparisons are against operand values
// echoed verbatim from the content stream (exactly representable), not computed results.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hayro_syntax::Pdf;
use kurbo::{Affine, Point as KPoint};
use pdfxray_core::capture::{Alignment, CapturedPage, PaintKind, capture_document};
use pdfxray_fixtures::fixtures_dir;

/// Every `.pdf` below `dir`, sorted.
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

fn corpus() -> Vec<PathBuf> {
    let root = fixtures_dir();
    let root = root.parent().unwrap_or(&root).to_path_buf();
    let mut files = Vec::new();
    pdfs_below(&root.join("third_party").join("verapdf"), &mut files);
    pdfs_below(&root.join("cache"), &mut files);
    files.sort();
    files
}

/// Captured pages of every corpus file that parses, with the file name.
fn captured() -> Vec<(String, CapturedPage)> {
    let mut pages = Vec::new();
    for file in corpus() {
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let Ok(pdf) = Pdf::new(bytes) else {
            continue;
        };
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        pages.extend(capture_document(&pdf).into_iter().map(|p| (name.clone(), p)));
    }
    pages
}

#[test]
fn most_corpus_pages_align_exactly() {
    let pages = captured();
    if pages.is_empty() {
        return;
    }

    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for (_, page) in &pages {
        if let Alignment::Degraded { reason } = &page.alignment {
            // Group by the reason with its numbers stripped.
            *reasons
                .entry(reason.chars().filter(|c| !c.is_ascii_digit()).collect())
                .or_default() += 1;
        }
    }
    let degraded: usize = reasons.values().sum();
    let exact = pages.len() - degraded;

    // Well-formed files must align: the only sources of degradation are malformed operators,
    // untokenizable streams, engine panics, vertical or lost text positions and forms that
    // inherit fonts they do not define. At least 95 % of the corpus pages must be exact.
    assert!(
        exact * 100 >= pages.len() * 95,
        "{exact} of {} pages exact; degraded by: {reasons:#?}",
        pages.len()
    );
    assert!(exact > 0);
}

#[test]
fn corpus_pages_are_internally_consistent() {
    for (name, page) in captured() {
        for (i, op) in page.ops.iter().enumerate() {
            assert_eq!(op.ix as usize, i, "{name} p{}: op index", page.page);
            assert!(
                op.span.is_some_and(|(s, e)| s <= e),
                "{name} p{} op {i}: span",
                page.page
            );
        }
        let mut counted = 0usize;
        for (i, item) in page.items.iter().enumerate() {
            assert_eq!(item.ix as usize, i, "{name} p{}: item index", page.page);
            let op = &page.ops[item.op as usize];
            let first = op.first_item.map(|f| f as usize);
            assert!(
                first.is_some_and(|f| (f..f + op.item_count as usize).contains(&i)),
                "{name} p{} item {i}: not within op {}'s items",
                page.page,
                item.op
            );
            let b = item.bbox;
            assert!(
                [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()) && b.x0 <= b.x1 && b.y0 <= b.y1,
                "{name} p{} item {i}: box {b:?}",
                page.page
            );
            assert!(item.mc_stack.iter().all(|m| (*m as usize) < page.marked.len()));
            assert!(
                item.xobject_stack
                    .iter()
                    .all(|x| (*x as usize) < page.xobjects.len())
            );
            counted += 1;
        }
        assert_eq!(
            counted,
            page.ops.iter().map(|o| o.item_count as usize).sum::<usize>()
        );
        for seq in &page.marked {
            assert!(matches!(
                page.ops[seq.begin_op as usize].operator.as_str(),
                "BMC" | "BDC"
            ));
            assert_eq!(
                seq.artifact.is_some(),
                seq.tag == "Artifact",
                "{name} p{}",
                page.page
            );
            if let Some(end) = seq.end_op {
                assert_eq!(page.ops[end as usize].operator, "EMC");
                assert!(end > seq.begin_op);
            }
            if let Some(parent) = seq.parent {
                assert!(parent < seq.ix);
            }
        }
    }
}

#[test]
fn exact_pages_place_glyphs_and_images_where_the_state_says() {
    let pages = captured();
    let mut checked_glyphs = 0usize;
    let mut checked_images = 0usize;
    for (name, page) in pages.iter().filter(|(_, p)| p.alignment == Alignment::Exact) {
        for item in &page.items {
            let gs = &item.gstate;
            match &item.kind {
                PaintKind::Glyph {
                    glyph_index: 0,
                    origin,
                    ..
                } if !page.ops[item.op as usize].operator.eq("TJ") => {
                    // First glyph of a plain string: at CTM * Tm * (0, rise).
                    let expect =
                        Affine::new(gs.ctm) * Affine::new(gs.text.tm) * KPoint::new(0.0, gs.text.rise);
                    assert!(
                        (expect.x - origin.x).abs() < 0.05 && (expect.y - origin.y).abs() < 0.05,
                        "{name} p{} op {}: origin {origin:?} vs {expect:?}",
                        page.page,
                        item.op
                    );
                    checked_glyphs += 1;
                }
                PaintKind::Image { .. } => {
                    // The unit square under the CTM.
                    let r = Affine::new(gs.ctm).transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, 1.0, 1.0));
                    let b = item.bbox;
                    assert!(
                        (r.x0 - b.x0).abs() < 0.05
                            && (r.y0 - b.y0).abs() < 0.05
                            && (r.x1 - b.x1).abs() < 0.05
                            && (r.y1 - b.y1).abs() < 0.05,
                        "{name} p{} op {}: image box {b:?} vs {r:?}",
                        page.page,
                        item.op
                    );
                    checked_images += 1;
                }
                _ => {}
            }
        }
    }
    if !pages.is_empty() {
        assert!(
            checked_glyphs > 0 && checked_images > 0,
            "{checked_glyphs} glyphs, {checked_images} images checked"
        );
    }
}
