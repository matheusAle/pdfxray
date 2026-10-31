//! Hand-built files with one deliberate defect each: the diagnostic must say what the file claims
//! versus what is there, and parsing must keep going.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

mod support;

use std::io::Write;
use std::sync::Arc;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use pdfxray_raw::{
    ContainerKind, DecodeIssue, EntryState, FreeListProblem, Name, ObjLocation, ObjRef, Object,
    RawDiagnostic, RawDiagnosticKind, RawDocument, RawError, Severity, Span, StartXref, XrefKind,
};
use pretty_assertions::assert_eq;
use support::{Builder, HEADER, base, codes, find, parse, rfind};

fn diag(kind: RawDiagnosticKind, start: usize, end: usize) -> RawDiagnostic {
    RawDiagnostic::new(kind, Span::new(start, end))
}

#[test]
fn a_well_formed_file_has_no_diagnostics() {
    let (b, xref) = base();
    let doc = parse(&b.buf);

    assert_eq!(doc.diagnostics(), vec![]);
    assert_eq!(doc.revisions().len(), 1);
    let rev = &doc.revisions()[0];
    assert_eq!(rev.xref.kind, XrefKind::Table);
    assert_eq!(rev.xref.span.start, xref);
    assert_eq!(rev.startxref.map(|s| s.offset), Some(xref as u64));
    assert_eq!(
        rev.startxref,
        Some(StartXref {
            offset: xref as u64,
            span: Span::new(rev.startxref.unwrap().span.start, rev.eof.unwrap().start - 1)
        })
    );
    assert_eq!(rev.eof.map(Span::len), Some(5));
    assert_eq!(
        doc.object_refs(),
        vec![ObjRef::new(1, 0), ObjRef::new(2, 0), ObjRef::new(3, 0)]
    );
    // Smoke: the bytes round-trip and the header is where it should be.
    assert_eq!(doc.bytes(), &b.buf[..]);
    assert_eq!(doc.header().span, Span::new(0, 8));
}

#[test]
fn objects_carry_exact_spans_and_locations() {
    let (b, _) = base();
    let doc = parse(&b.buf);
    let page = doc.object(ObjRef::new(3, 0)).unwrap();
    let at = b.offsets[&3];

    assert_eq!(page.r, ObjRef::new(3, 0));
    assert_eq!(page.location, ObjLocation::Direct { offset: at as u64 });
    assert_eq!(page.revision, 0);
    assert_eq!(page.span.start, at);
    assert_eq!(
        page.span.slice(doc.bytes()).unwrap(),
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>\nendobj"
    );
    assert_eq!(
        page.value.span.slice(doc.bytes()).unwrap(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>"
    );
    assert_eq!(
        page.value.value.to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>"
    );
    assert!(doc.object(ObjRef::new(99, 0)).is_none());
}

#[test]
fn a_different_generation_is_not_the_object() {
    let (b, _) = base();
    let doc = parse(&b.buf);

    assert_eq!(doc.object(ObjRef::new(3, 1)), None);
    assert!(doc.object(ObjRef::new(3, 0)).is_some());
}

#[test]
fn no_header_is_the_only_fatal_error() {
    assert_eq!(
        RawDocument::parse(Arc::new(b"hello world".to_vec())).unwrap_err(),
        RawError::MissingHeader
    );
    assert_eq!(
        RawDocument::parse(Arc::new(Vec::new())).unwrap_err(),
        RawError::MissingHeader
    );
    // Smoke: a header alone is enough to get a document, with rebuilt (empty) xref data.
    let doc = parse(b"%PDF-1.4\n");
    assert_eq!(doc.revisions().len(), 1);
    assert_eq!(doc.object_refs(), vec![]);
}

#[test]
fn header_after_junk_is_reported_and_offsets_stay_absolute() {
    let (b, _) = base();
    let mut bytes = b"junk\n".to_vec();
    bytes.extend_from_slice(&b.buf);
    let doc = parse(&bytes);

    assert_eq!(doc.header().span, Span::new(5, 13));
    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(kinds[0], RawDiagnosticKind::HeaderNotAtStart { offset: 5 });
    // The file's offsets are now 5 bytes off and are reported, not shifted.
    assert!(codes(&doc).contains(&"xref-offset-wrong") || codes(&doc).contains(&"startxref-wrong"));
}

#[test]
fn wrong_xref_offset_is_reported_and_the_real_object_is_returned() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog /Pages 2 0 R >>");
    b.object(2, 0, "<< /Type /Pages /Kids [] /Count 0 >>");
    let wrong = (b.offsets[&2] + 7) as u64;
    let xref = b.table(3, &[(2, wrong)]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let entry_start = xref + "xref\n0 3\n".len() + 2 * 20;
    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::XrefOffsetWrong {
                obj: ObjRef::new(2, 0),
                recorded: wrong,
                actual: Some(b.offsets[&2] as u64),
            },
            entry_start,
            entry_start + 18,
        )]
    );
    let found = doc.object(ObjRef::new(2, 0)).unwrap();
    assert_eq!(
        found.location,
        ObjLocation::Direct {
            offset: b.offsets[&2] as u64
        }
    );
    assert_eq!(
        found.value.value.to_string(),
        "<< /Type /Pages /Kids [] /Count 0 >>"
    );
    // The xref entry itself still says what the file says.
    assert_eq!(
        doc.revisions()[0].xref.entries[2].state,
        EntryState::InUse { offset: wrong }
    );
}

