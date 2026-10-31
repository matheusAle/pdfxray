//! Pixel comparison of renders against committed PNG baselines.

use std::path::{Path, PathBuf};

use pdfxray_render::{RenderedPage, write_png};

/// How far a render may drift from its baseline before it counts as different.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tolerance {
    /// Largest accepted absolute difference of any single channel.
    pub(crate) per_channel: u8,
    /// Number of pixels that may exceed `per_channel`.
    pub(crate) max_bad_pixels: usize,
}

impl Tolerance {
    /// Anti-aliasing noise up to 3/255 per channel, no differing pixel.
    pub(crate) const STRICT: Self = Self {
        per_channel: 3,
        max_bad_pixels: 0,
    };
}

/// Outcome of comparing two RGBA bitmaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Comparison {
    /// Same size and every pixel within tolerance.
    Match,
    /// The bitmaps have different dimensions.
    SizeMismatch {
        /// `(width, height)` of the baseline.
        expected: (u32, u32),
        /// `(width, height)` of the new render.
        actual: (u32, u32),
    },
    /// Some pixels differ by more than the tolerance.
    Differs {
        /// Pixels beyond the tolerance.
        bad_pixels: usize,
        /// Largest channel difference seen anywhere.
        max_delta: u8,
        /// Same-size RGBA image: differing pixels in red over a faded copy of the baseline.
        diff: Vec<u8>,
    },
}

/// Compares two straight RGBA8 bitmaps of the given sizes.
pub(crate) fn compare(
    expected: (&[u8], (u32, u32)),
    actual: (&[u8], (u32, u32)),
    tolerance: Tolerance,
) -> Comparison {
    if expected.1 != actual.1 {
        return Comparison::SizeMismatch {
            expected: expected.1,
            actual: actual.1,
        };
    }
    let mut bad_pixels = 0;
    let mut max_delta = 0;
    let mut diff = Vec::with_capacity(expected.0.len());
    for (e, a) in expected
        .0
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.0.as_chunks::<4>().0)
    {
        let delta = e.iter().zip(a).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        max_delta = max_delta.max(delta);
        if delta > tolerance.per_channel {
            bad_pixels += 1;
            diff.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            let fade = |c: u8| 255 - (255 - c) / 4;
            diff.extend_from_slice(&[fade(e[0]), fade(e[1]), fade(e[2]), 255]);
        }
    }
    if bad_pixels > tolerance.max_bad_pixels {
        Comparison::Differs {
            bad_pixels,
            max_delta,
            diff,
        }
    } else {
        Comparison::Match
    }
}

/// Directory holding the committed baselines.
pub(crate) fn snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
}

fn updating() -> bool {
    std::env::var_os("UPDATE_SNAPSHOTS").is_some_and(|v| v != "0" && !v.is_empty())
}

/// Asserts that `page` equals `<dir>/<name>.png`.
///
/// With `UPDATE_SNAPSHOTS=1` the baseline is (re)written instead. On a mismatch
/// `<name>.new.png` and `<name>.diff.png` are written next to the baseline and the call panics.
pub(crate) fn assert_snapshot(dir: &Path, name: &str, page: &RenderedPage, tolerance: Tolerance) {
    let baseline = dir.join(format!("{name}.png"));
    let new = dir.join(format!("{name}.new.png"));
    let diff = dir.join(format!("{name}.diff.png"));

    if updating() {
        std::fs::create_dir_all(dir).unwrap();
        write_png(page, &baseline).unwrap();
        for stale in [&new, &diff] {
            let _ = std::fs::remove_file(stale);
        }
        return;
    }

    let expected = image::open(&baseline)
        .unwrap_or_else(|e| {
            panic!(
                "missing baseline {} ({e}); run with UPDATE_SNAPSHOTS=1 to create it",
                baseline.display()
            )
        })
        .to_rgba8();
    let result = compare(
        (expected.as_raw(), (expected.width(), expected.height())),
        (&page.rgba, (page.width, page.height)),
        tolerance,
    );
    match result {
        Comparison::Match => {
            for stale in [&new, &diff] {
                let _ = std::fs::remove_file(stale);
            }
        }
        Comparison::SizeMismatch { expected, actual } => {
            write_png(page, &new).unwrap();
            panic!(
                "snapshot `{name}` is {}x{}, baseline is {}x{}; wrote {}",
                actual.0,
                actual.1,
                expected.0,
                expected.1,
                new.display()
            );
        }
        Comparison::Differs {
            bad_pixels,
            max_delta,
            diff: diff_pixels,
        } => {
            write_png(page, &new).unwrap();
            image::save_buffer(
                &diff,
                &diff_pixels,
                page.width,
                page.height,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
            panic!(
                "snapshot `{name}` differs in {bad_pixels} pixels (max channel delta {max_delta}); \
                 see {} and {}",
                new.display(),
                diff.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const SIZE: (u32, u32) = (2, 1);
    const BASE: [u8; 8] = [10, 20, 30, 255, 200, 200, 200, 255];

    #[test]
    fn identical_bitmaps_match() {
        assert_eq!(
            compare((&BASE, SIZE), (&BASE, SIZE), Tolerance::STRICT),
            Comparison::Match
        );
    }

    #[test]
    fn drift_within_tolerance_matches_and_beyond_does_not() {
        let mut near = BASE;
        near[0] += 3;
        let mut far = BASE;
        far[4] -= 4;

        assert_eq!(
            compare((&BASE, SIZE), (&near, SIZE), Tolerance::STRICT),
            Comparison::Match
        );
        let Comparison::Differs {
            bad_pixels,
            max_delta,
            diff,
        } = compare((&BASE, SIZE), (&far, SIZE), Tolerance::STRICT)
        else {
            panic!("expected a difference");
        };
        assert_eq!((bad_pixels, max_delta), (1, 4));
        // The differing pixel is red, the other one is a faded baseline pixel.
        assert_eq!(
            diff,
            [
                255 - (255 - 10) / 4,
                255 - (255 - 20) / 4,
                255 - (255 - 30) / 4,
                255,
                255,
                0,
                0,
                255
            ]
        );
    }

    #[test]
    fn allowed_bad_pixels_are_forgiven() {
        let mut far = BASE;
        far[0] = 0;
        let lenient = Tolerance {
            per_channel: 3,
            max_bad_pixels: 1,
        };

        assert_eq!(compare((&BASE, SIZE), (&far, SIZE), lenient), Comparison::Match);
    }

    #[test]
    fn size_mismatch_is_reported_before_pixels() {
        assert_eq!(
            compare((&BASE, SIZE), (&BASE[..4], (1, 1)), Tolerance::STRICT),
            Comparison::SizeMismatch {
                expected: (2, 1),
                actual: (1, 1)
            }
        );
    }
}
