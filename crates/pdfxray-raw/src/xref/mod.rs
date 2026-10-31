//! Cross-reference sections: classic tables, xref streams, hybrid files and the revision chain.
//!
//! This module holds the data model and [`parse_section_at`], which reads whichever kind of
//! section starts at an offset. The readers live in submodules: [`table`] for classic `xref`
//! tables, [`stream`] for cross-reference streams, [`startxref`] for the `startxref` / `%%EOF`
//! markers, and [`chain`] for following `/Prev` through the revisions (and rebuilding when that
//! fails).

mod chain;
mod startxref;
mod stream;
mod table;
#[cfg(test)]
mod test_util;

pub(crate) use chain::build;

use crate::lexer::ends_keyword;
use crate::object::Dict;
use crate::parser::skip_whitespace;
use crate::{ObjRef, RawDiagnostic, RawDiagnosticKind, Span};

/// What an xref entry says about an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    /// A free entry; `next` is the next object of the free list.
    Free {
        /// Object number of the next free entry (`0` ends the list).
        next: u32,
    },
    /// An object stored directly in the file body.
    InUse {
        /// Byte offset of `num gen obj`, exactly as recorded.
        offset: u64,
    },
    /// An object stored in an object stream (xref streams only).
    Compressed {
        /// Object number of the containing `/ObjStm`.
        stream: u32,
        /// Index of the object inside that stream.
        index: u32,
    },
}

impl EntryState {
    /// True for [`EntryState::Free`].
    pub const fn is_free(self) -> bool {
        matches!(self, Self::Free { .. })
    }
}

/// One xref entry exactly as recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XrefEntry {
    /// Object number.
    pub num: u32,
    /// Generation number.
    pub generation: u16,
    /// What the entry says.
    pub state: EntryState,
    /// The entry's bytes in the file. `None` for entries of xref streams (their bytes are inside
    /// the compressed stream data).
    pub span: Option<Span>,
}

/// Whether a section is a classic table or a cross-reference stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefKind {
    /// `xref` ... `trailer`.
    Table,
    /// A cross-reference stream object.
    Stream(ObjRef),
}

/// One cross-reference section.
#[derive(Debug, Clone, PartialEq)]
pub struct XrefSection {
    /// Table or stream.
    pub kind: XrefKind,
    /// From the `xref` keyword to the end of the trailer dictionary, or the whole xref stream
    /// object.
    pub span: Span,
    /// The entries in file order. A hybrid file's `/XRefStm` entries follow the table's.
    pub entries: Vec<XrefEntry>,
}

/// A `startxref` keyword and the offset after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartXref {
    /// The offset exactly as recorded.
    pub offset: u64,
    /// From the `startxref` keyword to the end of the offset.
    pub span: Span,
}

/// One revision of the file: an xref section, its trailer and the markers that close it.
#[derive(Debug, Clone, PartialEq)]
pub struct Revision {
    /// Position in [`crate::RawDocument::revisions`], oldest first.
    pub index: usize,
    /// The xref section.
    pub xref: XrefSection,
    /// The `startxref` that follows the section.
    pub startxref: Option<StartXref>,
    /// The trailer dictionary (the dictionary of an xref stream).
    pub trailer: Option<Dict>,
    /// The `%%EOF` marker that closes the revision.
    pub eof: Option<Span>,
}

/// An xref section as read from one place in the file.
#[derive(Debug, Clone)]
pub(crate) struct ParsedSection {
    pub section: XrefSection,
    pub trailer: Option<Dict>,
    /// The `/Prev` value and where it was written.
    pub prev: Option<(i64, Span)>,
    /// The `/XRefStm` value and where it was written.
    pub xrefstm: Option<(i64, Span)>,
}

impl ParsedSection {
    /// Where the section really starts.
    pub(crate) fn start(&self) -> usize {
        self.section.span.start
    }
}

/// The revisions of a file and the xref stream objects that no entry needs to list.
#[derive(Debug)]
pub(crate) struct Chain {
    pub revisions: Vec<Revision>,
    /// Offsets of xref stream objects, which are legitimately absent from every entry list.
    pub xref_stream_offsets: Vec<usize>,
}

fn report(diags: &mut Vec<RawDiagnostic>, kind: RawDiagnosticKind, span: Span) {
    diags.push(RawDiagnostic::new(kind, span));
}

fn int_entry(dict: &Dict, key: &[u8]) -> Option<(i64, Span)> {
    let value = dict.get(key)?;
    Some((value.value.as_int()?, value.span))
}

/// Reads the xref section that starts at `offset`, or after white space past it.
///
/// Returns `None` when neither an `xref` keyword nor an xref stream object is there. Diagnostics
/// are only recorded for sections that are found.
pub(crate) fn parse_section_at(
    src: &[u8],
    offset: usize,
    diags: &mut Vec<RawDiagnostic>,
) -> Option<ParsedSection> {
    let start = skip_whitespace(src, offset);
    if src.get(start..)?.starts_with(b"xref") && ends_keyword(src.get(start + 4).copied()) {
        return Some(table::parse_table(src, start, diags));
    }
    stream::parse_xref_stream(src, start, diags)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn non_xref_objects_are_not_sections() {
        let mut diags = Vec::new();

        assert!(parse_section_at(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n", 0, &mut diags).is_none());
        assert!(parse_section_at(b"hello", 0, &mut diags).is_none());
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn leading_white_space_moves_the_section_start() {
        let mut diags = Vec::new();
        let parsed = parse_section_at(b"\n\nxref\n0 0\ntrailer\n<< >>", 0, &mut diags).unwrap();

        assert_eq!(parsed.start(), 2);
    }
}