#[test]
fn offset_pointing_at_nothing_has_no_actual() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let xref = b.table(3, &[(2, 99_999)]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![RawDiagnosticKind::XrefOffsetWrong {
            obj: ObjRef::new(2, 0),
            recorded: 99_999,
            actual: None
        }]
    );
    assert_eq!(doc.diagnostics()[0].severity(), Severity::Error);
    assert_eq!(doc.object(ObjRef::new(2, 0)), None);
    assert!(doc.object(ObjRef::new(1, 0)).is_some());
}

#[test]
fn xref_pointing_at_another_object_reports_the_mismatch() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.object(2, 0, "<< /Marker /Two >>");
    let xref = b.table(3, &[(2, b.offsets[&1] as u64)]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            RawDiagnosticKind::ObjectNumberMismatch {
                expected: ObjRef::new(2, 0),
                found: ObjRef::new(1, 0)
            },
            RawDiagnosticKind::XrefOffsetWrong {
                obj: ObjRef::new(2, 0),
                recorded: b.offsets[&1] as u64,
                actual: Some(b.offsets[&2] as u64)
            },
        ]
    );
    assert_eq!(
        doc.object(ObjRef::new(2, 0)).unwrap().value.value.to_string(),
        "<< /Marker /Two >>"
    );
}

#[test]
fn generation_mismatch_at_the_recorded_offset_is_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.object(2, 3, "null");
    let xref = b.table(3, &[]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![RawDiagnosticKind::ObjectNumberMismatch {
            expected: ObjRef::new(2, 0),
            found: ObjRef::new(2, 3)
        }]
    );
    // The xref says generation 0, so asking for generation 3 finds nothing.
    assert_eq!(doc.object(ObjRef::new(2, 3)), None);
    assert_eq!(doc.object(ObjRef::new(2, 0)).unwrap().r, ObjRef::new(2, 3));
}

#[test]
fn wrong_startxref_is_reported_with_the_real_section() {
    let (b, xref) = base();
    let mut bytes = b.buf.clone();
    let at = rfind(&bytes, b"startxref");
    let wrong = xref - 3;
    bytes.truncate(at);
    bytes.extend_from_slice(format!("startxref\n{wrong}\n%%EOF\n").as_bytes());
    let doc = parse(&bytes);

    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::StartxrefWrong {
                recorded: wrong as u64,
                actual: Some(xref as u64)
            },
            at,
            at + "startxref\n".len() + wrong.to_string().len()
        )]
    );
    assert_eq!(doc.object_refs().len(), 3);
}

#[test]
fn startxref_pointing_at_white_space_before_xref_is_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< >>");
    b.raw(b"\n");
    let xref = b.table(2, &[]);
    b.trailer("/Size 2 /Root 1 0 R");
    b.startxref(xref as u64 - 1);
    let doc = parse(&b.buf);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![RawDiagnosticKind::StartxrefWrong {
            recorded: xref as u64 - 1,
            actual: Some(xref as u64)
        }]
    );
}

