//! The visual score of a page pair: two renders by **one** engine on **one** pixel grid,
//! compared perceptually.
//!
//! [`crate::diff`] compares two *engines*: their glyph rasterisers and anti-aliasing really
//! differ, so the largest channel difference against a lenient tolerance (48) is what that
//! comparison needs. Two files drawn by hayro are a different problem: the rasteriser is the
//! same and deterministic, so the only noise is a glyph edge that landed a fraction of a pixel
//! elsewhere, and a channel tolerance either swallows a real change or lights up every line of
//! text. This module is the comparator for that case, modelled on what Vitest's
//! `toMatchScreenshot` does with pixelmatch:
//!
//! * **A perceptual threshold** ([`Perceptual::threshold`], `0.0..=1.0`, default
//!   [`THRESHOLD`]): a pixel differs when its colour distance in YIQ space exceeds
//!   `threshold²` of the largest distance any two colours can have. A hue shift the eye barely
//!   sees and a black-to-white flip are far apart on that scale, which they are not on a
//!   channel scale.
//! * **Anti-aliasing detection** ([`Perceptual::include_aa`], default off): a pixel that differs
//!   but looks like the soft edge of a shape is counted apart ([`VisualScore::anti_aliased`]) and
//!   left out of the score.
//! * **Two limits, the stricter winning** ([`Limits`]): a differing-pixel count and a ratio. A
//!   ratio alone means nothing on a near-empty page, a count alone nothing on a dense one.
//! * **Two colours** ([`Perceptual::ink_added`], [`Perceptual::ink_removed`]): a pixel where B is
//!   darker than A is ink B added; lighter, ink B removed.
//!
//! Not borrowed: Vitest's capture-stability loop (hayro is deterministic) and its pass/fail
//! verdict. This is a debugger; [`VisualScore::exceeds`] is information, and the CLI's exit
//! status still comes from the pairing.
//!
//! # Where the algorithms come from
//!
//! Implemented here from the published descriptions, not from pixelmatch's source:
//!
//! * The colour distance is the one of Y. Kotsarenko and F. Ramos, *Measuring perceived color
//!   difference using YIQ NTSC transmission color space in mobile applications* (Programación
//!   Matemática y Software, 2010): RGB to NTSC YIQ, then `0.5053 ΔY² + 0.299 ΔI² + 0.1957 ΔQ²`.
//!   The largest value that takes over two 8-bit colours, [`MAX_DISTANCE`], is reached between
//!   red and cyan, and a test derives it from the formula. Translucent pixels are first
//!   composited over white. The threshold's meaning (a share of that largest distance, squared)
//!   is the one Vitest's and pixelmatch's documentation give.
//! * The anti-aliasing test is the one of V. Vyšniauskas, *Anti-aliased pixel and intensity
//!   slope detector* (Mokslas – Lietuvos ateitis, 2009), as pixelmatch's documentation
//!   describes it: see [`anti_aliased`].

use std::num::NonZeroUsize;
use std::thread;

use pdfxray_core::diff::{PageMatch, Pairing, Status};
use pdfxray_core::numeric::{f64_from_u64, i64_from_f64_round, usize_from_u32};
use pdfxray_core::{Document, PageIx};

use crate::RenderedPage;
use crate::annotate::status_rgba;
use crate::composite::{CompositeError, score_pages};
use crate::diff::luma;

/// Default [`Perceptual::threshold`]: Vitest's and pixelmatch's.
pub const THRESHOLD: f64 = 0.1;

/// Default [`Perceptual::ink_added`]: the `added` status colour.
pub const INK_ADDED: [u8; 4] = status_rgba(Status::Added);

/// Default [`Perceptual::ink_removed`]: the `removed` status colour.
pub const INK_REMOVED: [u8; 4] = status_rgba(Status::Removed);

/// The largest YIQ distance between two opaque 8-bit colours (red against cyan): what
/// [`Perceptual::threshold`] is a share of, squared.
pub const MAX_DISTANCE: f64 = 35_215.0;

/// How many differing pixels a page pair may have before [`VisualScore::exceeds`] says so. Both
/// limits apply and the stricter wins; with neither set, any differing pixel exceeds.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Limits {
    /// At most this many differing pixels.
    pub pixels: Option<u64>,
    /// At most this share of the compared pixels, `0.0..=1.0`.
    pub ratio: Option<f64>,
}

