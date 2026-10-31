//! Parsing must never panic, loop or allocate without bound, and every span it reports must lie
//! inside the input, whatever the bytes are.
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

use std::sync::{Arc, OnceLock};

use pdfxray_raw::{ObjLocation, ObjRef, Object, RawDocument};
use pretty_assertions::assert_eq;
use proptest::prelude::*;
use support::{HEADER, parse};

fn fixture_bytes() -> &'static [Vec<u8>] {
    static BYTES: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let dir = pdfxray_fixtures::fixtures_dir();
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
            .collect();
        files.sort();
        files.iter().map(|p| std::fs::read(p).unwrap()).collect()
    })
}

/// Walks everything the document exposes and checks that offsets and spans make sense.
fn assert_consistent(doc: &RawDocument) {
    let len = doc.bytes().len();
    let inside = |s: pdfxray_raw::Span| s.start <= s.end && s.end <= len;
    for d in doc.diagnostics() {
        assert!(inside(d.span), "diagnostic span {d}");
    }
    for rev in doc.revisions() {
        assert!(inside(rev.xref.span));
        assert!(rev.startxref.is_none_or(|s| inside(s.span)));
        assert!(rev.eof.is_none_or(inside));
        for e in &rev.xref.entries {
            assert!(e.span.is_none_or(inside));
        }
    }
    for r in doc.object_refs() {
        let Some(obj) = doc.object(r) else {
            continue;
        };
        match obj.location {
            ObjLocation::Direct { offset } => {
                assert!(inside(obj.span) && usize::try_from(offset).is_ok_and(|o| o == obj.span.start));
                assert!(
                    obj.value.span.start >= obj.span.start && obj.value.span.end <= obj.span.end.max(len)
                );
                if let Object::Stream(s) = &obj.value.value {
                    assert!(inside(s.data));
                    let _ = doc.decode_stream(s);
                }
            }
            ObjLocation::InObjectStream { .. } => {
                let data = doc.object_stream_data(&obj).unwrap_or_default();
                assert!(
                    obj.span.end <= data.len(),
                    "member span {:?} in {} bytes",
                    obj.span,
                    data.len()
                );
            }
        }
        let _ = doc.resolve(&Object::Ref(r));
    }
    let _ = doc.encryption();
}

#[derive(Debug, Clone)]
struct Mutation {
    flips: Vec<(usize, u8)>,
    cut: Option<usize>,
    delete: Option<(usize, usize)>,
    insert: Option<(usize, Vec<u8>)>,
}

fn mutation() -> impl Strategy<Value = Mutation> {
    (
        proptest::collection::vec((any::<usize>(), any::<u8>()), 0..12),
        proptest::option::of(any::<usize>()),
        proptest::option::of((any::<usize>(), 0usize..64)),
        proptest::option::of((any::<usize>(), proptest::collection::vec(any::<u8>(), 0..24))),
    )
        .prop_map(|(flips, cut, delete, insert)| Mutation {
            flips,
            cut,
            delete,
            insert,
        })
}