#[test]
fn missing_startxref_rebuilds_from_the_xref_sections_found() {
    let (b, xref) = base();
    let cut = rfind(&b.buf, b"startxref");
    let doc = parse(&b.buf[..cut]);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            RawDiagnosticKind::StartxrefMissing,
            RawDiagnosticKind::XrefRebuilt {
                sections: 1,
                objects: 4
            },
        ]
    );
    assert_eq!(doc.revisions()[0].xref.span.start, xref);
    assert_eq!(doc.revisions()[0].startxref, None);
    assert_eq!(
        doc.object_refs(),
        vec![ObjRef::new(1, 0), ObjRef::new(2, 0), ObjRef::new(3, 0)]
    );
}

#[test]
fn a_file_without_any_xref_is_rebuilt_from_object_headers() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.object(2, 0, "<< /A 1 >>");
    b.raw(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    let doc = parse(&b.buf);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            RawDiagnosticKind::StartxrefMissing,
            RawDiagnosticKind::XrefRebuilt {
                sections: 0,
                objects: 2
            },
        ]
    );
    assert_eq!(
        doc.object(ObjRef::new(2, 0)).unwrap().value.value.to_string(),
        "<< /A 1 >>"
    );
    assert_eq!(doc.trailer().and_then(|t| t.get_int(b"Size")), Some(3));
}

#[test]
fn wrong_stream_length_uses_the_real_data() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let start = b.buf.len();
    b.raw(b"2 0 obj\n<< /Length 3 >>\nstream\nHello World\nendstream\nendobj\n");
    b.offsets.insert(2, start);
    let xref = b.table(3, &[]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let stream = doc.object(ObjRef::new(2, 0)).unwrap();
    let Object::Stream(s) = &stream.value.value else {
        panic!("not a stream: {:?}", stream.value)
    };
    assert_eq!(s.data.slice(doc.bytes()).unwrap(), b"Hello World");
    assert_eq!(s.declared_length, Some(3));
    let data_start = start + "2 0 obj\n<< /Length 3 >>\nstream\n".len();
    assert_eq!(s.data, Span::new(data_start, data_start + 11));
    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::StreamLengthWrong {
                obj: ObjRef::new(2, 0),
                declared: Some(3),
                actual: 11
            },
            data_start,
            data_start + 11
        )]
    );
}

#[test]
fn indirect_length_is_resolved_and_checked() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let start = b.buf.len();
    b.raw(b"2 0 obj\n<< /Length 3 0 R >>\nstream\nabcde\nendstream\nendobj\n");
    b.offsets.insert(2, start);
    b.object(3, 0, "5");
    let xref = b.table(4, &[]);
    b.trailer("/Size 4 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let Object::Stream(s) = doc.object(ObjRef::new(2, 0)).unwrap().value.value else {
        panic!("not a stream")
    };
    assert_eq!(s.declared_length, Some(5));
    assert_eq!(s.data.slice(doc.bytes()).unwrap(), b"abcde");
    assert_eq!(doc.diagnostics(), vec![]);
}

#[test]
fn missing_endstream_and_endobj_are_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let start = b.buf.len();
    b.raw(b"2 0 obj\n<< /Length 4 >>\nstream\nabcd\n");
    b.offsets.insert(2, start);
    let tail = b.buf.len();
    b.raw(b"3 0 obj\n<< /K 1 >>\n");
    b.offsets.insert(3, tail);
    let xref = b.table(4, &[]);
    b.trailer("/Size 4 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let data_end = start + "2 0 obj\n<< /Length 4 >>\nstream\nabcd\n".len();
    assert_eq!(
        doc.diagnostics(),
        vec![
            diag(RawDiagnosticKind::MissingEndstream, data_end - 1, data_end - 1),
            diag(RawDiagnosticKind::MissingEndobj, data_end, data_end),
            diag(RawDiagnosticKind::MissingEndobj, xref, xref),
        ]
    );
    let Object::Stream(s) = doc.object(ObjRef::new(2, 0)).unwrap().value.value else {
        panic!("not a stream")
    };
    assert_eq!(s.data.slice(doc.bytes()).unwrap(), b"abcd");
}

