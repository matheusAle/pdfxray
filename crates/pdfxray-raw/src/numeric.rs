//! Audited numeric conversions: the one place in the workspace where a primitive `as` cast lives.
//!
//! This crate is the lowest one that needs them; `pdfxray-core` re-exports the module as
//! `pdfxray_core::numeric`, so every layer shares one rule per conversion kind. Each helper is
//! named after what it converts and how it rounds, and states that rule once:
//!
//! - **Integer widening** (`usize_from_u32`, `u64_from_usize`) is lossless. The workspace builds
//!   only for 32- and 64-bit targets; a compile-time assertion below pins that.
//! - **Integer to float** (`f32_from_*`, `f64_from_*`) is exact while the value fits the float's
//!   significand (24 bits for `f32`, 53 for `f64`) and otherwise rounds to the nearest
//!   representable value, ties to even. Callers pass pixel sizes, counts and indices, which stay
//!   far below those limits.
//! - **Float narrowing** (`f32_from_f64`) rounds to the nearest `f32`, ties to even; values beyond
//!   the `f32` range become infinite and NaN stays NaN. Callers pass screen and page geometry.
//! - **Float to integer** keeps Rust's `as` rule exactly: the value truncates toward zero, saturates
//!   at the target's bounds (so negatives become 0 for unsigned targets and infinities become the
//!   bound), and NaN becomes 0. The `_round` helpers first round half away from zero
//!   ([`f64::round`]); the `_trunc` helpers do not round, so a caller that wants the floor or the
//!   ceiling applies it first.

// Integer widening below is lossless only when `usize` has 32 to 64 bits.
const _: () = assert!(usize::BITS >= 32 && usize::BITS <= 64);

/// `v` as an index or length. Lossless: `usize` has at least 32 bits.
#[inline]
pub fn usize_from_u32(v: u32) -> usize {
    // Lossless: the assertion at the top of the module keeps `usize` at least 32 bits wide.
    v as usize
}

/// `v` as a byte count or offset. Lossless: `usize` has at most 64 bits.
#[inline]
pub fn u64_from_usize(v: usize) -> u64 {
    // Lossless: the assertion at the top of the module keeps `usize` at most 64 bits wide.
    v as u64
}

/// `v` as an `f32`; exact up to 2^24, rounded to nearest (ties to even) above.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "the rounding above 2^24 is the documented rule"
)]
pub fn f32_from_u32(v: u32) -> f32 {
    v as f32
}

/// `v` as an `f32`; exact up to 2^24, rounded to nearest (ties to even) above.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "the rounding above 2^24 is the documented rule"
)]
pub fn f32_from_usize(v: usize) -> f32 {
    v as f32
}

/// `v` as an `f32`; exact within ±2^24, rounded to nearest (ties to even) beyond.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "the rounding beyond 2^24 is the documented rule"
)]
pub fn f32_from_i64(v: i64) -> f32 {
    v as f32
}

/// `v` as an `f64`; exact up to 2^53, rounded to nearest (ties to even) above.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "the rounding above 2^53 is the documented rule"
)]
pub fn f64_from_usize(v: usize) -> f64 {
    v as f64
}

/// `v` as an `f64`; exact up to 2^53, rounded to nearest (ties to even) above.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "the rounding above 2^53 is the documented rule"
)]
pub fn f64_from_u64(v: u64) -> f64 {
    v as f64
}

/// `v` as an `f64`; exact within ±2^53, rounded to nearest (ties to even) beyond.
#[inline]
#[allow(
    clippy::cast_precision_loss,
    reason = "the rounding beyond 2^53 is the documented rule"
)]
pub fn f64_from_i64(v: i64) -> f64 {
    v as f64
}

/// `v` rounded to the nearest `f32` (ties to even); infinite beyond the `f32` range, NaN stays
/// NaN.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the rounding to f32 is the documented rule"
)]
pub fn f32_from_f64(v: f64) -> f32 {
    v as f32
}

/// `v` rounded half away from zero, then saturated to `0..=255`; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn u8_from_f32_round(v: f32) -> u8 {
    v.round() as u8
}

/// `v` rounded half away from zero, then saturated to `0..=255`; NaN is 0.
#[inline]
pub fn u8_from_f64_round(v: f64) -> u8 {
    u8_from_f64_trunc(v.round())
}

/// `v` truncated toward zero and saturated to `0..=255`; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn u8_from_f64_trunc(v: f64) -> u8 {
    v as u8
}

/// `v` rounded half away from zero, then saturated to `0..=u32::MAX`; NaN is 0.
#[inline]
pub fn u32_from_f32_round(v: f32) -> u32 {
    u32_from_f32_trunc(v.round())
}

/// `v` truncated toward zero and saturated to `0..=u32::MAX`; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn u32_from_f32_trunc(v: f32) -> u32 {
    v as u32
}

/// `v` rounded half away from zero, then saturated to `0..=u32::MAX`; NaN is 0.
#[inline]
pub fn u32_from_f64_round(v: f64) -> u32 {
    u32_from_f64_trunc(v.round())
}

/// `v` truncated toward zero and saturated to `0..=u32::MAX`; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn u32_from_f64_trunc(v: f64) -> u32 {
    v as u32
}

/// `v` truncated toward zero and saturated to `0..=usize::MAX`; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn usize_from_f32_trunc(v: f32) -> usize {
    v as usize
}

/// `v` truncated toward zero and saturated to `0..=usize::MAX`; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn usize_from_f64_trunc(v: f64) -> usize {
    v as usize
}

