//! Pixel comparison of two renders of the same page.
//!
//! [`diff`] works on any two [`RenderedPage`]s, whichever engine drew them, so the app, the CLI
//! and the tests can all use it to show where hayro and a reference renderer disagree.
use pdfxray_core::numeric::{f64_from_u64, usize_from_u32};

use crate::RenderedPage;

/// Result of [`diff`].
#[derive(Debug, Clone, PartialEq)]
pub struct Diff {
    /// Width of [`Diff::image`]: the wider of the two inputs.
    pub width: u32,
    /// Height of [`Diff::image`]: the taller of the two inputs.
    pub height: u32,
    /// Pixels whose largest channel difference exceeds the tolerance.
    pub differing_pixels: u64,
    /// Largest channel difference of any pixel, `0..=255`.
    pub max_channel_delta: u8,
    /// Mean over all `width * height` pixels of the pixel's largest channel difference, in
    /// `0.0..=255.0`. Sub-tolerance differences count here, so anti-aliasing noise shows up.
    pub mean_delta: f64,
    /// `width * height * 4` bytes of opaque RGBA: the first input as grayscale, with pixels that
    /// differ beyond the tolerance painted red.
    pub image: Vec<u8>,
}

impl Diff {
    /// Share of pixels that differ beyond the tolerance, `0.0..=1.0` (zero for an empty diff).
    pub fn differing_fraction(&self) -> f64 {
        let total = u64::from(self.width) * u64::from(self.height);
        if total == 0 {
            0.0
        } else {
            f64_from_u64(self.differing_pixels) / f64_from_u64(total)
        }
    }
}

/// Compares `a` with `b` pixel by pixel.
///
/// A pixel differs when any of its four channels differs by more than `tolerance`; the
/// tolerance absorbs anti-aliasing noise, so `0` means exact equality.
///
/// The images may have different sizes. The result then covers the union of both, and every
/// pixel that exists in only one of the inputs counts as differing with a delta of 255, so a
/// size mismatch can never look like a match. A bitmap whose `rgba` is shorter than
/// `width * height * 4` is treated as if the missing pixels were absent in the same way.
pub fn diff(a: &RenderedPage, b: &RenderedPage, tolerance: u8) -> Diff {
    let width = a.width.max(b.width);
    let height = a.height.max(b.height);
    let columns = usize_from_u32(width);
    let mut image = Vec::with_capacity(columns.saturating_mul(usize_from_u32(height)).saturating_mul(4));
    let mut differing_pixels = 0u64;
    let mut max_channel_delta = 0u8;
    let mut delta_sum = 0u64;

    for y in 0..height {
        let (row_a, row_b) = (row(a, y), row(b, y));
        for x in 0..columns {
            let (pa, pb) = (pixel(row_a, x), pixel(row_b, x));
            let delta = match (pa, pb) {
                (Some(pa), Some(pb)) => pa.iter().zip(&pb).map(|(p, q)| p.abs_diff(*q)).max().unwrap_or(0),
                _ => u8::MAX,
            };
            max_channel_delta = max_channel_delta.max(delta);
            delta_sum += u64::from(delta);
            if delta > tolerance {
                differing_pixels += 1;
                image.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                let gray = pa.map_or(255, luma);
                image.extend_from_slice(&[gray, gray, gray, 255]);
            }
        }
    }

    let total = u64::from(width) * u64::from(height);
    Diff {
        width,
        height,
        differing_pixels,
        max_channel_delta,
        mean_delta: mean(delta_sum, total),
        image,
    }
}

fn mean(sum: u64, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        f64_from_u64(sum) / f64_from_u64(count)
    }
}

/// Row `y` of `page` as RGBA bytes; empty when the row is outside the bitmap.
fn row(page: &RenderedPage, y: u32) -> &[u8] {
    if y >= page.height {
        return &[];
    }
    let stride = usize_from_u32(page.width) * 4;
    let start = usize_from_u32(y) * stride;
    let rest = page.rgba.get(start..).unwrap_or(&[]);
    &rest[..stride.min(rest.len())]
}

/// Pixel `x` of an RGBA row; `None` past its end.
fn pixel(row: &[u8], x: usize) -> Option<[u8; 4]> {
    let bytes = row.get(x * 4..x * 4 + 4)?;
    <[u8; 4]>::try_from(bytes).ok()
}