#[test]
fn duplicate_dictionary_keys_are_kept_and_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog /Lang (en) /Lang (de) >>");
    let xref = b.table(2, &[]);
    b.trailer("/Size 2 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let obj = doc.object(ObjRef::new(1, 0)).unwrap();
    let dict = obj.value.value.as_dict().unwrap();
    assert_eq!(dict.len(), 3);
    assert_eq!(dict.get(b"Lang").unwrap().value.to_string(), "(en)");
    assert_eq!(
        dict.get_all(b"Lang")
            .map(|v| v.value.to_string())
            .collect::<Vec<_>>(),
        ["(en)", "(de)"]
    );
    let key_at = obj.value.span.start + "<< /Type /Catalog /Lang (en) ".len();
    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::DuplicateDictKey {
                key: Name::new("Lang")
            },
            key_at,
            key_at + 5
        )]
    );
}

#[test]
fn data_after_the_last_eof_is_reported() {
    let (b, _) = base();
    let mut bytes = b.buf.clone();
    let end = bytes.len();
    bytes.extend_from_slice(b"\n\ngarbage!");
    let doc = parse(&bytes);

    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::TrailingData { len: 8 },
            end + 2,
            end + 10
        )]
    );
    // Trailing white space alone is fine.
    let mut quiet = b.buf.clone();
    quiet.extend_from_slice(b"\r\n \n");
    assert_eq!(parse(&quiet).diagnostics(), vec![]);
}

#[test]
fn missing_eof_is_reported() {
    let (b, _) = base();
    let cut = b.buf.len() - "%%EOF\n".len();
    let doc = parse(&b.buf[..cut]);

    assert_eq!(codes(&doc), vec!["missing-eof"]);
    assert_eq!(doc.revisions()[0].eof, None);
}

#[test]
fn objects_in_no_xref_section_are_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let lost = b.buf.len();
    b.raw(b"7 0 obj\n(lost)\nendobj\n");
    let xref = b.table(2, &[]);
    b.trailer("/Size 8 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::UnreferencedObject {
                obj: ObjRef::new(7, 0)
            },
            lost,
            lost + 7
        )]
    );
    assert_eq!(doc.object(ObjRef::new(7, 0)), None);
}

#[test]
fn object_like_text_inside_stream_data_is_not_an_object() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.stream(2, "", b"5 0 obj\n(inside a stream)\nendobj");
    let xref = b.table(3, &[]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    assert_eq!(doc.diagnostics(), vec![]);
}

#[test]
fn trailer_problems_are_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< >>");
    let xref = b.table(3, &[]);
    b.trailer("/Size 2 /Root 7");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let size_at = b.buf.windows(8).position(|w| w == b"/Size 2 ").unwrap() + "/Size ".len();
    let root_at = b.buf.windows(7).position(|w| w == b"/Root 7").unwrap() + "/Root ".len();
    assert_eq!(
        doc.diagnostics(),
        vec![
            diag(
                RawDiagnosticKind::TrailerSizeInconsistent { size: 2, required: 3 },
                size_at,
                size_at + 1
            ),
            diag(
                RawDiagnosticKind::TrailerKeyInvalid { key: "Root" },
                root_at,
                root_at + 1
            ),
        ]
    );
}

#[test]
fn free_list_problems_are_reported() {
    let table = |entries: &str, count: usize| {
        let mut b = Builder::new();
        b.object(1, 0, "<< >>");
        let xref = b.buf.len();
        b.raw(format!("xref\n0 {count}\n{entries}").as_bytes());
        b.trailer(&format!("/Size {count} /Root 1 0 R"));
        b.startxref(xref as u64);
        (parse(&b.buf), xref)
    };
    let one = format!("{:010} 00000 n \n", HEADER.len());

    // Entry 0 in use.
    let (doc, xref) = table(&format!("0000000000 00000 n \n{one}"), 2);
    let head = xref + "xref\n0 2\n".len();
    assert_eq!(
        doc.diagnostics()
            .iter()
            .filter(|d| d.code() == "free-list-broken")
            .cloned()
            .collect::<Vec<_>>(),
        vec![diag(
            RawDiagnosticKind::FreeListBroken {
                obj: 0,
                problem: FreeListProblem::HeadNotFree
            },
            head,
            head + 18
        )]
    );
    // 0 -> 1 where 1 is in use.
    let (doc, xref) = table(&format!("0000000001 65535 f \n{one}"), 2);
    let head = xref + "xref\n0 2\n".len();
    assert_eq!(
        doc.diagnostics()
            .iter()
            .filter(|d| d.code() == "free-list-broken")
            .cloned()
            .collect::<Vec<_>>(),
        vec![diag(
            RawDiagnosticKind::FreeListBroken {
                obj: 0,
                problem: FreeListProblem::NextInUse
            },
            head,
            head + 18
        )]
    );
    // 0 -> 2 -> 2 (loop), with 1 in use.
    let (doc, xref) = table(&format!("0000000002 65535 f \n{one}0000000002 00001 f \n"), 3);
    let looped = xref + "xref\n0 3\n".len() + 40;
    assert_eq!(
        doc.diagnostics()
            .iter()
            .filter(|d| d.code() == "free-list-broken")
            .cloned()
            .collect::<Vec<_>>(),
        vec![diag(
            RawDiagnosticKind::FreeListBroken {
                obj: 2,
                problem: FreeListProblem::Loop
            },
            looped,
            looped + 18
        )]
    );
}

