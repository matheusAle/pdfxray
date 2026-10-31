//! Diagnostic explanations.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_wrong_stream_length_is_explained_with_both_figures() {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>",
    );
    let mut content = String::new();
    while content.len() < 1204 {
        content.push_str("0 0 1 1 re f ");
    }
    content.truncate(1204);
    w.stream_with_length(4, "", content.as_bytes(), 1200);
    w.finish_table(&Trailer::new(1)).unwrap();
    let doc = Document::from_bytes("len.pdf", w.into_output().unwrap().bytes).unwrap();

    let info = doc.info();
    let diag = info
        .diagnostics
        .iter()
        .find(|d| d.code == "stream-length-wrong")
        .unwrap();

    assert_eq!(
        diag.explain().as_deref(),
        Some(
            "The stream says it is 1,200 bytes long but really is 1,204. Acrobat silently fixes this; other readers may drop the last command. pdfxray shows it as-is."
        )
    );
    assert_eq!(diag.object, Some(r(4)));
    let range = diag.byte_range().unwrap();
    assert_eq!([range.start, range.end], diag.span);
    assert!(
        range.end <= doc.bytes().len() && range.len() == 1204,
        "the stream data: {range:?}"
    );
    assert_eq!(
        explain::diagnostic("stream-length-wrong"),
        Some(
            "The stream's declared length does not match its real length. Acrobat silently fixes this; other readers may drop the last command. pdfxray shows it as-is."
        )
    );
}

#[test]
fn damaged_index_diagnostics_quote_the_offsets_and_point_at_bytes() {
    let doc = open("broken-xref-offsets");
    let info = doc.info();

    let explained: Vec<(&str, String)> = info
        .diagnostics
        .iter()
        .filter(|d| d.code == "xref-offset-wrong")
        .filter_map(|d| Some((d.code, d.explain()?)))
        .collect();

    assert!(
        !explained.is_empty(),
        "{:?}",
        info.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    for (_, text) in &explained {
        assert!(text.starts_with("The index says an object is at byte "), "{text}");
        assert!(
            text.contains(" but it really starts at byte ") || text.contains("not in the file at all"),
            "{text}"
        );
    }
    let first = info
        .diagnostics
        .iter()
        .find(|d| d.code == "xref-offset-wrong")
        .unwrap();
    let message = &first.message;
    let digits: Vec<&str> = message
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        first
            .explain()
            .unwrap()
            .contains(&format!(" {} ", digits[digits.len() - 2])),
        "the explanation quotes the recorded offset of {message}"
    );
    let range = first.byte_range();
    assert!(range.is_some_and(|r| r.end <= doc.bytes().len()));
    assert_eq!(
        doc.info()
            .diagnostics
            .iter()
            .filter(|d| d.byte_range().is_none())
            .count(),
        0
    );
}

#[test]
fn a_diagnostic_without_a_position_has_no_byte_range() {
    let doc = Document::from_bytes(
        "noroot.pdf",
        b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec(),
    )
    .unwrap();

    let info = doc.info();
    let missing = info.diagnostics.iter().find(|d| d.code == "page-tree-missing");

    let missing = missing.unwrap_or_else(|| {
        panic!(
            "{:?}",
            info.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        )
    });
    assert_eq!(missing.byte_range(), None);
    assert_eq!(
        missing.explain().as_deref(),
        Some("The document has no page tree, so there are no pages to show.")
    );
}

#[test]
fn every_diagnostic_the_test_corpus_produces_has_an_explanation() {
    let mut files = Vec::new();
    pdfs_below(&fixtures_dir(), &mut files);
    pdfs_below(&fixtures_dir().parent().unwrap().join("third_party"), &mut files);
    files.sort();
    let mut codes = BTreeSet::new();
    let mut count = 0;
    let mut missing = Vec::new();

    for file in &files {
        let Ok(doc) = Document::open(file) else {
            continue;
        };
        count += 1;
        for d in &doc.info().diagnostics {
            codes.insert(d.code);
            if d.explain().is_none() {
                missing.push(format!("{} in {}", d.code, file.display()));
            }
            if let Some(range) = d.byte_range() {
                assert!(
                    range.start <= range.end && range.end <= doc.bytes().len() + 1,
                    "{} {range:?}",
                    d.code
                );
            }
        }
    }

    assert_eq!(missing, Vec::<String>::new());
    assert!(count >= 60, "{count} files opened");
    assert!(codes.len() >= 10, "the corpus exercises many codes: {codes:?}");
    assert!(
        codes.contains("stream-length-wrong") || codes.contains("xref-offset-wrong"),
        "{codes:?}"
    );
}

#[test]
fn every_raw_and_core_code_has_a_plain_text_that_ends_like_a_sentence() {
    let mut all: BTreeSet<&str> = explain::diagnostic_codes().collect();

    for code in pdfxray_raw::RawDiagnosticKind::CODES {
        assert!(all.remove(code), "{code} is explained");
        let text = explain::diagnostic(code).unwrap();
        assert!(text.ends_with('.') && text.len() > 30, "{code}: {text}");
    }
    assert!(all.contains("box-outside-mediabox") && all.contains("untagged-content"));
    assert_eq!(
        all.len(),
        5 + 27 + 1 + 10,
        "page tree, structure and resource codes remain"
    );
    let mut figures = String::new();
    write!(figures, "{}", explain::diagnostic("trailing-data").unwrap()).unwrap();
    assert!(figures.starts_with("Bytes follow"));
}
