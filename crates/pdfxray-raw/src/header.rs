use std::fmt;

use crate::{RawDiagnostic, RawDiagnosticKind, RawError, Span};

/// How far into the file a `%PDF-` header is searched for. Readers commonly accept leading
/// garbage up to this point; pdfxray accepts it too but reports it.
pub const HEADER_SEARCH_WINDOW: usize = 1024;

const MARKER: &[u8] = b"%PDF-";

/// A `major.minor` PDF version as written in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PdfVersion {
    /// Major version digit.
    pub major: u8,
    /// Minor version digit.
    pub minor: u8,
}

impl fmt::Display for PdfVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// The file header (`%PDF-x.y`) exactly as found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Version declared in the header.
    pub version: PdfVersion,
    /// Bytes of `%PDF-x.y`.
    pub span: Span,
    /// Deviations found while locating the header.
    pub diagnostics: Vec<RawDiagnostic>,
}

/// Finds the `%PDF-x.y` header within the first [`HEADER_SEARCH_WINDOW`] bytes.
///
/// # Errors
///
/// [`RawError::MissingHeader`] when those bytes hold no `%PDF-x.y` header.
pub fn parse_header(source: &[u8]) -> Result<Header, RawError> {
    let window = &source[..source.len().min(HEADER_SEARCH_WINDOW)];
    let mut search_from = 0;
    while let Some(found) = find(&window[search_from..], MARKER) {
        let offset = search_from + found;
        if let Some(version) = version_at(source, offset + MARKER.len()) {
            let span = Span::new(offset, offset + MARKER.len() + 3);
            let diagnostics = if offset == 0 {
                Vec::new()
            } else {
                vec![RawDiagnostic {
                    kind: RawDiagnosticKind::HeaderNotAtStart { offset },
                    span,
                }]
            };
            return Ok(Header {
                version,
                span,
                diagnostics,
            });
        }
        search_from = offset + 1;
    }
    Err(RawError::MissingHeader)
}

fn version_at(source: &[u8], at: usize) -> Option<PdfVersion> {
    match source.get(at..at + 3)? {
        [major @ b'0'..=b'9', b'.', minor @ b'0'..=b'9'] => Some(PdfVersion {
            major: major - b'0',
            minor: minor - b'0',
        }),
        _ => None,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn header_at_start_has_no_diagnostics() {
        let header = parse_header(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n").unwrap();

        assert_eq!(
            header,
            Header {
                version: PdfVersion { major: 1, minor: 7 },
                span: Span::new(0, 8),
                diagnostics: vec![],
            }
        );
        assert_eq!(header.version.to_string(), "1.7");
    }

    #[test]
    fn leading_garbage_is_reported_not_repaired() {
        let header = parse_header(b"junk\r\n%PDF-2.0\n").unwrap();

        assert_eq!(header.version, PdfVersion { major: 2, minor: 0 });
        assert_eq!(
            header.diagnostics,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::HeaderNotAtStart { offset: 6 },
                span: Span::new(6, 14),
            }]
        );
    }

    #[test]
    fn malformed_marker_is_skipped_for_a_later_valid_one() {
        let header = parse_header(b"%PDF-x.y %PDF-1.4").unwrap();

        assert_eq!(header.span, Span::new(9, 17));
        assert_eq!(header.version, PdfVersion { major: 1, minor: 4 });
    }

    #[test]
    fn header_beyond_search_window_is_missing() {
        let mut bytes = vec![b' '; HEADER_SEARCH_WINDOW];
        bytes.extend_from_slice(b"%PDF-1.7");

        assert_eq!(parse_header(&bytes), Err(RawError::MissingHeader));
        assert_eq!(parse_header(b""), Err(RawError::MissingHeader));
    }

    proptest! {
        #[test]
        fn never_panics_and_span_points_at_marker(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            if let Ok(header) = parse_header(&bytes) {
                let slice = header.span.slice(&bytes).unwrap();
                prop_assert!(slice.starts_with(MARKER));
                prop_assert_eq!(slice.len(), 8);
                prop_assert!(header.span.start < HEADER_SEARCH_WINDOW);
            }
        }
    }
}