#[test]
fn incremental_updates_form_revisions_oldest_first() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.object(2, 0, "(old)");
    let first = b.table(3, &[]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(first as u64);
    let new = b.object(2, 0, "(new)");
    let second = b.buf.len();
    b.raw(format!("xref\n0 1\n0000000000 65535 f \n2 1\n{new:010} 00000 n \n").as_bytes());
    b.trailer(&format!("/Size 3 /Root 1 0 R /Prev {first}"));
    b.startxref(second as u64);
    let doc = parse(&b.buf);

    assert_eq!(doc.diagnostics(), vec![]);
    assert_eq!(
        doc.revisions()
            .iter()
            .map(|r| (r.index, r.xref.span.start))
            .collect::<Vec<_>>(),
        [(0, first), (1, second)]
    );
    assert_eq!(
        doc.object(ObjRef::new(2, 0)).unwrap().value.value.to_string(),
        "(new)"
    );
    assert_eq!(
        doc.object_in_revision(ObjRef::new(2, 0), 0)
            .unwrap()
            .value
            .value
            .to_string(),
        "(old)"
    );
    assert_eq!(doc.object(ObjRef::new(2, 0)).unwrap().revision, 1);
    assert_eq!(doc.object(ObjRef::new(1, 0)).unwrap().revision, 0);
    assert_eq!(doc.object_in_revision(ObjRef::new(2, 0), 5), None);
    assert_eq!(doc.trailer().and_then(|t| t.get_int(b"Prev")), Some(first as i64));
}

#[test]
fn a_prev_chain_that_loops_stops_with_a_diagnostic() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let at = b.buf.len();
    b.raw(b"xref\n0 2\n0000000000 65535 f \n");
    b.raw(format!("{:010} 00000 n \n", b.offsets[&1]).as_bytes());
    b.trailer(&format!("/Size 2 /Root 1 0 R /Prev {at}"));
    b.startxref(at as u64);
    let doc = parse(&b.buf);

    let prev_at = b.buf.windows(6).position(|w| w == b"/Prev ").unwrap() + "/Prev ".len();
    assert_eq!(doc.revisions().len(), 1);
    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::PrevCycle { offset: at as u64 },
            prev_at,
            prev_at + at.to_string().len()
        )]
    );
}

#[test]
fn a_prev_pointing_at_nothing_is_reported() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let at = b.table(2, &[]);
    b.trailer("/Size 2 /Root 1 0 R /Prev 5");
    b.startxref(at as u64);
    let doc = parse(&b.buf);

    let prev_at = b.buf.windows(7).position(|w| w == b"/Prev 5").unwrap() + "/Prev ".len();
    assert_eq!(
        doc.diagnostics(),
        vec![diag(
            RawDiagnosticKind::XrefSectionInvalid { offset: 5 },
            prev_at,
            prev_at + 1
        )]
    );
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

#[test]
fn decode_stream_applies_the_filter_chain() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.stream(2, "/Filter /FlateDecode", &zlib(b"q 1 0 0 1 0 0 cm Q"));
    let xref = b.table(3, &[]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let obj = doc.object(ObjRef::new(2, 0)).unwrap();
    let decoded = doc.decode_stream(obj.value.value.as_stream().unwrap()).unwrap();
    assert_eq!(decoded.bytes, b"q 1 0 0 1 0 0 cm Q");
    assert_eq!(decoded.filters, vec![Name::new("FlateDecode")]);
    assert!(decoded.complete);
    assert_eq!(doc.diagnostics(), vec![]);
}

