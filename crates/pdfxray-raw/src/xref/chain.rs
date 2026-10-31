//! The revision chain: follows `startxref` and `/Prev` from the newest section back, attaches
//! hybrid `/XRefStm` sections, and rebuilds from a whole-file scan when the chain cannot be
//! followed.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use memchr::memmem;

use super::startxref::{eof_after, final_startxref, startxref_after};
use super::{
    Chain, EntryState, ParsedSection, Revision, StartXref, XrefEntry, XrefKind, XrefSection,
    parse_section_at, report,
};
use crate::object::{Object, Spanned, is_whitespace};
use crate::parser::Parser;
use crate::scan::ObjectScan;
use crate::{RawDiagnostic, RawDiagnosticKind, Span};

/// The most revisions followed through `/Prev`, against hostile files.
const MAX_REVISIONS: usize = 10_000;

/// Candidate xref section starts found by scanning the whole file, ascending.
struct Sections<'a> {
    src: &'a [u8],
    scan: &'a ObjectScan,
    cache: OnceCell<Vec<usize>>,
}

impl<'a> Sections<'a> {
    fn new(src: &'a [u8], scan: &'a ObjectScan) -> Self {
        Self {
            src,
            scan,
            cache: OnceCell::new(),
        }
    }

    fn all(&self) -> &[usize] {
        self.cache.get_or_init(|| {
            let src = self.src;
            let mut found = Vec::new();
            for at in memmem::find_iter(src, b"xref") {
                let before_ok = at > 0 && src.get(at - 1).copied().is_some_and(is_whitespace);
                let after_ok = src.get(at + 4).copied().is_some_and(is_whitespace);
                if before_ok && after_ok {
                    found.push(at);
                }
            }
            for header in self.scan.headers() {
                let window = &src[header.end..src.len().min(header.end + 1024)];
                let dict = &window[..memmem::find(window, b"stream").unwrap_or(window.len())];
                if memmem::find(dict, b"/XRef").is_some() {
                    found.push(header.offset);
                }
            }
            found.sort_unstable();
            found.dedup();
            found
        })
    }
}

fn attach_hybrid(
    src: &[u8],
    mut section: ParsedSection,
    diags: &mut Vec<RawDiagnostic>,
    stream_offsets: &mut Vec<usize>,
) -> ParsedSection {
    let (XrefKind::Table, Some((value, span))) = (section.section.kind, section.xrefstm) else {
        return section;
    };
    let hybrid = usize::try_from(value)
        .ok()
        .and_then(|offset| parse_section_at(src, offset, diags))
        .filter(|p| matches!(p.section.kind, XrefKind::Stream(_)));
    if let Some(stream) = hybrid {
        stream_offsets.push(stream.start());
        section.section.entries.extend(stream.section.entries);
    } else {
        report(
            diags,
            RawDiagnosticKind::XrefSectionInvalid {
                offset: u64::try_from(value).unwrap_or(0),
            },
            span,
        );
    }
    section
}

