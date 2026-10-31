//! Lossless, never-repair PDF syntax layer.
//!
//! Everything this crate reports is tied to exact byte [`Span`]s of the input. It never
//! fixes anything: when the file deviates from ISO 32000-2 it records a [`RawDiagnostic`]
//! and describes what is actually there, then keeps reading as well as it can.
//!
//! The entry point is [`RawDocument::parse`]. Only a missing `%PDF-` header is fatal
//! ([`RawError::MissingHeader`]); every other defect becomes a [`RawDiagnostic`].

// The workspace allows these two pedantic lints; the two library crates under the query API
// document every error and panic of their public API.
#![warn(clippy::missing_errors_doc, clippy::missing_panics_doc)]

mod diagnostic;
mod document;
mod filters;
mod header;
mod lexer;
pub mod numeric;
mod object;
mod object_ref;
mod parser;
mod scan;
mod span;
mod xref;

pub use diagnostic::{
    ContainerKind, FreeListProblem, RawDiagnostic, RawDiagnosticKind, Severity, XrefEntryProblem,
};
pub use document::{IndirectObject, ObjLocation, RawDocument};
pub use filters::{DecodeError, DecodeIssue, Decoded, MAX_DECODED_LEN};
pub use header::{HEADER_SEARCH_WINDOW, Header, PdfVersion, parse_header};
pub use lexer::{Lexer, Token, TokenKind};
pub use object::{Dict, Name, Object, PdfString, Spanned, Stream};
pub use object_ref::ObjRef;
pub use parser::{MAX_NESTING, parse_object};
pub use span::Span;
pub use xref::{EntryState, Revision, StartXref, XrefEntry, XrefKind, XrefSection};

/// Errors that stop raw parsing entirely.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RawError {
    /// No `%PDF-x.y` header in the first [`HEADER_SEARCH_WINDOW`] bytes.
    #[error("no %PDF-x.y header in the first {HEADER_SEARCH_WINDOW} bytes")]
    MissingHeader,
}