impl Limits {
    /// The most differing pixels out of `total` that do not exceed: the smaller of the two
    /// limits, or `None` when neither is set — nothing to exceed, because no limit was asked for.
    pub fn allowed(&self, total: u64) -> Option<u64> {
        let by_ratio = self.ratio.map(|r| {
            let share = (r.clamp(0.0, 1.0) * f64_from_u64(total)).floor();
            u64::try_from(i64_from_f64_round(share)).unwrap_or(0)
        });
        match (self.pixels, by_ratio) {
            (Some(p), Some(r)) => Some(p.min(r)),
            (Some(n), None) | (None, Some(n)) => Some(n),
            (None, None) => None,
        }
    }
}

/// How two renders of one engine are compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perceptual {
    /// Per-pixel sensitivity, `0.0..=1.0`: a pixel differs when its YIQ distance exceeds
    /// `threshold²` of [`MAX_DISTANCE`]. `0.0` counts any change. Default [`THRESHOLD`].
    pub threshold: f64,
    /// Count the pixels that look like anti-aliasing as differing too. Default `false`: on a
    /// page of text, a sub-pixel shift otherwise outweighs any real change.
    pub include_aa: bool,
    /// When the score [`exceeds`](VisualScore::exceeds).
    pub limits: Limits,
    /// Mask colour of a pixel B draws darker than A: ink added. Default the `added` status
    /// colour.
    pub ink_added: [u8; 4],
    /// Mask colour of a pixel B draws lighter than A: ink removed. Default the `removed` status
    /// colour.
    pub ink_removed: [u8; 4],
}

impl Default for Perceptual {
    fn default() -> Self {
        Self {
            threshold: THRESHOLD,
            include_aa: false,
            limits: Limits::default(),
            ink_added: INK_ADDED,
            ink_removed: INK_REMOVED,
        }
    }
}

/// The visual score of a page pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualScore {
    /// Width of the compared grid, pixels.
    pub width: u32,
    /// Height of the compared grid, pixels.
    pub height: u32,
    /// Pixels beyond the threshold, without the anti-aliased ones unless
    /// [`VisualScore::include_aa`]: the score.
    pub differing_pixels: u64,
    /// Of [`VisualScore::differing_pixels`], those B draws darker than A.
    pub ink_added: u64,
    /// Of [`VisualScore::differing_pixels`], those B draws lighter than A.
    pub ink_removed: u64,
    /// Pixels beyond the threshold that look like anti-aliasing, whether or not they count.
    pub anti_aliased: u64,
    /// The threshold it was measured with.
    pub threshold: f64,
    /// Whether anti-aliased pixels count.
    pub include_aa: bool,
    /// The differing pixels allowed by the limits it was measured with; `None` when no limit
    /// was asked for.
    pub allowed: Option<u64>,
}

impl VisualScore {
    /// Share of the compared pixels that differ, `0.0..=1.0` (zero for an empty grid).
    pub fn ratio(&self) -> f64 {
        let total = u64::from(self.width) * u64::from(self.height);
        if total == 0 {
            0.0
        } else {
            f64_from_u64(self.differing_pixels) / f64_from_u64(total)
        }
    }

    /// True when any pixel differs at all: whether the page changed visually, which is a
    /// different question from whether it passed a limit nobody set.
    pub const fn differs(&self) -> bool {
        self.differing_pixels > 0
    }

    /// True when more pixels differ than the stricter limit allows. False when no limit was
    /// asked for: this reports a score, it does not assert one, so with nothing to exceed every
    /// page would otherwise read as exceeding and the flag would say nothing.
    pub const fn exceeds(&self) -> bool {
        match self.allowed {
            Some(allowed) => self.differing_pixels > allowed,
            None => false,
        }
    }

    /// The score in words, as the legend and the Changes list say it: `840 px differ (0.70%)`.
    pub fn summary(&self) -> String {
        format!("{} px differ ({})", self.differing_pixels, percent(self.ratio()))
    }

    /// What anti-aliasing did to the score, in words, when any pixel looked like it:
    /// `346 px anti-aliasing, ignored`.
    pub fn anti_aliasing(&self) -> Option<String> {
        (self.anti_aliased > 0).then(|| {
            let how = if self.include_aa { "counted" } else { "ignored" };
            format!("{} px anti-aliasing, {how}", self.anti_aliased)
        })
    }
}

