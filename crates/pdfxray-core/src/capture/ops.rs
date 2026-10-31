//! Parsing of content streams into operators with byte spans, and the instrumented stream that
//! is handed to the engine.

use hayro_syntax::content::UntypedIter;
use hayro_syntax::object::{Name, Object};

/// Tag of the marked-content pair injected before every operator. Its `/MCID` is the index of
/// the operator that follows.
pub(super) const MARKER_TAG: &[u8] = b"PdfxrayOp";

/// One operator with its operands, as read from a decoded stream.
pub(super) struct RawOp<'d> {
    /// The operator token.
    pub operator: String,
    /// The operands, exactly as parsed.
    pub operands: Vec<Object<'d>>,
    /// End of the previous instruction; everything in `chunk_start..end` belongs to this
    /// instruction (including leading whitespace and comments).
    pub chunk_start: usize,
    /// First byte of the instruction proper (leading whitespace and comments skipped).
    pub start: usize,
    /// One past the last byte of the instruction.
    pub end: usize,
}

impl RawOp<'_> {
    /// The numeric operands, trimmed from the front to the operator's arity: the engine reads
    /// the last ones when more were supplied.
    pub(super) fn numbers(&self) -> Vec<f64> {
        let mut v: Vec<f64> = self
            .operands
            .iter()
            .filter_map(|o| match o {
                Object::Number(n) => Some(n.as_f64()),
                _ => None,
            })
            .collect();
        if let Some(n) = arity(&self.operator)
            && v.len() > n
        {
            v.drain(..v.len() - n);
        }
        v
    }
}

/// Operand `i` of `op` when it is a name.
pub(super) fn name_at<'x, 'd>(op: &'x RawOp<'d>, i: usize) -> Option<&'x Name<'d>> {
    match op.operands.get(i) {
        Some(Object::Name(n)) => Some(n),
        _ => None,
    }
}

/// How many numeric operands `operator` takes; the engine reads the last that many when more
/// were supplied.
fn arity(operator: &str) -> Option<usize> {
    match operator {
        "w" | "J" | "j" | "M" | "i" | "Tc" | "Tw" | "Tz" | "TL" | "Ts" | "Tr" | "g" | "G" => Some(1),
        "m" | "l" | "Td" | "TD" | "\"" => Some(2),
        "rg" | "RG" => Some(3),
        "re" | "v" | "y" | "k" | "K" => Some(4),
        "c" | "cm" | "Tm" => Some(6),
        _ => None,
    }
}

/// Operators that put ink (or a clip) on the page.
pub(super) fn is_painting(operator: &str) -> bool {
    matches!(
        operator,
        "S" | "s"
            | "f"
            | "F"
            | "f*"
            | "B"
            | "B*"
            | "b"
            | "b*"
            | "Tj"
            | "TJ"
            | "'"
            | "\""
            | "Do"
            | "sh"
            | "BI"
    )
}

/// The operators of one stream.
pub(super) struct ParsedOps<'d> {
    /// Operators in stream order.
    pub ops: Vec<RawOp<'d>>,
    /// False when some span could not be located; such a stream cannot be instrumented.
    pub spans_ok: bool,
    /// Offset of the first byte the tokenizer could not turn into an operator, when the stream
    /// holds anything but whitespace and comments after the last operator.
    pub unparsed_from: Option<usize>,
}

/// Reads every operator of `data` with the same tokenizer the engine uses, so both agree on
/// operator boundaries.
pub(super) fn parse_ops(data: &[u8]) -> ParsedOps<'_> {
    let mut iter = UntypedIter::new(data);
    let base = data.as_ptr().addr();
    let mut ops = Vec::new();
    let mut spans_ok = true;
    let mut prev_end = 0usize;

    while let Some(inst) = iter.next() {
        let name: &[u8] = inst.operator;
        let operands: Vec<Object<'_>> = inst.operands().cloned().collect();
        let operator = String::from_utf8_lossy(name).into_owned();

        let located = name
            .as_ptr()
            .addr()
            .checked_sub(base)
            .filter(|off| off + name.len() <= data.len())
            .map(|off| off + name.len());
        let mut end = located.unwrap_or(prev_end);
        if located.is_none() {
            spans_ok = false;
        }
        if name == b"BI" {
            match inline_image_end(data, base, &operands) {
                Some(e) => end = e,
                None => spans_ok = false,
            }
        }
        let end = end.max(prev_end);
        let start = skip_trivia(data, prev_end).min(end);
        ops.push(RawOp {
            operator,
            operands,
            chunk_start: prev_end,
            start,
            end,
        });
        prev_end = end;
    }

    let tail = skip_trivia(data, prev_end);
    ParsedOps {
        ops,
        spans_ok,
        unparsed_from: (tail < data.len()).then_some(tail),
    }
}