/// Follows `/Prev` from `first`. Returns the sections newest first.
fn walk(
    src: &[u8],
    sections: &Sections<'_>,
    first: ParsedSection,
    diags: &mut Vec<RawDiagnostic>,
    stream_offsets: &mut Vec<usize>,
) -> Vec<ParsedSection> {
    let mut visited = HashSet::new();
    let mut out: Vec<ParsedSection> = Vec::new();
    let mut current = Some(first);
    while let Some(found) = current.take() {
        visited.insert(found.start());
        let found = attach_hybrid(src, found, diags, stream_offsets);
        let prev = found.prev;
        let start = found.start();
        out.push(found);
        if out.len() >= MAX_REVISIONS {
            break;
        }
        let Some((value, span)) = prev else {
            break;
        };
        let recorded = u64::try_from(value).unwrap_or(0);
        let direct = usize::try_from(value).ok().and_then(|offset| {
            let mut local = Vec::new();
            parse_section_at(src, offset, &mut local).map(|p| (p, local))
        });
        let (parsed, local) = if let Some(found) = direct {
            found
        } else {
            // `/Prev` points at nothing: take the nearest older section that was not visited.
            let nearest = sections
                .all()
                .iter()
                .copied()
                .filter(|&s| s < start && !visited.contains(&s))
                .min_by_key(|&s| u64::try_from(s).unwrap_or(u64::MAX).abs_diff(recorded));
            let mut local = Vec::new();
            let Some(p) = nearest.and_then(|s| parse_section_at(src, s, &mut local)) else {
                report(
                    diags,
                    RawDiagnosticKind::XrefSectionInvalid { offset: recorded },
                    span,
                );
                break;
            };
            (p, local)
        };
        if visited.contains(&parsed.start()) {
            report(
                diags,
                RawDiagnosticKind::PrevCycle {
                    offset: u64::try_from(parsed.start()).unwrap_or(0),
                },
                span,
            );
            break;
        }
        if u64::try_from(parsed.start()).ok() != Some(recorded) {
            report(
                diags,
                RawDiagnosticKind::XrefSectionOffsetWrong {
                    recorded,
                    actual: u64::try_from(parsed.start()).unwrap_or(0),
                },
                span,
            );
        }
        diags.extend(local);
        current = Some(parsed);
    }
    out
}

/// One section per entry-defining object found by scanning: the fallback when no xref section can
/// be followed.
fn synthetic_section(src: &[u8], scan: &ObjectScan, diags: &mut Vec<RawDiagnostic>) -> ParsedSection {
    let mut latest: HashMap<u32, usize> = HashMap::new();
    for (i, header) in scan.headers().iter().enumerate() {
        latest.insert(header.num, i);
    }
    let mut picked: Vec<usize> = latest.into_values().collect();
    picked.sort_unstable_by_key(|&i| scan.headers()[i].num);
    let entries = picked
        .into_iter()
        .map(|i| {
            let h = scan.headers()[i];
            XrefEntry {
                num: h.num,
                generation: u16::try_from(h.generation).unwrap_or(u16::MAX),
                state: EntryState::InUse {
                    offset: u64::try_from(h.offset).unwrap_or(0),
                },
                span: Some(Span::new(h.offset, h.end)),
            }
        })
        .collect();
    let trailer = memmem::rfind(src, b"trailer").and_then(|at| {
        let mut parser = Parser::new(src, at + b"trailer".len());
        match parser.value(0) {
            Ok(Spanned {
                value: Object::Dict(dict),
                ..
            }) => Some(dict),
            _ => None,
        }
    });
    if trailer.is_none() {
        report(
            diags,
            RawDiagnosticKind::TrailerMissing,
            Span::new(src.len(), src.len()),
        );
    }
    ParsedSection {
        section: XrefSection {
            kind: XrefKind::Table,
            span: Span::new(0, 0),
            entries,
        },
        trailer,
        prev: None,
        xrefstm: None,
    }
}

/// Reads every xref section found by scanning, oldest first. Falls back to one synthetic section
/// built from the object headers.
fn rebuild(
    src: &[u8],
    scan: &ObjectScan,
    sections: &Sections<'_>,
    diags: &mut Vec<RawDiagnostic>,
    stream_offsets: &mut Vec<usize>,
) -> Vec<ParsedSection> {
    let mut found = Vec::new();
    for &start in sections.all() {
        if let Some(parsed) = parse_section_at(src, start, diags) {
            found.push(attach_hybrid(src, parsed, diags, stream_offsets));
        }
    }
    let sections_found = found.len();
    if found.is_empty() {
        found.push(synthetic_section(src, scan, diags));
    }
    let mut numbers = HashSet::new();
    for section in &found {
        numbers.extend(section.section.entries.iter().map(|e| e.num));
    }
    report(
        diags,
        RawDiagnosticKind::XrefRebuilt {
            sections: sections_found,
            objects: numbers.len(),
        },
        Span::new(src.len(), src.len()),
    );
    found
}

