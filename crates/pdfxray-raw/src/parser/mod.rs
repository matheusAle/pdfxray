//! Object-level parsing on top of the [`Lexer`]: direct objects, indirect objects and streams.
//!
//! This module holds the [`Parser`] for direct objects (scalars, references, arrays and
//! dictionaries). [`indirect`] reads `num gen obj ... endobj` around it, and [`stream`] reads the
//! data of a stream object and finds where it ends.

mod indirect;
mod stream;
#[cfg(test)]
mod test_util;

pub(crate) use indirect::{LengthLookup, LengthSource, NoLengths, parse_indirect, read_obj_header};

use crate::lexer::{Lexer, Token, TokenKind};
use crate::object::{Dict, Object, Spanned, is_whitespace};
use crate::{ContainerKind, ObjRef, RawDiagnostic, RawDiagnosticKind, Span};

/// How deeply arrays and dictionaries may nest before parsing gives up.
pub const MAX_NESTING: usize = 96;

/// Why [`Parser::value`] did not produce an object. The lexer is left before the offending token
/// (except for [`Stop::TooDeep`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stop {
    Eof,
    /// `]` or `>>`.
    Closer,
    /// A keyword that is not an object (`endobj`, `stream`, `obj`, `R`, ...).
    Keyword,
    TooDeep,
}

