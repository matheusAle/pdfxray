//! Appends an incremental update (ISO 32000-2 7.5.6) to the bytes of a PDF.
//!
//! The original bytes are never touched: new and replaced objects, a cross-reference section
//! (a table, or a cross-reference stream when the newest section of the file is one) and a
//! trailer with `/Prev` are appended after them.

use std::fmt::Write as _;

use pdfxray_core::ObjRef;
use pdfxray_raw::{RawDocument, XrefKind};

use crate::RewriteError;

/// Objects to append, and the object numbers still free.
pub(crate) struct Update {
    next_num: u32,
    objects: Vec<(ObjRef, Vec<u8>)>,
    /// The catalog the new trailer names, in place of the file's `/Root`.
    root: Option<ObjRef>,
}

impl Update {
    /// An empty update that allocates numbers past everything `raw` knows.
    pub(crate) fn new(raw: &RawDocument) -> Self {
        let in_xref = raw
            .revisions()
            .iter()
            .flat_map(|r| r.xref.entries.iter().map(|e| e.num))
            .max();
        let in_trailer = raw
            .trailer()
            .and_then(|t| t.get_int(b"Size"))
            .and_then(|s| u32::try_from(s).ok());
        let next_num = in_xref
            .map(|n| n.saturating_add(1))
            .into_iter()
            .chain(in_trailer)
            .chain(raw.object_refs().iter().map(|r| r.num.saturating_add(1)))
            .max()
            .unwrap_or(1);
        Self {
            next_num,
            objects: Vec::new(),
            root: None,
        }
    }

    /// A fresh object number (generation 0).
    pub(crate) fn alloc(&mut self) -> ObjRef {
        let r = ObjRef::new(self.next_num, 0);
        self.next_num += 1;
        r
    }

    /// Defines (or redefines) `r` as `body`, the PDF syntax of the value including a stream.
    pub(crate) fn put(&mut self, r: ObjRef, body: Vec<u8>) {
        self.objects.push((r, body));
    }

    /// Makes the trailer of the update name `catalog` as `/Root`.
    pub(crate) fn set_root(&mut self, catalog: ObjRef) {
        self.root = Some(catalog);
    }

    /// `original` followed by the update.
    pub(crate) fn finish(mut self, raw: &RawDocument, original: &[u8]) -> Result<Vec<u8>, RewriteError> {
        let last = raw.revisions().last().ok_or(RewriteError::Unreadable(
            "the file has no cross-reference section",
        ))?;
        let prev = last.xref.span.start;
        let as_stream = matches!(last.xref.kind, XrefKind::Stream(_));
        let xref_stream = as_stream.then(|| self.alloc());

        let mut out = original.to_vec();
        if out.last().is_some_and(|b| *b != b'\n') {
            out.push(b'\n');
        }
        let mut offsets: Vec<(ObjRef, usize)> = Vec::with_capacity(self.objects.len() + 1);
        self.objects.sort_by_key(|(r, _)| r.num);
        for (r, body) in &self.objects {
            offsets.push((*r, out.len()));
            out.extend_from_slice(format!("{} {} obj\n", r.num, r.generation).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref_at = out.len();
        let size = self.next_num;
        let trailer = trailer_entries(raw, prev, self.root);
        match xref_stream {
            None => write_table(&mut out, &offsets, size, &trailer, xref_at),
            Some(me) => {
                offsets.push((me, xref_at));
                write_stream(&mut out, me, &offsets, size, &trailer, xref_at)?;
            }
        }
        Ok(out)
    }
}

/// `/Root` (or `root`), `/Info`, `/ID` of the newest trailer, and `/Prev`.
fn trailer_entries(raw: &RawDocument, prev: usize, root: Option<ObjRef>) -> String {
    let mut s = String::new();
    if let Some(root) = root {
        let _ = write!(s, " /Root {} {} R", root.num, root.generation);
    }
    if let Some(trailer) = raw.trailer() {
        for key in [&b"Root"[..], b"Info", b"ID"] {
            if key == b"Root" && root.is_some() {
                continue;
            }
            if let Some(v) = trailer.get(key) {
                let _ = write!(s, " /{} {}", String::from_utf8_lossy(key), v.value);
            }
        }
    }
    let _ = write!(s, " /Prev {prev}");
    s
}

/// Runs of consecutive object numbers: `(first, count)`.
fn runs(offsets: &[(ObjRef, usize)]) -> Vec<(u32, usize)> {
    let mut runs: Vec<(u32, usize)> = Vec::new();
    for (r, _) in offsets {
        match runs.last_mut() {
            Some((first, count))
                if first.saturating_add(u32::try_from(*count).unwrap_or(u32::MAX)) == r.num =>
            {
                *count += 1;
            }
            _ => runs.push((r.num, 1)),
        }
    }
    runs
}

fn write_table(out: &mut Vec<u8>, offsets: &[(ObjRef, usize)], size: u32, trailer: &str, xref_at: usize) {
    out.extend_from_slice(b"xref\n");
    let mut entries = offsets.iter();
    for (first, count) in runs(offsets) {
        out.extend_from_slice(format!("{first} {count}\n").as_bytes());
        for (r, offset) in entries.by_ref().take(count) {
            out.extend_from_slice(format!("{offset:010} {:05} n \n", r.generation).as_bytes());
        }
    }
    out.extend_from_slice(format!("trailer\n<< /Size {size}{trailer} >>\n").as_bytes());
    out.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
}

fn write_stream(
    out: &mut Vec<u8>,
    me: ObjRef,
    offsets: &[(ObjRef, usize)],
    size: u32,
    trailer: &str,
    xref_at: usize,
) -> Result<(), RewriteError> {
    let mut data = Vec::with_capacity(offsets.len() * 7);
    for (r, offset) in offsets {
        let offset = u32::try_from(*offset).map_err(|_| RewriteError::TooLarge)?;
        data.push(1);
        data.extend_from_slice(&offset.to_be_bytes());
        data.extend_from_slice(&r.generation.to_be_bytes());
    }
    let mut index = String::new();
    for (first, count) in runs(offsets) {
        let _ = write!(index, "{first} {count} ");
    }
    let dict = format!(
        "<< /Type /XRef /Size {size} /W [1 4 2] /Index [{}]{trailer} /Length {} >>",
        index.trim_end(),
        data.len()
    );
    out.extend_from_slice(format!("{} {} obj\n{dict}\nstream\n", me.num, me.generation).as_bytes());
    out.extend_from_slice(&data);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    out.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
    Ok(())
}

/// The syntax of a stream object with the dictionary entries `entries` (without `/Length`).
pub(crate) fn stream_body(entries: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("<<{entries} /Length {} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn consecutive_numbers_share_a_subsection() {
        let offsets: Vec<(ObjRef, usize)> = [4, 5, 9, 10, 11, 20]
            .iter()
            .map(|n| (ObjRef::new(*n, 0), 0))
            .collect();

        assert_eq!(runs(&offsets), vec![(4, 2), (9, 3), (20, 1)]);
        assert_eq!(runs(&[]), vec![]);
    }

    #[test]
    fn stream_body_declares_the_data_length() {
        let body = stream_body(" /Type /Foo", b"0 0 m");

        assert_eq!(
            String::from_utf8_lossy(&body),
            "<< /Type /Foo /Length 5 >>\nstream\n0 0 m\nendstream"
        );
    }
}
