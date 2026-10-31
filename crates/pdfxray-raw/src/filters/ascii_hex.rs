//! `ASCIIHexDecode`.

use super::{Step, StepStatus};

pub(super) fn ascii_hex(input: &[u8]) -> Step {
    let mut out = Vec::with_capacity(input.len() / 2);
    let mut pending: Option<u8> = None;
    let mut status = StepStatus::Done;
    for &b in input {
        if b == b'>' {
            break;
        }
        if crate::object::is_whitespace(b) {
            continue;
        }
        let Some(v) = crate::lexer::hex_value(b) else {
            status = StepStatus::Corrupt;
            continue;
        };
        match pending.take() {
            Some(hi) => out.push(hi << 4 | v),
            None => pending = Some(v),
        }
    }
    if let Some(hi) = pending {
        out.push(hi << 4);
    }
    Step { bytes: out, status }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::DecodeIssue;
    use super::super::test_util::{run, with_filter};
    use crate::Name;

    #[test]
    fn ascii_hex_and_85() {
        let hex = run(b"48 65 6C6c 6F7>", &with_filter("ASCIIHexDecode"));
        let a85 = run(b"<~9jqo^~>", &with_filter("ASCII85Decode"));
        let partial = run(b"9jqo~>", &with_filter("ASCII85Decode"));
        let zeros = run(b"z~>", &with_filter("A85"));

        assert_eq!(hex.bytes, b"Hellop");
        assert_eq!((a85.bytes.as_slice(), a85.complete), (&b"Man "[..], true));
        assert_eq!(partial.bytes, b"Man");
        assert_eq!(zeros.bytes, vec![0; 4]);
        assert!(!run(b"a~~v", &with_filter("ASCII85Decode")).complete);
    }

    #[test]
    fn ascii_hex_reports_invalid_digits() {
        let out = run(b"41 Z 42>", &with_filter("ASCIIHexDecode"));

        assert_eq!(out.bytes, b"AB");
        assert_eq!(
            out.issues,
            vec![DecodeIssue::Corrupt(Name::new("ASCIIHexDecode"))]
        );
    }
}
