//! `ASCII85Decode`.

use super::{Step, StepStatus};

pub(super) fn ascii85(input: &[u8]) -> Step {
    let mut out = Vec::with_capacity(input.len() / 5 * 4 + 4);
    let mut group = [0u8; 5];
    let mut n = 0usize;
    let mut status = StepStatus::Done;
    let body = {
        let start = input
            .iter()
            .position(|&b| !crate::object::is_whitespace(b))
            .unwrap_or(input.len());
        input[start..].strip_prefix(b"<~").unwrap_or(&input[start..])
    };
    for &b in body {
        match b {
            b'~' => break,
            _ if crate::object::is_whitespace(b) => {}
            b'z' if n == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group[n] = b - b'!';
                n += 1;
                if n == 5 {
                    let value = group.iter().fold(0u64, |acc, &d| acc * 85 + u64::from(d));
                    match u32::try_from(value) {
                        Ok(v) => out.extend_from_slice(&v.to_be_bytes()),
                        Err(_) => status = StepStatus::Corrupt,
                    }
                    n = 0;
                }
            }
            _ => status = StepStatus::Corrupt,
        }
    }
    match n {
        0 => {}
        1 => status = StepStatus::Corrupt,
        _ => {
            let mut padded = [84u8; 5];
            padded[..n].copy_from_slice(&group[..n]);
            let value = padded.iter().fold(0u64, |acc, &d| acc * 85 + u64::from(d));
            let bytes = u32::try_from(value).unwrap_or(u32::MAX).to_be_bytes();
            out.extend_from_slice(&bytes[..n - 1]);
        }
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
    fn ascii85_reports_invalid_input() {
        let overflow = run(b"uuuuu~>", &with_filter("ASCII85Decode"));
        let stray = run(b"9jq\x01o^~>", &with_filter("A85"));
        let lone = run(b"9~>", &with_filter("A85"));

        assert!(!overflow.complete);
        assert_eq!(
            overflow.issues,
            vec![DecodeIssue::Corrupt(Name::new("ASCII85Decode"))]
        );
        assert!(!stray.complete);
        assert_eq!(stray.bytes, b"Man ");
        assert!(!lone.complete);
    }
}
