//! Text helpers: PDF text strings and display shortening.

/// Decodes a PDF text string (ISO 32000-2 §7.9.2.2): UTF-16BE/LE with BOM, UTF-8 with BOM, or
/// `PDFDocEncoding` (approximated by Latin-1).
pub(super) fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, true)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, false)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
    }
}

fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| {
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Collapses runs of white space to single spaces and trims the ends.
pub(super) fn squash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Appends `s` to `out`, cut to at most `max` characters and ending in `…` when something was
/// dropped.
pub(super) fn push_ellipsized(out: &mut String, s: &str, max: usize) {
    if s.char_indices().nth(max).is_none() {
        out.push_str(s);
        return;
    }
    let keep = max.saturating_sub(1);
    let end = s.char_indices().nth(keep).map_or(s.len(), |(i, _)| i);
    out.push_str(&s[..end]);
    out.push('…');
}

/// Cuts `s` to at most `max` characters, ending in `…` when something was dropped.
pub(super) fn ellipsize(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max * 4 + 3));
    push_ellipsized(&mut out, s, max);
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn decodes_the_three_text_string_forms() {
        assert_eq!(decode(b"plain"), "plain");
        assert_eq!(decode(&[0xFE, 0xFF, 0x00, 0x48, 0x00, 0xE9]), "Hé");
        assert_eq!(decode(&[0xFF, 0xFE, 0x48, 0x00]), "H");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'o', b'k']), "ok");
        assert_eq!(decode(&[0xE9]), "é");
        // An unpaired surrogate must not panic.
        assert_eq!(decode(&[0xFE, 0xFF, 0xD8, 0x00]), "\u{FFFD}");
        assert_eq!(decode(b""), "");
    }

    #[test]
    fn squash_and_ellipsize() {
        assert_eq!(squash("  a \n b\tc  "), "a b c");
        assert_eq!(ellipsize("abcdef", 6), "abcdef");
        assert_eq!(ellipsize("abcdefg", 6), "abcde…");
        assert_eq!(ellipsize("", 3), "");
        assert!(ellipsize("abcdefg", 6).ends_with('…'));
    }
}
