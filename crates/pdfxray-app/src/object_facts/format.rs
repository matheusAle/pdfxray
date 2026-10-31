//! Text formatting the panels share: diagnostic words, thousands separators, byte sizes, and
//! `n g R` references split out of values.

use pdfxray_core::ObjRef;

use super::Piece;

/// The dictionary key a diagnostic code is about, when it is about one.
pub fn key_of(code: &str) -> Option<&'static str> {
    (code == "stream-length-wrong").then_some("/Length")
}

/// The short word a diagnostic shows on its row in the navigator: `length`, `xref offset`.
pub fn short_label(code: &str) -> String {
    let trimmed = code
        .strip_prefix("stream-")
        .unwrap_or(code)
        .trim_end_matches("-wrong")
        .trim_end_matches("-mismatch");
    trimmed.replace('-', " ")
}

/// `1200` as `1,200`.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A size for chips: `43 B`, `1.2 KB`, `3.9 MB`.
pub fn human_bytes(n: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * KB;
    if n < KB {
        format!("{n} B")
    } else if n < MB {
        format!("{} KB", tenths(n, KB))
    } else {
        format!("{} MB", tenths(n, MB))
    }
}

/// `n / unit` with one decimal, from integers.
fn tenths(n: usize, unit: usize) -> String {
    let scaled = (n * 10 + unit / 2) / unit;
    format!("{}.{}", scaled / 10, scaled % 10)
}

/// Splits `text` into plain text and `n g R` references.
pub fn split_refs(text: &str) -> Vec<Piece> {
    let bytes = text.as_bytes();
    let mut pieces = Vec::new();
    let mut plain_from = 0;
    let mut i = 0;
    while i < bytes.len() {
        let starts_number = bytes[i].is_ascii_digit()
            && (i == 0
                || !(bytes[i - 1].is_ascii_alphanumeric()
                    || matches!(bytes[i - 1], b'.' | b'/' | b'-' | b'+')));
        if !starts_number {
            i += 1;
            continue;
        }
        match reference_at(bytes, i) {
            Some((r, end)) => {
                if plain_from < i {
                    pieces.push(Piece::Text(text[plain_from..i].to_owned()));
                }
                pieces.push(Piece::Ref(r));
                i = end;
                plain_from = end;
            }
            None => {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
        }
    }
    if plain_from < bytes.len() {
        pieces.push(Piece::Text(text[plain_from..].to_owned()));
    }
    pieces
}

/// The reference `n g R` starting at `from`, and where it ends.
fn reference_at(bytes: &[u8], from: usize) -> Option<(ObjRef, usize)> {
    let (num, after_num) = digits(bytes, from)?;
    if bytes.get(after_num) != Some(&b' ') {
        return None;
    }
    let (generation, after_gen) = digits(bytes, after_num + 1)?;
    if bytes.get(after_gen) != Some(&b' ') || bytes.get(after_gen + 1) != Some(&b'R') {
        return None;
    }
    let end = after_gen + 2;
    if bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    Some((ObjRef::new(num, u16::try_from(generation).ok()?), end))
}

/// The unsigned number at `from` and the index after it.
fn digits(bytes: &[u8], from: usize) -> Option<(u32, usize)> {
    let len = bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    if len == 0 || len > 10 {
        return None;
    }
    let value = std::str::from_utf8(&bytes[from..from + len]).ok()?.parse().ok()?;
    Some((value, from + len))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn r(num: u32) -> ObjRef {
        ObjRef::new(num, 0)
    }

    #[test]
    fn references_inside_values_are_split_out() {
        assert_eq!(
            split_refs("[3 0 R 12 0 R]"),
            [
                Piece::Text("[".into()),
                Piece::Ref(r(3)),
                Piece::Text(" ".into()),
                Piece::Ref(r(12)),
                Piece::Text("]".into()),
            ]
        );
        assert_eq!(split_refs("25 0 R"), [Piece::Ref(r(25))]);
        assert_eq!(
            split_refs("/Fl 7 1 R"),
            [Piece::Text("/Fl ".into()), Piece::Ref(ObjRef::new(7, 1))]
        );
    }

    #[test]
    fn numbers_that_only_look_like_references_stay_text() {
        for text in [
            "1200",
            "[0 0 612 792]",
            "1.5 0 R",
            "10 0 Rx",
            "(3 0 R)x",
            "0 0 0 R1",
            "-3 0 R",
        ] {
            let pieces = split_refs(text);
            let joined: String = pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(t) => t.clone(),
                    Piece::Ref(r) => r.to_string(),
                })
                .collect();
            assert_eq!(joined, text, "text is preserved");
        }
        assert_eq!(split_refs("[0 0 612 792]"), [Piece::Text("[0 0 612 792]".into())]);
        assert_eq!(split_refs("1.5 0 R"), [Piece::Text("1.5 0 R".into())]);
        assert_eq!(split_refs("10 0 Rx"), [Piece::Text("10 0 Rx".into())]);
        assert_eq!(split_refs(""), Vec::<Piece>::new());
    }

    #[test]
    fn sizes_read_like_the_design() {
        assert_eq!(human_bytes(43), "43 B");
        assert_eq!(human_bytes(1229), "1.2 KB");
        assert_eq!(human_bytes(3994), "3.9 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(thousands(1200), "1,200");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn diagnostic_codes_become_short_words() {
        assert_eq!(short_label("stream-length-wrong"), "length");
        assert_eq!(short_label("xref-offset-wrong"), "xref offset");
        assert_eq!(short_label("missing-endobj"), "missing endobj");
        assert_eq!(key_of("stream-length-wrong"), Some("/Length"));
        assert_eq!(key_of("xref-offset-wrong"), None);
    }
}