/// `ratio` as a percentage with two decimals; a share too small for them reads `<0.01%`, so a
/// difference never reads as none.
pub fn percent(ratio: f64) -> String {
    if ratio > 0.0 && ratio < 0.000_05 {
        "<0.01%".to_owned()
    } else {
        format!("{:.2}%", ratio * 100.0)
    }
}

/// Scores `b` against `a`, which must be drawn on the same grid by the same engine.
///
/// The bitmaps may have different sizes: the score then covers the union of both, and a pixel
/// only one of them has always differs (as ink removed when only A has it, added when only B
/// does), so a size mismatch never looks like a match.
pub fn score(a: &RenderedPage, b: &RenderedPage, how: &Perceptual) -> VisualScore {
    compare(a, b, how, None)
}

/// [`score`], with the mask: `width * height * 4` bytes of opaque RGBA, A in grey with every
/// differing pixel in [`Perceptual::ink_added`] or [`Perceptual::ink_removed`].
pub fn mask(a: &RenderedPage, b: &RenderedPage, how: &Perceptual) -> (VisualScore, Vec<u8>) {
    let mut image = Vec::new();
    let score = compare(a, b, how, Some(&mut image));
    (score, image)
}

/// The visual score of one page pair of a pairing.
#[derive(Debug, Clone, PartialEq)]
pub struct PageScore {
    /// A's page.
    pub a: Option<PageIx>,
    /// B's page.
    pub b: Option<PageIx>,
    /// The score; `None` for a page only one side has (there is nothing to compare it with),
    /// the reason when the pair cannot be drawn on one grid.
    pub visual: Option<Result<VisualScore, CompositeError>>,
}

