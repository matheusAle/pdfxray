//! Helpers shared by the parser tests.

use super::indirect::ParsedObject;
use super::{NoLengths, parse_indirect};
use crate::{Dict, Object, RawDiagnostic, Spanned, Stream};

pub(super) fn indirect(src: &[u8]) -> (ParsedObject, Vec<RawDiagnostic>) {
    let mut sink = Vec::new();
    let parsed = parse_indirect(src, 0, &NoLengths, &mut sink).unwrap();
    (parsed, sink)
}

/// The dictionary `value` parsed to; anything else fails the test and shows what was parsed.
pub(super) fn as_dict(value: &Spanned<Object>) -> &Dict {
    match &value.value {
        Object::Dict(dict) => dict,
        other => panic!("expected a dictionary at {:?}, parsed {other:?}", value.span),
    }
}

/// The elements of the array `value` parsed to; anything else fails the test.
pub(super) fn as_array(value: &Spanned<Object>) -> &[Spanned<Object>] {
    match &value.value {
        Object::Array(items) => items,
        other => panic!("expected an array at {:?}, parsed {other:?}", value.span),
    }
}

/// The stream `value` parsed to; anything else fails the test.
pub(super) fn as_stream(value: &Spanned<Object>) -> &Stream {
    match &value.value {
        Object::Stream(stream) => stream,
        other => panic!("expected a stream at {:?}, parsed {other:?}", value.span),
    }
}
