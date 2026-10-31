//! Raw-syntax diagnostics: what the file says versus what is actually there.

use std::fmt;

use crate::{Name, ObjRef, Span};

/// How serious a [`RawDiagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Unusual but harmless; readers agree on the meaning.
    Info,
    /// A deviation from ISO 32000-2 that readers have to tolerate.
    Warning,
    /// Something is broken: data is lost, unreadable or contradictory.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// A deviation from ISO 32000-2 found while reading raw syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDiagnostic {
    /// What was found.
    pub kind: RawDiagnosticKind,
    /// Where in the file it was found.
    pub span: Span,
}

impl RawDiagnostic {
    /// Creates a diagnostic.
    pub const fn new(kind: RawDiagnosticKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Stable kebab-case code of the kind.
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Severity of the kind.
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }
}

impl fmt::Display for RawDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}..{}: {}",
            self.code(),
            self.span.start,
            self.span.end,
            self.kind
        )
    }
}

/// What is wrong with a classic xref table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrefEntryProblem {
    /// The offset field is not 10 digits or the generation field is not 5 digits.
    FieldWidth,
    /// The fields are not separated by exactly one space.
    Separator,
    /// The entry is not terminated by a two byte end-of-line (`SP LF`, `SP CR` or `CR LF`).
    Eol,
}

impl fmt::Display for XrefEntryProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FieldWidth => "offset is not 10 digits or generation is not 5 digits",
            Self::Separator => "fields are not separated by a single space",
            Self::Eol => "entry does not end with a two byte end-of-line marker",
        })
    }
}

/// What is wrong with the xref free list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreeListProblem {
    /// Entry 0 (the head of the free list) is not a free entry.
    HeadNotFree,
    /// Following the `next` links returns to an entry already visited without reaching 0.
    Loop,
    /// A free entry links to an object that the same section marks as in use.
    NextInUse,
}

impl fmt::Display for FreeListProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::HeadNotFree => "the head of the free list is not a free entry",
            Self::Loop => "the free list contains a loop",
            Self::NextInUse => "a free entry links to an object that is in use",
        })
    }
}

/// Which container was left open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerKind {
    /// `[ ... ]`
    Array,
    /// `<< ... >>`
    Dict,
}

impl fmt::Display for ContainerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Array => "array",
            Self::Dict => "dictionary",
        })
    }
}

