//! Stream data: the end-of-line after `stream`, the declared `/Length`, and where the data really
//! ends.

use super::{LengthLookup, LengthSource, Parser, skip_whitespace};
use crate::object::{Dict, Object, Stream};
use crate::{ObjRef, RawDiagnosticKind, Span};

/// Reads the stream body after the `stream` keyword. Returns the stream and the end offset.
pub(super) fn read_stream(
    parser: &mut Parser<'_>,
    dict: Dict,
    keyword: Span,
    r: ObjRef,
    lengths: &dyn LengthSource,
) -> (Stream, usize) {
    let src = parser.lex.source();
    let mut p = keyword.end;
    // `stream` must be followed by CRLF or LF.
    let eol_ok = match (src.get(p), src.get(p + 1)) {
        (Some(b'\r'), Some(b'\n')) => {
            p += 2;
            true
        }
        (Some(b'\n'), _) => {
            p += 1;
            true
        }
        _ => false,
    };
    if !eol_ok {
        let mut q = p;
        while matches!(src.get(q), Some(b' ' | b'\t')) {
            q += 1;
        }
        match (src.get(q), src.get(q + 1)) {
            (Some(b'\r'), Some(b'\n')) => q += 2,
            (Some(b'\r' | b'\n'), _) => q += 1,
            _ => q = p,
        }
        parser.report(RawDiagnosticKind::StreamKeywordEol, Span::new(keyword.end, q));
        p = q;
    }
    let data_start = p;

    let mut report_length = true;
    let declared = match dict.get(b"Length").map(|v| &v.value) {
        Some(Object::Int(n)) => Some(*n),
        Some(Object::Ref(len_ref)) => match lengths.length_of(*len_ref) {
            LengthLookup::Known(n) => Some(n),
            LengthLookup::Unresolved => {
                report_length = false;
                None
            }
        },
        _ => None,
    };

    let located = locate_stream_end(src, data_start, declared);
    let data = Span::new(data_start, located.data_end);
    let actual = u64::try_from(data.len()).unwrap_or(u64::MAX);
    if report_length && declared.is_none_or(|n| u64::try_from(n).ok() != Some(actual)) {
        parser.report(
            RawDiagnosticKind::StreamLengthWrong {
                obj: r,
                declared,
                actual,
            },
            Span::new(data_start, located.data_end),
        );
    }
    let end = if let Some(span) = located.endstream {
        parser.lex.set_pos(span.end);
        span.end
    } else {
        parser.report(
            RawDiagnosticKind::MissingEndstream,
            Span::new(located.data_end, located.data_end),
        );
        parser.lex.set_pos(located.data_end);
        located.data_end
    };
    (
        Stream {
            dict,
            data,
            declared_length: declared,
        },
        end,
    )
}

struct StreamEnd {
    data_end: usize,
    endstream: Option<Span>,
}

const ENDSTREAM: &[u8] = b"endstream";

