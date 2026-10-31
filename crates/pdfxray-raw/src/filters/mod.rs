//! Stream filter chains: `Flate`, `LZW`, `ASCIIHex`, `ASCII85`, `RunLength` and the PNG/TIFF predictors.
//!
//! Image codecs and `Crypt` are not decoded: the chain stops there and the result is flagged
//! incomplete. Nothing here panics on corrupt input and output is capped.
//!
//! This module holds the shared types and the chain itself ([`decode_with`]); each codec lives in
//! its own submodule: [`flate`], [`lzw`], [`ascii_hex`], [`ascii85`], [`run_length`], and the
//! [`predictor`] that runs after `Flate` and `LZW`.

mod ascii85;
mod ascii_hex;
mod flate;
mod lzw;
mod predictor;
mod run_length;
#[cfg(test)]
mod test_util;

use std::fmt;

use crate::{Dict, Name, Object};

/// Upper bound for the output of a single filter stage (256 MiB), against decompression bombs.
pub const MAX_DECODED_LEN: usize = 256 * 1024 * 1024;

/// Resolves indirect `/Filter` and `/DecodeParms` values.
pub(crate) type Resolver<'r> = &'r dyn Fn(&Object) -> Object;

/// Something that went wrong while decoding, short of failing outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeIssue {
    /// The filter is an image codec, `Crypt`, or unknown; decoding stopped before it.
    UnsupportedFilter(Name),
    /// The data ended before the filter's end marker; the bytes decoded so far are returned.
    Truncated(Name),
    /// The data is not valid for the filter; the bytes decoded before the error are returned.
    Corrupt(Name),
    /// The output reached [`MAX_DECODED_LEN`] and was cut.
    OutputLimit(Name),
    /// `/DecodeParms` has values the predictor cannot use; the data is returned without it.
    BadPredictorParams(Name),
    /// The last predictor row is shorter than a full row.
    TruncatedPredictorRow(Name),
    /// The document is encrypted and pdfxray does not decrypt; the raw bytes are returned.
    Encrypted,
}

impl fmt::Display for DecodeIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFilter(n) => write!(f, "filter {n} is not decoded"),
            Self::Truncated(n) => write!(f, "{n} data ends before its end marker"),
            Self::Corrupt(n) => write!(f, "{n} data is corrupt"),
            Self::OutputLimit(n) => write!(f, "{n} output exceeds {MAX_DECODED_LEN} bytes and was cut"),
            Self::BadPredictorParams(n) => write!(f, "{n} predictor parameters are invalid"),
            Self::TruncatedPredictorRow(n) => write!(f, "{n} predictor data ends inside a row"),
            Self::Encrypted => f.write_str("the document is encrypted; bytes are returned as stored"),
        }
    }
}

/// The result of running a stream's filter chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The decoded bytes, or whatever was produced before decoding stopped.
    pub bytes: Vec<u8>,
    /// The filters the stream declares, in order (abbreviations kept as written).
    pub filters: Vec<Name>,
    /// False when a filter is unsupported or the data is truncated, corrupt or cut at the limit.
    pub complete: bool,
    /// How many of [`Decoded::filters`] were applied (fully or partly).
    pub applied: usize,
    /// Why decoding is not complete, if it is not.
    pub issues: Vec<DecodeIssue>,
}

/// Errors that prevent decoding from starting at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The stream's data span does not lie inside the file.
    #[error("stream data span {start}..{end} is outside the file")]
    DataOutOfBounds {
        /// Start of the span.
        start: usize,
        /// End of the span.
        end: usize,
    },
    /// `/Filter` is neither a name nor an array of names.
    #[error("/Filter is neither a name nor an array of names")]
    InvalidFilterEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepStatus {
    Done,
    Truncated,
    Corrupt,
    Limit,
}

struct Step {
    bytes: Vec<u8>,
    status: StepStatus,
}

