//! A tiny PDF byte writer for building small, deliberately damaged files.
#![allow(dead_code, missing_docs, unreachable_pub, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;

use pdfxray_raw::{RawDiagnostic, RawDocument};

pub const HEADER: &[u8] = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n";

/// Appends objects and remembers where each one starts.
pub struct Builder {
    pub buf: Vec<u8>,
    pub offsets: BTreeMap<u32, usize>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            buf: HEADER.to_vec(),
            offsets: BTreeMap::new(),
        }
    }

    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(bytes);
        self
    }

    /// `num gen obj\nbody\nendobj\n`; returns the offset of `num`.
    pub fn object(&mut self, num: u32, generation: u16, body: &str) -> usize {
        let at = self.buf.len();
        self.offsets.insert(num, at);
        self.buf
            .extend_from_slice(format!("{num} {generation} obj\n{body}\nendobj\n").as_bytes());
        at
    }

    /// A stream object with an exact `/Length`; returns the offset of the data.
    pub fn stream(&mut self, num: u32, dict_extra: &str, data: &[u8]) -> usize {
        let at = self.buf.len();
        self.offsets.insert(num, at);
        self.buf.extend_from_slice(
            format!("{num} 0 obj\n<< {dict_extra} /Length {} >>\nstream\n", data.len()).as_bytes(),
        );
        let data_at = self.buf.len();
        self.buf.extend_from_slice(data);
        self.buf.extend_from_slice(b"\nendstream\nendobj\n");
        data_at
    }

    /// A classic table covering `0..size`; `claims` overrides the recorded offset of an object.
    /// Objects without an offset are written as free. Returns the offset of `xref`.
    pub fn table(&mut self, size: u32, claims: &[(u32, u64)]) -> usize {
        let at = self.buf.len();
        self.buf.extend_from_slice(format!("xref\n0 {size}\n").as_bytes());
        for num in 0..size {
            let claimed = claims.iter().find(|(n, _)| *n == num).map(|(_, o)| *o);
            let line = match (num, claimed.or_else(|| self.offsets.get(&num).map(|&o| o as u64))) {
                (0, _) => "0000000000 65535 f \n".to_owned(),
                (_, Some(offset)) => format!("{offset:010} 00000 n \n"),
                (_, None) => "0000000000 00000 f \n".to_owned(),
            };
            self.buf.extend_from_slice(line.as_bytes());
        }
        at
    }

    pub fn trailer(&mut self, dict: &str) -> &mut Self {
        self.raw(format!("trailer\n<< {dict} >>\n").as_bytes())
    }

    pub fn startxref(&mut self, offset: u64) -> &mut Self {
        self.raw(format!("startxref\n{offset}\n%%EOF\n").as_bytes())
    }
}

/// Three objects, a table and a correct trailer. Returns the bytes and the offset of `xref`.
pub fn base() -> (Builder, usize) {
    let mut b = Builder::new();
    b.object(1, 0, "<< /Type /Catalog /Pages 2 0 R >>");
    b.object(2, 0, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    b.object(3, 0, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>");
    let xref = b.table(4, &[]);
    b.trailer("/Size 4 /Root 1 0 R");
    b.startxref(xref as u64);
    (b, xref)
}

pub fn parse(bytes: &[u8]) -> RawDocument {
    RawDocument::parse(Arc::new(bytes.to_vec())).expect("header present")
}

pub fn codes(doc: &RawDocument) -> Vec<&'static str> {
    doc.diagnostics().iter().map(RawDiagnostic::code).collect()
}

/// Offset of the last occurrence of `needle`.
pub fn rfind(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len())
        .rposition(|w| w == needle)
        .expect("needle present")
}

/// Offset of the first occurrence of `needle`.
pub fn find(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len())
        .position(|w| w == needle)
        .expect("needle present")
}