/// End offset (after `EI`) of the inline image whose data is the stream operand.
fn inline_image_end(data: &[u8], base: usize, operands: &[Object<'_>]) -> Option<usize> {
    let Some(Object::Stream(stream)) = operands.last() else {
        return None;
    };
    let raw = stream.raw_data();
    let off = raw.as_ptr().addr().checked_sub(base)?;
    let after_data = off.checked_add(raw.len())?;
    if after_data > data.len() {
        return None;
    }
    let rest = &data[after_data..];
    let ei = rest.windows(2).position(|w| w == b"EI")?;
    Some(after_data + ei + 2)
}

/// Skips ASCII whitespace and `%` comments starting at `pos`.
fn skip_trivia(data: &[u8], mut pos: usize) -> usize {
    while let Some(&b) = data.get(pos) {
        if matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0C | 0x00) {
            pos += 1;
        } else if b == b'%' {
            while data.get(pos).is_some_and(|c| !matches!(c, b'\r' | b'\n')) {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

/// Builds the instrumented stream: a marker pair before every operator, optionally preceded by
/// `prologue` operators that restore inherited text state.
///
/// Text rendering mode 7 (clip only) is rewritten to 3 (invisible) so the engine still reports
/// the glyph run; the clip itself is tracked by the walker, not by the engine.
pub(super) fn instrument(data: &[u8], parsed: &ParsedOps<'_>, prologue: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + parsed.ops.len() * 40 + prologue.len());
    out.extend_from_slice(prologue.as_bytes());
    for (i, op) in parsed.ops.iter().enumerate() {
        out.extend_from_slice(b"\n/");
        out.extend_from_slice(MARKER_TAG);
        out.extend_from_slice(format!(" << /MCID {i} >> BDC EMC\n").as_bytes());
        let clip_only_mode =
            op.operator == "Tr" && matches!(op.operands.as_slice(), [Object::Number(n)] if n.as_i64() == 7);
        if clip_only_mode {
            out.extend_from_slice(b"3 Tr");
        } else {
            out.extend_from_slice(&data[op.chunk_start..op.end]);
        }
    }
    out.push(b'\n');
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn spans_cover_instructions_without_leading_whitespace() {
        let data = b"q\n  1 0 0 1 5 6 cm % note\n BT /F1 12 Tf (Hi) Tj ET Q";

        let parsed = parse_ops(data);
        let texts: Vec<_> = parsed
            .ops
            .iter()
            .map(|o| {
                (
                    o.operator.as_str(),
                    String::from_utf8_lossy(&data[o.start..o.end]).into_owned(),
                )
            })
            .collect();

        assert_eq!(
            texts,
            vec![
                ("q", "q".to_owned()),
                ("cm", "1 0 0 1 5 6 cm".to_owned()),
                ("BT", "BT".to_owned()),
                ("Tf", "/F1 12 Tf".to_owned()),
                ("Tj", "(Hi) Tj".to_owned()),
                ("ET", "ET".to_owned()),
                ("Q", "Q".to_owned()),
            ]
        );
        assert!(parsed.spans_ok);
    }

    #[test]
    fn inline_image_span_includes_data_and_ei() {
        let data = b"q BI /W 1 /H 1 /BPC 8 /CS /G ID \x80 EI Q";

        let parsed = parse_ops(data);
        let bi = parsed.ops.iter().find(|o| o.operator == "BI");

        assert_eq!(
            bi.map(|o| &data[o.start..o.end]),
            Some(&b"BI /W 1 /H 1 /BPC 8 /CS /G ID \x80 EI"[..])
        );
        assert_eq!(parsed.ops.last().map(|o| o.operator.as_str()), Some("Q"));
        assert!(parsed.spans_ok);
    }

    #[test]
    fn instrumented_stream_marks_every_operator_and_rewrites_clip_mode() {
        let data = b"7 Tr (A) Tj";
        let parsed = parse_ops(data);

        let out = String::from_utf8(instrument(data, &parsed, "")).unwrap_or_default();

        assert_eq!(
            out,
            "\n/PdfxrayOp << /MCID 0 >> BDC EMC\n3 Tr\n/PdfxrayOp << /MCID 1 >> BDC EMC\n (A) Tj\n"
        );
        assert!(out.contains("3 Tr"));
    }
}
