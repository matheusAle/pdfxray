//! A low-level PDF byte emitter that can write valid *and* deliberately broken files.
//!
//! [`PdfWriter`] deliberately knows nothing about pages, fonts or tags. It writes indirect
//! objects, streams, object streams, classic xref tables, xref streams and trailers, and it
//! lets the caller corrupt chosen xref entries. Everything is deterministic: there are no
//! timestamps and the trailer `/ID` is a fixed constant ([`FIXED_ID_HEX`]).
//!
//! A file is a sequence of *revisions*. Objects written with [`PdfWriter::object`],
//! [`PdfWriter::stream`] or [`PdfWriter::object_stream`] belong to the revision that is
//! currently open; [`PdfWriter::finish_table`] or [`PdfWriter::finish_xref_stream`] closes it
//! by appending an xref section and a trailer. Writing more objects afterwards opens an
//! incremental update whose trailer links back with `/Prev`.
//!
//! Invalid usage never panics. The first mistake is remembered and reported by the next
//! `finish_*` call (or by [`PdfWriter::into_output`]).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The trailer `/ID` bytes (hex encoded) used by every generated file.
pub const FIXED_ID_HEX: &str = "70646678726179000000000000000001";

/// Bytes of the binary-marker comment that follows the header line.
const BINARY_MARKER: [u8; 6] = [b'%', 0xE2, 0xE3, 0xCF, 0xD3, b'\n'];

/// Field widths of the xref stream rows: 1-byte type, 4-byte offset, 2-byte generation or index.
const XREF_STREAM_ROW_LEN: usize = 7;

/// A PDF header version such as `1.7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PdfVersion {
    /// Major part (always 1 or 2 in practice).
    pub major: u8,
    /// Minor part.
    pub minor: u8,
}

impl PdfVersion {
    /// PDF 1.4.
    pub const V1_4: Self = Self { major: 1, minor: 4 };
    /// PDF 1.5 (first version with xref streams and object streams).
    pub const V1_5: Self = Self { major: 1, minor: 5 };
    /// PDF 1.7.
    pub const V1_7: Self = Self { major: 1, minor: 7 };
    /// PDF 2.0.
    pub const V2_0: Self = Self { major: 2, minor: 0 };
}

impl fmt::Display for PdfVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// How a revision indexes its objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefKind {
    /// A classic `xref` table followed by a `trailer` dictionary.
    Table,
    /// A cross-reference stream object (PDF 1.5+).
    Stream,
}

/// Where an in-use object lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// A top-level indirect object starting at this byte offset.
    Offset(u64),
    /// A compressed object stored inside an object stream.
    InObjectStream {
        /// Object number of the containing object stream.
        stream: u32,
        /// Zero-based index of the object within that stream.
        index: u32,
    },
}

/// One in-use xref entry as it was *actually* written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryRecord {
    /// Object number.
    pub obj: u32,
    /// True location of the object in the file.
    pub location: Location,
}

/// An xref entry whose recorded offset deliberately differs from the real one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetMismatch {
    /// Object number.
    pub obj: u32,
    /// Offset the xref section claims.
    pub recorded: u64,
    /// Offset where `N 0 obj` really starts.
    pub actual: u64,
}

/// A `startxref` value that deliberately differs from the real xref section offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartxrefMismatch {
    /// Offset the file claims after `startxref`.
    pub recorded: u64,
    /// Offset where the xref section (or xref stream object) really starts.
    pub actual: u64,
}

/// What a finished revision looks like; returned in [`Output::revisions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRecord {
    /// Whether this revision uses an xref table or an xref stream.
    pub kind: XrefKind,
    /// Byte offset of the xref section (the value written after `startxref`).
    pub xref_offset: u64,
    /// Byte offset of the previous xref section (the `/Prev` value), if any.
    pub prev: Option<u64>,
    /// In-use entries of this revision in ascending object order (free head excluded).
    pub entries: Vec<EntryRecord>,
    /// Entries whose recorded offset was corrupted on purpose.
    pub offset_mismatches: Vec<OffsetMismatch>,
    /// Offset of the cross-reference stream a hybrid-reference file names in `/XRefStm`.
    pub xref_stm: Option<u64>,
    /// Set when the `startxref` value written for this revision deliberately lies.
    pub startxref_mismatch: Option<StartxrefMismatch>,
}

impl RevisionRecord {
    /// Object numbers defined by this revision in ascending order.
    ///
    /// Compressed objects and, for xref streams, the xref stream object itself are included.
    #[must_use]
    pub fn objects(&self) -> Vec<u32> {
        self.entries.iter().map(|e| e.obj).collect()
    }

    /// Compressed entries as `(object, containing stream, index)` triples.
    #[must_use]
    pub fn compressed(&self) -> Vec<(u32, u32, u32)> {
        self.entries
            .iter()
            .filter_map(|e| match e.location {
                Location::InObjectStream { stream, index } => Some((e.obj, stream, index)),
                Location::Offset(_) => None,
            })
            .collect()
    }
}