#[test]
fn indirect_filter_names_are_resolved_when_decoding() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.object(3, 0, "/ASCIIHexDecode");
    b.stream(2, "/Filter 3 0 R", b"48 69>");
    let xref = b.table(4, &[]);
    b.trailer("/Size 4 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let obj = doc.object(ObjRef::new(2, 0)).unwrap();
    assert_eq!(
        doc.decode_stream(obj.value.value.as_stream().unwrap())
            .unwrap()
            .bytes,
        b"Hi"
    );
}

#[test]
fn resolve_follows_chains_and_survives_cycles() {
    let mut b = Builder::new();
    b.object(1, 0, "2 0 R");
    b.object(2, 0, "3 0 R");
    b.object(3, 0, "42");
    b.object(4, 0, "5 0 R");
    b.object(5, 0, "4 0 R");
    let xref = b.table(6, &[]);
    b.trailer("/Size 6 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    assert_eq!(
        doc.resolve(&Object::Ref(ObjRef::new(1, 0))).into_owned(),
        Object::Int(42)
    );
    assert_eq!(
        doc.resolve(&Object::Ref(ObjRef::new(4, 0))).into_owned(),
        Object::Null
    );
    assert_eq!(
        doc.resolve(&Object::Ref(ObjRef::new(77, 0))).into_owned(),
        Object::Null
    );
    assert_eq!(doc.resolve(&Object::Int(5)).into_owned(), Object::Int(5));
}

#[test]
fn encrypted_documents_expose_the_dictionary_and_do_not_decode() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.object(2, 0, "<< /Filter /Standard /V 1 /R 2 /P -4 >>");
    b.stream(3, "/Filter /FlateDecode", &zlib(b"secret"));
    let xref = b.table(4, &[]);
    b.trailer("/Size 4 /Root 1 0 R /Encrypt 2 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    assert_eq!(
        doc.encryption().unwrap().to_string(),
        "<< /Filter /Standard /V 1 /R 2 /P -4 >>"
    );
    let obj = doc.object(ObjRef::new(3, 0)).unwrap();
    let decoded = doc.decode_stream(obj.value.value.as_stream().unwrap()).unwrap();
    assert_eq!(decoded.issues, vec![DecodeIssue::Encrypted]);
    assert!(!decoded.complete);
    assert_eq!(decoded.bytes, zlib(b"secret"));
    assert_eq!(doc.diagnostics(), vec![]);
}

#[test]
fn a_direct_encrypt_dictionary_is_returned_too() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    let xref = b.table(2, &[]);
    b.trailer("/Size 2 /Root 1 0 R /Encrypt << /Filter /Standard /V 4 >>");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    assert_eq!(
        doc.encryption().unwrap().to_string(),
        "<< /Filter /Standard /V 4 >>"
    );
}

/// Table + xref stream + object stream, all unfiltered.
fn hybrid() -> (Vec<u8>, usize, usize) {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog /Pages 2 0 R >>");
    // Object stream 5 holds objects 2 and 3.
    let members = "<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n<< /Type /Page /Parent 2 0 R >>";
    let header = "2 0 3 42 ";
    let body = format!("{header}{members}");
    b.stream(
        5,
        &format!("/Type /ObjStm /N 2 /First {}", header.len()),
        body.as_bytes(),
    );
    // Xref stream 6 lists 2 and 3 as compressed.
    let xs = b.buf.len();
    let rows: Vec<u8> = vec![2, 0, 5, 0, 2, 0, 5, 1];
    b.stream(6, "/Type /XRef /W [1 2 1] /Index [2 2] /Size 7", &rows);
    b.offsets.insert(6, xs);
    // Table: 2 and 3 are free in the table; the stream supplies them.
    let at = b.buf.len();
    let (o1, o5, o6) = (b.offsets[&1], b.offsets[&5], b.offsets[&6]);
    b.raw(format!(
        "xref\n0 7\n0000000000 65535 f \n{o1:010} 00000 n \n0000000000 00000 f \n0000000000 00000 f \n0000000000 00000 f \n{o5:010} 00000 n \n{o6:010} 00000 n \n"
    ).as_bytes());
    b.trailer(&format!("/Size 7 /Root 1 0 R /XRefStm {xs}"));
    b.startxref(at as u64);
    (b.buf, at, xs)
}

