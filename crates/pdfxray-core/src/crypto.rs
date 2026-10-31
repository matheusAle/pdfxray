//! The two primitives of the standard security handler's key derivation (ISO 32000-2 §7.6.4.3,
//! revisions 2 to 4): MD5 and RC4. Used only to test whether the empty user password opens a
//! file; pdfxray decrypts nothing.

use crate::numeric::{f64_from_usize, u32_from_f64_trunc, u64_from_usize};

/// Per-round left rotations of MD5.
const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5,
    9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6,
    10, 15, 21, 6, 10, 15, 21,
];

/// The MD5 round constants: the integer part of `2^32 × |sin(i + 1)|`.
fn round_constants() -> [u32; 64] {
    let mut k = [0_u32; 64];
    for (i, slot) in k.iter_mut().enumerate() {
        // The constants are defined as the truncated value of a sine scaled to 32 bits.
        *slot = u32_from_f64_trunc((f64_from_usize(i) + 1.0).sin().abs() * 4_294_967_296.0);
    }
    k
}

/// MD5 of `data` (RFC 1321).
pub(crate) fn md5(data: &[u8]) -> [u8; 16] {
    let constants = round_constants();
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&u64_from_usize(data.len()).wrapping_mul(8).to_le_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut words = [0_u32; 16];
        for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*bytes);
        }
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (mix, index) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let sum = mix
                .wrapping_add(a)
                .wrapping_add(constants[i])
                .wrapping_add(words[index]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(SHIFTS[i]));
        }
        for (slot, add) in state.iter_mut().zip([a, b, c, d]) {
            *slot = slot.wrapping_add(add);
        }
    }
    let mut out = [0_u8; 16];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *chunk = word.to_le_bytes();
    }
    out
}

/// RC4 of `data` with `key` (encryption and decryption are the same operation).
pub(crate) fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j = 0_u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, usize::from(j));
    }
    let (mut i, mut j) = (0_u8, 0_u8);
    data.iter()
        .map(|byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[usize::from(i)]);
            s.swap(usize::from(i), usize::from(j));
            let at = s[usize::from(i)].wrapping_add(s[usize::from(j)]);
            byte ^ s[usize::from(at)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use pretty_assertions::assert_eq;

    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    #[test]
    fn md5_matches_the_rfc_1321_test_suite() {
        let cases: [(&[u8], &str); 4] = [
            (b"", "d41d8cd98f00b204e9800998ecf8427e"),
            (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
            (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ];

        for (input, digest) in cases {
            assert_eq!(hex(&md5(input)), digest, "{}", String::from_utf8_lossy(input));
        }
    }

    #[test]
    fn md5_handles_inputs_around_the_block_boundary() {
        assert_eq!(hex(&md5(&[b'a'; 1000])), "cabe45dcc9ae5b66ba86600cca6b8ba8");
        let lengths: Vec<String> = [55, 56, 63, 64, 65]
            .iter()
            .map(|n| hex(&md5(&vec![0; *n])))
            .collect();
        let unique: std::collections::BTreeSet<&String> = lengths.iter().collect();
        assert_eq!(unique.len(), 5, "every length hashes differently");
    }

    #[test]
    fn rc4_matches_the_published_vectors() {
        assert_eq!(hex(&rc4(b"Key", b"Plaintext")), "bbf316e8d940af0ad3");
        assert_eq!(hex(&rc4(b"Wiki", b"pedia")), "1021bf0420");
        assert_eq!(rc4(b"Secret", &rc4(b"Secret", b"round trip")), b"round trip");
    }
}