/// The finished file plus a description of every revision in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The complete file.
    pub bytes: Vec<u8>,
    /// Revisions in file order (oldest first).
    pub revisions: Vec<RevisionRecord>,
}

/// Trailer entries. `/Size`, `/ID` and `/Prev` are managed by the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trailer {
    root: u32,
    info: Option<u32>,
    encrypt: Option<u32>,
}

impl Trailer {
    /// A trailer whose `/Root` is the given catalog object.
    #[must_use]
    pub const fn new(root: u32) -> Self {
        Self {
            root,
            info: None,
            encrypt: None,
        }
    }

    /// Adds an `/Info` reference.
    #[must_use]
    pub const fn info(mut self, obj: u32) -> Self {
        self.info = Some(obj);
        self
    }

    /// Adds an `/Encrypt` reference.
    #[must_use]
    pub const fn encrypt(mut self, obj: u32) -> Self {
        self.encrypt = Some(obj);
        self
    }
}

/// Things that can go wrong while assembling a file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriterError {
    /// Object number 0 is the head of the free list and cannot hold an object.
    #[error("object number 0 is reserved for the free-list head")]
    ReservedObjectNumber,
    /// The same object number was defined twice in one revision.
    #[error("object {0} is defined twice in the same revision")]
    DuplicateObject(u32),
    /// A classic xref table cannot index objects stored in object streams.
    #[error("object {0} lives in an object stream, which a classic xref table cannot index")]
    CompressedInTable(u32),
    /// A corruption was requested for an object the finished revision does not contain.
    #[error("cannot corrupt object {0}: it is not a top-level object of the revision being finished")]
    CorruptionTarget(u32),
    /// Applying the corruption delta would produce a negative or overflowing offset.
    #[error("corrupting object {0} would make its recorded offset negative or too large")]
    CorruptionOutOfRange(u32),
    /// An offset or index does not fit the fixed xref stream field widths.
    #[error("value {0} does not fit the xref stream field widths")]
    FieldOverflow(u64),
    /// Applying the `startxref` corruption delta would produce a negative or overflowing offset.
    #[error("corrupting startxref would make the recorded offset negative or too large")]
    StartxrefOutOfRange,
    /// A hybrid-reference revision needs at least one object held in an object stream.
    #[error("a hybrid-reference revision needs at least one object stream member")]
    HybridWithoutCompressed,
    /// `into_output` was called while objects were written after the last xref section.
    #[error("{0} object(s) were written after the last xref section")]
    UnfinishedRevision(usize),
    /// `into_output` was called before any revision was finished.
    #[error("no revision was finished")]
    NoRevision,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    obj: u32,
    location: Location,
}

/// The byte emitter. See the [module documentation](self).
#[derive(Debug)]
pub struct PdfWriter {
    buf: Vec<u8>,
    pending: Vec<Pending>,
    revisions: Vec<RevisionRecord>,
    corruptions: BTreeMap<u32, i64>,
    startxref_delta: Option<i64>,
    max_obj: u32,
    error: Option<WriterError>,
}

impl PdfWriter {
    /// Starts a file with `%PDF-x.y` and a binary-marker comment line.
    #[must_use]
    pub fn new(version: PdfVersion) -> Self {
        let mut buf = format!("%PDF-{version}\n").into_bytes();
        buf.extend_from_slice(&BINARY_MARKER);
        Self {
            buf,
            pending: Vec::new(),
            revisions: Vec::new(),
            corruptions: BTreeMap::new(),
            startxref_delta: None,
            max_obj: 0,
            error: None,
        }
    }

    /// Current write position (the offset the next byte lands at).
    #[must_use]
    pub fn position(&self) -> u64 {
        // Lossless: `usize` is at most 64 bits on every target.
        self.buf.len() as u64
    }

    /// Writes `num 0 obj <body> endobj` and returns the object's byte offset.
    pub fn object(&mut self, num: u32, body: impl AsRef<[u8]>) -> u64 {
        let offset = self.begin_object(num);
        self.buf.extend_from_slice(body.as_ref());
        self.buf.extend_from_slice(b"\nendobj\n");
        offset
    }

    /// Writes a stream whose `/Length` equals `data.len()`.
    ///
    /// `dict` holds the dictionary entries *without* the surrounding `<< >>` and without
    /// `/Length`, for example `"/Type /XObject /Subtype /Form"`.
    pub fn stream(&mut self, num: u32, dict: &str, data: &[u8]) -> u64 {
        self.stream_with_length(num, dict, data, data.len())
    }

    /// Writes a stream but declares `/Length` as `declared_length`, whatever `data.len()` is.
    pub fn stream_with_length(&mut self, num: u32, dict: &str, data: &[u8], declared_length: usize) -> u64 {
        let offset = self.begin_object(num);
        self.write_stream_body(dict, data, declared_length);
        offset
    }