pub(crate) struct Parser<'a> {
    pub(crate) lex: Lexer<'a>,
    too_deep: bool,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(src: &'a [u8], pos: usize) -> Self {
        Self {
            lex: Lexer::at(src, pos),
            too_deep: false,
        }
    }

    fn report(&mut self, kind: RawDiagnosticKind, span: Span) {
        // The lexer owns the diagnostics so that backtracking can drop them together.
        self.lex.push_diagnostic(RawDiagnostic { kind, span });
    }

    pub(crate) fn take_diagnostics(&mut self) -> Vec<RawDiagnostic> {
        self.lex.take_diagnostics()
    }

    fn rewind(&mut self, to: usize) {
        self.lex.set_pos(to);
    }

    /// Parses one direct object. Streams are never produced here.
    pub(crate) fn value(&mut self, depth: usize) -> Result<Spanned<Object>, Stop> {
        if self.too_deep {
            return Err(Stop::TooDeep);
        }
        let before = self.lex.pos();
        let token = self.lex.next_significant();
        let Token { kind, span } = token;
        match kind {
            TokenKind::Eof | TokenKind::Comment => Err(Stop::Eof),
            TokenKind::Int(n) => Ok(self.int_or_ref(n, span)),
            TokenKind::Real(v) => Ok(Spanned::new(Object::Real(v), span)),
            TokenKind::String(s) => Ok(Spanned::new(Object::String(s), span)),
            TokenKind::Name(n) => Ok(Spanned::new(Object::Name(n), span)),
            TokenKind::ArrayOpen => self.array(span, depth),
            TokenKind::DictOpen => self
                .dict(span, depth)
                .map(|(d, sp)| Spanned::new(Object::Dict(d), sp)),
            TokenKind::ArrayClose | TokenKind::DictClose => {
                self.rewind(before);
                Err(Stop::Closer)
            }
            TokenKind::Keyword => {
                let object = match self.lex.text(span) {
                    b"true" => Some(Object::Bool(true)),
                    b"false" => Some(Object::Bool(false)),
                    b"null" => Some(Object::Null),
                    _ => None,
                };
                if let Some(object) = object {
                    Ok(Spanned::new(object, span))
                } else {
                    self.rewind(before);
                    Err(Stop::Keyword)
                }
            }
        }
    }

    /// After an integer: `num gen R` becomes a reference, anything else stays an integer.
    fn int_or_ref(&mut self, n: i64, span: Span) -> Spanned<Object> {
        let int = Spanned::new(Object::Int(n), span);
        let Ok(num) = u32::try_from(n) else {
            return int;
        };
        let resume = self.lex.pos();
        let diag_len = self.lex.diagnostics().len();
        let second = self.lex.next_significant();
        if let TokenKind::Int(g) = second.kind
            && let Ok(generation) = u16::try_from(g)
        {
            let third = self.lex.next_significant();
            if third.kind == TokenKind::Keyword && self.lex.text(third.span) == b"R" {
                return Spanned::new(
                    Object::Ref(ObjRef::new(num, generation)),
                    Span::new(span.start, third.span.end),
                );
            }
        }
        self.rewind(resume);
        self.lex.truncate_diagnostics(diag_len);
        int
    }

    fn array(&mut self, open: Span, depth: usize) -> Result<Spanned<Object>, Stop> {
        if depth >= MAX_NESTING {
            self.too_deep = true;
            self.report(RawDiagnosticKind::NestingTooDeep, open);
            return Err(Stop::TooDeep);
        }
        let mut items = Vec::new();
        loop {
            match self.value(depth + 1) {
                Ok(item) => items.push(item),
                Err(Stop::Closer) => {
                    let before = self.lex.pos();
                    let closer = self.lex.next_significant();
                    if closer.kind == TokenKind::ArrayClose {
                        return Ok(Spanned::new(
                            Object::Array(items),
                            Span::new(open.start, closer.span.end),
                        ));
                    }
                    // `>>` inside an array: the array is unterminated, the dictionary may close.
                    self.rewind(before);
                    return Ok(self.unterminated_array(items, open));
                }
                Err(Stop::Keyword) => {
                    let before = self.lex.pos();
                    let t = self.lex.next_significant();
                    if is_structural_keyword(self.lex.text(t.span)) {
                        self.rewind(before);
                        return Ok(self.unterminated_array(items, open));
                    }
                    let text = keyword_text(self.lex.text(t.span));
                    self.report(RawDiagnosticKind::UnexpectedKeyword { text }, t.span);
                }
                Err(Stop::Eof | Stop::TooDeep) => return Ok(self.unterminated_array(items, open)),
            }
        }
    }

    fn unterminated_array(&mut self, items: Vec<Spanned<Object>>, open: Span) -> Spanned<Object> {
        let end = items.last().map_or(open.end, |i| i.span.end).max(open.end);
        let span = Span::new(open.start, end);
        if !self.too_deep {
            self.report(
                RawDiagnosticKind::UnterminatedContainer {
                    container: ContainerKind::Array,
                },
                span,
            );
        }
        Spanned::new(Object::Array(items), span)
    }

    fn dict(&mut self, open: Span, depth: usize) -> Result<(Dict, Span), Stop> {
        if depth >= MAX_NESTING {
            self.too_deep = true;
            self.report(RawDiagnosticKind::NestingTooDeep, open);
            return Err(Stop::TooDeep);
        }
        let mut dict = Dict::default();
        loop {
            if self.too_deep {
                return Ok(self.unterminated_dict(dict, open));
            }
            let before = self.lex.pos();
            let token = self.lex.next_significant();
            match token.kind {
                TokenKind::DictClose => {
                    return Ok((dict, Span::new(open.start, token.span.end)));
                }
                TokenKind::Eof => return Ok(self.unterminated_dict(dict, open)),
                TokenKind::Name(key) => {
                    let key = Spanned::new(key, token.span);
                    match self.value(depth + 1) {
                        Ok(value) => {
                            if dict.entries.iter().any(|(k, _)| k.value == key.value) {
                                self.report(
                                    RawDiagnosticKind::DuplicateDictKey {
                                        key: key.value.clone(),
                                    },
                                    key.span,
                                );
                            }
                            dict.entries.push((key, value));
                        }
                        Err(Stop::Closer) => {
                            self.report(
                                RawDiagnosticKind::DictValueMissing {
                                    key: key.value.clone(),
                                },
                                key.span,
                            );
                            let at = self.lex.pos();
                            let next = self.lex.next_significant();
                            if next.kind == TokenKind::DictClose {
                                return Ok((dict, Span::new(open.start, next.span.end)));
                            }
                            self.rewind(at);
                            return Ok(self.unterminated_dict(dict, open));
                        }
                        Err(Stop::Keyword) => {
                            let at = self.lex.pos();
                            let t = self.lex.next_significant();
                            if is_structural_keyword(self.lex.text(t.span)) {
                                self.report(
                                    RawDiagnosticKind::DictValueMissing {
                                        key: key.value.clone(),
                                    },
                                    key.span,
                                );
                                self.rewind(at);
                                return Ok(self.unterminated_dict(dict, open));
                            }
                            let text = keyword_text(self.lex.text(t.span));
                            self.report(RawDiagnosticKind::UnexpectedKeyword { text }, t.span);
                        }
                        Err(Stop::Eof | Stop::TooDeep) => {
                            return Ok(self.unterminated_dict(dict, open));
                        }
                    }
                }
                TokenKind::Keyword => {
                    if is_structural_keyword(self.lex.text(token.span)) {
                        self.rewind(before);
                        return Ok(self.unterminated_dict(dict, open));
                    }
                    self.report(RawDiagnosticKind::DictKeyNotName, token.span);
                }
                TokenKind::ArrayClose => {
                    self.report(RawDiagnosticKind::DictKeyNotName, token.span);
                }
                TokenKind::Comment => {}
                TokenKind::Int(_)
                | TokenKind::Real(_)
                | TokenKind::String(_)
                | TokenKind::ArrayOpen
                | TokenKind::DictOpen => {
                    self.report(RawDiagnosticKind::DictKeyNotName, token.span);
                    self.rewind(before);
                    // Skip the whole stray object so its contents are not read as keys.
                    if self.value(depth + 1).is_err() {
                        let _ = self.lex.next_significant();
                    }
                }
            }
        }
    }

    fn unterminated_dict(&mut self, dict: Dict, open: Span) -> (Dict, Span) {
        let end = dict
            .entries
            .last()
            .map_or(open.end, |(_, v)| v.span.end)
            .max(open.end);
        let span = Span::new(open.start, end);
        if !self.too_deep {
            self.report(
                RawDiagnosticKind::UnterminatedContainer {
                    container: ContainerKind::Dict,
                },
                span,
            );
        }
        (dict, span)
    }
}