/// The kinds of raw-syntax deviations pdfxray can report.
///
/// Every kind has a stable kebab-case [`code`](Self::code), a [`severity`](Self::severity) and a
/// human message through [`Display`](fmt::Display).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawDiagnosticKind {
    /// The `%PDF-` header does not start at byte 0 (ISO 32000-2 §7.5.2).
    HeaderNotAtStart {
        /// Byte offset where the header was found.
        offset: usize,
    },
    /// No `startxref` with an offset was found; the xref sections were located by scanning.
    StartxrefMissing,
    /// `startxref` does not point at an xref section. The section was found by scanning.
    StartxrefWrong {
        /// The offset `startxref` records.
        recorded: u64,
        /// Where the newest xref section really starts; `None` when the file has none.
        actual: Option<u64>,
    },
    /// The xref data was recovered by scanning the file instead of following `startxref`.
    XrefRebuilt {
        /// Number of xref sections found by the scan.
        sections: usize,
        /// Number of objects defined by the recovered data.
        objects: usize,
    },
    /// An xref section is not at the recorded offset but after some white space.
    XrefSectionOffsetWrong {
        /// The offset the file records (`startxref` or `/Prev`).
        recorded: u64,
        /// Where the section really starts.
        actual: u64,
    },
    /// A `/Prev` or `/XRefStm` offset does not point at an xref section; the chain stops there.
    XrefSectionInvalid {
        /// The offset the file records.
        offset: u64,
    },
    /// A classic xref table could not be read to its end.
    XrefTableMalformed {
        /// What was expected.
        detail: &'static str,
    },
    /// A classic xref entry is not laid out as the 20 byte format of ISO 32000-2 §7.5.4.
    XrefEntryMalformed {
        /// Object number of the entry.
        num: u32,
        /// What is wrong with it.
        problem: XrefEntryProblem,
    },
    /// A subsection declares more entries than the table contains.
    XrefSubsectionShort {
        /// First object number of the subsection.
        first: u32,
        /// Entry count in the subsection header.
        declared: u32,
        /// Entries actually present.
        found: u32,
    },
    /// The same object number has two entries in one xref section; the first in-use one is used.
    XrefDuplicateEntry {
        /// The object number.
        num: u32,
    },
    /// An xref stream is malformed (bad `/W`, `/Index`, truncated or undecodable data).
    XrefStreamMalformed {
        /// What is wrong.
        detail: &'static str,
    },
    /// An xref stream entry has a type other than 0, 1 or 2; it is ignored.
    XrefStreamBadEntryType {
        /// Object number of the entry.
        num: u32,
        /// The type value.
        entry_type: u64,
    },
    /// The `/Prev` chain returns to a section already visited.
    PrevCycle {
        /// The offset that closes the cycle.
        offset: u64,
    },
    /// A classic xref section has no readable `trailer` dictionary.
    TrailerMissing,
    /// A required trailer key is absent.
    TrailerKeyMissing {
        /// The key without the slash.
        key: &'static str,
    },
    /// A trailer key has the wrong type.
    TrailerKeyInvalid {
        /// The key without the slash.
        key: &'static str,
    },
    /// The trailer `/Size` is not larger than the highest object number defined so far.
    TrailerSizeInconsistent {
        /// The `/Size` the trailer declares.
        size: i64,
        /// The smallest `/Size` that would be consistent.
        required: u64,
    },
    /// The xref records an offset that is not where `num gen obj` really starts.
    XrefOffsetWrong {
        /// The object.
        obj: ObjRef,
        /// The offset the xref records.
        recorded: u64,
        /// Where the object really starts, found by scanning; `None` when it is nowhere in the file.
        actual: Option<u64>,
    },
    /// The object header at the recorded offset names a different object or generation.
    ObjectNumberMismatch {
        /// The object the xref entry is for.
        expected: ObjRef,
        /// The object whose header is at the recorded offset.
        found: ObjRef,
    },
    /// The stream's `/Length` does not match the distance to `endstream`; the actual one is used.
    StreamLengthWrong {
        /// The stream object.
        obj: ObjRef,
        /// The declared `/Length`, `None` when absent or not an integer.
        declared: Option<i64>,
        /// The actual number of data bytes.
        actual: u64,
    },
    /// `stream` is not followed by `CRLF` or `LF`.
    StreamKeywordEol,
    /// A stream has no `endstream` keyword.
    MissingEndstream,
    /// An indirect object has no `endobj` keyword.
    MissingEndobj,
    /// An indirect object has no value between `obj` and `endobj`.
    EmptyObjectBody,
    /// A dictionary has the same key more than once; readers use the first (or last) value.
    DuplicateDictKey {
        /// The repeated key.
        key: Name,
    },
    /// A dictionary has something other than a name where a key is expected.
    DictKeyNotName,
    /// A dictionary key is not followed by a value.
    DictValueMissing {
        /// The key without a value.
        key: Name,
    },
    /// An array or dictionary is not closed.
    UnterminatedContainer {
        /// Which one.
        container: ContainerKind,
    },
    /// Arrays or dictionaries are nested deeper than pdfxray reads.
    NestingTooDeep,
    /// A keyword that cannot appear here.
    UnexpectedKeyword {
        /// The keyword text (lossy, truncated).
        text: String,
    },
    /// A string is not closed before the end of the data.
    UnterminatedString,
    /// A hexadecimal string has an odd number of digits; a trailing 0 is assumed (§7.3.4.3).
    HexStringOddDigits,
    /// A hexadecimal string contains a byte that is neither a digit nor white space.
    HexStringInvalidChar {
        /// The byte.
        byte: u8,
    },
    /// A name contains `#` not followed by two hexadecimal digits.
    InvalidNameEscape,
    /// An integer does not fit in 64 bits; it is read as a real number.
    IntegerOverflow,
    /// Bytes other than white space follow the last `%%EOF`.
    TrailingData {
        /// Number of bytes after the last `%%EOF`.
        len: usize,
    },
    /// `startxref` is not followed by `%%EOF`.
    MissingEof,
    /// An object is present in the file body but no xref section lists it.
    UnreferencedObject {
        /// The object found.
        obj: ObjRef,
    },
    /// The xref free list is inconsistent.
    FreeListBroken {
        /// The object number where the problem shows.
        obj: u32,
        /// What is wrong.
        problem: FreeListProblem,
    },
    /// An object stream is malformed (missing `/N` or `/First`, short header, undecodable data).
    ObjStmMalformed {
        /// The object stream.
        stream: ObjRef,
        /// What is wrong.
        detail: &'static str,
    },
    /// An object stream is encrypted; pdfxray does not decrypt, so its members are not listed.
    ObjStmEncrypted {
        /// The object stream.
        stream: ObjRef,
    },
    /// The object stream header does not list the object at the index the xref records.
    ObjStmEntryMismatch {
        /// The object stream.
        stream: ObjRef,
        /// The index the xref records.
        index: u32,
        /// The object number the xref expects there.
        expected: u32,
        /// The object number the header has at that index, `None` if the index is out of range.
        found: Option<u32>,
    },
    /// An object stream member cannot be parsed as an object.
    ObjStmMemberMalformed {
        /// The object stream.
        stream: ObjRef,
        /// The member's index in the header.
        index: u32,
    },
    /// An xref entry places an object in an object stream that does not exist or is not one.
    ObjStmContainerMissing {
        /// The compressed object.
        obj: ObjRef,
        /// The object number of the missing container.
        stream: u32,
    },
}