    /// Writes an uncompressed object stream (`/Type /ObjStm`) holding `members`.
    ///
    /// Each member is `(object number, body)`; its xref entry will be of the compressed kind.
    /// Returns the byte offset of the object stream itself.
    pub fn object_stream(&mut self, num: u32, members: &[(u32, &str)]) -> u64 {
        let mut pairs = Vec::new();
        let mut body = String::new();
        for (obj, text) in members {
            pairs.push(format!("{obj} {} ", body.len()));
            body.push_str(text);
            body.push('\n');
        }
        let header = pairs.concat();
        let first = header.len();
        let mut data = header.into_bytes();
        data.extend_from_slice(body.as_bytes());
        let dict = format!("/Type /ObjStm /N {} /First {first}", members.len());

        let offset = self.stream(num, &dict, &data);
        for (index, (obj, _)) in members.iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            self.claim(*obj, Location::InObjectStream { stream: num, index });
        }
        offset
    }

    /// Makes the xref entry of `obj` in the next finished revision point `delta` bytes away
    /// from the object's real offset.
    pub fn corrupt_offset(&mut self, obj: u32, delta: i64) {
        self.corruptions.insert(obj, delta);
    }

    /// Makes the `startxref` value of the next finished revision point `delta` bytes away from
    /// the real offset of its xref section.
    pub fn corrupt_startxref(&mut self, delta: i64) {
        self.startxref_delta = Some(delta);
    }

    /// Closes the open revision with a classic xref table and a trailer.
    ///
    /// # Errors
    /// Returns the first usage error recorded so far, or an error found while finishing.
    pub fn finish_table(&mut self, trailer: &Trailer) -> Result<(), WriterError> {
        self.take_error()?;
        let entries = self.drain_entries();
        if let Some(e) = entries
            .iter()
            .find(|e| matches!(e.location, Location::InObjectStream { .. }))
        {
            return Err(WriterError::CompressedInTable(e.obj));
        }
        let mismatches = self.apply_corruptions(&entries)?;
        self.emit_table(entries, mismatches, trailer, None)
    }

    /// Closes the open revision as a *hybrid-reference* file: a classic xref table whose trailer
    /// carries `/XRefStm`, naming a cross-reference stream numbered `xref_stm_obj`.
    ///
    /// The stream (written first, so its offset is known) holds one compressed-object row for
    /// every member of an object stream; the classic table lists everything else as in use and
    /// the compressed members as free, which hides them from readers that ignore `/XRefStm`.
    ///
    /// # Errors
    /// Returns the first usage error recorded so far, fails when the revision has no object
    /// stream members, or when an offset does not fit the field widths.
    pub fn finish_hybrid(&mut self, xref_stm_obj: u32, trailer: &Trailer) -> Result<(), WriterError> {
        self.take_error()?;
        let stm_offset = self.position();
        self.claim(xref_stm_obj, Location::Offset(stm_offset));
        self.take_error()?;
        let entries = self.drain_entries();
        let mismatches = self.apply_corruptions(&entries)?;

        let mut compressed: Vec<(u32, [u8; XREF_STREAM_ROW_LEN])> = Vec::new();
        for e in &entries {
            if let Location::InObjectStream { stream, index } = e.location {
                let index = u16::try_from(index).map_err(|_| WriterError::FieldOverflow(u64::from(index)))?;
                compressed.push((e.obj, xref_row(2, stream, index)));
            }
        }
        if compressed.is_empty() {
            return Err(WriterError::HybridWithoutCompressed);
        }
        let objs: Vec<u32> = compressed.iter().map(|(obj, _)| *obj).collect();
        let index: Vec<String> = runs(&objs)
            .into_iter()
            .map(|(start, count)| format!("{start} {count}"))
            .collect();
        let data: Vec<u8> = compressed
            .iter()
            .flat_map(|(_, row)| row.iter().copied())
            .collect();
        let dict = format!(
            "/Type /XRef /W [1 4 2] /Index [{}] /Size {}",
            index.join(" "),
            self.max_obj + 1
        );
        self.buf
            .extend_from_slice(format!("{xref_stm_obj} 0 obj\n").as_bytes());
        self.write_stream_body(&dict, &data, data.len());

        self.emit_table(entries, mismatches, trailer, Some(stm_offset))
    }

    /// Writes the classic table, trailer and `startxref` for `entries` and records the revision.
    ///
    /// Entries that live in an object stream are written as free entries chained into the free
    /// list (only hybrid-reference files get here with any).
    fn emit_table(
        &mut self,
        entries: Vec<EntryRecord>,
        mismatches: Vec<OffsetMismatch>,
        trailer: &Trailer,
        xref_stm: Option<u64>,
    ) -> Result<(), WriterError> {
        let first = self.revisions.is_empty();
        let mut free: Vec<u32> = Vec::new();
        if first {
            free.push(0);
        }
        free.extend(
            entries
                .iter()
                .filter(|e| matches!(e.location, Location::InObjectStream { .. }))
                .map(|e| e.obj),
        );
        let next_free = |obj: u32| {
            free.iter()
                .position(|f| *f == obj)
                .and_then(|at| free.get(at + 1))
                .copied()
                .unwrap_or(0)
        };
        let recorded = |e: &EntryRecord| match e.location {
            Location::Offset(actual) => mismatches
                .iter()
                .find(|m| m.obj == e.obj)
                .map_or(actual, |m| m.recorded),
            Location::InObjectStream { .. } => 0,
        };

        let mut lines: Vec<(u32, String)> = entries
            .iter()
            .map(|e| match e.location {
                Location::Offset(_) => (e.obj, format!("{:010} {:05} n \n", recorded(e), 0)),
                Location::InObjectStream { .. } => (e.obj, format!("{:010} {:05} f \n", next_free(e.obj), 0)),
            })
            .collect();
        if first {
            lines.insert(0, (0, format!("{:010} 65535 f \n", next_free(0))));
        }

        let xref_offset = self.position();
        let mut sections = vec!["xref\n".to_owned()];
        // One subsection per run of consecutive object numbers.
        for run in lines.chunk_by(|(a, _), (b, _)| a.checked_add(1) == Some(*b)) {
            sections.push(format!("{} {}\n", run[0].0, run.len()));
            sections.extend(run.iter().map(|(_, line)| line.clone()));
        }

        let prev = self.revisions.last().map(|r| r.xref_offset);
        let size = self.max_obj + 1;
        let (startxref, startxref_mismatch) = self.startxref_value(xref_offset)?;
        let mut trailer_text = trailer_entries(trailer, size, prev);
        if let Some(stm) = xref_stm {
            trailer_text = format!("{trailer_text} /XRefStm {stm}");
        }
        sections.push(format!(
            "trailer\n<< {trailer_text} >>\nstartxref\n{startxref}\n%%EOF\n"
        ));
        self.buf.extend_from_slice(sections.concat().as_bytes());

        self.revisions.push(RevisionRecord {
            kind: XrefKind::Table,
            xref_offset,
            prev,
            entries,
            offset_mismatches: mismatches,
            xref_stm,
            startxref_mismatch,
        });
        Ok(())
    }

    /// The `startxref` number to write for a section at `actual`, honouring a pending corruption.
    fn startxref_value(&mut self, actual: u64) -> Result<(u64, Option<StartxrefMismatch>), WriterError> {
        let Some(delta) = self.startxref_delta.take() else {
            return Ok((actual, None));
        };
        let recorded = actual
            .checked_add_signed(delta)
            .ok_or(WriterError::StartxrefOutOfRange)?;
        let mismatch = (recorded != actual).then_some(StartxrefMismatch { recorded, actual });
        Ok((recorded, mismatch))
    }

    /// Closes the open revision with a cross-reference stream object numbered `xref_obj`.
    ///
    /// The stream is uncompressed (`/W [1 4 2]`, no `/Filter`) and lists the revision's
    /// objects, including itself and any objects held in object streams.
    ///
    /// # Errors
    /// Returns the first usage error recorded so far, or an error found while finishing.
    pub fn finish_xref_stream(&mut self, xref_obj: u32, trailer: &Trailer) -> Result<(), WriterError> {
        self.take_error()?;
        let xref_offset = self.position();
        self.claim(xref_obj, Location::Offset(xref_offset));
        self.take_error()?;
        let entries = self.drain_entries();
        let mismatches = self.apply_corruptions(&entries)?;

        let first = self.revisions.is_empty();
        let mut rows: Vec<(u32, [u8; XREF_STREAM_ROW_LEN])> = Vec::new();
        if first {
            rows.push((0, xref_row(0, 0, u16::MAX)));
        }
        for e in &entries {
            let row = match e.location {
                Location::Offset(actual) => {
                    let recorded = mismatches
                        .iter()
                        .find(|m| m.obj == e.obj)
                        .map_or(actual, |m| m.recorded);
                    xref_row(
                        1,
                        u32::try_from(recorded).map_err(|_| WriterError::FieldOverflow(recorded))?,
                        0,
                    )
                }
                Location::InObjectStream { stream, index } => xref_row(
                    2,
                    stream,
                    u16::try_from(index).map_err(|_| WriterError::FieldOverflow(u64::from(index)))?,
                ),
            };
            rows.push((e.obj, row));
        }

        let objs: Vec<u32> = rows.iter().map(|(obj, _)| *obj).collect();
        let index: Vec<String> = runs(&objs)
            .into_iter()
            .map(|(start, count)| format!("{start} {count}"))
            .collect();
        let data: Vec<u8> = rows.iter().flat_map(|(_, row)| row.iter().copied()).collect();

        let prev = self.revisions.last().map(|r| r.xref_offset);
        let size = self.max_obj + 1;
        let dict = format!(
            "/Type /XRef /W [1 4 2] /Index [{}] {}",
            index.join(" "),
            trailer_entries(trailer, size, prev)
        );

        self.buf
            .extend_from_slice(format!("{xref_obj} 0 obj\n").as_bytes());
        self.write_stream_body(&dict, &data, data.len());
        let (startxref, startxref_mismatch) = self.startxref_value(xref_offset)?;
        self.buf
            .extend_from_slice(format!("startxref\n{startxref}\n%%EOF\n").as_bytes());

        self.revisions.push(RevisionRecord {
            kind: XrefKind::Stream,
            xref_offset,
            prev,
            entries,
            offset_mismatches: mismatches,
            xref_stm: None,
            startxref_mismatch,
        });
        Ok(())
    }

    /// Consumes the writer and returns the file with a description of its revisions.
    ///
    /// # Errors
    /// Fails if a usage error was recorded, if objects are waiting for an xref section, or if
    /// no revision was finished at all.
    pub fn into_output(mut self) -> Result<Output, WriterError> {
        self.take_error()?;
        if !self.pending.is_empty() {
            return Err(WriterError::UnfinishedRevision(self.pending.len()));
        }
        if self.revisions.is_empty() {
            return Err(WriterError::NoRevision);
        }
        Ok(Output {
            bytes: self.buf,
            revisions: self.revisions,
        })
    }

    fn begin_object(&mut self, num: u32) -> u64 {
        let offset = self.position();
        self.claim(num, Location::Offset(offset));
        self.buf.extend_from_slice(format!("{num} 0 obj\n").as_bytes());
        offset
    }

    fn write_stream_body(&mut self, dict: &str, data: &[u8], declared_length: usize) {
        let entries = if dict.is_empty() {
            format!("/Length {declared_length}")
        } else {
            format!("{dict} /Length {declared_length}")
        };
        self.buf
            .extend_from_slice(format!("<< {entries} >>\nstream\n").as_bytes());
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    fn claim(&mut self, obj: u32, location: Location) {
        if obj == 0 {
            self.fail(WriterError::ReservedObjectNumber);
        } else if self.pending.iter().any(|p| p.obj == obj) {
            self.fail(WriterError::DuplicateObject(obj));
        } else {
            self.max_obj = self.max_obj.max(obj);
            self.pending.push(Pending { obj, location });
        }
    }

    fn fail(&mut self, error: WriterError) {
        self.error.get_or_insert(error);
    }

    fn take_error(&mut self) -> Result<(), WriterError> {
        self.error.take().map_or(Ok(()), Err)
    }

    fn drain_entries(&mut self) -> Vec<EntryRecord> {
        let mut entries: Vec<EntryRecord> = self
            .pending
            .drain(..)
            .map(|p| EntryRecord {
                obj: p.obj,
                location: p.location,
            })
            .collect();
        entries.sort_by_key(|e| e.obj);
        entries
    }

    fn apply_corruptions(&mut self, entries: &[EntryRecord]) -> Result<Vec<OffsetMismatch>, WriterError> {
        let mut mismatches = Vec::new();
        for (&obj, &delta) in &self.corruptions {
            let actual = entries
                .iter()
                .find_map(|e| match e.location {
                    Location::Offset(actual) if e.obj == obj => Some(actual),
                    _ => None,
                })
                .ok_or(WriterError::CorruptionTarget(obj))?;
            let recorded = actual
                .checked_add_signed(delta)
                .ok_or(WriterError::CorruptionOutOfRange(obj))?;
            if recorded != actual {
                mismatches.push(OffsetMismatch {
                    obj,
                    recorded,
                    actual,
                });
            }
        }
        self.corruptions.clear();
        Ok(mismatches)
    }
}

