//! Helpers over `hayro-syntax` objects: PDF-syntax rendering and text-string decoding.

use std::fmt::Write as _;

use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Object};

/// Nesting limit for rendering; deeper objects are elided.
const MAX_DEPTH: usize = 8;

/// Renders an object in PDF syntax.
pub(super) fn object_syntax(obj: &Object<'_>) -> String {
    let mut out = String::new();
    write_object(&mut out, obj, 0);
    out
}

/// Renders a dictionary entry (object or indirect reference) in PDF syntax.
pub(super) fn maybe_ref_syntax(value: &MaybeRef<Object<'_>>) -> String {
    let mut out = String::new();
    write_maybe_ref(&mut out, value, 0);
    out
}

fn write_maybe_ref(out: &mut String, value: &MaybeRef<Object<'_>>, depth: usize) {
    match value {
        MaybeRef::Ref(r) => {
            let _ = write!(out, "{} {} R", r.obj_number, r.gen_number);
        }
        MaybeRef::NotRef(obj) => write_object(out, obj, depth),
    }
}

fn write_object(out: &mut String, obj: &Object<'_>, depth: usize) {
    if depth > MAX_DEPTH {
        out.push_str("...");
        return;
    }
    match obj {
        Object::Null(_) => out.push_str("null"),
        Object::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Object::Number(n) => {
            let _ = write!(out, "{n}");
        }
        Object::String(s) => write_string(out, s.as_bytes()),
        Object::Name(n) => write_name(out, n),
        Object::Array(a) => write_array(out, a, depth),
        Object::Dict(d) => write_dict(out, d, depth),
        Object::Stream(s) => {
            write_dict(out, s.dict(), depth);
            out.push_str(" stream");
        }
    }
}

fn write_array(out: &mut String, array: &Array<'_>, depth: usize) {
    out.push('[');
    for (i, item) in array.raw_iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        write_maybe_ref(out, &item, depth + 1);
    }
    out.push(']');
}

fn write_dict(out: &mut String, dict: &Dict<'_>, depth: usize) {
    out.push_str("<<");
    for (key, value) in dict.entries() {
        out.push(' ');
        write_name(out, &key);
        out.push(' ');
        write_maybe_ref(out, &value, depth + 1);
    }
    out.push_str(" >>");
}

fn write_name(out: &mut String, name: &Name<'_>) {
    out.push_str(&name_token_bytes(name));
}

/// A name as a content-stream token (`/Name`, irregular bytes written as `#XX`).
pub(super) fn name_token(name: &str) -> String {
    name_token_bytes(name.as_bytes())
}

fn name_token_bytes(name: &[u8]) -> String {
    let mut out = String::from("/");
    for &b in name {
        let regular = b.is_ascii_graphic() && !b"()<>[]{}/%#".contains(&b);
        if regular {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "#{b:02X}");
        }
    }
    out
}

fn write_string(out: &mut String, bytes: &[u8]) {
    let printable = bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ');
    if printable {
        out.push('(');
        for &b in bytes {
            if matches!(b, b'(' | b')' | b'\\') {
                out.push('\\');
            }
            out.push(char::from(b));
        }
        out.push(')');
    } else {
        out.push('<');
        for b in bytes {
            let _ = write!(out, "{b:02X}");
        }
        out.push('>');
    }
}

/// Decodes a PDF text string: UTF-16BE/LE with BOM, UTF-8 with BOM, otherwise Latin-1 (a close
/// approximation of `PDFDocEncoding`).
pub(super) fn decode_text_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn strings_render_as_literal_or_hex() {
        let mut out = String::new();
        write_string(&mut out, b"a(b)\\");
        out.push('|');
        write_string(&mut out, &[0x00, 0xFF]);

        assert_eq!(out, "(a\\(b\\)\\\\)|<00FF>");
        assert!(out.contains('|'));
    }

    #[test]
    fn text_strings_decode_by_bom() {
        assert_eq!(
            decode_text_string(&[0xFE, 0xFF, 0x00, 0x41, 0x00, 0xE9]),
            "A\u{e9}"
        );
        assert_eq!(decode_text_string(b"plain"), "plain");
        assert_eq!(decode_text_string(&[0xFF, 0xFE, 0x41, 0x00]), "A");
    }
}
