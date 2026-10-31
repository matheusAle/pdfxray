//! Indirect objects: the `num gen obj` header, the body, an optional stream and `endobj`.

use super::stream::read_stream;
use super::{Parser, Stop, keyword_text, skip_whitespace};
use crate::lexer::{TokenKind, ends_keyword};
use crate::object::{Object, Spanned};
use crate::{ObjRef, RawDiagnostic, RawDiagnosticKind, Span};

/// Result of looking up an indirect `/Length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LengthLookup {
    /// The value of the length object.
    Known(i64),
    /// The length object cannot be read (yet); the stream end is found by scanning, silently.
    Unresolved,
}

/// Resolves `/Length 12 0 R` while parsing a stream.
pub(crate) trait LengthSource {
    fn length_of(&self, r: ObjRef) -> LengthLookup;
}

/// A [`LengthSource`] that resolves nothing.
pub(crate) struct NoLengths;

impl LengthSource for NoLengths {
    fn length_of(&self, _: ObjRef) -> LengthLookup {
        LengthLookup::Unresolved
    }
}

/// Reads `num gen obj` at exactly `pos` (no leading white space).
///
/// Returns the number, the generation (unclamped) and the offset just after `obj`.
pub(crate) fn read_obj_header(src: &[u8], pos: usize) -> Option<(u32, u32, usize)> {
    let (num, p) = read_digits(src, pos)?;
    let p = skip_whitespace(src, p);
    let (generation, p) = read_digits(src, p)?;
    let after_ws = skip_whitespace(src, p);
    if after_ws == p {
        return None;
    }
    let end = after_ws.checked_add(3)?;
    if src.get(after_ws..end)? == b"obj" && ends_keyword(src.get(end).copied()) {
        Some((num, generation, end))
    } else {
        None
    }
}

fn read_digits(src: &[u8], pos: usize) -> Option<(u32, usize)> {
    let mut p = pos;
    let mut value: u32 = 0;
    while let Some(d) = src.get(p).filter(|b| b.is_ascii_digit()) {
        value = value.checked_mul(10)?.checked_add(u32::from(d - b'0'))?;
        p += 1;
    }
    (p > pos).then_some((value, p))
}

/// A parsed indirect object.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedObject {
    /// Object number and generation from the header.
    pub num: u32,
    pub generation: u32,
    pub value: Spanned<Object>,
    /// From `num` to the end of `endobj` (or the last byte read when `endobj` is missing).
    pub span: Span,
}

/// Parses `num gen obj ... endobj` at exactly `offset`. Diagnostics go to `sink`.
///
/// A `/Length` that `lengths` cannot resolve raises no diagnostic: the end is found by scanning.
pub(crate) fn parse_indirect(
    src: &[u8],
    offset: usize,
    lengths: &dyn LengthSource,
    sink: &mut Vec<RawDiagnostic>,
) -> Option<ParsedObject> {
    let (num, generation, body) = read_obj_header(src, offset)?;
    let r = ObjRef::new(num, u16::try_from(generation).unwrap_or(u16::MAX));
    let mut parser = Parser::new(src, body);
    let value = match parser.value(0) {
        Ok(v) => v,
        Err(stop) => {
            let at = parser.lex.pos();
            let t = parser.lex.next_significant();
            let span = Span::new(body, t.span.start.max(body));
            if stop == Stop::Keyword && parser.lex.text(t.span) == b"endobj" {
                parser.lex.set_pos(at);
                parser.report(RawDiagnosticKind::EmptyObjectBody, span);
            } else if stop == Stop::Keyword {
                parser.report(
                    RawDiagnosticKind::UnexpectedKeyword {
                        text: keyword_text(parser.lex.text(t.span)),
                    },
                    t.span,
                );
                parser.lex.set_pos(at);
            } else {
                parser.lex.set_pos(at);
            }
            Spanned::new(Object::Null, span)
        }
    };

    let mut value = value;
    if matches!(value.value, Object::Dict(_)) {
        let at = parser.lex.pos();
        let t = parser.lex.next_significant();
        if t.kind == TokenKind::Keyword && parser.lex.text(t.span) == b"stream" {
            if let Object::Dict(dict) = std::mem::take(&mut value.value) {
                let (stream, end) = read_stream(&mut parser, dict, t.span, r, lengths);
                value = Spanned::new(Object::Stream(stream), Span::new(value.span.start, end));
            }
        } else {
            parser.lex.set_pos(at);
        }
    }

    let at = parser.lex.pos();
    let t = parser.lex.next_significant();
    let end = if t.kind == TokenKind::Keyword && parser.lex.text(t.span) == b"endobj" {
        t.span.end
    } else {
        parser.lex.set_pos(at);
        let stopped_at = if t.kind == TokenKind::Eof {
            src.len()
        } else {
            t.span.start
        };
        parser.report(
            RawDiagnosticKind::MissingEndobj,
            Span::new(stopped_at, stopped_at),
        );
        value.span.end.max(offset)
    };
    sink.extend(parser.take_diagnostics());
    Some(ParsedObject {
        num,
        generation,
        value,
        span: Span::new(offset, end.max(offset)),
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::test_util::indirect;
    use super::*;

    #[test]
    fn indirect_object_without_defects() {
        let src = b"4 0 obj\n<< /K 1 >>\nendobj\n";
        let (parsed, diags) = indirect(src);

        assert_eq!((parsed.num, parsed.generation), (4, 0));
        assert_eq!(parsed.span, Span::new(0, 25));
        assert_eq!(parsed.value.span, Span::new(8, 18));
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn missing_endobj_is_reported_at_the_next_token() {
        let src = b"1 0 obj\n42\n2 0 obj\n43\nendobj";
        let (parsed, diags) = indirect(src);

        assert_eq!(parsed.value.value, Object::Int(42));
        assert_eq!(parsed.span, Span::new(0, 10));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::MissingEndobj,
                span: Span::new(11, 11)
            }]
        );
    }

    #[test]
    fn empty_object_body_is_null() {
        let (parsed, diags) = indirect(b"1 0 obj endobj");

        assert_eq!(parsed.value.value, Object::Null);
        assert_eq!(
            diags.iter().map(|d| d.kind.code()).collect::<Vec<_>>(),
            vec!["empty-object-body"]
        );
    }

    #[test]
    fn headers_must_be_exact() {
        assert_eq!(read_obj_header(b"12 3 obj<<", 0), Some((12, 3, 8)));
        assert_eq!(read_obj_header(b" 12 3 obj", 0), None);
        assert_eq!(read_obj_header(b"12 3 object", 0), None);
        assert_eq!(read_obj_header(b"12 3obj", 0), None);
        assert_eq!(read_obj_header(b"99999999999 0 obj", 0), None);
    }
}
