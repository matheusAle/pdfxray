//! What scoring every page pair of a comparison visually costs (`pdfxray diff --visual`): two
//! renders per page pair, which is why it is opt-in on the CLI and done per page as the canvas
//! draws in the app.
//!
//! Timing in a debug build is meaningless, so the test is `#[ignore]`d. Run with
//! `cargo test --release -p pdfxray-render --test perf -- --ignored --nocapture`.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them.
#![allow(clippy::unwrap_used, clippy::print_stderr)]

#[path = "../../pdfxray-core/tests/common/mod.rs"]
mod synthetic;

use std::time::{Duration, Instant};

use pdfxray_core::Document;
use pdfxray_render::score::{self, Perceptual};
use pdfxray_render::{RenderSpec, page_score, page_scores, render_page};

/// Pages of the synthetic document (the one of the core's pairing budget): 612 x 792 pt, each
/// with a heading, 70 paragraphs, 8 images and 5 small tables.
const PAGES: u32 = 200;
/// Scoring two 200-page documents at scale 2 (400 renders of 1224 x 1584 px), spread over the
/// cores. Measured 400 ms on a 10-core Apple M4 (240 ms at scale 1; 12 ms per page pair
/// sequentially, of which 3.4 ms is the pixel comparison of identical pages); the headroom is
/// for slower CI machines and for pages that differ, whose pixels take the anti-aliasing test.
const SCORE_200_AT_2: Duration = Duration::from_secs(3);

#[test]
#[ignore = "timing budget; run with --release -- --ignored"]
fn scoring_every_page_pair_of_two_200_page_documents_within_budget() {
    let open = || Document::from_bytes("synthetic.pdf", synthetic::synthetic_pdf(PAGES)).unwrap();
    let (a, b) = (open(), open());
    let pairing = pdfxray_core::diff::pair(&a, &b);
    let how = Perceptual::default();

    let mut timings = Vec::new();
    for scale in [1.0, 2.0] {
        let start = Instant::now();
        let scores = page_scores(&a, &b, &pairing, scale, &how);
        let took = start.elapsed();
        eprintln!(
            "page_scores, {} page pairs at scale {scale}: {took:?}",
            scores.len()
        );
        assert_eq!(scores.len(), PAGES as usize);
        assert!(
            scores.iter().all(|s| s
                .visual
                .as_ref()
                .is_some_and(|v| v.as_ref().is_ok_and(|v| v.differing_pixels == 0))),
            "the same document scores zero on every page"
        );
        timings.push(took);
    }
    // The parts: one page pair sequentially, and the pixel comparison alone.
    let start = Instant::now();
    for m in pairing.pages.iter().take(20) {
        drop(page_score(&a, &b, m, 2.0, &how));
    }
    eprintln!(
        "page_score, one page pair at scale 2, sequential: {:?}",
        start.elapsed() / 20
    );
    let page = render_page(&a, 0, &RenderSpec::new(2.0)).unwrap();
    let start = Instant::now();
    let compared = score::score(&page, &page.clone(), &how);
    eprintln!(
        "score alone, {} x {} px: {:?}",
        compared.width,
        compared.height,
        start.elapsed()
    );

    assert!(
        timings[1] < SCORE_200_AT_2,
        "scoring took {:?}, budget {SCORE_200_AT_2:?}",
        timings[1]
    );
}