/// Runs the filter chain of `dict` over `raw`.
pub(crate) fn decode_with(
    raw: &[u8],
    dict: &Dict,
    resolve: Resolver<'_>,
    limit: usize,
) -> Result<Decoded, DecodeError> {
    let filters = filter_names(dict, resolve)?;
    let parms = decode_parms(dict, resolve, filters.len());
    let mut data = raw.to_vec();
    let mut issues = Vec::new();
    let mut applied = 0;
    for (filter, parms) in filters.iter().zip(&parms) {
        let step = match canonical(filter.as_bytes()) {
            Some("FlateDecode") => {
                let s = flate::inflate(&data, limit);
                predictor::predict(s, parms, resolve, filter, &mut issues, limit)
            }
            Some("LZWDecode") => {
                let early = int_param(parms, b"EarlyChange", resolve).is_none_or(|v| v != 0);
                let s = lzw::lzw(&data, early, limit);
                predictor::predict(s, parms, resolve, filter, &mut issues, limit)
            }
            Some("ASCIIHexDecode") => ascii_hex::ascii_hex(&data),
            Some("ASCII85Decode") => ascii85::ascii85(&data),
            Some("RunLengthDecode") => run_length::run_length(&data, limit),
            _ => {
                issues.push(DecodeIssue::UnsupportedFilter(filter.clone()));
                break;
            }
        };
        applied += 1;
        data = step.bytes;
        match step.status {
            StepStatus::Done => {}
            StepStatus::Truncated => issues.push(DecodeIssue::Truncated(filter.clone())),
            StepStatus::Corrupt => issues.push(DecodeIssue::Corrupt(filter.clone())),
            StepStatus::Limit => issues.push(DecodeIssue::OutputLimit(filter.clone())),
        }
        if step.status != StepStatus::Done {
            break;
        }
    }
    Ok(Decoded {
        bytes: data,
        complete: issues.is_empty(),
        filters,
        applied,
        issues,
    })
}

/// Maps a (possibly abbreviated) filter name to its full name.
fn canonical(name: &[u8]) -> Option<&'static str> {
    Some(match name {
        b"FlateDecode" | b"Fl" => "FlateDecode",
        b"LZWDecode" | b"LZW" => "LZWDecode",
        b"ASCIIHexDecode" | b"AHx" => "ASCIIHexDecode",
        b"ASCII85Decode" | b"A85" => "ASCII85Decode",
        b"RunLengthDecode" | b"RL" => "RunLengthDecode",
        _ => return None,
    })
}

pub(crate) fn filter_names(dict: &Dict, resolve: Resolver<'_>) -> Result<Vec<Name>, DecodeError> {
    let Some(entry) = dict.get(b"Filter") else {
        return Ok(Vec::new());
    };
    match &resolve(&entry.value) {
        Object::Null => Ok(Vec::new()),
        Object::Name(n) => Ok(vec![n.clone()]),
        Object::Array(items) => items
            .iter()
            .map(|item| match &resolve(&item.value) {
                Object::Name(n) => Ok(n.clone()),
                _ => Err(DecodeError::InvalidFilterEntry),
            })
            .collect(),
        _ => Err(DecodeError::InvalidFilterEntry),
    }
}

/// One parameter dictionary per filter (empty when absent or `null`).
fn decode_parms(dict: &Dict, resolve: Resolver<'_>, count: usize) -> Vec<Dict> {
    let mut out = vec![Dict::default(); count];
    let Some(entry) = dict.get(b"DecodeParms").or_else(|| dict.get(b"DP")) else {
        return out;
    };
    match &resolve(&entry.value) {
        Object::Dict(d) => {
            if let Some(first) = out.first_mut() {
                *first = d.clone();
            }
        }
        Object::Array(items) => {
            for (slot, item) in out.iter_mut().zip(items) {
                if let Object::Dict(d) = &resolve(&item.value) {
                    *slot = d.clone();
                }
            }
        }
        _ => {}
    }
    out
}

