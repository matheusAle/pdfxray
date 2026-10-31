//! The `startxref` keyword, the offset after it, and the `%%EOF` marker that closes a revision.

use memchr::memmem;

use super::StartXref;
use crate::Span;
use crate::lexer::{Lexer, TokenKind};
use crate::object::is_whitespace;
use crate::parser::skip_whitespace;

fn is_keyword_boundary_before(src: &[u8], at: usize) -> bool {
    at == 0
        || src
            .get(at - 1)
            .is_some_and(|&b| is_whitespace(b) || crate::object::is_delimiter(b))
}

/// Reads the offset after a `startxref` keyword at `keyword`.
fn startxref_at(src: &[u8], keyword: usize) -> Option<StartXref> {
    let mut lexer = Lexer::at(src, keyword + b"startxref".len());
    let token = lexer.next_significant();
    let TokenKind::Int(n) = token.kind else {
        return None;
    };
    Some(StartXref {
        offset: u64::try_from(n).ok()?,
        span: Span::new(keyword, token.span.end),
    })
}

/// The last `startxref` in the file.
pub(super) fn final_startxref(src: &[u8]) -> Option<StartXref> {
    let mut end = src.len();
    while let Some(at) = memmem::rfind(&src[..end], b"startxref") {
        if is_keyword_boundary_before(src, at) {
            return startxref_at(src, at);
        }
        end = at;
    }
    None
}

/// The first `startxref` at or after `from`.
pub(super) fn startxref_after(src: &[u8], from: usize) -> Option<StartXref> {
    let mut at = from.min(src.len());
    while let Some(found) = memmem::find(&src[at..], b"startxref") {
        let keyword = at + found;
        if is_keyword_boundary_before(src, keyword) {
            return startxref_at(src, keyword);
        }
        at = keyword + 1;
    }
    None
}

/// The `%%EOF` marker right after `from` (only white space between).
pub(super) fn eof_after(src: &[u8], from: usize) -> Option<Span> {
    let at = skip_whitespace(src, from);
    src.get(at..)?
        .starts_with(b"%%EOF")
        .then(|| Span::new(at, at + 5))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn startxref_helpers_read_the_last_one_with_spans() {
        let src = b"startxref\n12\n%%EOF\nstartxref\n34\n%%EOF\n";
        let first = StartXref {
            offset: 12,
            span: Span::new(0, 12),
        };
        let last = StartXref {
            offset: 34,
            span: Span::new(19, 31),
        };

        assert_eq!(final_startxref(src), Some(last));
        assert_eq!(startxref_after(src, 0), Some(first));
        assert_eq!(startxref_after(src, 1), Some(last));
        assert_eq!(eof_after(src, 12), Some(Span::new(13, 18)));
        assert_eq!(eof_after(src, 0), None);
        assert_eq!(final_startxref(b"nostartxref 5"), None);
        assert_eq!(startxref_after(b"startxref x", 0), None);
    }
}
