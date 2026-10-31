//! The checks that run once over the whole file after parsing: xref offsets against the object
//! headers, every object's syntax, trailers, free lists, the bytes after the last `%%EOF`, and
//! objects no xref entry lists.

use std::collections::{HashMap, HashSet};

use memchr::memmem;

use super::{DocLengths, RawDocument};
use crate::object::{Object, Spanned};
use crate::parser::{parse_indirect, read_obj_header, skip_whitespace};
use crate::scan::ObjectScan;
use crate::xref::{EntryState, XrefEntry};
use crate::{FreeListProblem, ObjRef, RawDiagnostic, RawDiagnosticKind, Span};

impl RawDocument {
    /// Checks every in-use xref entry against the bytes: the `num gen obj` header must be at the
    /// recorded offset, otherwise it is searched for.
    pub(super) fn verify_offsets(&mut self, scan: &ObjectScan, diags: &mut Vec<RawDiagnostic>) {
        for rev in 0..self.revisions.len() {
            let mut resolved = HashMap::new();
            let revision = &self.revisions[rev];
            for entry in self.index[rev].effective(revision) {
                let EntryState::InUse { offset } = entry.state else {
                    continue;
                };
                let at_entry = entry.span.unwrap_or(revision.xref.span);
                let obj = ObjRef::new(entry.num, entry.generation);
                let recorded_at = usize::try_from(offset).ok();
                let header = recorded_at.and_then(|at| read_obj_header(&self.bytes, at));
                let at = match header {
                    Some((n, g, _)) if n == entry.num && g == u32::from(entry.generation) => recorded_at,
                    Some((n, g, _)) if n == entry.num => {
                        diags.push(RawDiagnostic::new(
                            RawDiagnosticKind::ObjectNumberMismatch {
                                expected: obj,
                                found: ObjRef::new(n, u16::try_from(g).unwrap_or(u16::MAX)),
                            },
                            at_entry,
                        ));
                        recorded_at
                    }
                    other => {
                        if let Some((n, g, _)) = other {
                            diags.push(RawDiagnostic::new(
                                RawDiagnosticKind::ObjectNumberMismatch {
                                    expected: obj,
                                    found: ObjRef::new(n, u16::try_from(g).unwrap_or(u16::MAX)),
                                },
                                at_entry,
                            ));
                        }
                        let actual = scan.nearest(entry.num, u32::from(entry.generation), offset);
                        diags.push(RawDiagnostic::new(
                            RawDiagnosticKind::XrefOffsetWrong {
                                obj,
                                recorded: offset,
                                actual: actual.and_then(|a| u64::try_from(a).ok()),
                            },
                            at_entry,
                        ));
                        actual
                    }
                };
                resolved.insert(entry.num, at);
            }
            self.index[rev].resolved = resolved;
        }
    }

    /// Parses every object once, which records their syntax diagnostics. Returns the byte ranges
    /// occupied by stream data.
    pub(super) fn check_objects(&self, diags: &mut Vec<RawDiagnostic>) -> Vec<Span> {
        let mut visited = HashSet::new();
        let mut ranges = Vec::new();
        for (rev, revision) in self.revisions.iter().enumerate() {
            for entry in self.index[rev].effective(revision) {
                match entry.state {
                    EntryState::InUse { .. } => {
                        let Some(at) = self.resolved_offset(rev, entry.num) else {
                            continue;
                        };
                        if !visited.insert(at) {
                            continue;
                        }
                        let lengths = DocLengths { doc: self, rev };
                        if let Some(p) = parse_indirect(&self.bytes, at, &lengths, diags)
                            && let Object::Stream(s) = &p.value.value
                        {
                            ranges.push(s.data);
                        }
                    }
                    EntryState::Compressed { stream, index } => {
                        self.check_member(entry, rev, stream, index, revision.xref.span, diags);
                    }
                    EntryState::Free { .. } => {}
                }
            }
        }
        ranges
    }

