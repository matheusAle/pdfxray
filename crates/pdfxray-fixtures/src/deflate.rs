//! A tiny deterministic zlib encoder (fixed Huffman codes, greedy LZ77).
//!
//! Fixtures must be byte-identical on every build, and the output of a general purpose deflate
//! library depends on which backend cargo's feature unification picks. This encoder has exactly
//! one behaviour: it emits one final block with the fixed Huffman code of RFC 1951 and, at every
//! position, the longest match found in the previous 32 KiB (the nearest one on ties).
//!
//! It is far from optimal and does not need to be; it only has to produce a valid `/FlateDecode`
//! stream that exercises literals, lengths and distances.

/// Window the matcher searches backwards.
const WINDOW: usize = 32 * 1024;
/// Shortest and longest match deflate can express.
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;

/// `(code, first length, extra bits)` for length codes 257..=285.
const LENGTHS: [(u16, usize, u8); 29] = [
    (257, 3, 0),
    (258, 4, 0),
    (259, 5, 0),
    (260, 6, 0),
    (261, 7, 0),
    (262, 8, 0),
    (263, 9, 0),
    (264, 10, 0),
    (265, 11, 1),
    (266, 13, 1),
    (267, 15, 1),
    (268, 17, 1),
    (269, 19, 2),
    (270, 23, 2),
    (271, 27, 2),
    (272, 31, 2),
    (273, 35, 3),
    (274, 43, 3),
    (275, 51, 3),
    (276, 59, 3),
    (277, 67, 4),
    (278, 83, 4),
    (279, 99, 4),
    (280, 115, 4),
    (281, 131, 5),
    (282, 163, 5),
    (283, 195, 5),
    (284, 227, 5),
    (285, 258, 0),
];

/// `(code, first distance, extra bits)` for distance codes 0..=29.
const DISTANCES: [(u16, usize, u8); 30] = [
    (0, 1, 0),
    (1, 2, 0),
    (2, 3, 0),
    (3, 4, 0),
    (4, 5, 1),
    (5, 7, 1),
    (6, 9, 2),
    (7, 13, 2),
    (8, 17, 3),
    (9, 25, 3),
    (10, 33, 4),
    (11, 49, 4),
    (12, 65, 5),
    (13, 97, 5),
    (14, 129, 6),
    (15, 193, 6),
    (16, 257, 7),
    (17, 385, 7),
    (18, 513, 8),
    (19, 769, 8),
    (20, 1025, 9),
    (21, 1537, 9),
    (22, 2049, 10),
    (23, 3073, 10),
    (24, 4097, 11),
    (25, 6145, 11),
    (26, 8193, 12),
    (27, 12289, 12),
    (28, 16385, 13),
    (29, 24577, 13),
];

/// Packs bits starting at the least significant bit of each byte.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    acc: u32,
    used: u8,
}

impl BitWriter {
    /// Writes the low `count` bits of `value`, least significant bit first.
    fn bits(&mut self, value: u32, count: u8) {
        for i in 0..count {
            self.acc |= ((value >> i) & 1) << self.used;
            self.used += 1;
            if self.used == 8 {
                self.bytes.push(u8::try_from(self.acc).unwrap_or_default());
                self.acc = 0;
                self.used = 0;
            }
        }
    }

    /// Writes a Huffman code, most significant bit first.
    fn code(&mut self, code: u32, length: u8) {
        for i in (0..length).rev() {
            self.bits((code >> i) & 1, 1);
        }
    }

    /// The fixed literal/length code of RFC 1951 section 3.2.6.
    fn symbol(&mut self, symbol: u16) {
        let s = u32::from(symbol);
        match symbol {
            0..=143 => self.code(0x30 + s, 8),
            144..=255 => self.code(0x190 + s - 144, 9),
            256..=279 => self.code(s - 256, 7),
            _ => self.code(0xC0 + s - 280, 8),
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.used > 0 {
            self.bytes.push(u8::try_from(self.acc).unwrap_or_default());
        }
        self.bytes
    }
}

/// The longest earlier occurrence of the bytes at `at`, as `(length, distance)`.
fn longest_match(data: &[u8], at: usize) -> Option<(usize, usize)> {
    let max = MAX_MATCH.min(data.len() - at);
    if max < MIN_MATCH {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    for start in (at.saturating_sub(WINDOW)..at).rev() {
        let length = data[start..]
            .iter()
            .zip(&data[at..at + max])
            .take_while(|(a, b)| a == b)
            .count();
        if length >= MIN_MATCH && best.is_none_or(|(l, _)| length > l) {
            best = Some((length, at - start));
        }
    }
    best
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// `data` as a zlib stream: header `78 9C`, one fixed-Huffman block, Adler-32 trailer.
pub(crate) fn zlib(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.bits(1, 1); // BFINAL
    w.bits(1, 2); // BTYPE = 01, fixed Huffman
    let mut at = 0;
    while at < data.len() {
        if let Some((length, distance)) = longest_match(data, at) {
            let &(code, first, extra) = LENGTHS
                .iter()
                .rev()
                .find(|(_, first, _)| *first <= length)
                .unwrap_or(&LENGTHS[0]);
            w.symbol(code);
            w.bits(u32::try_from(length - first).unwrap_or_default(), extra);
            let &(code, first, extra) = DISTANCES
                .iter()
                .rev()
                .find(|(_, first, _)| *first <= distance)
                .unwrap_or(&DISTANCES[0]);
            w.code(u32::from(code), 5);
            w.bits(u32::try_from(distance - first).unwrap_or_default(), extra);
            at += length;
        } else {
            w.symbol(u16::from(data[at]));
            at += 1;
        }
    }
    w.symbol(256);

    let mut out = vec![0x78, 0x9C];
    out.extend(w.finish());
    out.extend(adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use pretty_assertions::assert_eq;

    use super::*;

    fn inflate(zlib: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(zlib)
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    #[test]
    fn empty_input_is_header_one_empty_block_and_adler_one() {
        // Bits LSB first: BFINAL 1, BTYPE 01 -> 1,1,0, then the 7-bit end-of-block code 0000000.
        assert_eq!(zlib(b""), [0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn a_repeated_run_becomes_a_literal_and_one_back_reference() {
        let packed = zlib(b"aaaaaaaaaa");

        // 'a' literal (8 bits) + length 9 (7 bits) + distance 1 (5 bits) + EOB (7 bits) = 27 bits.
        assert_eq!(packed.len(), 2 + 4 + 4);
        assert_eq!(inflate(&packed), b"aaaaaaaaaa");
        // Smoke: the Adler-32 trailer of ten 'a' bytes.
        assert_eq!(&packed[packed.len() - 4..], adler32(b"aaaaaaaaaa").to_be_bytes());
    }

    #[test]
    fn adler32_matches_the_published_example() {
        // RFC 1950 style check value for "Wikipedia".
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn round_trips_text_binary_and_long_matches() {
        let text = b"BT /F1 12 Tf 20 270 Td (Plain Tj) Tj ET\nBT /F1 12 Tf 20 240 Td (Plain Tj) Tj ET";
        let binary: Vec<u8> = (0..=255u8).chain((0..=255u8).rev()).collect();
        let long = vec![b'x'; 1000];
        let far: Vec<u8> = (0..40_000u32)
            .map(|i| u8::try_from(i * 7 % 251).unwrap())
            .collect();

        for input in [&text[..], &binary, &long, &far] {
            assert_eq!(inflate(&zlib(input)), input);
        }
        assert!(zlib(&long).len() < 40);
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(zlib(b"same bytes, same stream"), zlib(b"same bytes, same stream"));
    }
}
