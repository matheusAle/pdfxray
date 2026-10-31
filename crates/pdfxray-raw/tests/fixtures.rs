//! The generated fixtures: their documented facts (`*.expected.json`) must be exactly what the raw
//! layer reads out of the bytes.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use std::collections::BTreeSet;
use std::sync::Arc;

use pdfxray_fixtures::{Defect, Expected, XrefKind as ExpectedKind, fixtures_dir};
use pdfxray_raw::{
    EntryState, Name, ObjLocation, ObjRef, Object, RawDiagnostic, RawDiagnosticKind, RawDocument, Span,
    XrefKind,
};
use pretty_assertions::assert_eq;

/// The original catalog; other agents may add fixtures, which are not covered here.
const ORIGINAL: [&str; 11] = [
    "minimal-one-page",
    "tagged-simple",
    "artifacts",
    "form-xobject-nesting",
    "page-boxes",
    "rolemap-custom",
    "orphan-mcid",
    "broken-xref-offsets",
    "incremental-update",
    "xref-stream-objstm",
    "encrypted-stub",
];

fn load(name: &str) -> (RawDocument, Expected) {
    let dir = fixtures_dir();
    let bytes = std::fs::read(dir.join(format!("{name}.pdf"))).unwrap();
    let json = std::fs::read_to_string(dir.join(format!("{name}.expected.json"))).unwrap();
    let doc = RawDocument::parse(Arc::new(bytes)).unwrap();
    (doc, serde_json::from_str(&json).unwrap())
}

fn in_use_numbers(doc: &RawDocument, revision: usize) -> Vec<u32> {
    let nums: BTreeSet<u32> = doc.revisions()[revision]
        .xref
        .entries
        .iter()
        .filter(|e| !matches!(e.state, EntryState::Free { .. }))
        .map(|e| e.num)
        .collect();
    nums.into_iter().collect()
}

/// Diagnostics a fixture's documented defects imply at the raw level.
fn expected_raw_diagnostics(expected: &Expected) -> Vec<RawDiagnosticKind> {
    expected
        .defects
        .iter()
        .filter_map(|d| match d {
            Defect::XrefOffsetWrong {
                obj,
                recorded,
                actual,
            } => Some(RawDiagnosticKind::XrefOffsetWrong {
                obj: ObjRef::new(*obj, 0),
                recorded: *recorded,
                actual: Some(*actual),
            }),
            _ => None,
        })
        .collect()
}

#[test]
fn every_original_fixture_matches_its_documented_structure() {
    for name in ORIGINAL {
        let (doc, expected) = load(name);

        assert_eq!(
            doc.header().version.to_string(),
            expected.header_version,
            "{name}: version"
        );
        assert_eq!(
            doc.revisions().len(),
            expected.revision_count as usize,
            "{name}: revisions"
        );
        assert_eq!(doc.revisions().len(), expected.revisions.len(), "{name}: facts");
        for (i, (rev, facts)) in doc.revisions().iter().zip(&expected.revisions).enumerate() {
            assert_eq!(rev.index, i, "{name} rev {i}: index");
            assert_eq!(
                rev.xref.span.start as u64, facts.xref_offset,
                "{name} rev {i}: xref offset"
            );
            assert_eq!(
                rev.startxref.map(|s| s.offset),
                Some(facts.xref_offset),
                "{name} rev {i}: startxref"
            );
            assert!(rev.eof.is_some(), "{name} rev {i}: %%EOF");
            let kind_matches = matches!(
                (rev.xref.kind, facts.xref_kind),
                (XrefKind::Table, ExpectedKind::Table) | (XrefKind::Stream(_), ExpectedKind::Stream)
            );
            assert!(
                kind_matches,
                "{name} rev {i}: {:?} vs {:?}",
                rev.xref.kind, facts.xref_kind
            );
            let prev = rev
                .trailer
                .as_ref()
                .and_then(|t| t.get_int(b"Prev"))
                .map(|p| p as u64);
            assert_eq!(prev, facts.prev, "{name} rev {i}: /Prev");
            assert_eq!(in_use_numbers(&doc, i), facts.objects, "{name} rev {i}: objects");

            let compressed: Vec<_> = rev
                .xref
                .entries
                .iter()
                .filter_map(|e| match e.state {
                    EntryState::Compressed { stream, index } => Some((e.num, stream, index)),
                    _ => None,
                })
                .collect();
            let documented: Vec<_> = facts
                .compressed
                .iter()
                .map(|c| (c.obj, c.stream, c.index))
                .collect();
            assert_eq!(compressed, documented, "{name} rev {i}: compressed");
            for &num in &facts.objects {
                let obj = doc.object_in_revision(ObjRef::new(num, 0), i);
                assert_eq!(
                    obj.map(|o| (o.r, o.revision)),
                    Some((ObjRef::new(num, 0), i)),
                    "{name}: object {num} in rev {i}"
                );
            }
        }
        // Smoke: the newest view lists every documented object of every revision.
        let all: BTreeSet<u32> = expected
            .revisions
            .iter()
            .flat_map(|r| r.objects.iter().copied())
            .collect();
        assert_eq!(
            doc.object_refs().iter().map(|r| r.num).collect::<BTreeSet<_>>(),
            all,
            "{name}: object_refs"
        );
        assert!(doc.trailer().is_some(), "{name}: trailer");
    }
}

