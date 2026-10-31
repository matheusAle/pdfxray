//! `RunLengthDecode`.

use super::{Step, StepStatus};

pub(super) fn run_length(input: &[u8], limit: usize) -> Step {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&n) = input.get(i) {
        i += 1;
        match n {
            128 => {
                return Step {
                    bytes: out,
                    status: StepStatus::Done,
                };
            }
            0..=127 => {
                let count = usize::from(n) + 1;
                let end = i + count;
                if end > input.len() {
                    out.extend_from_slice(&input[i..]);
                    return Step {
                        bytes: out,
                        status: StepStatus::Truncated,
                    };
                }
                out.extend_from_slice(&input[i..end]);
                i = end;
            }
            129..=255 => {
                let Some(&byte) = input.get(i) else {
                    return Step {
                        bytes: out,
                        status: StepStatus::Truncated,
                    };
                };
                i += 1;
                out.resize(out.len() + 257 - usize::from(n), byte);
            }
        }
        if out.len() >= limit {
            out.truncate(limit);
            return Step {
                bytes: out,
                status: StepStatus::Limit,
            };
        }
    }
    Step {
        bytes: out,
        status: StepStatus::Done,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::test_util::{run, with_filter};

    #[test]
    fn run_length() {
        let out = run(
            &[2, b'a', b'b', b'c', 254, b'x', 128, 9, 9],
            &with_filter("RunLengthDecode"),
        );

        assert_eq!(out.bytes, b"abcxxx");
        assert!(out.complete);
        let cut = run(&[5, b'a'], &with_filter("RL"));
        assert_eq!((cut.bytes.as_slice(), cut.complete), (&b"a"[..], false));
    }

    #[test]
    fn run_length_repeat_and_literal_runs() {
        let out = run(
            &[0, b'a', 255, b'b', 129, b'c', 128],
            &with_filter("RunLengthDecode"),
        );

        assert_eq!(out.bytes, [b"a".as_slice(), b"bb", &[b'c'; 128]].concat());
        assert!(out.complete);
    }
}