/// Groups ascending object numbers into `(first, count)` runs of consecutive numbers.
fn runs(objs: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &obj in objs {
        match out.last_mut() {
            Some((start, count)) if *start + *count == obj => *count += 1,
            _ => out.push((obj, 1)),
        }
    }
    out
}

fn xref_row(kind: u8, field2: u32, field3: u16) -> [u8; XREF_STREAM_ROW_LEN] {
    let mut row = [0u8; XREF_STREAM_ROW_LEN];
    row[0] = kind;
    row[1..5].copy_from_slice(&field2.to_be_bytes());
    row[5..7].copy_from_slice(&field3.to_be_bytes());
    row
}

fn trailer_entries(trailer: &Trailer, size: u32, prev: Option<u64>) -> String {
    let mut parts = vec![format!("/Size {size}"), format!("/Root {} 0 R", trailer.root)];
    parts.extend(trailer.info.map(|info| format!("/Info {info} 0 R")));
    parts.extend(trailer.encrypt.map(|encrypt| format!("/Encrypt {encrypt} 0 R")));
    parts.push(format!("/ID [<{FIXED_ID_HEX}> <{FIXED_ID_HEX}>]"));
    parts.extend(prev.map(|prev| format!("/Prev {prev}")));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// Two-object file: catalog plus a pages node.
    fn tiny_table() -> Output {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
        w.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
        w.finish_table(&Trailer::new(1)).unwrap();
        w.into_output().unwrap()
    }

    #[test]
    fn table_file_matches_the_expected_bytes_exactly() {
        let out = tiny_table();

        // Header 9 bytes + marker 6 bytes = 15, so object 1 starts at 15 and is 49 bytes
        // long ("1 0 obj\n" 8 + body 33 + "\nendobj\n" 8) -> object 2 at 64, xref at 64 + 52 = 116.
        let expected = "\
%PDF-1.4\n%\u{e2}\u{e3}\u{cf}\u{d3}\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n\
xref\n0 3\n\
0000000000 65535 f \n\
0000000015 00000 n \n\
0000000064 00000 n \n\
trailer\n<< /Size 3 /Root 1 0 R /ID [<70646678726179000000000000000001> <70646678726179000000000000000001>] >>\n\
startxref\n116\n%%EOF\n";
        // The marker bytes are not valid UTF-8 on their own, so compare as Latin-1.
        let actual: String = out.bytes.iter().map(|&b| char::from(b)).collect();
        assert_eq!(actual, expected);
        // Smoke: the single revision points at the xref keyword.
        assert_eq!(out.revisions.len(), 1);
        assert!(out.bytes[116..].starts_with(b"xref\n"));
    }

    #[test]
    fn xref_entries_are_exactly_twenty_bytes() {
        let out = tiny_table();
        let xref = &out.bytes[usize::try_from(out.revisions[0].xref_offset).unwrap()..];
        let body = text(xref);
        let entries: Vec<&str> = body
            .lines()
            .skip(2) // "xref" and "0 3"
            .take(3)
            .collect();

        assert_eq!(
            entries,
            [
                "0000000000 65535 f ",
                "0000000015 00000 n ",
                "0000000064 00000 n "
            ]
        );
        // Each entry is 19 visible characters plus the one-byte EOL after the trailing space.
        let entry_bytes = &body.as_bytes()["xref\n0 3\n".len()..];
        for chunk in entry_bytes.chunks(20).take(3) {
            assert_eq!(chunk.len(), 20);
            assert_eq!(chunk[19], b'\n');
            assert_eq!(chunk[18], b' ');
        }
    }

    #[test]
    fn recorded_offsets_point_at_the_object_headers() {
        let out = tiny_table();
        for entry in &out.revisions[0].entries {
            let Location::Offset(offset) = entry.location else {
                unreachable!("tiny_table has no compressed objects");
            };
            let at = &out.bytes[usize::try_from(offset).unwrap()..];
            assert!(
                at.starts_with(format!("{} 0 obj\n", entry.obj).as_bytes()),
                "object {} not found at {offset}",
                entry.obj
            );
        }
        assert_eq!(out.revisions[0].objects(), vec![1, 2]);
    }

    #[test]
    fn startxref_matches_the_revision_record() {
        let out = tiny_table();
        let tail = text(&out.bytes[out.bytes.len() - 20..]);
        assert_eq!(tail.trim_end(), "startxref\n116\n%%EOF");
        assert_eq!(out.revisions[0].xref_offset, 116);
        assert_eq!(out.revisions[0].prev, None);
    }

    #[test]
    fn incremental_update_chains_prev_and_lists_only_changed_objects() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
        w.object(2, "<< /Type /Pages /Kids [] /Count 0 >>");
        w.finish_table(&Trailer::new(1)).unwrap();
        w.object(2, "<< /Type /Pages /Kids [] /Count 0 /Rev 2 >>");
        w.object(5, "<< /Title (updated) >>");
        w.finish_table(&Trailer::new(1).info(5)).unwrap();
        let out = w.into_output().unwrap();

        let [rev1, rev2] = out.revisions.as_slice() else {
            unreachable!("two revisions were finished");
        };
        assert_eq!(rev1.objects(), vec![1, 2]);
        assert_eq!(rev2.objects(), vec![2, 5]);
        assert_eq!(rev2.prev, Some(rev1.xref_offset));

        let tail = text(&out.bytes[usize::try_from(rev2.xref_offset).unwrap()..]);
        assert_eq!(
            tail,
            format!(
                "xref\n2 1\n{:010} 00000 n \n5 1\n{:010} 00000 n \ntrailer\n\
                 << /Size 6 /Root 1 0 R /Info 5 0 R /ID [<{FIXED_ID_HEX}> <{FIXED_ID_HEX}>] /Prev {} >>\n\
                 startxref\n{}\n%%EOF\n",
                entry_offset(rev2, 2),
                entry_offset(rev2, 5),
                rev1.xref_offset,
                rev2.xref_offset
            )
        );
        // Smoke: the original revision is still intact byte for byte at the start of the file.
        assert!(
            out.bytes
                .starts_with(&tiny_table().bytes[..usize::try_from(rev1.xref_offset).unwrap()])
        );
    }

    fn entry_offset(rev: &RevisionRecord, obj: u32) -> u64 {
        rev.entries
            .iter()
            .find_map(|e| match e.location {
                Location::Offset(o) if e.obj == obj => Some(o),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn corrupted_offsets_are_recorded_and_written() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<< /Type /Catalog >>");
        w.object(2, "<< /A 1 >>");
        w.object(3, "<< /B 2 >>");
        w.corrupt_offset(2, 7);
        w.corrupt_offset(3, -4);
        w.finish_table(&Trailer::new(1)).unwrap();
        let out = w.into_output().unwrap();

        let rev = &out.revisions[0];
        let actual2 = entry_offset(rev, 2);
        let actual3 = entry_offset(rev, 3);
        assert_eq!(
            rev.offset_mismatches,
            vec![
                OffsetMismatch {
                    obj: 2,
                    recorded: actual2 + 7,
                    actual: actual2
                },
                OffsetMismatch {
                    obj: 3,
                    recorded: actual3 - 4,
                    actual: actual3
                },
            ]
        );
        let table = text(&out.bytes[usize::try_from(rev.xref_offset).unwrap()..]);
        assert!(table.contains(&format!("{:010} 00000 n \n", actual2 + 7)));
        assert!(table.contains(&format!("{:010} 00000 n \n", actual3 - 4)));
        // Object 1 is untouched.
        assert!(table.contains(&format!("{:010} 00000 n \n", entry_offset(rev, 1))));
    }

    #[test]
    fn stream_length_is_exact_by_default_and_overridable() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.stream(1, "/Type /Test", b"abcd");
        w.stream_with_length(2, "", b"abcd", 99);
        w.finish_table(&Trailer::new(1)).unwrap();
        let out = w.into_output().unwrap();
        let body = text(&out.bytes);

        assert!(body.contains("1 0 obj\n<< /Type /Test /Length 4 >>\nstream\nabcd\nendstream\nendobj\n"));
        assert!(body.contains("2 0 obj\n<< /Length 99 >>\nstream\nabcd\nendstream\nendobj\n"));
    }

    #[test]
    fn xref_stream_rows_decode_to_the_written_objects() {
        let mut w = PdfWriter::new(PdfVersion::V1_5);
        w.object(1, "<< /Type /Catalog >>");
        w.object_stream(2, &[(3, "<< /A 1 >>"), (4, "<< /B 2 >>")]);
        w.finish_xref_stream(5, &Trailer::new(1)).unwrap();
        let out = w.into_output().unwrap();

        let rev = &out.revisions[0];
        assert_eq!(rev.kind, XrefKind::Stream);
        assert_eq!(rev.objects(), vec![1, 2, 3, 4, 5]);
        assert_eq!(rev.compressed(), vec![(3, 2, 0), (4, 2, 1)]);

        let at = usize::try_from(rev.xref_offset).unwrap();
        let obj = &out.bytes[at..];
        let header_end = obj
            .windows(8)
            .position(|w| w == b"stream\n\x00")
            .map(|p| p + 7)
            .unwrap();
        let rows = &obj[header_end..header_end + 6 * XREF_STREAM_ROW_LEN];
        let decoded: Vec<(u8, u32, u16)> = rows
            .chunks(XREF_STREAM_ROW_LEN)
            .map(|r| {
                (
                    r[0],
                    u32::from_be_bytes([r[1], r[2], r[3], r[4]]),
                    u16::from_be_bytes([r[5], r[6]]),
                )
            })
            .collect();
        assert_eq!(
            decoded,
            vec![
                (0, 0, 65535),
                (1, u32::try_from(entry_offset(rev, 1)).unwrap(), 0),
                (1, u32::try_from(entry_offset(rev, 2)).unwrap(), 0),
                (2, 2, 0),
                (2, 2, 1),
                (1, u32::try_from(rev.xref_offset).unwrap(), 0),
            ]
        );
        let header = text(&obj[..header_end]);
        assert!(header.starts_with("5 0 obj\n<< /Type /XRef /W [1 4 2] /Index [0 6] /Size 6 /Root 1 0 R"));
        assert!(header.contains("/Length 42 >>"));
    }

    #[test]
    fn object_stream_offsets_point_at_member_bodies() {
        let mut w = PdfWriter::new(PdfVersion::V1_5);
        w.object_stream(1, &[(2, "<< /A 1 >>"), (3, "[ 1 2 3 ]")]);
        w.finish_xref_stream(4, &Trailer::new(2)).unwrap();
        let out = w.into_output().unwrap();
        let body = text(&out.bytes);

        assert!(body.contains(
            "1 0 obj\n<< /Type /ObjStm /N 2 /First 9 /Length 30 >>\nstream\n2 0 3 11 << /A 1 >>\n[ 1 2 3 ]\n\nendstream"
        ));
    }

    #[test]
    fn duplicate_object_numbers_are_reported_on_finish() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<<>>");
        w.object(1, "<<>>");
        assert_eq!(
            w.finish_table(&Trailer::new(1)),
            Err(WriterError::DuplicateObject(1))
        );
    }

    #[test]
    fn object_zero_is_rejected() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(0, "<<>>");
        assert_eq!(
            w.finish_table(&Trailer::new(1)),
            Err(WriterError::ReservedObjectNumber)
        );
    }

    #[test]
    fn compressed_objects_need_an_xref_stream() {
        let mut w = PdfWriter::new(PdfVersion::V1_5);
        w.object_stream(1, &[(2, "<<>>")]);
        assert_eq!(
            w.finish_table(&Trailer::new(2)),
            Err(WriterError::CompressedInTable(2))
        );
    }

    #[test]
    fn corrupting_an_unknown_object_is_rejected() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<<>>");
        w.corrupt_offset(9, 1);
        assert_eq!(
            w.finish_table(&Trailer::new(1)),
            Err(WriterError::CorruptionTarget(9))
        );
    }

    #[test]
    fn negative_corrupted_offsets_are_rejected() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<<>>");
        w.corrupt_offset(1, -1000);
        assert_eq!(
            w.finish_table(&Trailer::new(1)),
            Err(WriterError::CorruptionOutOfRange(1))
        );
    }

    #[test]
    fn output_requires_a_finished_revision() {
        let empty = PdfWriter::new(PdfVersion::V1_4);
        assert_eq!(empty.into_output(), Err(WriterError::NoRevision));

        let mut open = PdfWriter::new(PdfVersion::V1_4);
        open.object(1, "<<>>");
        assert_eq!(open.into_output(), Err(WriterError::UnfinishedRevision(1)));
    }

    #[test]
    fn writing_the_same_sequence_twice_is_byte_identical() {
        assert_eq!(tiny_table(), tiny_table());
        assert!(tiny_table().bytes.starts_with(b"%PDF-1.4\n"));
    }

    #[test]
    fn corrupted_startxref_is_recorded_and_written() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<< /Type /Catalog >>");
        w.corrupt_startxref(5);
        w.finish_table(&Trailer::new(1)).unwrap();
        w.object(2, "<< /A 1 >>");
        w.finish_table(&Trailer::new(1)).unwrap();
        let out = w.into_output().unwrap();

        let [first, second] = out.revisions.as_slice() else {
            unreachable!("two revisions were finished");
        };
        // Only the revision that asked for it lies, and the lie is exactly the delta.
        assert_eq!(
            first.startxref_mismatch,
            Some(StartxrefMismatch {
                recorded: first.xref_offset + 5,
                actual: first.xref_offset
            })
        );
        assert_eq!(second.startxref_mismatch, None);
        let tail = text(&out.bytes[usize::try_from(first.xref_offset).unwrap()..]);
        assert!(tail.contains(&format!("startxref\n{}\n%%EOF", first.xref_offset + 5)));
        // Smoke: the /Prev chain still uses the true offset.
        assert_eq!(second.prev, Some(first.xref_offset));
    }

    #[test]
    fn startxref_corruption_cannot_go_negative() {
        let mut w = PdfWriter::new(PdfVersion::V1_4);
        w.object(1, "<<>>");
        w.corrupt_startxref(-100_000);
        assert_eq!(
            w.finish_table(&Trailer::new(1)),
            Err(WriterError::StartxrefOutOfRange)
        );
    }

    #[test]
    fn hybrid_revision_hides_compressed_members_and_names_the_xref_stream() {
        let mut w = PdfWriter::new(PdfVersion::V1_5);
        w.object(1, "<< /Type /Catalog >>");
        w.object_stream(2, &[(3, "<< /A 1 >>"), (4, "<< /B 2 >>")]);
        w.finish_hybrid(5, &Trailer::new(1)).unwrap();
        let out = w.into_output().unwrap();

        let rev = &out.revisions[0];
        let stm = entry_offset(rev, 5);
        assert_eq!(rev.kind, XrefKind::Table);
        assert_eq!(rev.xref_stm, Some(stm));
        assert_eq!(rev.objects(), vec![1, 2, 3, 4, 5]);
        assert_eq!(rev.compressed(), vec![(3, 2, 0), (4, 2, 1)]);

        let table = text(&out.bytes[usize::try_from(rev.xref_offset).unwrap()..]);
        // Free list: 0 -> 3 -> 4 -> 0; the stream object itself is in use.
        let free_chain: Vec<&str> = table.lines().skip(2).take(6).collect();
        assert_eq!(
            free_chain,
            [
                "0000000003 65535 f ",
                &format!("{:010} 00000 n ", entry_offset(rev, 1)),
                &format!("{:010} 00000 n ", entry_offset(rev, 2)),
                "0000000004 00000 f ",
                "0000000000 00000 f ",
                &format!("{stm:010} 00000 n "),
            ]
        );
        assert!(table.contains(&format!("/XRefStm {stm} >>\nstartxref\n{}", rev.xref_offset)));
        // The stream holds two type-2 rows: (object stream 2, index 0) and (2, 1).
        let at = usize::try_from(stm).unwrap();
        let header = text(&out.bytes[at..at + 80]);
        assert!(header.starts_with("5 0 obj\n<< /Type /XRef /W [1 4 2] /Index [3 2] /Size 6 /Length 14 >>"));
        let data = out.bytes[at..]
            .windows(7)
            .position(|w| w == b"stream\n")
            .map(|p| at + p + 7)
            .unwrap();
        assert_eq!(
            &out.bytes[data..data + 14],
            [2, 0, 0, 0, 2, 0, 0, 2, 0, 0, 0, 2, 0, 1]
        );
    }

    #[test]
    fn hybrid_needs_a_compressed_member() {
        let mut w = PdfWriter::new(PdfVersion::V1_5);
        w.object(1, "<<>>");
        assert_eq!(
            w.finish_hybrid(2, &Trailer::new(1)),
            Err(WriterError::HybridWithoutCompressed)
        );
    }

    #[test]
    fn runs_group_consecutive_numbers() {
        assert_eq!(runs(&[0, 1, 2, 5, 7, 8]), vec![(0, 3), (5, 1), (7, 2)]);
        assert_eq!(runs(&[]), Vec::<(u32, u32)>::new());
    }
}