/// The visual layer of `pairing`, which must pair `a` with `b`: one [`PageScore`] per
/// [`Pairing::pages`] entry, in its order, each page pair drawn at `scale` pixels per point (B
/// into A's grid, never scaled or registered) and compared as `how` says.
///
/// An enrichment beside the pairing, never part of it: pairing stays renderer-free, so it works
/// on files that cannot be drawn and costs no rendering. This costs two renders per page pair,
/// spread over the available cores; results do not depend on how they are spread.
pub fn page_scores(
    a: &Document,
    b: &Document,
    pairing: &Pairing,
    scale: f32,
    how: &Perceptual,
) -> Vec<PageScore> {
    let one = |index: usize| page_score(a, b, &pairing.pages[index], scale, how);
    let count = pairing.pages.len();
    let workers = thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(count);
    if workers <= 1 {
        return (0..count).map(one).collect();
    }
    let mut slots: Vec<Option<PageScore>> = vec![None; count];
    thread::scope(|scope| {
        let one = &one;
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                scope.spawn(move || {
                    (w..count)
                        .step_by(workers)
                        .map(|i| (i, one(i)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            // A worker that panicked leaves its slots empty; they are scored here instead.
            for (i, scored) in handle.join().unwrap_or_default() {
                slots[i] = Some(scored);
            }
        }
    });
    slots
        .into_iter()
        .enumerate()
        .map(|(i, slot)| slot.unwrap_or_else(|| one(i)))
        .collect()
}

/// The visual score of one page pair `m` of a pairing of `a` with `b`: one entry of
/// [`page_scores`], for when only one page pair is wanted.
pub fn page_score(a: &Document, b: &Document, m: &PageMatch, scale: f32, how: &Perceptual) -> PageScore {
    PageScore {
        a: m.a,
        b: m.b,
        visual: match (m.a, m.b) {
            (Some(pa), Some(pb)) => Some(score_pages((a, pa), (b, pb), scale, how)),
            _ => None,
        },
    }
}

/// What one pixel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Within the threshold.
    Same,
    /// Beyond it, but anti-aliasing.
    AntiAliased { added: bool },
    /// Beyond it.
    Differs { added: bool },
}

fn compare(
    a: &RenderedPage,
    b: &RenderedPage,
    how: &Perceptual,
    mut image: Option<&mut Vec<u8>>,
) -> VisualScore {
    let width = a.width.max(b.width);
    let height = a.height.max(b.height);
    if let Some(image) = image.as_deref_mut() {
        image.clear();
        image.reserve(usize_from_u32(width) * usize_from_u32(height) * 4);
    }
    let limit = MAX_DISTANCE * how.threshold * how.threshold;
    let mut score = VisualScore {
        width,
        height,
        differing_pixels: 0,
        ink_added: 0,
        ink_removed: 0,
        anti_aliased: 0,
        threshold: how.threshold,
        include_aa: how.include_aa,
        allowed: how.limits.allowed(u64::from(width) * u64::from(height)),
    };
    for y in 0..height {
        for x in 0..width {
            let (pa, pb) = (at(a, x, y), at(b, x, y));
            let verdict = match (pa, pb) {
                (Some(p), Some(q)) if p == q => Verdict::Same,
                (Some(p), Some(q)) => {
                    let (distance, added) = distance(p, q);
                    if distance <= limit {
                        Verdict::Same
                    } else if anti_aliased(a, b, x, y) || anti_aliased(b, a, x, y) {
                        Verdict::AntiAliased { added }
                    } else {
                        Verdict::Differs { added }
                    }
                }
                (None, Some(_)) => Verdict::Differs { added: true },
                (Some(_) | None, None) => Verdict::Differs { added: false },
            };
            let counted = match verdict {
                Verdict::Same => None,
                Verdict::AntiAliased { added } => {
                    score.anti_aliased += 1;
                    how.include_aa.then_some(added)
                }
                Verdict::Differs { added } => Some(added),
            };
            if let Some(added) = counted {
                score.differing_pixels += 1;
                *if added {
                    &mut score.ink_added
                } else {
                    &mut score.ink_removed
                } += 1;
            }
            if let Some(image) = image.as_deref_mut() {
                let px = match counted {
                    Some(true) => how.ink_added,
                    Some(false) => how.ink_removed,
                    None => {
                        let gray = pa.map_or(255, luma);
                        [gray, gray, gray, 255]
                    }
                };
                image.extend_from_slice(&px);
            }
        }
    }
    score
}

/// The pixel at `(x, y)` of `page`; `None` outside it or past a short bitmap.
fn at(page: &RenderedPage, x: u32, y: u32) -> Option<[u8; 4]> {
    if x >= page.width || y >= page.height {
        return None;
    }
    let i = (usize_from_u32(y) * usize_from_u32(page.width) + usize_from_u32(x)) * 4;
    page.rgba.get(i..i + 4).and_then(|p| <[u8; 4]>::try_from(p).ok())
}

/// `p` composited over white, `0.0..=255.0` per channel.
fn over_white(p: [u8; 4]) -> [f64; 3] {
    let alpha = f64::from(p[3]) / 255.0;
    let c = |v: u8| 255.0 + (f64::from(v) - 255.0) * alpha;
    [c(p[0]), c(p[1]), c(p[2])]
}

/// NTSC luminance Y of an RGB colour.
fn y_of([r, g, b]: [f64; 3]) -> f64 {
    0.298_895_31 * r + 0.586_622_47 * g + 0.114_482_23 * b
}

/// NTSC chrominance I.
fn i_of([r, g, b]: [f64; 3]) -> f64 {
    0.595_977_99 * r - 0.274_176_10 * g - 0.321_801_89 * b
}

/// NTSC chrominance Q.
fn q_of([r, g, b]: [f64; 3]) -> f64 {
    0.211_470_17 * r - 0.522_617_11 * g + 0.311_146_94 * b
}

/// The weighted YIQ distance (Kotsarenko & Ramos) of RGB difference `d`; linear in the
/// conversion, so a difference of colours converts like a colour.
fn yiq_distance(d: [f64; 3]) -> f64 {
    let (y, i, q) = (y_of(d), i_of(d), q_of(d));
    0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q
}

/// The YIQ distance from `a` to `b`, and whether `b` is the darker (ink added; a tie in
/// brightness counts as added, the colour B drew).
fn distance(a: [u8; 4], b: [u8; 4]) -> (f64, bool) {
    let (ca, cb) = (over_white(a), over_white(b));
    let d = [ca[0] - cb[0], ca[1] - cb[1], ca[2] - cb[2]];
    (yiq_distance(d), y_of(cb) <= y_of(ca))
}

/// Brightness for the anti-aliasing test, in integer thousandths so that "equal" is exact:
/// Rec. 601 luma of the pixel over white.
fn brightness(p: [u8; 4]) -> i32 {
    let alpha = i32::from(p[3]);
    let c = |v: u8| 255 - (255 - i32::from(v)) * alpha / 255;
    c(p[0]) * 299 + c(p[1]) * 587 + c(p[2]) * 114
}

/// The up to eight neighbours of `(x, y)` inside a `width` x `height` grid.
fn neighbours(x: u32, y: u32, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    let span = |v: u32, max: u32| v.saturating_sub(1)..=(v + 1).min(max.saturating_sub(1));
    span(y, height)
        .flat_map(move |ny| span(x, width).map(move |nx| (nx, ny)))
        .filter(move |&(nx, ny)| (nx, ny) != (x, y))
}

/// True when pixel `(x, y)` of `img` looks like the anti-aliased edge of a shape, judged against
/// `other`, the image it is compared with (Vyšniauskas 2009):
///
/// 1. At most two of its neighbours are exactly as bright as it: an edge pixel sits on a slope,
///    while a pixel inside a flat area or along a hard line has more equals.
/// 2. It has both a darker and a brighter neighbour: it lies between the shape and the
///    background.
/// 3. Its darkest or its brightest neighbour is *flat* — has at least three neighbours of its
///    exact colour — in **both** images: that neighbour is the interior of a shape or of the
///    background that both renders agree on, so the pixel is the blend at its edge.
///
/// A real change fails one of these: new ink makes its pixels flat (1), a line too thin to have
/// an interior leaves no flat neighbour (3), and a recoloured area is flat in one image only (3).
fn anti_aliased(img: &RenderedPage, other: &RenderedPage, x: u32, y: u32) -> bool {
    let Some(centre) = at(img, x, y) else {
        return false;
    };
    let level = brightness(centre);
    let mut equal = 0;
    let mut darkest: Option<(i32, u32, u32)> = None;
    let mut brightest: Option<(i32, u32, u32)> = None;
    for (nx, ny) in neighbours(x, y, img.width, img.height) {
        let Some(n) = at(img, nx, ny) else {
            continue;
        };
        let step = brightness(n) - level;
        if step == 0 {
            equal += 1;
            if equal > 2 {
                return false;
            }
        } else if step < 0 {
            if darkest.is_none_or(|(d, ..)| step < d) {
                darkest = Some((step, nx, ny));
            }
        } else if brightest.is_none_or(|(b, ..)| step > b) {
            brightest = Some((step, nx, ny));
        }
    }
    let (Some((_, dx, dy)), Some((_, bx, by))) = (darkest, brightest) else {
        return false;
    };
    let agreed_flat = |x, y| flat(img, x, y) && flat(other, x, y);
    agreed_flat(dx, dy) || agreed_flat(bx, by)
}

/// True when at least three neighbours of `(x, y)` in `img` have its exact colour.
fn flat(img: &RenderedPage, x: u32, y: u32) -> bool {
    let Some(centre) = at(img, x, y) else {
        return false;
    };
    neighbours(x, y, img.width, img.height)
        .filter(|&(nx, ny)| at(img, nx, ny) == Some(centre))
        .nth(2)
        .is_some()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Transform;

    const WHITE: [u8; 4] = [255; 4];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    fn page(width: u32, height: u32, fill: [u8; 4]) -> RenderedPage {
        RenderedPage {
            width,
            height,
            rgba: fill.repeat(usize_from_u32(width * height)),
            page_to_pixel: Transform::new(kurbo::Affine::IDENTITY).unwrap(),
        }
    }

    fn set(page: &mut RenderedPage, x: u32, y: u32, value: [u8; 4]) {
        let i = usize_from_u32((y * page.width + x) * 4);
        page.rgba[i..i + 4].copy_from_slice(&value);
    }

    fn fill(page: &mut RenderedPage, (x0, y0, x1, y1): (u32, u32, u32, u32), value: [u8; 4]) {
        for y in y0..y1 {
            for x in x0..x1 {
                set(page, x, y, value);
            }
        }
    }

    fn gray(v: u8) -> [u8; 4] {
        [v, v, v, 255]
    }

    #[test]
    fn the_largest_distance_is_derived_from_the_formula_between_red_and_cyan() {
        let corners: Vec<[f64; 3]> = (0..8)
            .map(|bits: u8| [0, 1, 2].map(|c| if (bits >> c) & 1 == 1 { 255.0 } else { -255.0 }))
            .collect();

        let largest = corners.iter().map(|d| yiq_distance(*d)).fold(0.0, f64::max);

        assert_eq!(largest.round(), MAX_DISTANCE);
        let (red_cyan, _) = distance([255, 0, 0, 255], [0, 255, 255, 255]);
        assert_eq!(red_cyan.round(), MAX_DISTANCE);
        // Smoke: black against white is far, but not the farthest.
        let (black_white, b_darker) = distance(WHITE, BLACK);
        assert!(black_white > 0.9 * MAX_DISTANCE && black_white < MAX_DISTANCE);
        assert!(b_darker);
    }

    #[test]
    fn a_hue_shift_the_eye_barely_sees_is_within_the_threshold_and_a_flip_is_not() {
        let a = page(2, 1, [200, 100, 100, 255]);
        let mut b = a.clone();
        set(&mut b, 0, 0, [204, 100, 96, 255]);
        set(&mut b, 1, 0, BLACK);

        let s = score(&a, &b, &Perceptual::default());
        let exact = score(
            &a,
            &b,
            &Perceptual {
                threshold: 0.0,
                ..Perceptual::default()
            },
        );

        assert_eq!((s.differing_pixels, s.ink_added, s.ink_removed), (1, 1, 0));
        assert_eq!(exact.differing_pixels, 2);
        // Smoke: on a channel scale the shift is a difference like the flip: a tolerance tight
        // enough to see any change sees both.
        assert_eq!(crate::diff(&a, &b, 3).differing_pixels, 2);
    }

    #[test]
    fn darker_in_b_is_ink_added_and_lighter_is_ink_removed_each_in_its_colour() {
        let mut a = page(3, 1, WHITE);
        let mut b = page(3, 1, WHITE);
        set(&mut b, 0, 0, BLACK);
        set(&mut a, 1, 0, BLACK);
        set(&mut a, 2, 0, gray(30));
        set(&mut b, 2, 0, gray(30));
        let how = Perceptual::default();

        let (s, image) = mask(&a, &b, &how);

        assert_eq!((s.differing_pixels, s.ink_added, s.ink_removed), (2, 1, 1));
        assert_eq!(
            image.as_chunks::<4>().0,
            [how.ink_added, how.ink_removed, gray(30)]
        );
        // Smoke: the two colours really are different by default.
        assert_ne!(how.ink_added, how.ink_removed);
    }

    /// A dark band across a white page with a one-pixel soft edge of `edge` above and below:
    /// what a rasteriser draws for a shape whose edges are not on the pixel grid.
    fn band(edge: u8) -> RenderedPage {
        let mut p = page(12, 12, WHITE);
        fill(&mut p, (0, 3, 12, 9), gray(edge));
        fill(&mut p, (0, 4, 12, 8), BLACK);
        p
    }

    #[test]
    fn a_soft_edge_that_shifts_is_anti_aliasing_and_ignored_unless_asked() {
        let (a, b) = (band(160), band(90));

        let ignored = score(&a, &b, &Perceptual::default());
        let included = score(
            &a,
            &b,
            &Perceptual {
                include_aa: true,
                ..Perceptual::default()
            },
        );

        // Two edge rows of 12: every pixel a softer edge between black and white.
        assert_eq!(ignored.anti_aliased, 24);
        assert_eq!(ignored.differing_pixels, 0);
        assert_eq!((included.differing_pixels, included.ink_added), (24, 24));
        // Smoke: the AA count does not depend on whether it counts.
        assert_eq!(included.anti_aliased, ignored.anti_aliased);
    }

    #[test]
    fn a_pixel_with_three_equal_neighbours_is_not_on_a_slope() {
        // A soft-edged square: the edge pixels beside each corner have a third equal neighbour
        // (the corner's other edge), so only the corners and the middles of the sides qualify.
        let square = |edge| {
            let mut p = page(12, 12, WHITE);
            fill(&mut p, (3, 3, 9, 9), gray(edge));
            fill(&mut p, (4, 4, 8, 8), BLACK);
            p
        };

        let s = score(&square(160), &square(90), &Perceptual::default());

        // The 20-pixel ring, less the two beside each of the 4 corners.
        assert_eq!(s.anti_aliased, 20 - 8);
        // Smoke: what is not anti-aliasing counts.
        assert_eq!(s.differing_pixels, 8);
    }

    #[test]
    fn new_ink_is_never_mistaken_for_anti_aliasing() {
        let a = page(12, 12, WHITE);
        let mut b = a.clone();
        // A solid block, and a hairline too thin to have an interior.
        fill(&mut b, (2, 2, 6, 6), BLACK);
        fill(&mut b, (9, 1, 10, 11), gray(120));

        let s = score(&a, &b, &Perceptual::default());

        assert_eq!(s.anti_aliased, 0);
        assert_eq!((s.differing_pixels, s.ink_added), (16 + 10, 16 + 10));
        // Smoke: a recoloured flat area is not anti-aliasing either.
        let recoloured = score(
            &page(6, 6, gray(60)),
            &page(6, 6, gray(200)),
            &Perceptual::default(),
        );
        assert_eq!((recoloured.anti_aliased, recoloured.ink_removed), (0, 36));
    }

    #[test]
    fn the_stricter_limit_decides() {
        let a = page(10, 10, WHITE);
        let mut b = a.clone();
        fill(&mut b, (0, 0, 10, 2), BLACK);
        let with = |pixels, ratio| {
            score(
                &a,
                &b,
                &Perceptual {
                    limits: Limits { pixels, ratio },
                    ..Perceptual::default()
                },
            )
        };

        // 20 of 100 pixels differ.
        let count_strict = with(Some(10), Some(0.5));
        let ratio_strict = with(Some(1000), Some(0.1));
        let both_loose = with(Some(50), Some(0.5));

        assert_eq!((count_strict.allowed, count_strict.exceeds()), (Some(10), true));
        assert_eq!((ratio_strict.allowed, ratio_strict.exceeds()), (Some(10), true));
        assert_eq!((both_loose.allowed, both_loose.exceeds()), (Some(50), false));
        assert!(!with(None, Some(0.2)).exceeds(), "the limit is inclusive");
        // Smoke: with no limit, any differing pixel exceeds, and the score is the same.
        let unlimited = with(None, None);
        assert_eq!(
            (unlimited.allowed, unlimited.exceeds()),
            (None, false),
            "no limit asked for means nothing to exceed"
        );
        assert_eq!(unlimited.differing_pixels, count_strict.differing_pixels);
        assert!((unlimited.ratio() - 0.2).abs() < 1e-12);
    }

    #[test]
    fn a_size_mismatch_covers_the_union_and_never_matches() {
        let a = page(2, 2, WHITE);
        let b = page(3, 1, WHITE);

        let s = score(&a, &b, &Perceptual::default());

        // 3 x 2 = 6: two shared pixels, one only B has, three neither or only A has.
        assert_eq!((s.width, s.height), (3, 2));
        assert_eq!((s.differing_pixels, s.ink_added, s.ink_removed), (4, 1, 3));
        // Smoke: identical pages of any size score nothing.
        let same = score(&a, &a.clone(), &Perceptual::default());
        assert_eq!(
            (same.differing_pixels, same.anti_aliased, same.ratio()),
            (0, 0, 0.0)
        );
    }

    #[test]
    fn translucent_pixels_are_compared_over_white() {
        let a = page(1, 1, WHITE);
        let b = page(1, 1, [0, 0, 0, 0]);

        let s = score(&a, &b, &Perceptual::default());

        assert_eq!(s.differing_pixels, 0, "transparent black over white is white");
        // Smoke: opaque black is not.
        assert_eq!(
            score(&a, &page(1, 1, BLACK), &Perceptual::default()).differing_pixels,
            1
        );
    }

    #[test]
    fn empty_pages_score_nothing() {
        let a = page(0, 0, WHITE);

        let (s, image) = mask(&a, &a.clone(), &Perceptual::default());

        assert_eq!((s.differing_pixels, s.ratio(), s.exceeds()), (0, 0.0, false));
        // Smoke: nothing to draw.
        assert_eq!(image, Vec::<u8>::new());
    }
}
