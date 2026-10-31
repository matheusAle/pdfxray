//! Byte-exact tokenizer for PDF syntax (ISO 32000-2 §7.2).

use crate::object::{is_delimiter, is_regular, is_whitespace};
use crate::{Name, PdfString, RawDiagnostic, RawDiagnosticKind, Span};

/// What a [`Token`] is.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An integer such as `42`, `-17`, `+3`.
    Int(i64),
    /// A real such as `3.14`, `-.5`, `4.`, `+.5`.
    Real(f64),
    /// A literal or hexadecimal string, already decoded.
    String(PdfString),
    /// A name, `#xx` escapes decoded.
    Name(Name),
    /// `[`
    ArrayOpen,
    /// `]`
    ArrayClose,
    /// `<<`
    DictOpen,
    /// `>>`
    DictClose,
    /// Any other run of regular characters (`obj`, `R`, `true`, `stream`, ...) or a stray delimiter.
    /// The text is the token's span in the source.
    Keyword,
    /// A `%` comment up to (not including) the end of the line.
    Comment,
    /// The end of the data. The span is empty and at the end.
    Eof,
}

/// A token and the exact bytes it covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What it is.
    pub kind: TokenKind,
    /// Its bytes in the source.
    pub span: Span,
}

/// Splits bytes into [`Token`]s without ever failing: malformed input produces the best token it
/// can plus a [`RawDiagnostic`].
#[derive(Debug, Clone)]
pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    diagnostics: Vec<RawDiagnostic>,
}

impl<'a> Lexer<'a> {
    /// A lexer positioned at the start of `src`.
    pub fn new(src: &'a [u8]) -> Self {
        Self::at(src, 0)
    }

    /// A lexer positioned at `pos` (clamped to the length of `src`).
    pub fn at(src: &'a [u8], pos: usize) -> Self {
        Self {
            src,
            pos: pos.min(src.len()),
            diagnostics: Vec::new(),
        }
    }