#[test]
fn hybrid_files_merge_the_xref_stream_entries() {
    let (bytes, at, _) = hybrid();
    let doc = parse(&bytes);

    assert_eq!(doc.diagnostics(), vec![]);
    assert_eq!(doc.revisions().len(), 1);
    assert_eq!(doc.revisions()[0].xref.span.start, at);
    assert_eq!(
        doc.object_refs(),
        (1..=3)
            .chain(5..=6)
            .map(|n| ObjRef::new(n, 0))
            .collect::<Vec<_>>()
    );
    let pages = doc.object(ObjRef::new(2, 0)).unwrap();
    assert_eq!(
        pages.location,
        ObjLocation::InObjectStream {
            stream: ObjRef::new(5, 0),
            index: 0
        }
    );
    assert_eq!(
        pages.value.value.to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>"
    );
    // Spans of compressed objects index into the decoded object stream.
    assert_eq!(
        pages.span.slice(doc.object_stream_data(&pages).unwrap()).unwrap(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"
    );
    let page = doc.object(ObjRef::new(3, 0)).unwrap();
    assert_eq!(
        page.location,
        ObjLocation::InObjectStream {
            stream: ObjRef::new(5, 0),
            index: 1
        }
    );
    assert_eq!(page.value.value.to_string(), "<< /Type /Page /Parent 2 0 R >>");
}

#[test]
fn object_stream_index_mismatch_is_reported() {
    let (mut bytes, ..) = hybrid();
    // Swap the indexes recorded for objects 2 and 3.
    let pos = find(&bytes, &[2, 0, 5, 0, 2, 0, 5, 1]);
    bytes[pos + 3] = 1;
    bytes[pos + 7] = 0;
    let doc = parse(&bytes);

    let mismatches: Vec<_> = doc
        .diagnostics()
        .iter()
        .filter(|d| d.code() == "objstm-entry-mismatch")
        .map(|d| d.kind.clone())
        .collect();
    assert_eq!(
        mismatches,
        vec![
            RawDiagnosticKind::ObjStmEntryMismatch {
                stream: ObjRef::new(5, 0),
                index: 1,
                expected: 2,
                found: Some(3)
            },
            RawDiagnosticKind::ObjStmEntryMismatch {
                stream: ObjRef::new(5, 0),
                index: 0,
                expected: 3,
                found: Some(2)
            },
        ]
    );
    // The members are still found by object number.
    assert_eq!(
        doc.object(ObjRef::new(3, 0)).unwrap().value.value.to_string(),
        "<< /Type /Page /Parent 2 0 R >>"
    );
}

#[test]
fn unterminated_containers_in_objects_are_reported_with_spans() {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog /K [1 2");
    let xref = b.table(2, &[]);
    b.trailer("/Size 2 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert!(kinds.contains(&RawDiagnosticKind::UnterminatedContainer {
        container: ContainerKind::Array
    }));
    assert!(kinds.contains(&RawDiagnosticKind::UnterminatedContainer {
        container: ContainerKind::Dict
    }));
    // Smoke: whatever was readable is still available.
    assert_eq!(
        doc.object(ObjRef::new(1, 0))
            .unwrap()
            .value
            .value
            .as_dict()
            .unwrap()
            .get_name(b"Type"),
        Some(&Name::new("Catalog"))
    );
}

#[test]
fn every_diagnostic_has_a_code_message_and_severity() {
    let (b, _) = base();
    let mut bytes = b"junk\n".to_vec();
    bytes.extend_from_slice(&b.buf);
    let doc = parse(&bytes);

    for d in doc.diagnostics() {
        assert!(!d.code().is_empty() && d.code().chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        assert_ne!(d.kind.to_string(), "");
        let _ = d.severity();
    }
    assert_eq!(
        doc.diagnostics()[0].to_string(),
        "header-not-at-start at 5..13: the %PDF- header starts at byte 5 instead of byte 0"
    );
}
