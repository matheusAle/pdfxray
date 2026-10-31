//! Builds a copy of a PDF in which the operators a [`ContentFilter`] rejects no longer paint.
//!
//! hayro's renderer device is private, so draw calls cannot be dropped on the way. Instead the
//! page's `/Contents` (and every Form `XObject` whose operators are affected) is rewritten and
//! appended to the file as an incremental update; the copy is then rendered like any document.
//!
//! # Rewriting rules
//!
//! Only painting operators are touched, by splicing the bytes at the operator's span in the
//! decoded stream; everything else is copied verbatim, so state, nesting and text positioning
//! stay exactly as they were.
//!
//! | rejected operator                 | replaced by                                                     |
//! |-----------------------------------|-----------------------------------------------------------------|
//! | `f F f* B B* b b* S s`            | `n` (ends the path; a pending `W` clip still applies)           |
//! | `Tj TJ ' "`                       | `<neutral> Tr <the operator> <previous> Tr`                     |
//! | `sh`, inline image `BI ... EI`    | a blank                                                         |
//! | `Do` of an image                  | a blank                                                         |
//! | `Do` of a Form `XObject`          | `/PdfxrayForm<n> Do` when anything inside changed, else kept    |
//!
//! `<neutral>` is `3` (invisible) for fill/stroke modes `0..=3` and `7` (clip only) for the
//! clipping modes `4..=7`, so a hidden clipping text run still clips. `'` and `"` keep their line
//! advance and spacing side effects because the operator itself stays.
//!
//! A rewritten Form `XObject` is a new stream object (uncompressed) with the original
//! dictionary, a `/Resources` that also names the filtered copies of the forms it invokes, and
//! no `/Filter`. The page gets a new `/Contents` stream holding all its filtered content streams
//! and, when it invokes rewritten forms, a `/Resources` that names them.

use std::fmt::Write as _;
use std::sync::Arc;

use pdfxray_core::capture::{CapturedPage, OpRecord, StreamRef, XObjectScope};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, ObjRef, PageIx};
use pdfxray_raw::{Dict, Object, RawDocument};

use crate::RewriteError;
use crate::filter::ContentFilter;
use crate::update::{Update, stream_body};

/// The bytes of `doc` followed by an incremental update that applies `filter` to page `page`.
///
/// The original bytes are an exact prefix of the result.
pub fn filtered_pdf(doc: &Document, page: PageIx, filter: &ContentFilter) -> Result<Vec<u8>, RewriteError> {
    let raw = RawDocument::parse(Arc::new(doc.bytes().to_vec()))
        .map_err(|_| RewriteError::Unreadable("the file has no PDF header"))?;
    if raw.encryption().is_some() {
        return Err(RewriteError::Unreadable("the file is encrypted"));
    }
    let cap = doc.captured(page).ok_or(RewriteError::NotCaptured)?;
    let info = doc.page(page).ok_or(RewriteError::NotCaptured)?;
    let page_ref = info.obj.ok_or(RewriteError::NoPageObject)?;
    let page_obj = raw
        .object(page_ref)
        .ok_or(RewriteError::Unreadable("the page object cannot be read"))?;
    let Object::Dict(page_dict) = &page_obj.value.value else {
        return Err(RewriteError::Unreadable("the page object is not a dictionary"));
    };

    let mut rewriter = Rewriter {
        raw: &raw,
        cap,
        filter,
        update: Update::new(&raw),
    };
    let resources = page_resources(&raw, info.resources_from, page_dict);

    let mut content = Vec::new();
    let mut extras = Vec::new();
    for (index, data) in rewriter.page_streams(page_dict).into_iter().enumerate() {
        let ops = cap
            .ops
            .iter()
            .filter(
                |o| matches!(&o.stream, StreamRef::PageContent { index: i } if usize_from_u32(*i) == index),
            )
            .collect();
        let done = rewriter.rewrite_stream(&data, ops, &resources);
        if !content.is_empty() {
            content.push(b'\n');
        }
        content.extend_from_slice(&done.bytes);
        extras.extend(done.extras);
    }

    let contents = rewriter.update.alloc();
    rewriter.update.put(contents, stream_body("", &content));
    let mut dict = String::from("<<");
    for (key, value) in &page_dict.entries {
        let name = key.value.as_bytes();
        if name == b"Contents" || (name == b"Resources" && !extras.is_empty()) {
            continue;
        }
        let _ = write!(dict, " {} {}", key.value, value.value);
    }
    if !extras.is_empty() {
        let _ = write!(
            dict,
            " /Resources {}",
            merged_resources(&raw, &resources, &extras)
        );
    }
    let _ = write!(dict, " /Contents {} {} R >>", contents.num, contents.generation);
    rewriter.update.put(page_ref, dict.into_bytes());

    rewriter.update.finish(&raw, doc.bytes())
}

