//! The visual score of page pairs over the fixture pairs: what anti-aliasing detection ignores,
//! how the score and the structural residue answer different questions, and that every pair
//! scores where its declared mutation changed ink and nowhere else.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use pdfxray_core::Document;
use pdfxray_core::diff::Pairing;
use pdfxray_render::composite::{self, Comparator, Fit, Mode, Request};
use pdfxray_render::score::{self, PageScore, Perceptual, VisualScore};
use pdfxray_render::{PixelGrid, RenderSpec, page_scores, render_page};
use pretty_assertions::assert_eq;

/// The CLI's render scale, and a retina canvas at 100 %.
const SCALE: f32 = 2.0;

fn pair(name: &str) -> (Document, Document) {
    let (a, b) = pdfxray_fixtures::pairs::pair(name);
    (Document::open(a).unwrap(), Document::open(b).unwrap())
}

fn scores(a: &Document, b: &Document) -> (Pairing, Vec<PageScore>) {
    let pairing = pdfxray_core::diff::pair(a, b);
    let scores = page_scores(a, b, &pairing, SCALE, &Perceptual::default());
    (pairing, scores)
}

fn visual(s: &PageScore) -> VisualScore {
    *s.visual.as_ref().unwrap().as_ref().unwrap()
}

fn including_aa() -> Perceptual {
    Perceptual {
        include_aa: true,
        ..Perceptual::default()
    }
}

#[test]
fn two_renders_of_one_page_a_tenth_of_a_pixel_apart_differ_only_by_anti_aliasing() {
    let (a, _) = pair("drop-alt");
    let plain = render_page(&a, 0, &RenderSpec::new(SCALE)).unwrap();
    let again = render_page(&a, 0, &RenderSpec::new(SCALE)).unwrap();
    let nudged = render_page(
        &a,
        0,
        &RenderSpec {
            grid: Some(PixelGrid {
                offset: (0.1, 0.1),
                ..PixelGrid::of(&plain)
            }),
            ..RenderSpec::new(SCALE)
        },
    )
    .unwrap();

    let ignored = score::score(&plain, &nudged, &Perceptual::default());
    let included = score::score(&plain, &nudged, &including_aa());

    // Every glyph edge moved: well over a thousand pixels beyond the threshold, nearly all of
    // them recognised as anti-aliasing (measured: 87 of 1 412 are not, the darkest pixels of
    // strokes too thin to have an interior, which the heuristic deliberately keeps).
    assert!(included.differing_pixels > 1000, "{included:?}");
    assert!(
        ignored.differing_pixels * 10 < included.differing_pixels,
        "{ignored:?}"
    );
    assert!(ignored.ratio() < 0.000_5, "{}", ignored.ratio());
    assert_eq!(ignored.anti_aliased, included.anti_aliased);
    assert_eq!(
        ignored.differing_pixels + ignored.anti_aliased,
        included.differing_pixels
    );
    // Smoke: the same page drawn twice on one grid is identical, AA counted or not.
    let same = score::score(&plain, &again, &including_aa());
    assert_eq!((same.differing_pixels, same.anti_aliased), (0, 0));
}

#[test]
fn a_changed_page_number_is_many_residue_items_and_a_low_but_clear_score() {
    let (a, b) = pair("artifact-page-number");

    let (pairing, scores) = scores(&a, &b);

    let first = &pairing.pages[0];
    let v = visual(&scores[0]);
    // Structurally the footer's every glyph differs ...
    assert_eq!((first.residue.only_a, first.residue.only_b), (6, 5));
    // ... visually it is a few dozen pixels of a page: low, but clearly not zero.
    assert!(v.differing_pixels > 50, "{v:?}");
    assert!(v.ratio() < 0.001, "{}", v.ratio());
    assert!(!v.exceeds(), "with no limit set there is nothing to exceed");
    // `Page 1` became `- 1 -`: more ink removed than added.
    assert!(v.ink_removed > v.ink_added, "{v:?}");
    // Smoke: every page pair says the same.
    assert!(
        scores
            .iter()
            .map(visual)
            .all(|v| v.differing_pixels > 50 && v.ratio() < 0.001)
    );
}

