//! Object streams (`/Type /ObjStm`): decoded once while parsing, then members are parsed on
//! demand.

use std::collections::{HashMap, HashSet};

use super::{DocLengths, IndirectObject, ObjLocation, ObjStm, RawDocument};
use crate::filters::DecodeIssue;
use crate::lexer::{Lexer, TokenKind};
use crate::object::{Object, Spanned};
use crate::parser::{Parser, parse_indirect};
use crate::xref::{EntryState, XrefEntry};
use crate::{ObjRef, RawDiagnostic, RawDiagnosticKind, Span};

impl RawDocument {
    pub(super) fn load_compressed(
        &self,
        r: ObjRef,
        found: usize,
        entry: &XrefEntry,
        sink: &mut Vec<RawDiagnostic>,
    ) -> Option<IndirectObject> {
        let EntryState::Compressed { stream, index } = entry.state else {
            return None;
        };
        let offset = self.container_offset(stream, found)?;
        let os = self.objstms.get(&offset)?;
        let member = usize::try_from(index)
            .ok()
            .filter(|&i| os.members.get(i).is_some_and(|m| m.0 == r.num));
        let member = member.or_else(|| os.members.iter().position(|m| m.0 == r.num))?;
        let value = Self::parse_member(os, member, sink)?;
        Some(IndirectObject {
            r,
            span: value.span,
            value,
            location: ObjLocation::InObjectStream {
                stream: os.stream,
                index: u32::try_from(member).unwrap_or(u32::MAX),
            },
            revision: found,
        })
    }

    /// Parses one object stream member. Diagnostics are moved to the stream's data span because
    /// spans inside the decoded bytes are not file offsets.
    pub(super) fn parse_member(
        os: &ObjStm,
        member: usize,
        sink: &mut Vec<RawDiagnostic>,
    ) -> Option<Spanned<Object>> {
        let (_, start) = *os.members.get(member)?;
        let mut parser = Parser::new(&os.decoded, start);
        let value = parser.value(0).ok();
        sink.extend(
            parser
                .take_diagnostics()
                .into_iter()
                .map(|d| RawDiagnostic::new(d.kind, os.data)),
        );
        value
    }

    /// Decodes every object stream that some compressed entry points at.
    pub(super) fn load_object_streams(&mut self, diags: &mut Vec<RawDiagnostic>) {
        let mut loaded = HashMap::new();
        let mut attempted = HashSet::new();
        for (rev, revision) in self.revisions.iter().enumerate() {
            for entry in self.index[rev].effective(revision) {
                let EntryState::Compressed { stream, .. } = entry.state else {
                    continue;
                };
                let span = entry.span.unwrap_or(revision.xref.span);
                let Some(offset) = self.container_offset(stream, rev) else {
                    diags.push(RawDiagnostic::new(
                        RawDiagnosticKind::ObjStmContainerMissing {
                            obj: ObjRef::new(entry.num, 0),
                            stream,
                        },
                        span,
                    ));
                    continue;
                };
                if attempted.insert(offset)
                    && let Some(os) = self.read_object_stream(offset, entry, stream, span, diags)
                {
                    loaded.insert(offset, os);
                }
            }
        }
        self.objstms = loaded;
    }

    fn read_object_stream(
        &self,
        offset: usize,
        entry: &XrefEntry,
        stream_num: u32,
        span: Span,
        diags: &mut Vec<RawDiagnostic>,
    ) -> Option<ObjStm> {
        let rev = self.revisions.len() - 1;
        let parsed = parse_indirect(
            &self.bytes,
            offset,
            &DocLengths { doc: self, rev },
            &mut Vec::new(),
        )?;
        let sref = ObjRef::new(parsed.num, u16::try_from(parsed.generation).unwrap_or(u16::MAX));
        let malformed = |diags: &mut Vec<RawDiagnostic>, detail: &'static str, at: Span| {
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::ObjStmMalformed { stream: sref, detail },
                at,
            ));
        };
        let Object::Stream(stream) = &parsed.value.value else {
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::ObjStmContainerMissing {
                    obj: ObjRef::new(entry.num, 0),
                    stream: stream_num,
                },
                span,
            ));
            return None;
        };
        if stream
            .dict
            .get_name(b"Type")
            .is_none_or(|n| n.as_bytes() != b"ObjStm")
        {
            diags.push(RawDiagnostic::new(
                RawDiagnosticKind::ObjStmContainerMissing {
                    obj: ObjRef::new(entry.num, 0),
                    stream: stream_num,
                },
                span,
            ));
            return None;
        }
        let decoded = match self.decode_stream(stream) {
            Ok(d) if d.issues == [DecodeIssue::Encrypted] => {
                diags.push(RawDiagnostic::new(
                    RawDiagnosticKind::ObjStmEncrypted { stream: sref },
                    stream.data,
                ));
                return None;
            }
            Ok(d) => d,
            Err(_) => {
                malformed(diags, "stream cannot be decoded", stream.data);
                return None;
            }
        };
        if !decoded.complete {
            malformed(diags, "stream data could not be decoded completely", stream.data);
        }
        let count = stream
            .dict
            .get(b"N")
            .and_then(|v| self.resolve(&v.value).as_int());
        let first = stream
            .dict
            .get(b"First")
            .and_then(|v| self.resolve(&v.value).as_int());
        let (Some(count), Some(first)) = (
            count.and_then(|n| usize::try_from(n).ok()),
            first.and_then(|n| usize::try_from(n).ok()),
        ) else {
            malformed(diags, "/N or /First is missing or invalid", stream.data);
            return None;
        };
        let bytes = decoded.bytes;
        let mut lexer = Lexer::new(&bytes[..first.min(bytes.len())]);
        let mut members = Vec::new();
        for _ in 0..count.min(bytes.len() / 2) {
            let (num, rel) = (lexer.next_significant(), lexer.next_significant());
            let (TokenKind::Int(num), TokenKind::Int(rel)) = (num.kind, rel.kind) else {
                malformed(diags, "header has fewer number pairs than /N", stream.data);
                break;
            };
            let (Ok(num), Ok(rel)) = (u32::try_from(num), usize::try_from(rel)) else {
                malformed(diags, "header has a negative number", stream.data);
                break;
            };
            members.push((num, first.saturating_add(rel)));
        }
        Some(ObjStm {
            stream: sref,
            decoded: bytes,
            members,
            data: stream.data,
        })
    }
}
