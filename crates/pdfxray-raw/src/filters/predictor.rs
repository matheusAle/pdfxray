//! The PNG (10–15) and TIFF (2) predictors of `/DecodeParms`, run after `Flate` and `LZW`.

use super::{DecodeIssue, Resolver, Step, StepStatus, int_param};
use crate::{Dict, Name};

/// Predictor rows wider than this (16 MiB) are refused instead of allocated.
const MAX_ROW_BYTES: usize = 16 * 1024 * 1024;

struct Predictor {
    kind: i64,
    colors: usize,
    bits: usize,
    columns: usize,
}

/// Applies `/Predictor` to a successfully (or partly) decoded Flate/LZW step.
pub(super) fn predict(
    step: Step,
    parms: &Dict,
    resolve: Resolver<'_>,
    filter: &Name,
    issues: &mut Vec<DecodeIssue>,
    limit: usize,
) -> Step {
    let kind = int_param(parms, b"Predictor", resolve).unwrap_or(1);
    if kind <= 1 {
        return step;
    }
    let get = |key: &[u8], default: i64| int_param(parms, key, resolve).unwrap_or(default);
    let colors = usize::try_from(get(b"Colors", 1))
        .ok()
        .filter(|c| (1..=64).contains(c));
    let bits = usize::try_from(get(b"BitsPerComponent", 8))
        .ok()
        .filter(|b| matches!(b, 1 | 2 | 4 | 8 | 16));
    let columns = usize::try_from(get(b"Columns", 1))
        .ok()
        .filter(|c| (1..=1 << 24).contains(c));
    let valid_kind = kind == 2 || (10..=15).contains(&kind);
    let (Some(colors), Some(bits), Some(columns), true) = (colors, bits, columns, valid_kind) else {
        issues.push(DecodeIssue::BadPredictorParams(filter.clone()));
        return step;
    };
    let predictor = Predictor {
        kind,
        colors,
        bits,
        columns,
    };
    let row_bits = colors * bits * columns;
    if row_bits.div_ceil(8) > MAX_ROW_BYTES {
        issues.push(DecodeIssue::BadPredictorParams(filter.clone()));
        return step;
    }
    let row_bytes = row_bits.div_ceil(8);
    let mut bytes = step.bytes;
    let truncated_row = if predictor.kind == 2 {
        tiff_predict(&mut bytes, &predictor, row_bytes)
    } else {
        let (out, partial) = png_predict(&bytes, row_bytes, (colors * bits).div_ceil(8).max(1), limit);
        bytes = out;
        partial
    };
    if truncated_row && step.status == StepStatus::Done {
        issues.push(DecodeIssue::TruncatedPredictorRow(filter.clone()));
    }
    Step {
        bytes,
        status: step.status,
    }
}

/// Undoes PNG row filters. Returns the data and whether the last row was short.
fn png_predict(data: &[u8], row_bytes: usize, bpp: usize, limit: usize) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row_bytes];
    let mut short = false;
    for chunk in data.chunks(row_bytes + 1) {
        let tag = chunk[0];
        let mut row = chunk[1..].to_vec();
        if row.len() < row_bytes {
            short = true;
        }
        for i in 0..row.len() {
            let left = if i >= bpp { row[i - bpp] } else { 0 };
            let up = prev.get(i).copied().unwrap_or(0);
            let up_left = if i >= bpp {
                prev.get(i - bpp).copied().unwrap_or(0)
            } else {
                0
            };
            let add = match tag {
                1 => left,
                2 => up,
                3 => u8::try_from(u16::midpoint(u16::from(left), u16::from(up))).unwrap_or(u8::MAX),
                4 => paeth(left, up, up_left),
                _ => 0,
            };
            row[i] = row[i].wrapping_add(add);
        }
        out.extend_from_slice(&row);
        row.resize(row_bytes, 0);
        prev = row;
        if out.len() >= limit {
            out.truncate(limit);
            break;
        }
    }
    (out, short)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i32::from(a), i32::from(b), i32::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undoes TIFF predictor 2 in place. Returns whether the last row was short.
