//! Dictionaries and encoders shared by the filter tests.

use std::fmt::Write as _;
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use super::{Decoded, MAX_DECODED_LEN, decode_with};
use crate::{Dict, Name, Object, Span, Spanned};

pub(super) fn identity(o: &Object) -> Object {
    o.clone()
}

pub(super) fn sp(o: Object) -> Spanned<Object> {
    Spanned::new(o, Span::new(0, 0))
}

pub(super) fn key(k: &str) -> Spanned<Name> {
    Spanned::new(Name::new(k), Span::new(0, 0))
}

pub(super) fn names(list: &[&str]) -> Object {
    Object::Array(list.iter().map(|n| sp(Object::Name(Name::new(*n)))).collect())
}

pub(super) fn dict(entries: Vec<(&str, Object)>) -> Dict {
    Dict {
        entries: entries.into_iter().map(|(k, v)| (key(k), sp(v))).collect(),
    }
}

pub(super) fn run(raw: &[u8], d: &Dict) -> Decoded {
    decode_with(raw, d, &identity, MAX_DECODED_LEN).unwrap()
}

pub(super) fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

pub(super) fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

pub(super) fn with_filter(filter: &str) -> Dict {
    dict(vec![("Filter", Object::Name(Name::new(filter)))])
}