/// Finds and reads every revision of the file.
pub(crate) fn build(src: &[u8], scan: &ObjectScan, diags: &mut Vec<RawDiagnostic>) -> Chain {
    let sections = Sections::new(src, scan);
    let mut stream_offsets = Vec::new();
    let end = Span::new(src.len(), src.len());
    let recorded = final_startxref(src);
    let (ordered, followed) = match recorded {
        None => {
            report(diags, RawDiagnosticKind::StartxrefMissing, end);
            (rebuild(src, scan, &sections, diags, &mut stream_offsets), false)
        }
        Some(sx) => {
            let direct = usize::try_from(sx.offset).ok().and_then(|offset| {
                let mut local = Vec::new();
                parse_section_at(src, offset, &mut local).map(|p| (p, local))
            });
            let first = direct.or_else(|| {
                let mut local = Vec::new();
                let last = sections.all().last().copied()?;
                parse_section_at(src, last, &mut local).map(|p| (p, local))
            });
            if let Some((first, local)) = first {
                if u64::try_from(first.start()).ok() != Some(sx.offset) {
                    report(
                        diags,
                        RawDiagnosticKind::StartxrefWrong {
                            recorded: sx.offset,
                            actual: u64::try_from(first.start()).ok(),
                        },
                        sx.span,
                    );
                }
                diags.extend(local);
                let mut newest_first = walk(src, &sections, first, diags, &mut stream_offsets);
                newest_first.reverse();
                (newest_first, true)
            } else {
                report(
                    diags,
                    RawDiagnosticKind::StartxrefWrong {
                        recorded: sx.offset,
                        actual: None,
                    },
                    sx.span,
                );
                (rebuild(src, scan, &sections, diags, &mut stream_offsets), false)
            }
        }
    };
    let revisions = finish(src, ordered, if followed { recorded } else { None }, diags);
    for revision in &revisions {
        if matches!(revision.xref.kind, XrefKind::Stream(_)) {
            stream_offsets.push(revision.xref.span.start);
        }
    }
    Chain {
        revisions,
        xref_stream_offsets: stream_offsets,
    }
}

/// Attaches `startxref` and `%%EOF` to every section, oldest first.
fn finish(
    src: &[u8],
    ordered: Vec<ParsedSection>,
    final_startxref: Option<StartXref>,
    diags: &mut Vec<RawDiagnostic>,
) -> Vec<Revision> {
    let count = ordered.len();
    let starts: HashSet<u64> = ordered
        .iter()
        .filter_map(|p| u64::try_from(p.start()).ok())
        .collect();
    let mut revisions = Vec::with_capacity(count);
    for (index, parsed) in ordered.into_iter().enumerate() {
        let is_newest = index + 1 == count;
        let startxref = match final_startxref {
            Some(sx) if is_newest => Some(sx),
            _ => startxref_after(src, parsed.section.span.end),
        };
        if let Some(sx) = startxref {
            let start = u64::try_from(parsed.start()).unwrap_or(0);
            // A `startxref` shared with the newest revision is checked there. One that names another
            // section of the file is how linearized files close their main xref section.
            let shared = final_startxref.is_some_and(|f| f.span == sx.span);
            if !shared && sx.offset != 0 && sx.offset != start && !starts.contains(&sx.offset) {
                report(
                    diags,
                    RawDiagnosticKind::StartxrefWrong {
                        recorded: sx.offset,
                        actual: Some(start),
                    },
                    sx.span,
                );
            }
        }
        let eof = startxref.and_then(|sx| eof_after(src, sx.span.end));
        if let (Some(sx), None) = (startxref, eof) {
            report(
                diags,
                RawDiagnosticKind::MissingEof,
                Span::new(sx.span.end, sx.span.end),
            );
        }
        revisions.push(Revision {
            index,
            xref: parsed.section,
            startxref,
            trailer: parsed.trailer,
            eof,
        });
    }
    revisions
}