fn tiff_predict(data: &mut [u8], p: &Predictor, row_bytes: usize) -> bool {
    let mut short = false;
    for row in data.chunks_mut(row_bytes) {
        if row.len() < row_bytes {
            short = true;
        }
        match p.bits {
            8 => {
                for i in p.colors..row.len() {
                    row[i] = row[i].wrapping_add(row[i - p.colors]);
                }
            }
            16 => {
                let step = p.colors * 2;
                let mut i = step;
                while i + 1 < row.len() {
                    let prev = u16::from_be_bytes([row[i - step], row[i - step + 1]]);
                    let cur = u16::from_be_bytes([row[i], row[i + 1]]);
                    let sum = cur.wrapping_add(prev).to_be_bytes();
                    row[i] = sum[0];
                    row[i + 1] = sum[1];
                    i += 2;
                }
            }
            bits => {
                let samples = (row.len() * 8 / bits).min(p.columns * p.colors);
                let mask = (1u16 << bits) - 1;
                let read = |row: &[u8], s: usize| -> u16 {
                    let bit = s * bits;
                    let shift = 8 - bits - (bit % 8);
                    u16::from(row[bit / 8] >> shift) & mask
                };
                for s in p.colors..samples {
                    let sum = (read(row, s) + read(row, s - p.colors)) & mask;
                    let bit = s * bits;
                    let shift = 8 - bits - (bit % 8);
                    let cleared = row[bit / 8] & !(u8::try_from(mask).unwrap_or(0xFF) << shift);
                    row[bit / 8] = cleared | (u8::try_from(sum).unwrap_or(0) << shift);
                }
            }
        }
    }
    short
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::test_util::{dict, run, zlib};
    use super::*;
    use crate::Object;

    #[test]
    fn png_predictors() {
        // Two rows of three bytes: Sub then Up.
        let filtered = [1, 1, 1, 1, 2, 1, 1, 1];
        let parms = dict(vec![("Predictor", Object::Int(12)), ("Columns", Object::Int(3))]);
        let d = dict(vec![
            ("Filter", Object::Name(Name::new("FlateDecode"))),
            ("DecodeParms", Object::Dict(parms)),
        ]);
        let out = run(&zlib(&filtered), &d);

        assert_eq!(out.bytes, [1, 2, 3, 2, 3, 4]);
        assert!(out.complete);
    }

    #[test]
    fn png_average_and_paeth() {
        assert_eq!(png_predict(&[3, 10, 10, 3, 2, 2], 2, 1, 100).0, [10, 15, 7, 13]);
        assert_eq!(png_predict(&[0, 5, 5, 4, 1, 1], 2, 1, 100).0, [5, 5, 6, 7]);
    }

    #[test]
    fn tiff_predictor_8_and_sub_byte() {
        let mut eight = vec![1, 1, 1, 5, 1, 0];
        let p8 = Predictor {
            kind: 2,
            colors: 1,
            bits: 8,
            columns: 3,
        };
        assert!(!tiff_predict(&mut eight, &p8, 3));
        assert_eq!(eight, [1, 2, 3, 5, 6, 6]);

        let mut one_bit = vec![0b1100_0000u8];
        let p1 = Predictor {
            kind: 2,
            colors: 1,
            bits: 1,
            columns: 8,
        };
        tiff_predict(&mut one_bit, &p1, 1);
        assert_eq!(one_bit, [0b1000_0000]);
    }

    #[test]
    fn bad_predictor_parameters_are_flagged() {
        let parms = dict(vec![("Predictor", Object::Int(12)), ("Columns", Object::Int(0))]);
        let d = dict(vec![
            ("Filter", Object::Name(Name::new("FlateDecode"))),
            ("DecodeParms", Object::Dict(parms)),
        ]);
        let out = run(&zlib(b"abc"), &d);

        assert_eq!(out.bytes, b"abc");
        assert!(
            out.issues
                .contains(&DecodeIssue::BadPredictorParams(Name::new("FlateDecode")))
        );
    }

    #[test]
    fn all_five_png_row_filters() {
        let filtered = [
            0, 1, 2, 3, // None
            1, 1, 1, 1, // Sub
            2, 1, 1, 1, // Up
            3, 2, 2, 2, // Average
            4, 1, 1, 1, // Paeth
        ];
        let (out, short) = png_predict(&filtered, 3, 1, 1000);

        assert_eq!(out, [1, 2, 3, 1, 2, 3, 2, 3, 4, 3, 5, 6, 4, 6, 7]);
        assert!(!short);
        // Smoke: unknown filter tags leave the row untouched.
        assert_eq!(png_predict(&[9, 7, 8, 9], 3, 1, 1000).0, [7, 8, 9]);
    }

    #[test]
    fn png_predictor_uses_bytes_per_pixel_for_the_left_neighbour() {
        // Two RGB pixels, Sub filter: the left neighbour is three bytes back.
        let (out, _) = png_predict(&[1, 10, 20, 30, 1, 2, 3], 6, 3, 1000);

        assert_eq!(out, [10, 20, 30, 11, 22, 33]);
    }

    #[test]
    fn short_last_png_row_is_flagged() {
        let parms = dict(vec![("Predictor", Object::Int(12)), ("Columns", Object::Int(3))]);
        let d = dict(vec![
            ("Filter", Object::Name(Name::new("FlateDecode"))),
            ("DecodeParms", Object::Dict(parms)),
        ]);
        let out = run(&zlib(&[2, 1, 2, 3, 2, 1]), &d);

        assert_eq!(out.bytes, [1, 2, 3, 2]);
        assert!(!out.complete);
        assert_eq!(
            out.issues,
            vec![DecodeIssue::TruncatedPredictorRow(Name::new("FlateDecode"))]
        );
    }

    #[test]
    fn tiff_predictor_16_bit_and_rgb() {
        let mut sixteen = vec![0, 1, 0, 1, 0xFF, 0xFF, 0, 2];
        let p16 = Predictor {
            kind: 2,
            colors: 1,
            bits: 16,
            columns: 4,
        };
        assert!(!tiff_predict(&mut sixteen, &p16, 8));
        assert_eq!(sixteen, [0, 1, 0, 2, 0, 1, 0, 3]);

        let mut rgb = vec![1, 2, 3, 1, 1, 1];
        let p3 = Predictor {
            kind: 2,
            colors: 3,
            bits: 8,
            columns: 2,
        };
        tiff_predict(&mut rgb, &p3, 6);
        assert_eq!(rgb, [1, 2, 3, 2, 3, 4]);
    }

    #[test]
    fn tiff_predictor_through_the_chain() {
        let parms = dict(vec![("Predictor", Object::Int(2)), ("Columns", Object::Int(4))]);
        let d = dict(vec![
            ("Filter", Object::Name(Name::new("Fl"))),
            ("DP", Object::Dict(parms)),
        ]);
        let out = run(&zlib(&[5, 1, 1, 1, 9, 0, 0, 1]), &d);

        assert_eq!(out.bytes, [5, 6, 7, 8, 9, 9, 9, 10]);
        assert!(out.complete);
    }
}
