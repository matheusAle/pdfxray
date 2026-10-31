//! [`RawDocument`]: the whole file as it is, with every deviation recorded.
//!
//! This module holds the document, its lookup tables and the object accessors. Object streams are
//! read in [`object_stream`]; the whole-file checks that run once after parsing (offsets, objects,
//! trailers, free lists, the tail and unreferenced objects) are in [`checks`].

mod checks;
mod object_stream;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use crate::filters::{DecodeError, DecodeIssue, Decoded, MAX_DECODED_LEN, decode_with, filter_names};
use crate::object::{Dict, Object, Spanned, Stream};
use crate::parser::{LengthLookup, LengthSource, NoLengths, parse_indirect};
use crate::scan::ObjectScan;
use crate::xref::{self, EntryState, Revision, XrefEntry};
use crate::{Header, ObjRef, RawDiagnostic, RawDiagnosticKind, RawError, Span, parse_header};

/// How many `n 0 R` hops [`RawDocument::resolve`] follows before giving up.
const MAX_REF_HOPS: usize = 32;

/// Where an indirect object's bytes live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjLocation {
    /// In the file body, `num gen obj` starting at `offset` (the offset actually used, which may
    /// differ from the one the xref records; see [`RawDiagnosticKind::XrefOffsetWrong`]).
    Direct {
        /// Byte offset of the object header.
        offset: u64,
    },
    /// Inside an object stream. Spans of the object refer to the decoded stream bytes, see
    /// [`RawDocument::object_stream_data`].
    InObjectStream {
        /// The containing object stream.
        stream: ObjRef,
        /// The member's index in the stream's header.
        index: u32,
    },
}

/// An indirect object and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct IndirectObject {
    /// The object as written in its header (or, for compressed objects, as asked for).
    pub r: ObjRef,
    /// The value. For a stream this is [`Object::Stream`].
    pub value: Spanned<Object>,
    /// From the start of `num gen obj` to the end of `endobj` (file offsets), or the member's
    /// bytes in the decoded object stream.
    pub span: Span,
    /// Where the bytes are.
    pub location: ObjLocation,
    /// The revision whose xref entry defines this object.
    pub revision: usize,
}

/// An object stream, decoded once while parsing.
#[derive(Debug)]
struct ObjStm {
    stream: ObjRef,
    /// The decoded stream bytes; member spans index into this.
    decoded: Vec<u8>,
    /// `(object number, start offset in decoded)` per header pair.
    members: Vec<(u32, usize)>,
    /// The stream's data in the file; diagnostics inside members are reported here.
    data: Span,
}

/// Per revision lookup tables.
#[derive(Debug, Default)]
struct RevIndex {
    /// Object number to the index of its entry in `xref.entries`.
    by_num: HashMap<u32, usize>,
    /// Object number to the offset its header was verified at. `None`: nowhere in the file.
    resolved: HashMap<u32, Option<usize>>,
}

impl RevIndex {
    /// Builds the table. A free entry never hides an in-use one (hybrid files list compressed
    /// objects as free in the table); among equals the first wins.
    fn new(revision: &Revision, diags: &mut Vec<RawDiagnostic>) -> Self {
        let entries = &revision.xref.entries;
        let mut by_num: HashMap<u32, usize> = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            match by_num.get(&entry.num).copied() {
                None => {
                    by_num.insert(entry.num, i);
                }
                Some(kept) => {
                    let kept_free = entries[kept].state.is_free();
                    if kept_free && !entry.state.is_free() {
                        by_num.insert(entry.num, i);
                    } else if kept_free == entry.state.is_free() {
                        diags.push(RawDiagnostic::new(
                            RawDiagnosticKind::XrefDuplicateEntry { num: entry.num },
                            entry.span.unwrap_or(revision.xref.span),
                        ));
                    }
                }
            }
        }
        Self {
            by_num,
            resolved: HashMap::new(),
        }
    }

    /// The entries that count, in file order.
    fn effective<'r>(&'r self, revision: &'r Revision) -> impl Iterator<Item = &'r XrefEntry> + 'r {
        revision
            .xref
            .entries
            .iter()
            .enumerate()
            .filter(|(i, e)| self.by_num.get(&e.num) == Some(i))
            .map(|(_, e)| e)
    }
}

/// A parsed PDF file: header, revisions, objects and the diagnostics found on the way.
///
/// Parsing never repairs anything. Objects are found where the xref says they are; when that is
/// wrong the real place is found by scanning and the difference is reported.
pub struct RawDocument {
    bytes: Arc<Vec<u8>>,
    header: Header,
    revisions: Vec<Revision>,
    index: Vec<RevIndex>,
    objstms: HashMap<usize, ObjStm>,
    encryption: Option<Dict>,
    diagnostics: Vec<RawDiagnostic>,
}