/// `v` rounded half away from zero, then saturated to the `i64` range; NaN is 0.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    reason = "`as` from float to int saturates and maps NaN to 0, which is the documented rule"
)]
pub fn i64_from_f64_round(v: f64) -> i64 {
    v.round() as i64
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the expected floats are exact; their rounding is what is pinned"
)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn integer_widening_keeps_every_value() {
        assert_eq!(usize_from_u32(0), 0);
        assert_eq!(usize_from_u32(u32::MAX), 4_294_967_295);
        assert_eq!(u64_from_usize(usize::MAX), u64::MAX >> (64 - usize::BITS));
        // Smoke: an ordinary index passes through unchanged.
        assert_eq!(u64_from_usize(usize_from_u32(42)), 42);
    }

    #[test]
    fn integers_become_floats_exactly_up_to_the_significand_then_round_to_even() {
        assert_eq!(f32_from_u32(16_777_216), 16_777_216.0);
        // 2^24 + 1 is a tie between 2^24 and 2^24 + 2: ties go to the even significand.
        assert_eq!(f32_from_u32(16_777_217), 16_777_216.0);
        assert_eq!(f32_from_u32(16_777_219), 16_777_220.0);
        assert_eq!(f32_from_usize(1_000), 1_000.0);
        assert_eq!(f32_from_i64(-3), -3.0);
        assert_eq!(f64_from_u64(9_007_199_254_740_993), 9_007_199_254_740_992.0);
        assert_eq!(f64_from_i64(i64::MIN), -9_223_372_036_854_775_808.0);
        // Smoke: small counts are exact in both widths.
        assert_eq!(f64_from_usize(7), 7.0);
    }

    #[test]
    fn f64_narrows_to_the_nearest_f32_and_overflows_to_infinity() {
        assert_eq!(f32_from_f64(0.5), 0.5);
        assert_eq!(f32_from_f64(0.1), 0.1_f32);
        assert_eq!(f32_from_f64(1e300), f32::INFINITY);
        assert_eq!(f32_from_f64(-1e300), f32::NEG_INFINITY);
        // Smoke: NaN stays NaN.
        assert!(f32_from_f64(f64::NAN).is_nan());
    }

    #[test]
    fn rounding_to_u8_goes_half_away_from_zero_and_saturates() {
        assert_eq!(u8_from_f32_round(0.5), 1);
        assert_eq!(u8_from_f32_round(1.5), 2);
        assert_eq!(u8_from_f32_round(2.5), 3);
        assert_eq!(u8_from_f32_round(254.5), 255);
        assert_eq!(u8_from_f32_round(255.5), 255);
        assert_eq!(u8_from_f32_round(-0.5), 0);
        assert_eq!(u8_from_f32_round(-300.0), 0);
        assert_eq!(u8_from_f32_round(f32::INFINITY), 255);
        assert_eq!(u8_from_f64_round(127.5), 128);
        assert_eq!(u8_from_f64_round(1e9), 255);
        // Smoke: NaN is 0 in both widths.
        assert_eq!((u8_from_f32_round(f32::NAN), u8_from_f64_round(f64::NAN)), (0, 0));
    }

    #[test]
    fn truncating_to_u8_drops_the_fraction_and_saturates() {
        assert_eq!(u8_from_f64_trunc(254.99), 254);
        assert_eq!(u8_from_f64_trunc(-0.99), 0);
        assert_eq!(u8_from_f64_trunc(f64::NEG_INFINITY), 0);
        assert_eq!(u8_from_f64_trunc(f64::INFINITY), 255);
        // Smoke: NaN is 0.
        assert_eq!(u8_from_f64_trunc(f64::NAN), 0);
    }

    #[test]
    fn u32_conversions_round_or_truncate_and_saturate() {
        assert_eq!(u32_from_f32_round(23.5), 24);
        assert_eq!(u32_from_f32_round(23.49), 23);
        assert_eq!(u32_from_f32_trunc(23.99), 23);
        assert_eq!(u32_from_f32_trunc(-1.0), 0);
        assert_eq!(u32_from_f32_trunc(1e20), u32::MAX);
        assert_eq!(u32_from_f64_round(99.5), 100);
        assert_eq!(u32_from_f64_trunc(4_294_967_295.9), u32::MAX);
        assert_eq!(u32_from_f64_trunc(4_294_967_296.0), u32::MAX);
        assert_eq!(u32_from_f64_trunc(f64::NEG_INFINITY), 0);
        // Smoke: NaN is 0 in every variant.
        assert_eq!(
            [
                u32_from_f32_round(f32::NAN),
                u32_from_f32_trunc(f32::NAN),
                u32_from_f64_round(f64::NAN),
                u32_from_f64_trunc(f64::NAN),
            ],
            [0; 4]
        );
    }

    #[test]
    fn usize_and_i64_conversions_follow_the_same_rule() {
        assert_eq!(usize_from_f32_trunc(3.99), 3);
        assert_eq!(usize_from_f32_trunc(-2.0), 0);
        assert_eq!(usize_from_f64_trunc(7.0), 7);
        assert_eq!(usize_from_f64_trunc(f64::INFINITY), usize::MAX);
        assert_eq!(i64_from_f64_round(-2.5), -3);
        assert_eq!(i64_from_f64_round(2.5), 3);
        assert_eq!(i64_from_f64_round(-1e30), i64::MIN);
        assert_eq!(i64_from_f64_round(1e30), i64::MAX);
        // Smoke: NaN is 0.
        assert_eq!(
            (
                usize_from_f32_trunc(f32::NAN),
                usize_from_f64_trunc(f64::NAN),
                i64_from_f64_round(f64::NAN)
            ),
            (0, 0, 0)
        );
    }
}