#[test]
fn diagnostics_are_exactly_the_documented_defects() {
    for name in ORIGINAL {
        let (doc, expected) = load(name);
        let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();

        assert_eq!(kinds, expected_raw_diagnostics(&expected), "{name}: diagnostics");
    }
}

#[test]
fn clean_fixtures_have_an_empty_diagnostics_list() {
    let mut checked = Vec::new();
    for name in ORIGINAL.iter().filter(|n| **n != "broken-xref-offsets") {
        let (doc, _) = load(name);

        assert_eq!(doc.diagnostics(), vec![], "{name}");
        assert_eq!(doc.header().diagnostics, vec![], "{name}: header");
        checked.push(*name);
    }
    // Smoke: everything except the deliberately broken file was checked, in catalog order.
    assert_eq!(
        checked,
        [
            "minimal-one-page",
            "tagged-simple",
            "artifacts",
            "form-xobject-nesting",
            "page-boxes",
            "rolemap-custom",
            "orphan-mcid",
            "incremental-update",
            "xref-stream-objstm",
            "encrypted-stub",
        ]
    );
}

#[test]
fn broken_xref_offsets_reports_exactly_the_two_wrong_entries() {
    let (doc, expected) = load("broken-xref-offsets");

    let kinds: Vec<_> = doc.diagnostics().iter().map(|d| d.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            RawDiagnosticKind::XrefOffsetWrong {
                obj: ObjRef::new(3, 0),
                recorded: 128,
                actual: Some(121)
            },
            RawDiagnosticKind::XrefOffsetWrong {
                obj: ObjRef::new(4, 0),
                recorded: 244,
                actual: Some(247)
            },
        ]
    );
    assert_eq!(
        doc.diagnostics()
            .iter()
            .map(RawDiagnostic::code)
            .collect::<Vec<_>>(),
        ["xref-offset-wrong"; 2]
    );
    // The diagnostic points at the xref entries that lie.
    let spans: Vec<_> = doc
        .diagnostics()
        .iter()
        .map(|d| d.span.slice(doc.bytes()).unwrap().to_vec())
        .collect();
    assert_eq!(
        spans,
        vec![b"0000000128 00000 n".to_vec(), b"0000000244 00000 n".to_vec()]
    );
    assert_eq!(expected.defects.len(), 2);

    // Objects are returned from where they really are.
    let page = doc.object(ObjRef::new(3, 0)).unwrap();
    assert_eq!(page.location, ObjLocation::Direct { offset: 121 });
    let contents = doc.object(ObjRef::new(4, 0)).unwrap();
    assert_eq!(contents.location, ObjLocation::Direct { offset: 247 });
    let stream = contents.value.value.as_stream().unwrap();
    assert_eq!(
        doc.decode_stream(stream).unwrap().bytes,
        b"BT /F1 18 Tf 20 40 Td (Broken xref) Tj ET"
    );
    // The xref entries are reported as written.
    let recorded: Vec<_> = doc.revisions()[0].xref.entries.iter().map(|e| e.state).collect();
    assert_eq!(recorded[3], EntryState::InUse { offset: 128 });
    assert_eq!(recorded[4], EntryState::InUse { offset: 244 });
}

#[test]
fn incremental_update_has_two_revisions_linked_by_prev() {
    let (doc, _) = load("incremental-update");

    let revs = doc.revisions();
    assert_eq!(
        revs.iter().map(|r| r.xref.span.start).collect::<Vec<_>>(),
        [410, 826]
    );
    assert_eq!(
        revs.iter()
            .map(|r| r.startxref.unwrap().offset)
            .collect::<Vec<_>>(),
        [410, 826]
    );
    assert_eq!(revs[0].trailer.as_ref().unwrap().get_int(b"Prev"), None);
    assert_eq!(revs[1].trailer.as_ref().unwrap().get_int(b"Prev"), Some(410));
    assert_eq!(in_use_numbers(&doc, 0), [1, 2, 3, 4, 5]);
    assert_eq!(in_use_numbers(&doc, 1), [4, 6]);

    let text = |rev: usize| {
        let obj = doc.object_in_revision(ObjRef::new(4, 0), rev).unwrap();
        (
            obj.revision,
            doc.decode_stream(obj.value.value.as_stream().unwrap())
                .unwrap()
                .bytes,
        )
    };
    assert_eq!(text(0).0, 0);
    assert_eq!(
        text(1),
        (1, b"BT /F1 18 Tf 20 40 Td (Hello pdfxray rev 2) Tj ET".to_vec())
    );
    assert_ne!(text(0).1, text(1).1);
    assert_eq!(doc.object_in_revision(ObjRef::new(6, 0), 0), None);
    assert_eq!(
        doc.object(ObjRef::new(6, 0)).unwrap().value.value.to_string(),
        "<< /Title (pdfxray incremental fixture) >>"
    );
}