/// One stream after rewriting.
struct Rewritten {
    bytes: Vec<u8>,
    /// Names of filtered forms the stream now invokes, to be added to its resources.
    extras: Vec<(String, ObjRef)>,
}

struct Rewriter<'a> {
    raw: &'a RawDocument,
    cap: &'a CapturedPage,
    filter: &'a ContentFilter,
    update: Update,
}

impl Rewriter<'_> {
    fn dict_of(&self, obj: &Object) -> Option<Dict> {
        self.raw.resolve(obj).as_dict().cloned()
    }

    /// The decoded `/Contents` streams, skipping those that cannot be decoded exactly like the
    /// capture does, so indices agree with `StreamRef::PageContent`.
    fn page_streams(&self, page: &Dict) -> Vec<Vec<u8>> {
        let Some(contents) = page.get(b"Contents") else {
            return Vec::new();
        };
        let refs: Vec<ObjRef> = match &contents.value {
            Object::Ref(r) => match self.raw.object(*r).map(|o| o.value.value) {
                Some(Object::Array(items)) => items.iter().filter_map(|i| i.value.as_ref_obj()).collect(),
                Some(Object::Stream(_)) => vec![*r],
                _ => Vec::new(),
            },
            Object::Array(items) => items.iter().filter_map(|i| i.value.as_ref_obj()).collect(),
            _ => Vec::new(),
        };
        refs.into_iter().filter_map(|r| self.decoded(r)).collect()
    }

    /// The decoded bytes of the stream object `r`, when it is a stream that decodes completely.
    fn decoded(&self, r: ObjRef) -> Option<Vec<u8>> {
        let object = self.raw.object(r)?;
        let Object::Stream(stream) = &object.value.value else {
            return None;
        };
        let decoded = self.raw.decode_stream(stream).ok()?;
        decoded.complete.then_some(decoded.bytes)
    }

    /// `data` with the operators of `ops` rewritten.
    fn rewrite_stream(&mut self, data: &[u8], mut ops: Vec<&OpRecord>, resources: &Dict) -> Rewritten {
        ops.sort_by_key(|o| o.span.map(|s| s.0));
        let mut bytes = Vec::with_capacity(data.len());
        let mut extras = Vec::new();
        let mut at = 0;
        for op in ops {
            let Some((start, end)) = op.span else {
                continue;
            };
            if start < at || end < start || end > data.len() {
                continue;
            }
            let Some(replacement) = self.replacement(op, &data[start..end], resources, &mut extras) else {
                continue;
            };
            bytes.extend_from_slice(&data[at..start]);
            bytes.extend_from_slice(&replacement);
            at = end;
        }
        bytes.extend_from_slice(&data[at..]);
        Rewritten { bytes, extras }
    }

    /// What replaces the instruction `source` of `op`, or `None` to keep it.
    fn replacement(
        &mut self,
        op: &OpRecord,
        source: &[u8],
        resources: &Dict,
        extras: &mut Vec<(String, ObjRef)>,
    ) -> Option<Vec<u8>> {
        let allowed = self.filter.allows(op.ix);
        match op.operator.as_str() {
            "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s" if !allowed => Some(b"n".to_vec()),
            "Tj" | "TJ" | "'" | "\"" if !allowed => {
                let previous = op.gstate.text.render_mode;
                let mut out = format!("{} Tr ", neutral_mode(previous)).into_bytes();
                out.extend_from_slice(source);
                out.extend_from_slice(format!(" {previous} Tr").as_bytes());
                Some(out)
            }
            "sh" | "BI" if !allowed => Some(b" ".to_vec()),
            "Do" => match self.cap.xobjects.iter().find(|x| x.op == op.ix) {
                Some(scope) => {
                    let copy = self.rewrite_form(scope, resources)?;
                    let name = format!("PdfxrayForm{}", scope.ix);
                    extras.push((name.clone(), copy));
                    Some(format!("/{name} Do").into_bytes())
                }
                None => (!allowed).then(|| b" ".to_vec()),
            },
            _ => None,
        }
    }

    /// A filtered copy of the Form `XObject` drawn by `scope`; `None` when nothing inside it
    /// changes or it cannot be rewritten (then the original is invoked).
    fn rewrite_form(&mut self, scope: &XObjectScope, parent: &Dict) -> Option<ObjRef> {
        let object = self.raw.object(scope.obj?)?;
        let Object::Stream(stream) = &object.value.value else {
            return None;
        };
        let decoded = self.raw.decode_stream(stream).ok().filter(|d| d.complete)?;
        let own = stream.dict.get(b"Resources").and_then(|r| self.dict_of(&r.value));
        let resources = own.unwrap_or_else(|| parent.clone());
        let ops = self
            .cap
            .ops
            .iter()
            .filter(|o| o.xobject == Some(scope.ix))
            .collect();
        let done = self.rewrite_stream(&decoded.bytes, ops, &resources);
        if done.bytes == decoded.bytes {
            return None;
        }

        let mut entries = String::new();
        for (key, value) in &stream.dict.entries {
            let skip = matches!(
                key.value.as_bytes(),
                b"Length" | b"Filter" | b"DecodeParms" | b"DL" | b"F" | b"FFilter" | b"FDecodeParms"
            ) || (key.value.as_bytes() == b"Resources" && !done.extras.is_empty());
            if !skip {
                let _ = write!(entries, " {} {}", key.value, value.value);
            }
        }
        if !done.extras.is_empty() {
            let _ = write!(
                entries,
                " /Resources {}",
                merged_resources(self.raw, &resources, &done.extras)
            );
        }
        let copy = self.update.alloc();
        self.update.put(copy, stream_body(&entries, &done.bytes));
        Some(copy)
    }
}