impl RawDiagnosticKind {
    /// Every code [`RawDiagnosticKind::code`] can return, in declaration order.
    pub const CODES: &'static [&'static str] = &[
        "header-not-at-start",
        "startxref-missing",
        "startxref-wrong",
        "xref-rebuilt",
        "xref-section-offset-wrong",
        "xref-section-invalid",
        "xref-table-malformed",
        "xref-entry-malformed",
        "xref-subsection-short",
        "xref-duplicate-entry",
        "xref-stream-malformed",
        "xref-stream-bad-entry-type",
        "prev-cycle",
        "trailer-missing",
        "trailer-key-missing",
        "trailer-key-invalid",
        "trailer-size-inconsistent",
        "xref-offset-wrong",
        "object-number-mismatch",
        "stream-length-wrong",
        "stream-keyword-eol",
        "missing-endstream",
        "missing-endobj",
        "empty-object-body",
        "duplicate-dict-key",
        "dict-key-not-name",
        "dict-value-missing",
        "unterminated-container",
        "nesting-too-deep",
        "unexpected-keyword",
        "unterminated-string",
        "hex-string-odd-digits",
        "hex-string-invalid-char",
        "invalid-name-escape",
        "integer-overflow",
        "trailing-data",
        "missing-eof",
        "unreferenced-object",
        "free-list-broken",
        "objstm-malformed",
        "objstm-encrypted",
        "objstm-entry-mismatch",
        "objstm-member-malformed",
        "objstm-container-missing",
    ];

    /// The object this diagnostic is about, when the kind names one.
    pub fn object(&self) -> Option<ObjRef> {
        match self {
            Self::XrefOffsetWrong { obj, .. }
            | Self::StreamLengthWrong { obj, .. }
            | Self::UnreferencedObject { obj, .. }
            | Self::ObjStmContainerMissing { obj, .. } => Some(*obj),
            Self::ObjectNumberMismatch { expected, .. } => Some(*expected),
            Self::ObjStmMalformed { stream, .. }
            | Self::ObjStmEncrypted { stream, .. }
            | Self::ObjStmEntryMismatch { stream, .. }
            | Self::ObjStmMemberMalformed { stream, .. } => Some(*stream),
            _ => None,
        }
    }

    /// Stable kebab-case identifier; never changes once released.
    pub fn code(&self) -> &'static str {
        match self {
            Self::HeaderNotAtStart { .. } => "header-not-at-start",
            Self::StartxrefMissing => "startxref-missing",
            Self::StartxrefWrong { .. } => "startxref-wrong",
            Self::XrefRebuilt { .. } => "xref-rebuilt",
            Self::XrefSectionOffsetWrong { .. } => "xref-section-offset-wrong",
            Self::XrefSectionInvalid { .. } => "xref-section-invalid",
            Self::XrefTableMalformed { .. } => "xref-table-malformed",
            Self::XrefEntryMalformed { .. } => "xref-entry-malformed",
            Self::XrefSubsectionShort { .. } => "xref-subsection-short",
            Self::XrefDuplicateEntry { .. } => "xref-duplicate-entry",
            Self::XrefStreamMalformed { .. } => "xref-stream-malformed",
            Self::XrefStreamBadEntryType { .. } => "xref-stream-bad-entry-type",
            Self::PrevCycle { .. } => "prev-cycle",
            Self::TrailerMissing => "trailer-missing",
            Self::TrailerKeyMissing { .. } => "trailer-key-missing",
            Self::TrailerKeyInvalid { .. } => "trailer-key-invalid",
            Self::TrailerSizeInconsistent { .. } => "trailer-size-inconsistent",
            Self::XrefOffsetWrong { .. } => "xref-offset-wrong",
            Self::ObjectNumberMismatch { .. } => "object-number-mismatch",
            Self::StreamLengthWrong { .. } => "stream-length-wrong",
            Self::StreamKeywordEol => "stream-keyword-eol",
            Self::MissingEndstream => "missing-endstream",
            Self::MissingEndobj => "missing-endobj",
            Self::EmptyObjectBody => "empty-object-body",
            Self::DuplicateDictKey { .. } => "duplicate-dict-key",
            Self::DictKeyNotName => "dict-key-not-name",
            Self::DictValueMissing { .. } => "dict-value-missing",
            Self::UnterminatedContainer { .. } => "unterminated-container",
            Self::NestingTooDeep => "nesting-too-deep",
            Self::UnexpectedKeyword { .. } => "unexpected-keyword",
            Self::UnterminatedString => "unterminated-string",
            Self::HexStringOddDigits => "hex-string-odd-digits",
            Self::HexStringInvalidChar { .. } => "hex-string-invalid-char",
            Self::InvalidNameEscape => "invalid-name-escape",
            Self::IntegerOverflow => "integer-overflow",
            Self::TrailingData { .. } => "trailing-data",
            Self::MissingEof => "missing-eof",
            Self::UnreferencedObject { .. } => "unreferenced-object",
            Self::FreeListBroken { .. } => "free-list-broken",
            Self::ObjStmMalformed { .. } => "objstm-malformed",
            Self::ObjStmEncrypted { .. } => "objstm-encrypted",
            Self::ObjStmEntryMismatch { .. } => "objstm-entry-mismatch",
            Self::ObjStmMemberMalformed { .. } => "objstm-member-malformed",
            Self::ObjStmContainerMissing { .. } => "objstm-container-missing",
        }
    }

    /// How serious the deviation is.
    pub fn severity(&self) -> Severity {
        match self {
            Self::XrefDuplicateEntry { .. }
            | Self::StreamKeywordEol
            | Self::XrefEntryMalformed { .. }
            | Self::HexStringOddDigits
            | Self::TrailingData { .. }
            | Self::UnreferencedObject { .. }
            | Self::EmptyObjectBody
            | Self::ObjStmEncrypted { .. }
            | Self::DuplicateDictKey { .. } => Severity::Info,
            Self::XrefTableMalformed { .. }
            | Self::XrefStreamMalformed { .. }
            | Self::XrefSectionInvalid { .. }
            | Self::PrevCycle { .. }
            | Self::ObjStmMalformed { .. }
            | Self::ObjStmMemberMalformed { .. }
            | Self::ObjStmContainerMissing { .. }
            | Self::UnterminatedContainer { .. }
            | Self::NestingTooDeep
            | Self::UnterminatedString
            | Self::UnexpectedKeyword { .. }
            | Self::DictKeyNotName
            | Self::DictValueMissing { .. } => Severity::Error,
            Self::XrefOffsetWrong { actual: None, .. } | Self::StartxrefWrong { actual: None, .. } => {
                Severity::Error
            }
            Self::HeaderNotAtStart { .. }
            | Self::StartxrefMissing
            | Self::StartxrefWrong { actual: Some(_), .. }
            | Self::XrefRebuilt { .. }
            | Self::XrefSectionOffsetWrong { .. }
            | Self::XrefSubsectionShort { .. }
            | Self::XrefStreamBadEntryType { .. }
            | Self::TrailerMissing
            | Self::TrailerKeyMissing { .. }
            | Self::TrailerKeyInvalid { .. }
            | Self::TrailerSizeInconsistent { .. }
            | Self::XrefOffsetWrong { .. }
            | Self::ObjectNumberMismatch { .. }
            | Self::StreamLengthWrong { .. }
            | Self::MissingEndstream
            | Self::MissingEndobj
            | Self::HexStringInvalidChar { .. }
            | Self::InvalidNameEscape
            | Self::IntegerOverflow
            | Self::MissingEof
            | Self::FreeListBroken { .. }
            | Self::ObjStmEntryMismatch { .. } => Severity::Warning,
        }
    }
}

