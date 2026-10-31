//! `LZWDecode`, honouring `/EarlyChange`.

use weezl::decode::Decoder as LzwDecoder;
use weezl::{BitOrder, LzwStatus};

use super::{Step, StepStatus};

pub(super) fn lzw(input: &[u8], early_change: bool, limit: usize) -> Step {
    let mut decoder = if early_change {
        LzwDecoder::with_tiff_size_switch(BitOrder::Msb, 8)
    } else {
        LzwDecoder::new(BitOrder::Msb, 8)
    };
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut rest = input;
    loop {
        let r = decoder.decode_bytes(rest, &mut buf);
        out.extend_from_slice(&buf[..r.consumed_out]);
        rest = &rest[r.consumed_in..];
        if out.len() >= limit {
            out.truncate(limit);
            return Step {
                bytes: out,
                status: StepStatus::Limit,
            };
        }
        match r.status {
            // Many writers omit the EOD code; running out of input cleanly is not an error.
            Ok(LzwStatus::Done | LzwStatus::NoProgress) => {
                return Step {
                    bytes: out,
                    status: StepStatus::Done,
                };
            }
            Ok(LzwStatus::Ok) => {
                if r.consumed_in == 0 && r.consumed_out == 0 {
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
    use weezl::BitOrder;

    use super::super::test_util::{dict, run, with_filter};
    use crate::{Name, Object};

    #[test]
    fn lzw_decodes_the_pdf_spec_example() {
        // ISO 32000-2 §7.4.4.2: 45 45 45 45 45 65 45 45 45 66.
        let encoded = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
        let out = run(&encoded, &with_filter("LZWDecode"));

        assert_eq!(out.bytes, [45, 45, 45, 45, 45, 65, 45, 45, 45, 66]);
        assert!(out.complete);
    }

    fn lzw_encode(data: &[u8], early_change: bool) -> Vec<u8> {
        let mut encoder = if early_change {
            weezl::encode::Encoder::with_tiff_size_switch(BitOrder::Msb, 8)
        } else {
            weezl::encode::Encoder::new(BitOrder::Msb, 8)
        };
        encoder.encode(data).unwrap()
    }

    #[test]
    fn lzw_honours_early_change_in_both_modes() {
        let payload: Vec<u8> = (0..6000u32)
            .map(|i| u8::try_from(i % 7 * 31 % 256).unwrap())
            .collect();
        for early in [true, false] {
            let parms = dict(vec![("EarlyChange", Object::Int(i64::from(early)))]);
            let d = dict(vec![
                ("Filter", Object::Name(Name::new("LZWDecode"))),
                ("DecodeParms", Object::Dict(parms)),
            ]);
            let out = run(&lzw_encode(&payload, early), &d);

            assert_eq!(out.bytes, payload, "early change {early}");
            assert!(out.complete);
        }
        // Smoke: decoding with the wrong mode does not reproduce the data.
        let wrong = dict(vec![
            ("Filter", Object::Name(Name::new("LZW"))),
            (
                "DecodeParms",
                Object::Dict(dict(vec![("EarlyChange", Object::Int(0))])),
            ),
        ]);
        assert_ne!(run(&lzw_encode(&payload, true), &wrong).bytes, payload);
    }
}
