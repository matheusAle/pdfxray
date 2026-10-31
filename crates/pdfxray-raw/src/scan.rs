//! Finding `num gen obj` headers by scanning the raw bytes.

use std::collections::HashMap;

use memchr::memmem;

use crate::lexer::ends_keyword;
use crate::object::{is_delimiter, is_whitespace};

/// One `num gen obj` header found in the file body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanHeader {
    pub num: u32,
    pub generation: u32,
    /// Offset of the first digit of `num`.
    pub offset: usize,
    /// Offset just after `obj`.
    pub end: usize,
}

/// Every object header in the file, in file order.
#[derive(Debug, Default)]
pub(crate) struct ObjectScan {
    headers: Vec<ScanHeader>,
    by_num: HashMap<u32, Vec<usize>>,
}

impl ObjectScan {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        let mut headers = Vec::new();
        for at in memmem::find_iter(bytes, b"obj") {
            if let Some(h) = header_ending_at(bytes, at) {
                headers.push(h);
            }
        }
        let mut by_num: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, h) in headers.iter().enumerate() {
            by_num.entry(h.num).or_default().push(i);
        }
        Self { headers, by_num }
    }

    pub(crate) fn headers(&self) -> &[ScanHeader] {
        &self.headers
    }

    /// The offset of the header for `num` closest to `recorded`, preferring the same generation.
    pub(crate) fn nearest(&self, num: u32, generation: u32, recorded: u64) -> Option<usize> {
        let candidates = self.by_num.get(&num)?;
        let distance = |h: &ScanHeader| u64::try_from(h.offset).unwrap_or(u64::MAX).abs_diff(recorded);
        let best = |same_generation: bool| {
            candidates
                .iter()
                .map(|&i| &self.headers[i])
                .filter(|h| !same_generation || h.generation == generation)
                .min_by_key(|h| (distance(h), h.offset))
                .map(|h| h.offset)
        };
        best(true).or_else(|| best(false))
    }
}

/// Checks whether the `obj` at `at` ends a `num gen obj` header.
fn header_ending_at(bytes: &[u8], at: usize) -> Option<ScanHeader> {
    let end = at + 3;
    if !ends_keyword(bytes.get(end).copied()) {
        return None;
    }
    let mut p = at;
    let ws_end = p;
    while p > 0 && is_whitespace(bytes[p - 1]) {
        p -= 1;
    }
    if p == ws_end {
        return None;
    }
    let gen_end = p;
    while p > 0 && bytes[p - 1].is_ascii_digit() {
        p -= 1;
    }
    let generation = parse_u32(&bytes[p..gen_end])?;
    let ws_end = p;
    while p > 0 && is_whitespace(bytes[p - 1]) {
        p -= 1;
    }
    if p == ws_end {
        return None;
    }
    let num_end = p;
    while p > 0 && bytes[p - 1].is_ascii_digit() {
        p -= 1;
    }
    let num = parse_u32(&bytes[p..num_end])?;
    if p > 0 && !is_whitespace(bytes[p - 1]) && !is_delimiter(bytes[p - 1]) {
        return None;
    }
    Some(ScanHeader {
        num,
        generation,
        offset: p,
        end,
    })
}

fn parse_u32(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |acc, d| {
        acc.checked_mul(10)?.checked_add(u32::from(d - b'0'))
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn finds_headers_but_not_endobj_or_prose() {
        let src = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\n12 3 obj\nnull\nendobj\n5 0 object\nx1 0 obj\n";
        let scan = ObjectScan::new(src);

        assert_eq!(
            scan.headers().to_vec(),
            vec![
                ScanHeader {
                    num: 1,
                    generation: 0,
                    offset: 9,
                    end: 16
                },
                ScanHeader {
                    num: 12,
                    generation: 3,
                    offset: 29,
                    end: 37
                },
            ]
        );
    }

    #[test]
    fn nearest_prefers_generation_then_distance() {
        let src = b"1 0 obj\nendobj\n1 0 obj\nendobj\n1 1 obj\nendobj\n";
        let scan = ObjectScan::new(src);

        assert_eq!(scan.nearest(1, 0, 20), Some(15));
        assert_eq!(scan.nearest(1, 0, 0), Some(0));
        assert_eq!(scan.nearest(1, 1, 0), Some(30));
        assert_eq!(scan.nearest(1, 7, 31), Some(30));
        assert_eq!(scan.nearest(9, 0, 0), None);
    }
}