    /// The bytes being tokenized.
    pub fn source(&self) -> &'a [u8] {
        self.src
    }

    /// Offset of the next unread byte.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Moves to `pos` (clamped to the length of the source).
    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos.min(self.src.len());
    }

    /// Diagnostics recorded so far.
    pub fn diagnostics(&self) -> &[RawDiagnostic] {
        &self.diagnostics
    }

    /// Removes and returns the recorded diagnostics.
    pub fn take_diagnostics(&mut self) -> Vec<RawDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Drops diagnostics recorded after the first `len` (used when backtracking).
    pub(crate) fn truncate_diagnostics(&mut self, len: usize) {
        self.diagnostics.truncate(len);
    }

    /// The text of a keyword token.
    pub fn text(&self, span: Span) -> &'a [u8] {
        span.slice(self.src).unwrap_or_default()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.src.get(self.pos.checked_add(ahead)?).copied()
    }

    fn report(&mut self, kind: RawDiagnosticKind, span: Span) {
        self.diagnostics.push(RawDiagnostic { kind, span });
    }

    pub(crate) fn push_diagnostic(&mut self, d: RawDiagnostic) {
        self.diagnostics.push(d);
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(is_whitespace) {
            self.pos += 1;
        }
    }

    /// Reads the next token, including comments. Returns [`TokenKind::Eof`] at the end, forever.
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        let start = self.pos;
        let Some(b) = self.peek() else {
            return Token {
                kind: TokenKind::Eof,
                span: Span::new(start, start),
            };
        };
        let kind = match b {
            b'%' => {
                while self.peek().is_some_and(|c| c != b'\n' && c != b'\r') {
                    self.pos += 1;
                }
                TokenKind::Comment
            }
            b'(' => self.literal_string(),
            b'<' if self.peek_at(1) == Some(b'<') => {
                self.pos += 2;
                TokenKind::DictOpen
            }
            b'<' => self.hex_string(),
            b'>' if self.peek_at(1) == Some(b'>') => {
                self.pos += 2;
                TokenKind::DictClose
            }
            b'[' => {
                self.pos += 1;
                TokenKind::ArrayOpen
            }
            b']' => {
                self.pos += 1;
                TokenKind::ArrayClose
            }
            b'/' => self.name(),
            b')' | b'>' | b'{' | b'}' => {
                self.pos += 1;
                TokenKind::Keyword
            }
            _ => self.regular(),
        };
        Token {
            kind,
            span: Span::new(start, self.pos),
        }
    }

    /// Reads the next token, skipping comments.
    pub fn next_significant(&mut self) -> Token {
        loop {
            let token = self.next_token();
            if token.kind != TokenKind::Comment {
                return token;
            }
        }
    }

    fn regular(&mut self) -> TokenKind {
        let start = self.pos;
        while self.peek().is_some_and(is_regular) {
            self.pos += 1;
        }
        let text = &self.src[start..self.pos];
        match number(text) {
            Some(Number::Int(v)) => TokenKind::Int(v),
            Some(Number::Real(v)) => TokenKind::Real(v),
            Some(Number::Overflow(v)) => {
                self.report(RawDiagnosticKind::IntegerOverflow, Span::new(start, self.pos));
                TokenKind::Real(v)
            }
            None => TokenKind::Keyword,
        }
    }

    fn name(&mut self) -> TokenKind {
        let slash = self.pos;
        self.pos += 1;
        let mut out = Vec::new();
        while let Some(b) = self.peek().filter(|&b| is_regular(b)) {
            if b == b'#' {
                let hi = self.peek_at(1).and_then(hex_value);
                let lo = self.peek_at(2).and_then(hex_value);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push(hi << 4 | lo);
                    self.pos += 3;
                    continue;
                }
                self.report(
                    RawDiagnosticKind::InvalidNameEscape,
                    Span::new(self.pos, self.pos + 1),
                );
            }
            out.push(b);
            self.pos += 1;
        }
        debug_assert!(self.pos > slash);
        TokenKind::Name(Name(out))
    }

    fn literal_string(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1;
        let mut depth = 1usize;
        let mut out = Vec::new();
        loop {
            let Some(b) = self.peek() else {
                self.report(RawDiagnosticKind::UnterminatedString, Span::new(start, self.pos));
                break;
            };
            self.pos += 1;
            match b {
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b);
                }
                b'\\' => {
                    let Some(c) = self.peek() else {
                        self.report(RawDiagnosticKind::UnterminatedString, Span::new(start, self.pos));
                        break;
                    };
                    self.pos += 1;
                    match c {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut value = u32::from(c - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        value = value * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            // High-order overflow is ignored (ISO 32000-2 §7.3.4.2).
                            out.push(u8::try_from(value & 0xFF).unwrap_or_default());
                        }
                        // `\(`, `\)`, `\\` and any unknown escape: the backslash is ignored.
                        other => out.push(other),
                    }
                }
                b'\r' => {
                    if self.peek() == Some(b'\n') {
                        self.pos += 1;
                    }
                    out.push(b'\n');
                }
                _ => out.push(b),
            }
        }
        TokenKind::String(PdfString {
            bytes: out,
            hex: false,
        })
    }

    fn hex_string(&mut self) -> TokenKind {
        let start = self.pos;
        self.pos += 1;
        let mut out = Vec::new();
        let mut pending: Option<u8> = None;
        loop {
            let Some(b) = self.peek() else {
                self.report(RawDiagnosticKind::UnterminatedString, Span::new(start, self.pos));
                break;
            };
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if is_whitespace(b) {
                continue;
            }
            let Some(v) = hex_value(b) else {
                self.report(
                    RawDiagnosticKind::HexStringInvalidChar { byte: b },
                    Span::new(self.pos - 1, self.pos),
                );
                continue;
            };
            match pending.take() {
                Some(hi) => out.push(hi << 4 | v),
                None => pending = Some(v),
            }
        }
        if let Some(hi) = pending {
            out.push(hi << 4);
            self.report(RawDiagnosticKind::HexStringOddDigits, Span::new(start, self.pos));
        }
        TokenKind::String(PdfString {
            bytes: out,
            hex: true,
        })
    }
}

/// Value of an ASCII hexadecimal digit.
pub(crate) const fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

enum Number {
    Int(i64),
    Real(f64),
    /// An integer literal too large for `i64`, read as a real.
    Overflow(f64),
}

