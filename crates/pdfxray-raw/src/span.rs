/// A half-open byte range `start..end` into the original file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    /// First byte, inclusive.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

impl Span {
    /// Creates a span; `start` must not exceed `end`.
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "span start {start} > end {end}");
        Self { start, end }
    }

    /// Length in bytes.
    pub fn len(self) -> usize {
        self.end - self.start
    }

    /// True when the span covers no bytes.
    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The bytes this span covers in `source`, or `None` if it is out of bounds.
    pub fn slice(self, source: &[u8]) -> Option<&[u8]> {
        source.get(self.start..self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_returns_exact_bytes() {
        let span = Span::new(2, 5);

        assert_eq!(span.slice(b"%PDF-1.7"), Some(&b"DF-"[..]));
        assert_eq!(span.len(), 3);
    }

    #[test]
    fn slice_out_of_bounds_is_none() {
        assert_eq!(Span::new(4, 99).slice(b"%PDF"), None);
        assert!(Span::new(3, 3).is_empty());
    }
}
