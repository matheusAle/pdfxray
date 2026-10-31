//! Cross-reference streams: `/Type /XRef` objects with binary entries described by `/W` and
//! `/Index`.

use super::{EntryState, ParsedSection, XrefEntry, XrefKind, XrefSection, int_entry, report};
use crate::filters::{MAX_DECODED_LEN, decode_with};
use crate::object::{Dict, Object, Stream};
use crate::parser::{NoLengths, parse_indirect};
use crate::{ObjRef, RawDiagnostic, RawDiagnosticKind, Span};

pub(super) fn parse_xref_stream(
    src: &[u8],
    start: usize,
    diags: &mut Vec<RawDiagnostic>,
) -> Option<ParsedSection> {
    let mut local = Vec::new();
    let parsed = parse_indirect(src, start, &NoLengths, &mut local)?;
    let Object::Stream(stream) = &parsed.value.value else {
        return None;
    };
    if stream
        .dict
        .get_name(b"Type")
        .is_none_or(|n| n.as_bytes() != b"XRef")
    {
        return None;
    }
    diags.extend(local);
    let obj = ObjRef::new(parsed.num, u16::try_from(parsed.generation).unwrap_or(u16::MAX));
    let mut entries = Vec::new();
    read_stream_entries(src, stream, parsed.span, &mut entries, diags);
    Some(ParsedSection {
        section: XrefSection {
            kind: XrefKind::Stream(obj),
            span: parsed.span,
            entries,
        },
        prev: int_entry(&stream.dict, b"Prev"),
        xrefstm: None,
        trailer: Some(stream.dict.clone()),
    })
}

fn stream_error(diags: &mut Vec<RawDiagnostic>, detail: &'static str, span: Span) {
    report(diags, RawDiagnosticKind::XrefStreamMalformed { detail }, span);
}

/// The `/W` widths when they are three integers of at most 8 bytes.
fn field_widths(dict: &Dict) -> Option<[usize; 3]> {
    let items = dict.get(b"W")?.value.as_array()?;
    if items.len() != 3 {
        return None;
    }
    let mut widths = [0usize; 3];
    for (slot, item) in widths.iter_mut().zip(items) {
        *slot = usize::try_from(item.value.as_int()?).ok().filter(|&n| n <= 8)?;
    }
    Some(widths)
}

/// The `/Index` pairs, or `[0 Size]` when absent. `None` when present but unusable.
fn index_pairs(dict: &Dict) -> Option<Vec<(i64, i64)>> {
    let Some(index) = dict.get(b"Index") else {
        return Some(vec![(0, dict.get_int(b"Size")?)]);
    };
    let items = index.value.as_array()?;
    if items.len() % 2 != 0 {
        return None;
    }
    items
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[first, count]| Some((first.value.as_int()?, count.value.as_int()?)))
        .collect()
}

fn big_endian(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |acc, &b| acc << 8 | u64::from(b))
}