    fn check_member(
        &self,
        entry: &XrefEntry,
        rev: usize,
        stream: u32,
        index: u32,
        section: Span,
        diags: &mut Vec<RawDiagnostic>,
    ) {
        let span = entry.span.unwrap_or(section);
        let Some(os) = self
            .container_offset(stream, rev)
            .and_then(|o| self.objstms.get(&o))
        else {
            return;
        };
        let at_index = usize::try_from(index).ok().and_then(|i| os.members.get(i));
        let member = if at_index.is_some_and(|m| m.0 == entry.num) {
            usize::try_from(index).ok()
        } else {
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::ObjStmEntryMismatch {
                    stream: os.stream,
                    index,
                    expected: entry.num,
                    found: at_index.map(|m| m.0),
                },
                span,
            ));
            os.members.iter().position(|m| m.0 == entry.num)
        };
        let Some(member) = member else {
            return;
        };
        if Self::parse_member(os, member, diags).is_none() {
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::ObjStmMemberMalformed {
                    stream: os.stream,
                    index: u32::try_from(member).unwrap_or(u32::MAX),
                },
                os.data,
            ));
        }
    }

    pub(super) fn check_trailers(&self, diags: &mut Vec<RawDiagnostic>) {
        let mut highest: Option<u32> = None;
        for revision in &self.revisions {
            highest = revision.xref.entries.iter().map(|e| e.num).chain(highest).max();
            let Some(trailer) = &revision.trailer else {
                continue;
            };
            let missing = |key: &'static str| {
                RawDiagnostic::new(RawDiagnosticKind::TrailerKeyMissing { key }, revision.xref.span)
            };
            let invalid = |key: &'static str, span: Span| {
                RawDiagnostic::new(RawDiagnosticKind::TrailerKeyInvalid { key }, span)
            };
            match trailer.get(b"Size") {
                None => diags.push(missing("Size")),
                Some(Spanned {
                    value: Object::Int(size),
                    span,
                }) => {
                    let required = highest.map_or(0, |h| u64::from(h) + 1);
                    if u64::try_from(*size).ok().is_none_or(|s| s < required) {
                        diags.push(RawDiagnostic::new(
                            RawDiagnosticKind::TrailerSizeInconsistent {
                                size: *size,
                                required,
                            },
                            *span,
                        ));
                    }
                }
                Some(other) => diags.push(invalid("Size", other.span)),
            }
            // Only the newest trailer needs /Root; the main trailer of a linearized file has none.
            match trailer.get(b"Root") {
                None if revision.index + 1 < self.revisions.len() => {}
                None => diags.push(missing("Root")),
                Some(Spanned {
                    value: Object::Ref(_),
                    ..
                }) => {}
                Some(other) => diags.push(invalid("Root", other.span)),
            }
        }
    }

    pub(super) fn check_free_lists(&self, diags: &mut Vec<RawDiagnostic>) {
        for (revision, index) in self.revisions.iter().zip(&self.index) {
            let entries = &revision.xref.entries;
            let entry_of = |num: u32| index.by_num.get(&num).map(|&i| &entries[i]);
            let Some(head) = entry_of(0) else {
                continue;
            };
            let span = head.span.unwrap_or(revision.xref.span);
            let EntryState::Free { .. } = head.state else {
                diags.push(RawDiagnostic::new(
                    RawDiagnosticKind::FreeListBroken {
                        obj: 0,
                        problem: FreeListProblem::HeadNotFree,
                    },
                    span,
                ));
                continue;
            };
            // Hybrid files list compressed objects as free in the table, so links into entries
            // that the stream defines are expected there.
            let hybrid = revision
                .trailer
                .as_ref()
                .is_some_and(|t| t.get(b"XRefStm").is_some());
            let mut visited = HashSet::from([0u32]);
            let mut current = head;
            while let EntryState::Free { next } = current.state {
                if next == 0 {
                    break;
                }
                let Some(target) = entry_of(next) else {
                    break;
                };
                let at = target.span.unwrap_or(revision.xref.span);
                if !visited.insert(next) {
                    diags.push(RawDiagnostic::new(
                        RawDiagnosticKind::FreeListBroken {
                            obj: next,
                            problem: FreeListProblem::Loop,
                        },
                        at,
                    ));
                    break;
                }
                if !target.state.is_free() {
                    if !hybrid {
                        diags.push(RawDiagnostic::new(
                            RawDiagnosticKind::FreeListBroken {
                                obj: current.num,
                                problem: FreeListProblem::NextInUse,
                            },
                            current.span.unwrap_or(revision.xref.span),
                        ));
                    }
                    break;
                }
                current = target;
            }
        }
    }

    pub(super) fn check_tail(&self, diags: &mut Vec<RawDiagnostic>) {
        let bytes = self.bytes();
        let Some(eof) = memmem::rfind(bytes, b"%%EOF") else {
            return;
        };
        let after = eof + b"%%EOF".len();
        let rest = skip_whitespace(bytes, after);
        if rest < bytes.len() {
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::TrailingData {
                    len: bytes.len() - rest,
                },
                Span::new(rest, bytes.len()),
            ));
        }
    }

    pub(super) fn check_unreferenced(
        &self,
        scan: &ObjectScan,
        xref_streams: &[usize],
        mut stream_ranges: Vec<Span>,
        diags: &mut Vec<RawDiagnostic>,
    ) {
        let mut known: HashSet<usize> = xref_streams.iter().copied().collect();
        for index in &self.index {
            known.extend(index.resolved.values().flatten().copied());
        }
        stream_ranges.sort_unstable();
        let inside_stream = |offset: usize| {
            let upto = stream_ranges.partition_point(|s| s.start <= offset);
            upto.checked_sub(1)
                .is_some_and(|last| offset < stream_ranges[last].end)
        };
        for header in scan.headers() {
            if known.contains(&header.offset) || inside_stream(header.offset) {
                continue;
            }
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::UnreferencedObject {
                    obj: ObjRef::new(header.num, u16::try_from(header.generation).unwrap_or(u16::MAX)),
                },
                Span::new(header.offset, header.end),
            ));
        }
    }
}