impl fmt::Debug for RawDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawDocument")
            .field("len", &self.bytes.len())
            .field("header", &self.header)
            .field("revisions", &self.revisions.len())
            .field("diagnostics", &self.diagnostics.len())
            .finish_non_exhaustive()
    }
}

/// Resolves indirect `/Length` values while a stream is parsed.
struct DocLengths<'a> {
    doc: &'a RawDocument,
    rev: usize,
}

impl LengthSource for DocLengths<'_> {
    fn length_of(&self, r: ObjRef) -> LengthLookup {
        let Some((found, entry)) = self.doc.lookup_entry(r.num, self.rev) else {
            return LengthLookup::Unresolved;
        };
        let value = match entry.state {
            EntryState::InUse { .. } => self
                .doc
                .resolved_offset(found, entry.num)
                .and_then(|at| parse_indirect(&self.doc.bytes, at, &NoLengths, &mut Vec::new()))
                .map(|p| p.value.value),
            EntryState::Compressed { .. } => self
                .doc
                .load_compressed(r, found, entry, &mut Vec::new())
                .map(|o| o.value.value),
            EntryState::Free { .. } => None,
        };
        match value {
            Some(Object::Int(n)) => LengthLookup::Known(n),
            _ => LengthLookup::Unresolved,
        }
    }
}

impl RawDocument {
    /// Parses `bytes`. Fails only when there is no `%PDF-` header; everything else becomes a
    /// diagnostic.
    ///
    /// # Errors
    ///
    /// [`RawError::MissingHeader`] when the first [`HEADER_SEARCH_WINDOW`](crate::HEADER_SEARCH_WINDOW)
    /// bytes hold no `%PDF-x.y` header.
    pub fn parse(bytes: Arc<Vec<u8>>) -> Result<Self, RawError> {
        let header = parse_header(&bytes)?;
        let mut diags = header.diagnostics.clone();
        let scan = ObjectScan::new(&bytes);
        let chain = xref::build(&bytes, &scan, &mut diags);
        let index = chain
            .revisions
            .iter()
            .map(|r| RevIndex::new(r, &mut diags))
            .collect();
        let mut doc = Self {
            bytes,
            header,
            revisions: chain.revisions,
            index,
            objstms: HashMap::new(),
            encryption: None,
            diagnostics: Vec::new(),
        };
        doc.verify_offsets(&scan, &mut diags);
        doc.encryption = doc.read_encryption();
        doc.load_object_streams(&mut diags);
        let stream_ranges = doc.check_objects(&mut diags);
        doc.check_trailers(&mut diags);
        doc.check_free_lists(&mut diags);
        doc.check_tail(&mut diags);
        doc.check_unreferenced(&scan, &chain.xref_stream_offsets, stream_ranges, &mut diags);
        diags.sort_by_key(|d| (d.span.start, d.span.end));
        doc.diagnostics = diags;
        Ok(doc)
    }

    /// The whole file.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The `%PDF-x.y` header.
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The revisions, oldest first.
    pub fn revisions(&self) -> &[Revision] {
        &self.revisions
    }

    /// The trailer dictionary of the newest revision.
    pub fn trailer(&self) -> Option<&Dict> {
        self.revisions.last().and_then(|r| r.trailer.as_ref())
    }

    /// The `/Encrypt` dictionary. Nothing is decrypted.
    pub fn encryption(&self) -> Option<&Dict> {
        self.encryption.as_ref()
    }

    /// Everything that deviates from ISO 32000-2, ordered by position.
    pub fn diagnostics(&self) -> &[RawDiagnostic] {
        &self.diagnostics
    }

    /// The newest definition of `r`, or `None` when it is free, unknown, has a different
    /// generation than the xref records, or cannot be found in the file.
    pub fn object(&self, r: ObjRef) -> Option<IndirectObject> {
        self.object_in_revision(r, self.revisions.len().checked_sub(1)?)
    }

    /// The definition of `r` as seen after revision `rev` (so later revisions are ignored).
    pub fn object_in_revision(&self, r: ObjRef, rev: usize) -> Option<IndirectObject> {
        if rev >= self.revisions.len() {
            return None;
        }
        self.load(r, rev, &mut Vec::new())
    }

    /// Every in-use object (direct or compressed) of the newest view, sorted.
    pub fn object_refs(&self) -> Vec<ObjRef> {
        let mut seen = HashSet::new();
        let mut refs = Vec::new();
        for (revision, index) in self.revisions.iter().zip(&self.index).rev() {
            for entry in index.effective(revision) {
                if seen.insert(entry.num) && !entry.state.is_free() {
                    refs.push(ObjRef::new(entry.num, entry.generation));
                }
            }
        }
        refs.sort_unstable();
        refs
    }