fn int_param(parms: &Dict, key: &[u8], resolve: Resolver<'_>) -> Option<i64> {
    parms.get(key).and_then(|v| resolve(&v.value).as_int())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::test_util::{dict, hex_of, identity, names, run, sp, with_filter, zlib};
    use super::*;

    #[test]
    fn no_filter_returns_the_raw_bytes() {
        let out = run(b"plain", &Dict::default());

        assert_eq!(
            out,
            Decoded {
                bytes: b"plain".to_vec(),
                filters: vec![],
                complete: true,
                applied: 0,
                issues: vec![]
            }
        );
    }

    #[test]
    fn chains_apply_in_order() {
        let hex = hex_of(&zlib(b"chained"));
        let d = dict(vec![("Filter", names(&["ASCIIHexDecode", "FlateDecode"]))]);
        let out = run(format!("{hex}>").as_bytes(), &d);

        assert_eq!(out.bytes, b"chained");
        assert_eq!((out.applied, out.complete), (2, true));
    }

    #[test]
    fn image_filters_stop_the_chain() {
        let d = dict(vec![("Filter", names(&["ASCIIHexDecode", "DCTDecode"]))]);
        let out = run(b"4142>", &d);

        assert_eq!(out.bytes, b"AB");
        assert_eq!(out.applied, 1);
        assert!(!out.complete);
        assert_eq!(
            out.issues,
            vec![DecodeIssue::UnsupportedFilter(Name::new("DCTDecode"))]
        );
        assert_eq!(
            run(b"x", &with_filter("Crypt")).issues,
            vec![DecodeIssue::UnsupportedFilter(Name::new("Crypt"))]
        );
    }

    #[test]
    fn invalid_filter_entry_is_an_error() {
        let d = dict(vec![("Filter", Object::Int(3))]);

        assert_eq!(
            decode_with(b"", &d, &identity, 10),
            Err(DecodeError::InvalidFilterEntry)
        );
    }

    #[test]
    fn decode_parms_array_pairs_with_each_filter() {
        let hex = hex_of(&zlib(&[2, 1, 2, 3, 2, 1, 1, 1]));
        let png = dict(vec![("Predictor", Object::Int(12)), ("Columns", Object::Int(3))]);
        let d = dict(vec![
            ("Filter", names(&["AHx", "Fl"])),
            (
                "DecodeParms",
                Object::Array(vec![sp(Object::Null), sp(Object::Dict(png))]),
            ),
        ]);
        let out = run(format!("{hex}>").as_bytes(), &d);

        assert_eq!(out.bytes, [1, 2, 3, 2, 3, 4]);
        assert_eq!(out.filters, vec![Name::new("AHx"), Name::new("Fl")]);
        assert_eq!((out.applied, out.complete), (2, true));
    }

    #[test]
    fn abbreviated_names_are_kept_as_written_and_unknown_names_stop() {
        let out = run(
            b"41>",
            &dict(vec![("Filter", names(&["AHx", "FooDecode", "Fl"]))]),
        );

        assert_eq!(
            out.filters,
            vec![Name::new("AHx"), Name::new("FooDecode"), Name::new("Fl")]
        );
        assert_eq!(out.bytes, b"A");
        assert_eq!(out.applied, 1);
        assert_eq!(
            out.issues,
            vec![DecodeIssue::UnsupportedFilter(Name::new("FooDecode"))]
        );
        for codec in ["DCTDecode", "JPXDecode", "JBIG2Decode", "CCITTFaxDecode", "Crypt"] {
            let image = run(b"x", &with_filter(codec));
            assert_eq!(
                (image.bytes.as_slice(), image.complete, image.applied),
                (&b"x"[..], false, 0),
                "{codec}"
            );
        }
    }

    #[test]
    fn issues_describe_themselves() {
        assert_eq!(
            DecodeIssue::UnsupportedFilter(Name::new("DCTDecode")).to_string(),
            "filter /DCTDecode is not decoded"
        );
        assert_eq!(
            DecodeIssue::Truncated(Name::new("FlateDecode")).to_string(),
            "/FlateDecode data ends before its end marker"
        );
        assert_eq!(
            DecodeError::InvalidFilterEntry.to_string(),
            "/Filter is neither a name nor an array of names"
        );
    }
}