/// Parses `[+-]digits`, `[+-]digits.digits`, `[+-].digits`, `[+-]digits.` and nothing else.
fn number(text: &[u8]) -> Option<Number> {
    let body = match text.first()? {
        b'+' | b'-' => &text[1..],
        _ => text,
    };
    let dots = memchr::memchr_iter(b'.', body).count();
    let digits = body.iter().filter(|b| b.is_ascii_digit()).count();
    if dots > 1 || digits == 0 || digits + dots != body.len() {
        return None;
    }
    let s = std::str::from_utf8(text).ok()?;
    if dots == 0 {
        return match s.parse::<i64>() {
            Ok(v) => Some(Number::Int(v)),
            Err(_) => s.parse::<f64>().ok().map(Number::Overflow),
        };
    }
    s.parse::<f64>().ok().map(Number::Real)
}

/// True when `b` ends a keyword that starts with regular characters.
pub(crate) fn ends_keyword(b: Option<u8>) -> bool {
    b.is_none_or(|b| is_whitespace(b) || is_delimiter(b))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;

    use super::*;

    fn lex_all(src: &[u8]) -> (Vec<Token>, Vec<RawDiagnostic>) {
        let mut lexer = Lexer::new(src);
        let mut tokens = Vec::new();
        loop {
            let t = lexer.next_token();
            let done = t.kind == TokenKind::Eof;
            tokens.push(t);
            if done {
                break;
            }
        }
        (tokens, lexer.take_diagnostics())
    }

    fn tok(kind: TokenKind, start: usize, end: usize) -> Token {
        Token {
            kind,
            span: Span::new(start, end),
        }
    }

    fn string(bytes: &[u8], hex: bool) -> TokenKind {
        TokenKind::String(PdfString {
            bytes: bytes.to_vec(),
            hex,
        })
    }

    #[test]
    fn numbers_in_every_form() {
        let (tokens, diags) = lex_all(b"0 +17 -3. +.5 .25 4.0 -.002 123456789012");

        assert_eq!(
            tokens,
            vec![
                tok(TokenKind::Int(0), 0, 1),
                tok(TokenKind::Int(17), 2, 5),
                tok(TokenKind::Real(-3.0), 6, 9),
                tok(TokenKind::Real(0.5), 10, 13),
                tok(TokenKind::Real(0.25), 14, 17),
                tok(TokenKind::Real(4.0), 18, 21),
                tok(TokenKind::Real(-0.002), 22, 27),
                tok(TokenKind::Int(123_456_789_012), 28, 40),
                tok(TokenKind::Eof, 40, 40),
            ]
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn malformed_numbers_are_keywords() {
        let (tokens, _) = lex_all(b"1.2.3 - . +x 1-2 --5");

        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![&TokenKind::Keyword; 6]
                .into_iter()
                .chain([&TokenKind::Eof])
                .collect::<Vec<_>>()
        );
        assert_eq!(tokens[0].span, Span::new(0, 5));
    }

    #[test]
    fn integer_overflow_becomes_real_with_diagnostic() {
        let (tokens, diags) = lex_all(b"99999999999999999999");

        assert_eq!(tokens[0].kind, TokenKind::Real(1e20));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::IntegerOverflow,
                span: Span::new(0, 20)
            }]
        );
    }

    #[test]
    fn literal_string_escapes() {
        let (tokens, diags) = lex_all(b"(a\\n\\r\\t\\b\\f\\(\\)\\\\ \\101\\60\\0053\\q)");

        assert_eq!(
            tokens[0],
            tok(string(b"a\n\r\t\x08\x0c()\\ A0\x053q", false), 0, 34)
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn literal_string_balanced_parens_and_eols() {
        let (tokens, _) = lex_all(b"(a(b(c))d)(x\\\ny)(p\\\r\nq)(r\r\ns\rt)");

        assert_eq!(tokens[0], tok(string(b"a(b(c))d", false), 0, 10));
        assert_eq!(tokens[1], tok(string(b"xy", false), 10, 16));
        assert_eq!(tokens[2], tok(string(b"pq", false), 16, 23));
        assert_eq!(tokens[3], tok(string(b"r\ns\nt", false), 23, 31));
    }

    #[test]
    fn octal_overflow_keeps_low_byte() {
        let (tokens, _) = lex_all(b"(\\777)");

        assert_eq!(tokens[0].kind, string(&[0xFF], false));
    }

    #[test]
    fn unterminated_literal_string_runs_to_the_end() {
        let (tokens, diags) = lex_all(b"(abc");

        assert_eq!(tokens[0], tok(string(b"abc", false), 0, 4));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::UnterminatedString,
                span: Span::new(0, 4)
            }]
        );
    }

    #[test]
    fn hex_strings() {
        let (tokens, diags) = lex_all(b"<48 65\n6C6c6F><> <ABC>");

        assert_eq!(tokens[0], tok(string(b"Hello", true), 0, 14));
        assert_eq!(tokens[1], tok(string(b"", true), 14, 16));
        assert_eq!(tokens[2], tok(string(&[0xAB, 0xC0], true), 17, 22));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::HexStringOddDigits,
                span: Span::new(17, 22)
            }]
        );
    }

    #[test]
    fn hex_string_invalid_char_is_reported_and_skipped() {
        let (tokens, diags) = lex_all(b"<4Z1>");

        assert_eq!(tokens[0].kind, string(&[0x41], true));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::HexStringInvalidChar { byte: b'Z' },
                span: Span::new(2, 3)
            }]
        );
    }

    #[test]
    fn names_decode_escapes() {
        let (tokens, diags) = lex_all(b"/Name /A#20B /#41 / /a#zz/b");

        assert_eq!(tokens[0], tok(TokenKind::Name(Name::new("Name")), 0, 5));
        assert_eq!(tokens[1], tok(TokenKind::Name(Name::new("A B")), 6, 12));
        assert_eq!(tokens[2], tok(TokenKind::Name(Name::new("A")), 13, 17));
        assert_eq!(tokens[3], tok(TokenKind::Name(Name::new("")), 18, 19));
        assert_eq!(tokens[4], tok(TokenKind::Name(Name::new("a#zz")), 20, 25));
        assert_eq!(tokens[5], tok(TokenKind::Name(Name::new("b")), 25, 27));
        assert_eq!(
            diags,
            vec![RawDiagnostic {
                kind: RawDiagnosticKind::InvalidNameEscape,
                span: Span::new(22, 23)
            }]
        );
    }

    #[test]
    fn delimiters_keywords_and_comments() {
        let (tokens, _) = lex_all(b"<< /K [1] >> obj%c\nR endobj>");

        let kinds: Vec<_> = tokens.iter().map(|t| (t.kind.clone(), t.span)).collect();
        assert_eq!(
            kinds,
            vec![
                (TokenKind::DictOpen, Span::new(0, 2)),
                (TokenKind::Name(Name::new("K")), Span::new(3, 5)),
                (TokenKind::ArrayOpen, Span::new(6, 7)),
                (TokenKind::Int(1), Span::new(7, 8)),
                (TokenKind::ArrayClose, Span::new(8, 9)),
                (TokenKind::DictClose, Span::new(10, 12)),
                (TokenKind::Keyword, Span::new(13, 16)),
                (TokenKind::Comment, Span::new(16, 18)),
                (TokenKind::Keyword, Span::new(19, 20)),
                (TokenKind::Keyword, Span::new(21, 27)),
                (TokenKind::Keyword, Span::new(27, 28)),
                (TokenKind::Eof, Span::new(28, 28)),
            ]
        );
    }

    #[test]
    fn next_significant_skips_comments() {
        let mut lexer = Lexer::new(b"%one\r%two\n 7");

        assert_eq!(lexer.next_significant(), tok(TokenKind::Int(7), 11, 12));
        assert_eq!(lexer.next_significant().kind, TokenKind::Eof);
    }

    #[test]
    fn eof_is_sticky_and_set_pos_clamps() {
        let mut lexer = Lexer::at(b"ab", 99);

        assert_eq!(lexer.pos(), 2);
        assert_eq!(lexer.next_token(), tok(TokenKind::Eof, 2, 2));
        assert_eq!(lexer.next_token(), tok(TokenKind::Eof, 2, 2));
        lexer.set_pos(0);
        assert_eq!(lexer.next_token().kind, TokenKind::Keyword);
    }

    proptest! {
        #[test]
        fn never_panics_and_spans_are_inside_the_input(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let (tokens, diags) = lex_all(&bytes);
            let mut last_end = 0;
            for t in &tokens {
                prop_assert!(t.span.start <= t.span.end && t.span.end <= bytes.len());
                prop_assert!(t.span.start >= last_end);
                last_end = t.span.end;
            }
            prop_assert!(tokens.last().is_some_and(|t| t.kind == TokenKind::Eof));
            for d in &diags {
                prop_assert!(d.span.start <= d.span.end && d.span.end <= bytes.len());
            }
        }
    }
}