fn apply(mut bytes: Vec<u8>, m: &Mutation) -> Vec<u8> {
    for (at, value) in &m.flips {
        let i = at % bytes.len().max(1);
        if let Some(b) = bytes.get_mut(i) {
            *b = *value;
        }
    }
    if let Some((at, len)) = m.delete {
        let start = at % bytes.len().max(1);
        let end = (start + len).min(bytes.len());
        bytes.drain(start..end);
    }
    if let Some((at, data)) = &m.insert {
        let start = at % (bytes.len() + 1);
        bytes.splice(start..start, data.iter().copied());
    }
    if let Some(cut) = m.cut {
        let keep = cut % (bytes.len() + 1);
        bytes.truncate(keep);
    }
    bytes
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        if let Ok(doc) = RawDocument::parse(Arc::new(bytes)) {
            assert_consistent(&doc);
        }
    }

    #[test]
    fn arbitrary_bytes_after_a_header_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let mut file = HEADER.to_vec();
        file.extend_from_slice(&bytes);
        let doc = parse(&file);
        assert_consistent(&doc);
        prop_assert!(doc.header().span.end <= file.len());
    }

    #[test]
    fn pdf_flavoured_garbage_never_panics(
        pieces in proptest::collection::vec(
            prop_oneof![
                Just(b"1 0 obj\n".to_vec()), Just(b"endobj\n".to_vec()), Just(b"<<".to_vec()), Just(b">>".to_vec()),
                Just(b"stream\n".to_vec()), Just(b"endstream".to_vec()), Just(b"xref\n0 3\n".to_vec()),
                Just(b"trailer\n".to_vec()), Just(b"startxref\n9\n".to_vec()), Just(b"%%EOF\n".to_vec()),
                Just(b"/Prev 9 ".to_vec()), Just(b"/Length 5 ".to_vec()), Just(b"/Type /XRef /W [1 2 1] ".to_vec()),
                Just(b"/Type /ObjStm /N 2 /First 9 ".to_vec()), Just(b"[ ".to_vec()), Just(b"( ".to_vec()),
                Just(b"0000000000 65535 f \n".to_vec()), Just(b"0000000015 00000 n \n".to_vec()),
                proptest::collection::vec(any::<u8>(), 0..8),
            ],
            0..64,
        )
    ) {
        let mut file = HEADER.to_vec();
        for p in pieces {
            file.extend_from_slice(&p);
        }
        assert_consistent(&parse(&file));
    }

    #[test]
    fn mutated_fixtures_never_panic_and_keep_spans_inside(index in any::<usize>(), m in mutation()) {
        let fixtures = fixture_bytes();
        let original = fixtures[index % fixtures.len()].clone();
        let mutated = apply(original, &m);
        if let Ok(doc) = RawDocument::parse(Arc::new(mutated)) {
            assert_consistent(&doc);
        }
    }
}

#[test]
fn unmutated_fixtures_are_consistent() {
    let fixtures = fixture_bytes();
    for bytes in fixtures {
        assert_consistent(&parse(bytes));
    }
    // Smoke: the catalog is not empty.
    assert!(fixtures.len() >= 11);
}

#[test]
fn deeply_nested_containers_do_not_overflow_the_stack() {
    let mut file = HEADER.to_vec();
    file.extend_from_slice(b"1 0 obj\n");
    file.extend(std::iter::repeat_n(b'[', 200_000));
    file.extend_from_slice(b"\nendobj\n");
    let doc = parse(&file);

    assert!(doc.diagnostics().iter().any(|d| d.code() == "nesting-too-deep"));
    assert_consistent(&doc);

    let mut dicts = HEADER.to_vec();
    dicts.extend_from_slice(b"1 0 obj\n");
    for _ in 0..50_000 {
        dicts.extend_from_slice(b"<</A ");
    }
    assert_consistent(&parse(&dicts));
}

#[test]
fn absurd_counts_and_lengths_are_bounded() {
    let mut file = HEADER.to_vec();
    file.extend_from_slice(b"1 0 obj\n<< /Length 99999999999999 >>\nstream\nabc\nendstream\nendobj\n");
    file.extend_from_slice(
        b"xref\n0 4294967295\n0000000000 65535 f \ntrailer\n<< /Size 1 /Root 1 0 R >>\nstartxref\n",
    );
    let at = file.windows(4).position(|w| w == b"xref").unwrap();
    file.extend_from_slice(format!("{at}\n%%EOF\n").as_bytes());
    let doc = parse(&file);

    assert_consistent(&doc);
    assert_eq!(doc.object(ObjRef::new(1, 0)), None);
    assert!(
        doc.diagnostics()
            .iter()
            .any(|d| d.code() == "xref-subsection-short")
    );
}

#[test]
fn a_huge_predictor_row_is_refused_not_allocated() {
    use std::io::Write;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(b"\x02abc").unwrap();
    let packed = z.finish().unwrap();
    let mut b = support::Builder::new();
    b.object(1, 0, "<< /Type /Catalog >>");
    b.stream(
        2,
        "/Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns 16777216 /Colors 64 /BitsPerComponent 16 >>",
        &packed,
    );
    let xref = b.table(3, &[]);
    b.trailer("/Size 3 /Root 1 0 R");
    b.startxref(xref as u64);
    let doc = parse(&b.buf);

    let obj = doc.object(ObjRef::new(2, 0)).unwrap();
    let decoded = doc.decode_stream(obj.value.value.as_stream().unwrap()).unwrap();
    assert_eq!(decoded.bytes, b"\x02abc");
    assert!(!decoded.complete);
    assert_eq!(
        decoded.issues,
        vec![pdfxray_raw::DecodeIssue::BadPredictorParams(
            pdfxray_raw::Name::new("FlateDecode")
        )]
    );
}