fn read_stream_entries(
    src: &[u8],
    stream: &Stream,
    object: Span,
    entries: &mut Vec<XrefEntry>,
    diags: &mut Vec<RawDiagnostic>,
) {
    let Some(raw) = stream.data.slice(src) else {
        stream_error(diags, "stream data lies outside the file", object);
        return;
    };
    let identity = |o: &Object| o.clone();
    let Ok(decoded) = decode_with(raw, &stream.dict, &identity, MAX_DECODED_LEN) else {
        stream_error(diags, "/Filter is invalid", object);
        return;
    };
    if !decoded.complete {
        stream_error(diags, "stream data could not be decoded completely", stream.data);
    }
    let Some(widths) = field_widths(&stream.dict) else {
        stream_error(
            diags,
            "/W is missing or is not three integers from 0 to 8",
            object,
        );
        return;
    };
    let pairs = index_pairs(&stream.dict).unwrap_or_else(|| {
        stream_error(diags, "/Index is missing, odd or not integers", object);
        Vec::new()
    });
    let row = widths.iter().sum::<usize>();
    if row == 0 {
        stream_error(diags, "all /W widths are 0", object);
        return;
    }
    let data = decoded.bytes.as_slice();
    let mut pos = 0usize;
    for (first, count) in pairs {
        let (Ok(first), Ok(count)) = (u32::try_from(first), u64::try_from(count)) else {
            stream_error(diags, "/Index has a negative or too large number", object);
            continue;
        };
        for i in 0..count {
            let Some(bytes) = data.get(pos..pos + row) else {
                stream_error(diags, "data is shorter than /Index describes", stream.data);
                return;
            };
            pos += row;
            let Some(num) = u32::try_from(i).ok().and_then(|i| first.checked_add(i)) else {
                break;
            };
            let (w0, w1) = (widths[0], widths[0] + widths[1]);
            let kind = if widths[0] == 0 {
                1
            } else {
                big_endian(&bytes[..w0])
            };
            let f2 = big_endian(&bytes[w0..w1]);
            let f3 = big_endian(&bytes[w1..]);
            let clamp16 = |v: u64| u16::try_from(v).unwrap_or(u16::MAX);
            let clamp32 = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
            let (generation, state) = match kind {
                0 => (clamp16(f3), EntryState::Free { next: clamp32(f2) }),
                1 => (clamp16(f3), EntryState::InUse { offset: f2 }),
                2 => (
                    0,
                    EntryState::Compressed {
                        stream: clamp32(f2),
                        index: clamp32(f3),
                    },
                ),
                other => {
                    report(
                        diags,
                        RawDiagnosticKind::XrefStreamBadEntryType {
                            num,
                            entry_type: other,
                        },
                        stream.data,
                    );
                    continue;
                }
            };
            entries.push(XrefEntry {
                num,
                generation,
                state,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::parse_section_at;
    use super::super::test_util::codes;
    use super::*;

    fn xref_stream(dict_extra: &str, data: &[u8]) -> Vec<u8> {
        xref_stream_w("[1 2 1]", dict_extra, data)
    }

    fn xref_stream_w(w: &str, dict_extra: &str, data: &[u8]) -> Vec<u8> {
        let mut out = format!(
            "9 0 obj\n<< /Type /XRef /Size 4 /W {w} {dict_extra} /Length {} >>\nstream\n",
            data.len()
        )
        .into_bytes();
        out.extend_from_slice(data);
        out.extend_from_slice(b"\nendstream\nendobj\n");
        out
    }

    #[test]
    fn xref_stream_decodes_all_three_entry_types() {
        let data = [0, 0, 0, 255, 1, 0, 15, 0, 2, 0, 9, 3, 7, 0, 0, 0];
        let src = xref_stream("", &data);
        let mut diags = Vec::new();
        let parsed = parse_section_at(&src, 0, &mut diags).unwrap();

        assert_eq!(
            codes(&diags),
            vec!["xref-stream-bad-entry-type"],
            "entry 3 has type 7: {diags:?}"
        );
        assert_eq!(parsed.section.kind, XrefKind::Stream(ObjRef::new(9, 0)));
        assert_eq!(
            parsed.section.entries,
            vec![
                XrefEntry {
                    num: 0,
                    generation: 255,
                    state: EntryState::Free { next: 0 },
                    span: None
                },
                XrefEntry {
                    num: 1,
                    generation: 0,
                    state: EntryState::InUse { offset: 15 },
                    span: None
                },
                XrefEntry {
                    num: 2,
                    generation: 0,
                    state: EntryState::Compressed { stream: 9, index: 3 },
                    span: None
                },
            ]
        );
    }

    #[test]
    fn xref_stream_honours_index_and_default_type() {
        let src = xref_stream_w("[0 2 1]", "/Index [10 1 20 1]", &[0, 100, 1, 0, 200, 0]);
        let mut diags = Vec::new();
        let parsed = parse_section_at(&src, 0, &mut diags).unwrap();

        assert_eq!(diags, vec![]);
        let seen: Vec<_> = parsed.section.entries.iter().map(|e| (e.num, e.state)).collect();
        assert_eq!(
            seen,
            vec![
                (10, EntryState::InUse { offset: 100 }),
                (20, EntryState::InUse { offset: 200 })
            ]
        );
    }

    #[test]
    fn xref_stream_with_short_data_or_bad_w_is_reported() {
        let short = xref_stream("", &[1, 0, 15]);
        let mut diags = Vec::new();
        let parsed = parse_section_at(&short, 0, &mut diags).unwrap();
        assert_eq!(parsed.section.entries, vec![]);
        assert_eq!(codes(&diags), vec!["xref-stream-malformed"]);

        let bad_w = xref_stream_w("[1 2]", "", &[0; 4]);
        let mut diags = Vec::new();
        parse_section_at(&bad_w, 0, &mut diags).unwrap();
        assert_eq!(
            diags.iter().map(|d| d.kind.clone()).collect::<Vec<_>>(),
            vec![RawDiagnosticKind::XrefStreamMalformed {
                detail: "/W is missing or is not three integers from 0 to 8"
            }]
        );
    }
}