    /// Follows indirect references until a direct object is reached. A dangling or circular
    /// reference resolves to `null`.
    pub fn resolve<'a>(&'a self, obj: &'a Object) -> Cow<'a, Object> {
        let Object::Ref(first) = obj else {
            return Cow::Borrowed(obj);
        };
        let mut current = *first;
        let mut seen = Vec::new();
        for _ in 0..MAX_REF_HOPS {
            if seen.contains(&current) {
                break;
            }
            seen.push(current);
            let Some(found) = self.object(current) else {
                break;
            };
            match found.value.value {
                Object::Ref(next) => current = next,
                other => return Cow::Owned(other),
            }
        }
        Cow::Owned(Object::Null)
    }

    /// Runs the filter chain of `s`. Encrypted documents return the stored bytes with
    /// [`DecodeIssue::Encrypted`].
    ///
    /// Problems met while decoding (truncated or corrupt data, unsupported filters, the size
    /// limit) do not fail the call: they are [`Decoded::issues`] next to what could be decoded.
    ///
    /// # Errors
    ///
    /// [`DecodeError::DataOutOfBounds`] when the stream's data span is not inside the file, and
    /// (for streams that are not encrypted) [`DecodeError::InvalidFilterEntry`] when `/Filter` is
    /// neither a name nor an array of names.
    pub fn decode_stream(&self, s: &Stream) -> Result<Decoded, DecodeError> {
        let raw = s.data.slice(&self.bytes).ok_or(DecodeError::DataOutOfBounds {
            start: s.data.start,
            end: s.data.end,
        })?;
        let resolver = |o: &Object| self.resolve(o).into_owned();
        if self.is_encrypted_stream(s) {
            let filters = filter_names(&s.dict, &resolver).unwrap_or_default();
            return Ok(Decoded {
                bytes: raw.to_vec(),
                filters,
                complete: false,
                applied: 0,
                issues: vec![DecodeIssue::Encrypted],
            });
        }
        decode_with(raw, &s.dict, &resolver, MAX_DECODED_LEN)
    }

    /// The raw (still encoded) bytes of a stream.
    pub fn stream_data(&self, s: &Stream) -> Option<&[u8]> {
        s.data.slice(&self.bytes)
    }

    /// The decoded bytes of the object stream that holds `obj`; the spans of `obj` refer to them.
    pub fn object_stream_data(&self, obj: &IndirectObject) -> Option<&[u8]> {
        let ObjLocation::InObjectStream { stream, .. } = obj.location else {
            return None;
        };
        let offset = self.container_offset(stream.num, obj.revision)?;
        self.objstms.get(&offset).map(|os| os.decoded.as_slice())
    }

    fn is_encrypted_stream(&self, s: &Stream) -> bool {
        self.encryption.is_some() && s.dict.get_name(b"Type").is_none_or(|n| n.as_bytes() != b"XRef")
    }

    /// The newest entry for `num` as of revision `upto`, with the revision that holds it.
    fn lookup_entry(&self, num: u32, upto: usize) -> Option<(usize, &XrefEntry)> {
        let last = upto.min(self.revisions.len().checked_sub(1)?);
        (0..=last).rev().find_map(|rev| {
            let i = *self.index[rev].by_num.get(&num)?;
            Some((rev, &self.revisions[rev].xref.entries[i]))
        })
    }

    fn resolved_offset(&self, rev: usize, num: u32) -> Option<usize> {
        self.index.get(rev)?.resolved.get(&num).copied().flatten()
    }

    fn container_offset(&self, stream_num: u32, rev: usize) -> Option<usize> {
        let (found, entry) = self.lookup_entry(stream_num, rev)?;
        match entry.state {
            EntryState::InUse { .. } => self.resolved_offset(found, stream_num),
            _ => None,
        }
    }

    fn load(&self, r: ObjRef, rev: usize, sink: &mut Vec<RawDiagnostic>) -> Option<IndirectObject> {
        let (found, entry) = self.lookup_entry(r.num, rev)?;
        if entry.generation != r.generation {
            return None;
        }
        match entry.state {
            EntryState::Free { .. } => None,
            EntryState::InUse { .. } => {
                let at = self.resolved_offset(found, r.num)?;
                let parsed = parse_indirect(&self.bytes, at, &DocLengths { doc: self, rev }, sink)?;
                Some(IndirectObject {
                    r: ObjRef::new(parsed.num, u16::try_from(parsed.generation).unwrap_or(u16::MAX)),
                    value: parsed.value,
                    span: parsed.span,
                    location: ObjLocation::Direct {
                        offset: u64::try_from(at).unwrap_or(0),
                    },
                    revision: found,
                })
            }
            EntryState::Compressed { .. } => self.load_compressed(r, found, entry, sink),
        }
    }

    fn read_encryption(&self) -> Option<Dict> {
        let entry = self.trailer()?.get(b"Encrypt")?;
        match &entry.value {
            Object::Dict(d) => Some(d.clone()),
            Object::Ref(r) => self.object(*r)?.value.value.as_dict().cloned(),
            _ => None,
        }
    }
}