fn is_structural_keyword(text: &[u8]) -> bool {
    matches!(
        text,
        b"endobj" | b"stream" | b"endstream" | b"obj" | b"xref" | b"trailer" | b"startxref"
    )
}

fn keyword_text(text: &[u8]) -> String {
    let shown = &text[..text.len().min(32)];
    String::from_utf8_lossy(shown).into_owned()
}

/// Parses one direct object at `offset`. The spans are offsets into `source`.
///
/// Returns `None` when there is no object at `offset`.
pub fn parse_object(source: &[u8], offset: usize) -> (Option<Spanned<Object>>, Vec<RawDiagnostic>) {
    let mut parser = Parser::new(source, offset);
    let value = parser.value(0).ok();
    (value, parser.take_diagnostics())
}

/// Offset of the first non-white-space byte at or after `pos`.
pub(crate) fn skip_whitespace(src: &[u8], pos: usize) -> usize {
    let mut p = pos.min(src.len());
    while src.get(p).copied().is_some_and(is_whitespace) {
        p += 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::test_util::{as_array, as_dict};
    use super::*;
    use crate::{Name, PdfString};

    fn name(s: &str) -> Name {
        Name::new(s)
    }

    fn obj(src: &[u8]) -> (Spanned<Object>, Vec<RawDiagnostic>) {
        let (value, diags) = parse_object(src, 0);
        (value.unwrap(), diags)
    }

    fn sp(o: Object, start: usize, end: usize) -> Spanned<Object> {
        Spanned::new(o, Span::new(start, end))
    }

    #[test]
    fn scalars_keep_their_spans() {
        assert_eq!(obj(b"  true").0, sp(Object::Bool(true), 2, 6));
        assert_eq!(obj(b"null").0, sp(Object::Null, 0, 4));
        assert_eq!(obj(b"-3.5").0, sp(Object::Real(-3.5), 0, 4));
        assert_eq!(obj(b"/A#42").0, sp(Object::Name(name("AB")), 0, 5));
        assert_eq!(
            obj(b"(hi)").0,
            sp(
                Object::String(PdfString {
                    bytes: b"hi".to_vec(),
                    hex: false
                }),
                0,
                4
            )
        );
    }

    #[test]
    fn references_need_all_three_tokens() {
        assert_eq!(obj(b"12 0 R").0, sp(Object::Ref(ObjRef::new(12, 0)), 0, 6));
        assert_eq!(obj(b"12  7 %c\n R").0, sp(Object::Ref(ObjRef::new(12, 7)), 0, 11));
        assert_eq!(obj(b"12 0 obj").0, sp(Object::Int(12), 0, 2));
        assert_eq!(obj(b"-1 0 R").0, sp(Object::Int(-1), 0, 2));
        assert_eq!(obj(b"1 70000 R").0, sp(Object::Int(1), 0, 1));
    }

    #[test]
    fn arrays_hold_spanned_elements() {
        let (value, diags) = obj(b"[1 2 0 R /N (s) [<41>] ]");

        assert_eq!(diags, vec![]);
        assert_eq!(value.span, Span::new(0, 24));
        let items = as_array(&value);
        let spans: Vec<_> = items.iter().map(|i| i.span).collect();
        assert_eq!(
            spans,
            vec![
                Span::new(1, 2),
                Span::new(3, 8),
                Span::new(9, 11),
                Span::new(12, 15),
                Span::new(16, 22),
            ]
        );
    }

    #[test]
    fn dicts_keep_order_and_duplicates() {
        let (value, diags) = obj(b"<< /A 1 /B [2] /A 3 >>");

        let dict = as_dict(&value);
        assert_eq!(
            dict.keys().map(Name::to_string_lossy).collect::<Vec<_>>(),
            vec!["A", "B", "A"]
        );
        assert_eq!(dict.get(b"A").map(|v| &v.value), Some(&Object::Int(1)));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::DuplicateDictKey { key: name("A") },
                span: Span::new(15, 17)
            }]
        );
        assert_eq!(value.span, Span::new(0, 22));
    }

    #[test]
    fn display_round_trips_through_the_parser() {
        let src =
            b"<< /Type /Page /Kids [1 0 R 2 0 R] /S (a\\(b) /H <0A> /R 1.5 /N#20x -2 /T true /Z null >>";
        let (first, _) = obj(src);
        let printed = first.value.to_string();
        let (second, diags) = obj(printed.as_bytes());

        assert_eq!(diags, vec![]);
        assert_eq!(second.value, first.value);
    }

    #[test]
    fn unterminated_containers_are_reported() {
        let (value, diags) = obj(b"<< /A [1 2");

        let dict = as_dict(&value);
        assert_eq!(dict.len(), 1);
        assert_eq!(
            diags.iter().map(|d| d.kind.clone()).collect::<Vec<_>>(),
            vec![
                RawDiagnosticKind::UnterminatedContainer {
                    container: ContainerKind::Array
                },
                RawDiagnosticKind::UnterminatedContainer {
                    container: ContainerKind::Dict
                },
            ]
        );
    }

    #[test]
    fn dict_defects_do_not_stop_parsing() {
        let (value, diags) = obj(b"<< /A 1 7 /B /C >> ");

        let dict = as_dict(&value);
        assert_eq!(dict.len(), 2);
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::DictKeyNotName,
                span: Span::new(8, 9)
            }]
        );

        let (_, diags) = obj(b"<< /A >>");
        assert_eq!(
            diags[0],
            RawDiagnostic {
                kind: RawDiagnosticKind::DictValueMissing { key: name("A") },
                span: Span::new(3, 5)
            }
        );
    }

    #[test]
    fn nesting_is_bounded() {
        let src = vec![b'['; MAX_NESTING * 4];
        let (value, diags) = parse_object(&src, 0);

        drop(value);
        assert!(diags.iter().any(|d| d.kind == RawDiagnosticKind::NestingTooDeep));
    }

    proptest! {
        #[test]
        fn parsing_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let (value, diags) = parse_object(&bytes, 0);
            if let Some(v) = value {
                prop_assert!(v.span.end <= bytes.len());
            }
            let mut sink = Vec::new();
            let _ = parse_indirect(&bytes, 0, &NoLengths, &mut sink);
            for d in diags.iter().chain(&sink) {
                prop_assert!(d.span.start <= d.span.end && d.span.end <= bytes.len());
            }
        }
    }

    #[test]
    fn nested_values_and_strings_keep_exact_spans() {
        let (value, diags) = obj(b"<< /A (x\\)y) /B <4a> /C [/N 1.5 true] >>");

        assert_eq!(diags, vec![]);
        let d = as_dict(&value);
        let spans: Vec<_> = d.entries.iter().map(|(k, v)| (k.span, v.span)).collect();
        assert_eq!(
            spans,
            vec![
                (Span::new(3, 5), Span::new(6, 12)),
                (Span::new(13, 15), Span::new(16, 20)),
                (Span::new(21, 23), Span::new(24, 37)),
            ]
        );
        assert_eq!(
            d.get(b"A").unwrap().value,
            Object::String(PdfString {
                bytes: b"x)y".to_vec(),
                hex: false
            })
        );
    }
}