/// Finds where stream data ends: at `declared` bytes when `endstream` follows there, otherwise
/// at the first `endstream` (minus one end-of-line marker).
fn locate_stream_end(src: &[u8], data_start: usize, declared: Option<i64>) -> StreamEnd {
    let declared_end = declared
        .and_then(|n| usize::try_from(n).ok())
        .and_then(|len| data_start.checked_add(len))
        .filter(|&e| e <= src.len());
    if let Some(end) = declared_end {
        let after = skip_whitespace(src, end);
        if src.get(after..).is_some_and(|rest| rest.starts_with(ENDSTREAM)) {
            return StreamEnd {
                data_end: end,
                endstream: Some(Span::new(after, after + ENDSTREAM.len())),
            };
        }
    }
    let rest = src.get(data_start..).unwrap_or_default();
    if let Some(found) = memchr::memmem::find(rest, ENDSTREAM) {
        let keyword = data_start + found;
        let mut end = keyword;
        if end > data_start && src.get(end - 1) == Some(&b'\n') {
            end -= 1;
        }
        if end > data_start && src.get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
        return StreamEnd {
            data_end: end,
            endstream: Some(Span::new(keyword, keyword + ENDSTREAM.len())),
        };
    }
    // No `endstream`: trust the declared length if it fits, else stop at `endobj`, else at the end.
    if let Some(end) = declared_end {
        return StreamEnd {
            data_end: end,
            endstream: None,
        };
    }
    let end = memchr::memmem::find(rest, b"endobj").map_or(src.len(), |f| data_start + f);
    StreamEnd {
        data_end: end,
        endstream: None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::test_util::{as_stream, indirect};
    use super::super::{NoLengths, parse_indirect};
    use super::*;
    use crate::RawDiagnostic;

    #[test]
    fn stream_with_correct_length() {
        let src = b"5 0 obj\n<< /Length 5 >>\nstream\nhello\nendstream\nendobj\n";
        let (parsed, diags) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data, Span::new(31, 36));
        assert_eq!(s.declared_length, Some(5));
        assert_eq!(s.data.slice(src), Some(&b"hello"[..]));
        assert_eq!(diags, vec![]);
        assert_eq!(parsed.span.end, src.len() - 1);
    }

    #[test]
    fn stream_with_wrong_length_uses_the_actual_one() {
        let src = b"5 0 obj\n<< /Length 99 >>\nstream\r\nhello\r\nendstream\nendobj";
        let (parsed, diags) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data.slice(src), Some(&b"hello"[..]));
        assert_eq!(s.declared_length, Some(99));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::StreamLengthWrong {
                    obj: ObjRef::new(5, 0),
                    declared: Some(99),
                    actual: 5
                },
                span: s.data
            }]
        );
    }

    #[test]
    fn stream_without_length_or_endstream_or_endobj() {
        let src = b"5 0 obj\n<< >>\nstream\nabc";
        let (parsed, diags) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data.slice(src), Some(&b"abc"[..]));
        let kinds: Vec<_> = diags.iter().map(|d| d.kind.code()).collect();
        assert_eq!(
            kinds,
            vec!["stream-length-wrong", "missing-endstream", "missing-endobj"]
        );
    }

    #[test]
    fn stream_keyword_followed_by_lone_cr_is_reported() {
        let src = b"5 0 obj\n<< /Length 2 >>\nstream\rab\nendstream\nendobj";
        let (parsed, diags) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data.slice(src), Some(&b"ab"[..]));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::StreamKeywordEol,
                span: Span::new(30, 31)
            }]
        );
    }

    struct FixedLength(i64);

    impl LengthSource for FixedLength {
        fn length_of(&self, _: ObjRef) -> LengthLookup {
            LengthLookup::Known(self.0)
        }
    }

    #[test]
    fn indirect_length_is_checked_against_the_data() {
        let src = b"5 0 obj\n<< /Length 9 0 R >>\nstream\nabcde\nendstream\nendobj";
        let mut sink = Vec::new();
        let parsed = parse_indirect(src, 0, &FixedLength(5), &mut sink).unwrap();

        let s = as_stream(&parsed.value);
        assert_eq!(
            (s.declared_length, s.data.slice(src)),
            (Some(5), Some(&b"abcde"[..]))
        );
        assert_eq!(sink, vec![]);

        let mut sink = Vec::new();
        parse_indirect(src, 0, &FixedLength(2), &mut sink).unwrap();
        assert_eq!(
            sink.iter().map(|d| d.kind.clone()).collect::<Vec<_>>(),
            vec![RawDiagnosticKind::StreamLengthWrong {
                obj: ObjRef::new(5, 0),
                declared: Some(2),
                actual: 5
            }]
        );
        // An unresolvable length is not reported: the end is found by scanning.
        let mut sink = Vec::new();
        parse_indirect(src, 0, &NoLengths, &mut sink).unwrap();
        assert_eq!(sink, vec![]);
    }

    #[test]
    fn stream_data_may_contain_the_word_endstream_when_length_says_so() {
        let src = b"5 0 obj\n<< /Length 15 >>\nstream\nab endstream cd\nendstream\nendobj";
        let (parsed, diags) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data.slice(src), Some(&b"ab endstream cd"[..]));
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn crlf_before_endstream_is_not_data() {
        let src = b"5 0 obj\n<< /Length 1 >>\nstream\r\nab\r\nendstream\r\nendobj\r\n";
        let (parsed, _) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data.slice(src), Some(&b"ab"[..]));
        assert_eq!(parsed.span.end, src.len() - 2);
    }

    #[test]
    fn stream_with_spaces_before_its_eol_is_reported() {
        let src = b"5 0 obj\n<< /Length 2 >>\nstream  \nab\nendstream\nendobj";
        let (parsed, diags) = indirect(src);

        let s = as_stream(&parsed.value);
        assert_eq!(s.data.slice(src), Some(&b"ab"[..]));
        assert_eq!(
            diags.iter().map(RawDiagnostic::code).collect::<Vec<_>>(),
            vec!["stream-keyword-eol"]
        );
    }
}