/// Rec. 601 luma of an RGB pixel.
fn luma(p: [u8; 4]) -> u8 {
    let weighted = u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114;
    u8::try_from(weighted / 1000).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::Transform;

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

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    /// The means and fractions are quotients: equal within a billionth.
    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} is not {expected}");
    }

    #[test]
    fn identical_pages_have_no_difference() {
        let a = page(4, 3, [10, 20, 30, 255]);

        let d = diff(&a, &a.clone(), 0);

        assert_eq!(d.differing_pixels, 0);
        assert_eq!(d.max_channel_delta, 0);
        assert_close(d.mean_delta, 0.0);
        assert_eq!((d.width, d.height), (4, 3));
        assert_close(d.differing_fraction(), 0.0);
        // smoke: no red anywhere and the image is the right size
        assert_eq!(d.image.len(), 4 * 3 * 4);
        assert!(
            d.image
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255)
        );
    }

    #[test]
    fn one_changed_pixel_is_counted_exactly_and_painted_red() {
        let a = page(3, 2, WHITE);
        let mut b = a.clone();
        set(&mut b, 2, 1, [255, 255, 200, 255]);

        let d = diff(&a, &b, 0);

        assert_eq!(d.differing_pixels, 1);
        assert_eq!(d.max_channel_delta, 55);
        assert_close(d.mean_delta, 55.0 / 6.0);
        assert_close(d.differing_fraction(), 1.0 / 6.0);
        let red: Vec<usize> = d
            .image
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == [255, 0, 0, 255])
            .map(|(i, _)| i)
            .collect();
        assert_eq!(red, [5]);
        // smoke: every other pixel is the white of `a` as gray
        assert_eq!(d.image[0..4], [255, 255, 255, 255]);
    }

    #[test]
    fn tolerance_absorbs_small_deltas_but_they_still_count_in_the_mean() {
        let a = page(2, 1, WHITE);
        let mut b = a.clone();
        set(&mut b, 0, 0, [253, 255, 255, 255]);
        set(&mut b, 1, 0, [255, 255, 240, 255]);

        let loose = diff(&a, &b, 3);
        let strict = diff(&a, &b, 1);

        assert_eq!(loose.differing_pixels, 1);
        assert_eq!(loose.max_channel_delta, 15);
        assert_close(loose.mean_delta, 8.5);
        assert_eq!(strict.differing_pixels, 2);
        // the tolerance is inclusive: a delta equal to it does not differ
        assert_eq!(diff(&a, &b, 15).differing_pixels, 0);
    }

    #[test]
    fn alpha_differences_count() {
        let a = page(1, 1, WHITE);
        let b = page(1, 1, [255, 255, 255, 100]);

        assert_eq!(diff(&a, &b, 0).max_channel_delta, 155);
    }

    #[test]
    fn image_is_the_grayscale_of_the_first_input() {
        let a = page(1, 1, [255, 0, 0, 255]);
        let b = a.clone();

        let d = diff(&a, &b, 0);

        // Rec. 601: 0.299 * 255 = 76
        assert_eq!(d.image, [76, 76, 76, 255]);
    }

    #[test]
    fn size_mismatch_covers_the_union_and_counts_missing_pixels_as_different() {
        let a = page(2, 2, WHITE);
        let b = page(3, 1, WHITE);

        let d = diff(&a, &b, 0);

        // union is 3 x 2 = 6 pixels; only the 2 shared pixels of the first row overlap
        assert_eq!((d.width, d.height), (3, 2));
        assert_eq!(d.differing_pixels, 4);
        assert_eq!(d.max_channel_delta, 255);
        assert_eq!(d.image.len(), 6 * 4);
        assert_close(d.differing_fraction(), 4.0 / 6.0);
        // smoke: even with the loosest tolerance a size mismatch is never a match
        assert_eq!(diff(&a, &b, 254).differing_pixels, 4);
    }

    #[test]
    fn truncated_bitmaps_do_not_panic() {
        let a = page(2, 2, WHITE);
        let mut b = a.clone();
        b.rgba.truncate(4 * 3);

        let d = diff(&a, &b, 0);

        assert_eq!(d.differing_pixels, 1);
    }

    #[test]
    fn empty_pages_diff_to_nothing() {
        let a = page(0, 0, WHITE);

        let d = diff(&a, &a.clone(), 0);

        assert_eq!(d.differing_pixels, 0);
        assert_close(d.mean_delta, 0.0);
        assert_close(d.differing_fraction(), 0.0);
        assert_eq!(d.image, Vec::<u8>::new());
    }
}
