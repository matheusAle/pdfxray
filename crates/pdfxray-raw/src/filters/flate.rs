//! `FlateDecode`: zlib inflate with an output cap.

use flate2::{Decompress, FlushDecompress, Status};

use super::{Step, StepStatus};

fn grow(out: &mut Vec<u8>) {
    out.reserve(out.len().clamp(4096, 8 << 20));
}

pub(super) fn inflate(input: &[u8], limit: usize) -> Step {
    let mut d = Decompress::new(true);
    let mut out: Vec<u8> = Vec::with_capacity(input.len().saturating_mul(3).clamp(1024, 1 << 22));
    loop {
        if out.len() >= limit {
            out.truncate(limit);
            return Step {
                bytes: out,
                status: StepStatus::Limit,
            };
        }
        if out.capacity() - out.len() < 1024 {
            grow(&mut out);
        }
        let in_pos = usize::try_from(d.total_in())
            .unwrap_or(usize::MAX)
            .min(input.len());
        let out_before = out.len();
        let result = d.decompress_vec(&input[in_pos..], &mut out, FlushDecompress::None);
        let in_after = usize::try_from(d.total_in()).unwrap_or(usize::MAX);
        match result {
            Ok(Status::StreamEnd) => {
                return Step {
                    bytes: out,
                    status: StepStatus::Done,
                };
            }
            Ok(Status::Ok | Status::BufError) => {
                if out.len() == out_before && in_after == in_pos {
                    return Step {
                        bytes: out,
                        status: StepStatus::Truncated,
                    };
                }
            }
            Err(_) => {
                return Step {
                    bytes: out,
                    status: StepStatus::Corrupt,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::test_util::{identity, run, with_filter, zlib};
    use super::super::{DecodeIssue, decode_with};
    use crate::Name;

    #[test]
    fn flate_round_trip() {
        let out = run(&zlib(b"hello hello hello"), &with_filter("FlateDecode"));

        assert_eq!(out.bytes, b"hello hello hello");
        assert_eq!((out.complete, out.applied), (true, 1));
        assert_eq!(out.filters, vec![Name::new("FlateDecode")]);
    }

    #[test]
    fn truncated_flate_returns_the_prefix() {
        let payload: Vec<u8> = (0..20_000u32)
            .map(|i| u8::try_from(i * 7 % 251).unwrap())
            .collect();
        let packed = zlib(&payload);
        let out = run(&packed[..packed.len() / 2], &with_filter("FlateDecode"));

        assert!(!out.complete);
        assert_eq!(out.issues, vec![DecodeIssue::Truncated(Name::new("FlateDecode"))]);
        assert!(!out.bytes.is_empty() && out.bytes.len() < payload.len());
        assert_eq!(out.bytes, payload[..out.bytes.len()]);
    }

    #[test]
    fn corrupt_flate_is_flagged() {
        let out = run(b"\x78\x9c\xff\xff\xff\xff", &with_filter("FlateDecode"));

        assert!(!out.complete);
        assert_eq!(out.issues, vec![DecodeIssue::Corrupt(Name::new("FlateDecode"))]);
    }

    #[test]
    fn flate_output_is_capped() {
        let packed = zlib(&vec![0u8; 100_000]);
        let out = decode_with(&packed, &with_filter("FlateDecode"), &identity, 10_000).unwrap();

        assert_eq!(out.bytes.len(), 10_000);
        assert_eq!(
            out.issues,
            vec![DecodeIssue::OutputLimit(Name::new("FlateDecode"))]
        );
    }
}