#[test]
fn xref_stream_fixture_exposes_the_object_stream_members() {
    let (doc, _) = load("xref-stream-objstm");

    assert_eq!(doc.revisions()[0].xref.kind, XrefKind::Stream(ObjRef::new(7, 0)));
    let members: Vec<_> = [1u32, 2, 3, 5]
        .iter()
        .map(|&n| {
            let obj = doc.object(ObjRef::new(n, 0)).unwrap();
            let text = obj
                .span
                .slice(doc.object_stream_data(&obj).unwrap())
                .unwrap()
                .to_vec();
            (n, obj.location, String::from_utf8(text).unwrap())
        })
        .collect();
    let stream = ObjRef::new(6, 0);
    assert_eq!(
        members,
        vec![
            (1, ObjLocation::InObjectStream { stream, index: 0 }, "<< /Type /Catalog /Pages 2 0 R >>".to_owned()),
            (2, ObjLocation::InObjectStream { stream, index: 1 }, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned()),
            (
                3,
                ObjLocation::InObjectStream { stream, index: 2 },
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 100] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
                    .to_owned()
            ),
            (5, ObjLocation::InObjectStream { stream, index: 3 }, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned()),
        ]
    );
    // Direct objects next to them: the content stream, the object stream and the xref stream.
    assert_eq!(
        doc.object(ObjRef::new(4, 0)).unwrap().location,
        ObjLocation::Direct { offset: 15 }
    );
    assert_eq!(
        doc.object(ObjRef::new(6, 0)).unwrap().location,
        ObjLocation::Direct { offset: 115 }
    );
    assert_eq!(
        doc.object(ObjRef::new(7, 0)).unwrap().location,
        ObjLocation::Direct { offset: 457 }
    );
    // The trailer of an xref stream is the stream dictionary.
    let trailer = doc.trailer().unwrap();
    assert_eq!(trailer.get_name(b"Type"), Some(&Name::new("XRef")));
    assert_eq!(
        trailer.get(b"Root").unwrap().value,
        Object::Ref(ObjRef::new(1, 0))
    );
    // Entries of an xref stream have no span in the file.
    assert!(doc.revisions()[0].xref.entries.iter().all(|e| e.span.is_none()));
    assert_eq!(doc.revisions()[0].xref.span, Span::new(457, 694));
}

#[test]
fn encrypted_stub_exposes_the_encryption_dictionary_without_decrypting() {
    let (doc, expected) = load("encrypted-stub");

    let enc = doc.encryption().unwrap();
    assert_eq!(enc.get_name(b"Filter"), Some(&Name::new("Standard")));
    assert_eq!(enc.get_int(b"V"), Some(1));
    assert_eq!(enc.get_int(b"R"), Some(2));
    assert_eq!(enc.get_int(b"P"), Some(-44));
    assert_eq!(
        enc.to_string(),
        format!(
            "<< /Filter /Standard /V 1 /R 2 /O <{}> /U <{}> /P -44 >>",
            "AB".repeat(32),
            "CD".repeat(32)
        )
    );
    assert_eq!(
        doc.trailer().unwrap().get(b"Encrypt").unwrap().value,
        Object::Ref(ObjRef::new(6, 0))
    );
    assert_eq!(expected.defects.len(), 1);
    // Structure is still readable.
    assert_eq!(
        doc.object(ObjRef::new(1, 0)).unwrap().value.value.to_string(),
        "<< /Type /Catalog /Pages 2 0 R >>"
    );
}

#[test]
fn unencrypted_fixtures_have_no_encryption_dictionary() {
    for name in ORIGINAL.iter().filter(|n| **n != "encrypted-stub") {
        let (doc, _) = load(name);

        assert_eq!(doc.encryption(), None, "{name}");
    }
}

#[test]
fn every_object_of_every_fixture_parses_inside_the_file() {
    for name in ORIGINAL {
        let (doc, _) = load(name);
        let refs = doc.object_refs();

        for r in &refs {
            let obj = doc.object(*r).unwrap_or_else(|| panic!("{name}: {r} missing"));
            assert_eq!(obj.r, *r, "{name}");
            if matches!(obj.location, ObjLocation::Direct { .. }) {
                assert!(
                    obj.span.end <= doc.bytes().len() && obj.value.span.end <= obj.span.end,
                    "{name}: {r}"
                );
                assert!(
                    obj.span.slice(doc.bytes()).unwrap().ends_with(b"endobj"),
                    "{name}: {r}"
                );
            }
        }
        assert!(!refs.is_empty(), "{name}");
    }
}
