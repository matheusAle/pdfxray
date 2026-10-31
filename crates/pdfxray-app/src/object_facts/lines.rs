//! The lines of the decoded view: dictionary entries, content stream operators, and the text body
//! of a decoded stream that reads as text.

use pdfxray_core::capture::{CapturedPage, PaintKind, StreamRef};
use pdfxray_core::{Document, ObjRef, PageIx, explain};

use super::{DictLine, LineKind, OpLine, Piece, TextBody, split_refs};

/// Longest decoded text shown line by line; the rest is summarised.
const MAX_TEXT_LINES: usize = 2_000;

/// Bytes sampled to decide whether a decoded stream is text.
const TEXT_SAMPLE: usize = 1_024;

/// The lines of a dictionary, array or single value.
pub(super) fn dict_lines(rows: &[(String, String)], type_word: &str) -> Vec<DictLine> {
    let bracket = |text: &str| DictLine {
        kind: LineKind::Bracket,
        key: None,
        pieces: vec![Piece::Text(text.to_owned())],
    };
    let entry = |key: Option<&str>, value: &str| DictLine {
        kind: LineKind::Entry,
        key: key.map(str::to_owned),
        pieces: split_refs(value),
    };
    let mut lines = Vec::new();
    match type_word {
        "dictionary" | "stream" => {
            lines.push(bracket("<<"));
            lines.extend(rows.iter().map(|(k, v)| entry(Some(k), v)));
            lines.push(bracket(">>"));
            if type_word == "stream" {
                lines.push(DictLine {
                    kind: LineKind::Keyword,
                    key: None,
                    pieces: vec![Piece::Text("stream".to_owned())],
                });
            }
        }
        "array" => {
            lines.push(bracket("["));
            lines.extend(rows.iter().map(|(_, v)| entry(None, v)));
            lines.push(bracket("]"));
        }
        _ => lines.extend(rows.iter().map(|(_, v)| entry(None, v))),
    }
    lines
}

/// Which operators of a captured page an object runs.
#[derive(Debug, Clone, Copy)]
enum OpSource {
    /// The `index`-th stream of a page's `/Contents`.
    Contents(u32),
    /// The operators written in a Form `XObject`.
    Form(ObjRef),
}

impl OpSource {
    fn selects(self, stream: &StreamRef) -> bool {
        match (self, stream) {
            (Self::Contents(index), StreamRef::PageContent { index: i }) => index == *i,
            (Self::Form(r), StreamRef::FormXObject { obj: Some(o), .. }) => r == *o,
            _ => false,
        }
    }
}

/// The drawing instructions an object runs: those of the page content stream it is, or of the
/// Form `XObject` it is.
pub(super) fn op_lines(
    doc: &Document,
    r: ObjRef,
    contents: Option<&(PageIx, u32, String)>,
    form: bool,
) -> Option<Vec<OpLine>> {
    let (captured, source) = match contents {
        Some((page, index, _)) => (doc.captured(*page)?, OpSource::Contents(*index)),
        None if form => {
            let captured = (0..doc.page_count()).find_map(|ix| {
                let captured = doc.captured(PageIx::try_from(ix).ok()?)?;
                captured
                    .ops
                    .iter()
                    .any(|op| OpSource::Form(r).selects(&op.stream))
                    .then_some(captured)
            })?;
            (captured, OpSource::Form(r))
        }
        None => return None,
    };
    let mut seen = std::collections::HashSet::new();
    let mut depth: u8 = 0;
    let mut lines = Vec::new();
    for op in captured.ops.iter().filter(|op| source.selects(&op.stream)) {
        // A form drawn twice runs its operators twice; they are one instruction in the stream.
        if !seen.insert(op.local_ix) {
            continue;
        }
        if matches!(op.operator.as_str(), "Q" | "ET" | "EMC") {
            depth = depth.saturating_sub(1);
        }
        let text = shown_text(captured, op);
        lines.push(OpLine {
            ix: op.ix,
            indent: depth,
            note: explain::operator_with_text(&op.operator, &op.operands, text.as_deref()),
            operator: op.operator.clone(),
            operands: op.operands.join(" "),
        });
        if matches!(op.operator.as_str(), "q" | "BT" | "BDC" | "BMC") {
            depth = depth.saturating_add(1);
        }
    }
    Some(lines)
}

/// The text a text-showing operator draws, when the font says what its glyphs are.
fn shown_text(captured: &CapturedPage, op: &pdfxray_core::capture::OpRecord) -> Option<String> {
    if !matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\"") {
        return None;
    }
    let first = usize::try_from(op.first_item?).ok()?;
    let count = usize::try_from(op.item_count).ok()?;
    let text: String = captured
        .items
        .get(first..first + count)?
        .iter()
        .filter_map(|item| match &item.kind {
            PaintKind::Glyph { unicode, .. } => unicode.as_deref(),
            _ => None,
        })
        .collect();
    (!text.is_empty()).then_some(text)
}

/// The lines of `bytes` when they read as text.
pub(super) fn text_body(bytes: &[u8]) -> Option<TextBody> {
    let sample = &bytes[..bytes.len().min(TEXT_SAMPLE)];
    let printable = sample
        .iter()
        .filter(|b| b.is_ascii_graphic() || b.is_ascii_whitespace())
        .count();
    if sample.is_empty() || printable * 10 < sample.len() * 9 {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let mut total = 0;
    let mut lines = Vec::new();
    for line in text.lines() {
        total += 1;
        if lines.len() < MAX_TEXT_LINES {
            lines.push(line.to_owned());
        }
    }
    Some(TextBody { lines, total })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn r(num: u32) -> ObjRef {
        ObjRef::new(num, 0)
    }

    #[test]
    fn dictionary_lines_wrap_the_entries_in_brackets() {
        let rows = vec![
            ("/Length".to_owned(), "1200".to_owned()),
            ("/Parent".to_owned(), "2 0 R".to_owned()),
        ];

        let lines = dict_lines(&rows, "stream");

        let shown: Vec<String> = lines
            .iter()
            .map(|l| {
                let key = l.key.clone().unwrap_or_default();
                let value: String = l
                    .pieces
                    .iter()
                    .map(|p| match p {
                        Piece::Text(t) => t.clone(),
                        Piece::Ref(r) => r.to_string(),
                    })
                    .collect();
                format!("{key} {value}").trim().to_owned()
            })
            .collect();
        assert_eq!(shown, ["<<", "/Length 1200", "/Parent 2 0 R", ">>", "stream"]);
        assert_eq!(lines[2].pieces, [Piece::Ref(r(2))]);
        assert_eq!(lines.last().map(|l| l.kind), Some(LineKind::Keyword));
    }

    #[test]
    fn only_text_streams_have_a_text_body() {
        let body = text_body(b"BT\n/F1 18 Tf\nET").unwrap();
        assert_eq!(body.lines, ["BT", "/F1 18 Tf", "ET"]);
        assert_eq!(body.total, 3);
        assert_eq!(text_body(&[0, 1, 2, 3, 255, 254, 253, 0]), None);
        assert_eq!(text_body(b""), None);
    }
}