/// Drop-alt's A with its figures' grey `0.25 g` repainted `0.75 g`: the same bytes but for one
/// digit, so the cross-reference table still holds and nothing but a colour changes.
fn recoloured() -> (Document, Document) {
    let (path, _) = pdfxray_fixtures::pairs::pair("drop-alt");
    let bytes = std::fs::read(path).unwrap();
    let recoloured = replace_all(&bytes, b"0.25 g", b"0.75 g");
    assert_ne!(recoloured, bytes, "the fixture paints its figures in 0.25 g");
    (
        Document::from_bytes("a.pdf", bytes).unwrap(),
        Document::from_bytes("b.pdf", recoloured).unwrap(),
    )
}

fn replace_all(haystack: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len(), "offsets must not move");
    let mut out = haystack.to_vec();
    let mut at = 0;
    while let Some(found) = out[at..].windows(from.len()).position(|w| w == from) {
        out[at + found..at + found + to.len()].copy_from_slice(to);
        at += found + to.len();
    }
    out
}

#[test]
fn a_colour_change_with_identical_geometry_is_a_high_score_and_no_residue() {
    let (a, b) = recoloured();
    let (numbers_a, numbers_b) = pair("artifact-page-number");

    let (pairing, scores) = scores(&a, &b);
    let footer = visual(&self::scores(&numbers_a, &numbers_b).1[0]);

    // Nothing structural changed: every element pairs as same, no residue item differs.
    assert!(pairing.summary().is_unchanged(), "{:?}", pairing.summary());
    assert!(
        pairing
            .pages
            .iter()
            .all(|m| m.residue.only_a == 0 && m.residue.only_b == 0)
    );
    // The figures on pages 1 and 2 turned light grey: ink removed, and far more of it than a
    // changed page number.
    let (p0, p1, p2) = (visual(&scores[0]), visual(&scores[1]), visual(&scores[2]));
    assert!(p0.differing_pixels > 20 * footer.differing_pixels, "{p0:?}");
    assert!(p1.differing_pixels > 20 * footer.differing_pixels, "{p1:?}");
    assert_eq!((p0.ink_added, p1.ink_added), (0, 0));
    assert_eq!(p2.differing_pixels, 0, "page 3 has no figure");
    // Smoke: these are real changes, not anti-aliasing.
    assert!(p0.anti_aliased * 10 < p0.differing_pixels, "{p0:?}");
}

/// What a pair's page pair is expected to score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Identical ink.
    Zero,
    /// Ink changed.
    Some,
    /// A page only one side has: nothing to compare.
    Unscored,
}

/// Per pair, its page pairs in order, from the mutation it declares.
const EXPECTED: [(&str, &[Expect]); 12] = {
    use Expect::{Some as S, Unscored as U, Zero as Z};
    [
        // Object numbers and byte order only.
        ("renumber-objects", &[Z, Z, Z]),
        // Content streams re-encoded, same ink.
        ("rewrite-content-same-ink", &[Z, Z, Z]),
        // Every page's ink moves by (2, -2) pt; the score is unregistered, like the canvas.
        ("global-shift-2pt", &[S, S, S]),
        // Tagging only.
        ("reorder-tags", &[Z, Z, Z]),
        // One paragraph on page 2 drawn 3 pt lower.
        ("move-paragraph-3pt", &[Z, S, Z]),
        ("drop-alt", &[Z, Z, Z]),
        ("change-heading-level", &[Z, Z, Z]),
        ("retag-figure-as-formula", &[Z, Z, Z]),
        // A page inserted as page 2; the pages after it carry the next page number.
        ("insert-page", &[Z, U, S, S]),
        // Every footer reworded.
        ("artifact-page-number", &[S, S, S]),
        // A table row on page 3 removed.
        ("identical-rows", &[Z, Z, S]),
        // A page inserted as 2, a word changed on the Method page, the later page renumbered.
        ("untagged-edits", &[Z, U, S, S]),
    ]
};