impl fmt::Display for RawDiagnosticKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderNotAtStart { offset } => {
                write!(f, "the %PDF- header starts at byte {offset} instead of byte 0")
            }
            Self::StartxrefMissing => {
                f.write_str("no usable startxref; xref sections were found by scanning")
            }
            Self::StartxrefWrong {
                recorded,
                actual: Some(actual),
            } => {
                write!(
                    f,
                    "startxref records {recorded} but the newest xref section starts at {actual}"
                )
            }
            Self::StartxrefWrong {
                recorded,
                actual: None,
            } => {
                write!(f, "startxref records {recorded} but the file has no xref section")
            }
            Self::XrefRebuilt { sections, objects } => write!(
                f,
                "xref data recovered by scanning the file: {sections} section(s), {objects} object(s)"
            ),
            Self::XrefSectionOffsetWrong { recorded, actual } => {
                write!(
                    f,
                    "xref section recorded at {recorded} actually starts at {actual}"
                )
            }
            Self::XrefSectionInvalid { offset } => {
                write!(
                    f,
                    "offset {offset} does not point at an xref section; the chain stops here"
                )
            }
            Self::XrefTableMalformed { detail } => write!(f, "malformed xref table: {detail}"),
            Self::XrefEntryMalformed { num, problem } => {
                write!(f, "xref entry for object {num} is malformed: {problem}")
            }
            Self::XrefSubsectionShort {
                first,
                declared,
                found,
            } => write!(
                f,
                "xref subsection starting at {first} declares {declared} entries but has {found}"
            ),
            Self::XrefDuplicateEntry { num } => {
                write!(f, "object {num} has more than one entry in the same xref section")
            }
            Self::XrefStreamMalformed { detail } => write!(f, "malformed xref stream: {detail}"),
            Self::XrefStreamBadEntryType { num, entry_type } => {
                write!(
                    f,
                    "xref stream entry for object {num} has unknown type {entry_type}"
                )
            }
            Self::PrevCycle { offset } => write!(f, "the /Prev chain returns to the section at {offset}"),
            Self::TrailerMissing => f.write_str("no readable trailer dictionary"),
            Self::TrailerKeyMissing { key } => write!(f, "trailer has no /{key}"),
            Self::TrailerKeyInvalid { key } => write!(f, "trailer /{key} has the wrong type"),
            Self::TrailerSizeInconsistent { size, required } => {
                write!(f, "trailer /Size is {size} but at least {required} is required")
            }
            Self::XrefOffsetWrong {
                obj,
                recorded,
                actual: Some(actual),
            } => write!(
                f,
                "xref records object {obj} at offset {recorded} but it starts at {actual}"
            ),
            Self::XrefOffsetWrong {
                obj,
                recorded,
                actual: None,
            } => write!(
                f,
                "xref records object {obj} at offset {recorded} but it is nowhere in the file"
            ),
            Self::ObjectNumberMismatch { expected, found } => {
                write!(f, "xref entry for {expected} points at the header of {found}")
            }
            Self::StreamLengthWrong {
                obj,
                declared: Some(declared),
                actual,
            } => write!(
                f,
                "stream {obj} declares /Length {declared} but has {actual} data bytes before endstream"
            ),
            Self::StreamLengthWrong {
                obj,
                declared: None,
                actual,
            } => write!(
                f,
                "stream {obj} has no usable /Length; {actual} data bytes before endstream"
            ),
            Self::StreamKeywordEol => f.write_str("the stream keyword is not followed by CRLF or LF"),
            Self::MissingEndstream => f.write_str("stream has no endstream keyword"),
            Self::MissingEndobj => f.write_str("object has no endobj keyword"),
            Self::EmptyObjectBody => f.write_str("object has no value"),
            Self::DuplicateDictKey { key } => write!(f, "dictionary has more than one {key} key"),
            Self::DictKeyNotName => f.write_str("dictionary has a non-name where a key is expected"),
            Self::DictValueMissing { key } => write!(f, "dictionary key {key} has no value"),
            Self::UnterminatedContainer { container } => write!(f, "{container} is not closed"),
            Self::NestingTooDeep => f.write_str("arrays and dictionaries are nested too deeply"),
            Self::UnexpectedKeyword { text } => write!(f, "unexpected keyword `{text}`"),
            Self::UnterminatedString => f.write_str("string is not closed"),
            Self::HexStringOddDigits => {
                f.write_str("hexadecimal string has an odd number of digits; a trailing 0 is assumed")
            }
            Self::HexStringInvalidChar { byte } => {
                write!(f, "hexadecimal string contains byte 0x{byte:02X}")
            }
            Self::InvalidNameEscape => f.write_str("name has # not followed by two hexadecimal digits"),
            Self::IntegerOverflow => f.write_str("integer does not fit in 64 bits; read as a real"),
            Self::TrailingData { len } => write!(f, "{len} byte(s) of data after the last %%EOF"),
            Self::MissingEof => f.write_str("startxref is not followed by %%EOF"),
            Self::UnreferencedObject { obj } => {
                write!(f, "object {obj} is in the file body but in no xref section")
            }
            Self::FreeListBroken { obj, problem } => write!(f, "free list at object {obj}: {problem}"),
            Self::ObjStmMalformed { stream, detail } => {
                write!(f, "object stream {stream} is malformed: {detail}")
            }
            Self::ObjStmEncrypted { stream } => {
                write!(
                    f,
                    "object stream {stream} is encrypted; its members cannot be read"
                )
            }
            Self::ObjStmEntryMismatch {
                stream,
                index,
                expected,
                found: Some(found),
            } => write!(
                f,
                "xref expects object {expected} at index {index} of object stream {stream} but the header has {found}"
            ),
            Self::ObjStmEntryMismatch {
                stream,
                index,
                expected,
                found: None,
            } => write!(
                f,
                "xref expects object {expected} at index {index} of object stream {stream} but the header has no such index"
            ),
            Self::ObjStmMemberMalformed { stream, index } => {
                write!(
                    f,
                    "member {index} of object stream {stream} is not a valid object"
                )
            }
            Self::ObjStmContainerMissing { obj, stream } => {
                write!(
                    f,
                    "object {obj} should be in object stream {stream}, which is missing"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use pretty_assertions::assert_eq;

    use super::*;

    /// One value of every kind.
    fn every_kind() -> Vec<RawDiagnosticKind> {
        use RawDiagnosticKind as K;
        let obj = ObjRef::new(7, 0);
        vec![
            K::HeaderNotAtStart { offset: 3 },
            K::StartxrefMissing,
            K::StartxrefWrong {
                recorded: 1,
                actual: None,
            },
            K::XrefRebuilt {
                sections: 1,
                objects: 2,
            },
            K::XrefSectionOffsetWrong {
                recorded: 1,
                actual: 2,
            },
            K::XrefSectionInvalid { offset: 1 },
            K::XrefTableMalformed { detail: "x" },
            K::XrefEntryMalformed {
                num: 1,
                problem: XrefEntryProblem::Eol,
            },
            K::XrefSubsectionShort {
                first: 0,
                declared: 2,
                found: 1,
            },
            K::XrefDuplicateEntry { num: 1 },
            K::XrefStreamMalformed { detail: "x" },
            K::XrefStreamBadEntryType {
                num: 1,
                entry_type: 9,
            },
            K::PrevCycle { offset: 1 },
            K::TrailerMissing,
            K::TrailerKeyMissing { key: "Root" },
            K::TrailerKeyInvalid { key: "Root" },
            K::TrailerSizeInconsistent { size: 1, required: 2 },
            K::XrefOffsetWrong {
                obj,
                recorded: 1,
                actual: Some(2),
            },
            K::ObjectNumberMismatch {
                expected: obj,
                found: ObjRef::new(8, 0),
            },
            K::StreamLengthWrong {
                obj,
                declared: Some(1),
                actual: 2,
            },
            K::StreamKeywordEol,
            K::MissingEndstream,
            K::MissingEndobj,
            K::EmptyObjectBody,
            K::DuplicateDictKey { key: Name::new("A") },
            K::DictKeyNotName,
            K::DictValueMissing { key: Name::new("A") },
            K::UnterminatedContainer {
                container: ContainerKind::Dict,
            },
            K::NestingTooDeep,
            K::UnexpectedKeyword { text: "x".into() },
            K::UnterminatedString,
            K::HexStringOddDigits,
            K::HexStringInvalidChar { byte: b'g' },
            K::InvalidNameEscape,
            K::IntegerOverflow,
            K::TrailingData { len: 1 },
            K::MissingEof,
            K::UnreferencedObject { obj },
            K::FreeListBroken {
                obj: 1,
                problem: FreeListProblem::Loop,
            },
            K::ObjStmMalformed {
                stream: obj,
                detail: "x",
            },
            K::ObjStmEncrypted { stream: obj },
            K::ObjStmEntryMismatch {
                stream: obj,
                index: 0,
                expected: 1,
                found: None,
            },
            K::ObjStmMemberMalformed {
                stream: obj,
                index: 0,
            },
            K::ObjStmContainerMissing { obj, stream: 9 },
        ]
    }

    #[test]
    fn codes_lists_exactly_the_codes_the_kinds_return() {
        let from_kinds: BTreeSet<&str> = every_kind().iter().map(RawDiagnosticKind::code).collect();
        let listed: BTreeSet<&str> = RawDiagnosticKind::CODES.iter().copied().collect();

        assert_eq!(from_kinds, listed);
        assert_eq!(
            RawDiagnosticKind::CODES.len(),
            listed.len(),
            "no code is listed twice"
        );
        assert_eq!(RawDiagnosticKind::CODES.len(), 44);
        assert!(
            RawDiagnosticKind::CODES.iter().all(|c| c
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-' || b.is_ascii_digit())),
            "codes are kebab-case"
        );
    }
}