/// The `/Resources` that apply to the page `page`: from the object they were found on (`from`,
/// see `PageInfo::resources_from`), else the page's own.
pub(crate) fn page_resources(raw: &RawDocument, from: Option<ObjRef>, page: &Dict) -> Dict {
    let owner = from.and_then(|r| raw.object(r));
    let holder = owner
        .as_ref()
        .and_then(|o| o.value.value.as_dict())
        .unwrap_or(page);
    holder
        .get(b"Resources")
        .and_then(|r| raw.resolve(&r.value).as_dict().cloned())
        .unwrap_or_default()
}

/// The syntax of `base` with `extras` added to its `/XObject` dictionary.
pub(crate) fn merged_resources(raw: &RawDocument, base: &Dict, extras: &[(String, ObjRef)]) -> String {
    let mut out = String::from("<<");
    for (key, value) in &base.entries {
        if key.value.as_bytes() != b"XObject" {
            let _ = write!(out, " {} {}", key.value, value.value);
        }
    }
    out.push_str(" /XObject <<");
    if let Some(existing) = base
        .get(b"XObject")
        .and_then(|x| raw.resolve(&x.value).as_dict().cloned())
    {
        for (key, value) in &existing.entries {
            let _ = write!(out, " {} {}", key.value, value.value);
        }
    }
    for (name, copy) in extras {
        let _ = write!(out, " /{name} {} {} R", copy.num, copy.generation);
    }
    out.push_str(" >> >>");
    out
}

/// The text rendering mode that paints nothing but keeps `mode`'s clipping behaviour.
const fn neutral_mode(mode: u8) -> u8 {
    if mode >= 4 { 7 } else { 3 }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn clipping_text_modes_stay_clipping_when_hidden() {
        let neutral: Vec<u8> = (0..8).map(neutral_mode).collect();

        assert_eq!(neutral, [3, 3, 3, 3, 7, 7, 7, 7]);
        // Smoke: mode 3 (already invisible) maps to itself, as does clip-only 7.
        assert_eq!((neutral_mode(3), neutral_mode(7)), (3, 7));
    }
}