#[test]
fn every_pair_scores_where_its_mutation_changed_ink_and_nowhere_else() {
    let mut seen = Vec::new();
    for (name, expected) in EXPECTED {
        let (a, b) = pair(name);

        let (pairing, scores) = scores(&a, &b);

        let got: Vec<Expect> = scores
            .iter()
            .map(|s| match &s.visual {
                None => Expect::Unscored,
                Some(Ok(v)) if v.differing_pixels == 0 => Expect::Zero,
                Some(Ok(_)) => Expect::Some,
                Some(Err(e)) => panic!("{name}: {e}"),
            })
            .collect();
        assert_eq!(got, expected, "{name}");
        // The scores line up with the page pairs they score.
        let pages: Vec<_> = pairing.pages.iter().map(|m| (m.a, m.b)).collect();
        let listed: Vec<_> = scores.iter().map(|s| (s.a, s.b)).collect();
        assert_eq!(listed, pages, "{name}");
        seen.push(name);
    }
    // Smoke: every fixture pair is covered.
    assert_eq!(seen, pdfxray_fixtures::pairs::PAIRS);
}

#[test]
fn the_moved_paragraph_is_ink_removed_above_and_added_below_in_equal_measure() {
    let (a, b) = pair("move-paragraph-3pt");

    let (_, scores) = scores(&a, &b);

    let v = visual(&scores[1]);
    // The same text, 3 pt lower: what leaves one place arrives at the other.
    let (added, removed) = (v.ink_added.cast_signed(), v.ink_removed.cast_signed());
    let (gap, sum) = ((added - removed).unsigned_abs(), v.ink_added + v.ink_removed);
    assert!(gap * 4 < sum, "{v:?}");
    // Smoke: a 3 pt move at 2 px/pt counts well over a thousand pixels even with the
    // anti-aliasing left out (where moved glyph edges cross old ones, some do read as AA).
    assert!(v.differing_pixels > 1000, "{v:?}");
}

#[test]
fn scores_are_byte_identical_across_runs_and_agree_with_the_mask() {
    let (a, b) = pair("untagged-edits");
    let pairing = pdfxray_core::diff::pair(&a, &b);

    let first = page_scores(&a, &b, &pairing, SCALE, &Perceptual::default());
    let second = page_scores(&a, &b, &pairing, SCALE, &Perceptual::default());
    let request = Request {
        mode: Mode::Mask,
        scale: SCALE,
        fit: Fit::Refuse,
        registration: None,
        comparator: Comparator::same_engine(),
        step: None,
    };
    let mask = || composite::composite((&a, 1), (&b, 2), &request).unwrap();
    let (m1, m2) = (mask(), mask());

    assert_eq!(first, second);
    assert_eq!(m1.rgba, m2.rgba);
    // The mask counts what the score counts.
    assert_eq!(m1.delta.unwrap().visual().copied(), Some(visual(&first[2])));
    // Smoke: there was something to score.
    assert!(visual(&first[2]).differing_pixels > 0);
}

#[test]
fn a_page_pair_of_different_sizes_is_refused_not_scaled() {
    let open = |name| Document::open(pdfxray_fixtures::pdf_path(name)).unwrap();
    let (a, b) = (open("minimal-one-page"), open("tagged-simple"));

    let refused = composite::score_pages((&a, 0), (&b, 0), 1.0, &Perceptual::default());

    let why = refused.unwrap_err().to_string();
    assert!(why.contains("the pages differ in size"), "{why}");
    // Smoke: each page scores against itself.
    let same = composite::score_pages((&a, 0), (&a, 0), 1.0, &Perceptual::default()).unwrap();
    assert_eq!(same.differing_pixels, 0);
}
