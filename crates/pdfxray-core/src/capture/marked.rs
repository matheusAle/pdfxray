//! The marked-content tracker: every `BMC`/`BDC` sequence of the page, the ones open now, and
//! the boxes of what was painted inside them.

use hayro_syntax::object::{Array, Dict, Name, Object};
use hayro_syntax::page::Resources;

use super::object::{decode_text_string, maybe_ref_syntax};
use super::ops::{RawOp, name_at};
use super::types::{ArtifactInfo, MarkedSeq, StreamRef};
use crate::Rect;
use crate::numeric::usize_from_u32;

/// The sequences seen so far and the stack of open ones.
#[derive(Default)]
pub(super) struct MarkedContent {
    seqs: Vec<MarkedSeq>,
    /// Open sequences, outermost first, as indices into `seqs`.
    open: Vec<u32>,
    /// Open sequences below this depth belong to an invoking stream; `EMC` cannot close them.
    base: usize,
}

/// Where the open stack stood when a Form `XObject`'s stream started.
pub(super) struct FormMark {
    base: usize,
    depth: usize,
}

/// What a `BMC`/`BDC` says about the sequence it opens.
pub(super) struct Opening {
    tag: String,
    mcid: Option<i32>,
    properties: Vec<(String, String)>,
    property_resource: Option<String>,
    actual_text: Option<String>,
    alt: Option<String>,
    lang: Option<String>,
    artifact: Option<ArtifactInfo>,
}

impl Opening {
    /// Reads the tag and property list of a `BMC`/`BDC`, resolving a named list in
    /// `resources`.
    pub(super) fn read(op: &RawOp<'_>, resources: &Resources<'_>) -> Self {
        let tag = name_at(op, 0).map(|n| n.as_str().to_owned()).unwrap_or_default();
        let mut opening = Self {
            tag,
            mcid: None,
            properties: Vec::new(),
            property_resource: None,
            actual_text: None,
            alt: None,
            lang: None,
            artifact: None,
        };
        let named;
        let dict: Option<&Dict<'_>> = match op.operands.get(1) {
            Some(Object::Dict(d)) => Some(d),
            Some(Object::Name(n)) => {
                opening.property_resource = Some(n.as_str().to_owned());
                named = match resources.properties.get::<Object<'_>>(n.as_ref()) {
                    Some(Object::Dict(d)) => Some(d),
                    Some(Object::Stream(s)) => Some(s.dict().clone()),
                    _ => None,
                };
                named.as_ref()
            }
            _ => None,
        };
        if let Some(d) = dict {
            opening.properties = d
                .entries()
                .map(|(k, v)| (k.as_str().to_owned(), maybe_ref_syntax(&v)))
                .collect();
            opening.properties.sort();
            opening.mcid = d.get::<i32>(b"MCID");
            let text = |key: &[u8]| {
                d.get::<hayro_syntax::object::String<'_>>(key)
                    .map(|s| decode_text_string(s.as_bytes()))
            };
            opening.actual_text = text(b"ActualText");
            opening.alt = text(b"Alt");
            opening.lang = text(b"Lang");
        }
        if opening.tag == "Artifact" {
            opening.artifact = Some(artifact_info(dict));
        }
        opening
    }
}

fn artifact_info(dict: Option<&Dict<'_>>) -> ArtifactInfo {
    let name_of = |key: &[u8]| {
        dict.and_then(|d| d.get::<Name<'_>>(key))
            .map(|n| n.as_str().to_owned())
    };
    ArtifactInfo {
        kind: name_of(b"Type"),
        subtype: name_of(b"Subtype"),
        bbox: dict
            .and_then(|d| d.get::<[f64; 4]>(b"BBox"))
            .map(|b| Rect::new(b[0], b[1], b[2], b[3])),
        attached: dict
            .and_then(|d| d.get::<Array<'_>>(b"Attached"))
            .map(|a| a.iter::<Name<'_>>().map(|n| n.as_str().to_owned()).collect())
            .unwrap_or_default(),
    }
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

impl MarkedContent {
    /// Open sequences, outermost first.
    pub(super) fn open(&self) -> &[u32] {
        &self.open
    }

    /// `BMC`/`BDC` at operator `op` of `stream`.
    pub(super) fn begin(&mut self, opening: Opening, op: u32, stream: StreamRef) {
        let ix = u32::try_from(self.seqs.len()).unwrap_or(u32::MAX);
        let Opening {
            tag,
            mcid,
            properties,
            property_resource,
            actual_text,
            alt,
            lang,
            artifact,
        } = opening;
        self.seqs.push(MarkedSeq {
            ix,
            tag,
            mcid,
            properties,
            property_resource,
            actual_text,
            alt,
            lang,
            artifact,
            begin_op: op,
            end_op: None,
            parent: self.open.last().copied(),
            stream,
            bbox: None,
        });
        self.open.push(ix);
    }

    /// `EMC` at operator `op`. Returns false, and changes nothing, when no sequence of this
    /// stream is open.
    pub(super) fn end(&mut self, op: u32) -> bool {
        if self.open.len() <= self.base {
            return false;
        }
        if let Some(seq) = self.open.pop()
            && let Some(s) = self.seqs.get_mut(usize_from_u32(seq))
        {
            s.end_op = Some(op);
        }
        true
    }

    /// Grows the box of every open sequence by `bbox`, painted inside them.
    pub(super) fn painted(&mut self, bbox: Rect) {
        for &m in &self.open {
            if let Some(seq) = self.seqs.get_mut(usize_from_u32(m)) {
                seq.bbox = Some(seq.bbox.map_or(bbox, |b| union(b, bbox)));
            }
        }
    }

    /// Starts a Form `XObject`'s stream: the form cannot close the invoker's sequences.
    pub(super) fn enter_form(&mut self) -> FormMark {
        let depth = self.open.len();
        FormMark {
            base: std::mem::replace(&mut self.base, depth),
            depth,
        }
    }

    /// Ends a Form `XObject`'s stream: sequences it left open are closed implicitly (their
    /// `end_op` stays `None`).
    pub(super) fn leave_form(&mut self, FormMark { base, depth }: FormMark) {
        self.open.truncate(depth);
        self.base = base;
    }

    /// Every sequence, in order of its opening operator.
    pub(super) fn into_sequences(self) -> Vec<MarkedSeq> {
        self.seqs
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn opening(tag: &str) -> Opening {
        Opening {
            tag: tag.to_owned(),
            mcid: None,
            properties: Vec::new(),
            property_resource: None,
            actual_text: None,
            alt: None,
            lang: None,
            artifact: None,
        }
    }

    const PAGE: StreamRef = StreamRef::PageContent { index: 0 };

    #[test]
    fn nested_sequences_record_parents_ends_and_painted_boxes() {
        let mut mc = MarkedContent::default();
        mc.begin(opening("Sect"), 0, PAGE);
        mc.begin(opening("P"), 1, PAGE);
        mc.painted(Rect::new(10.0, 10.0, 20.0, 20.0));
        let inner_closed = mc.end(3);
        mc.painted(Rect::new(0.0, 30.0, 5.0, 40.0));

        let open = mc.open().to_vec();
        let seqs = mc.into_sequences();

        assert!(inner_closed);
        assert_eq!(open, [0]);
        assert_eq!(
            seqs.iter()
                .map(|s| (s.tag.as_str(), s.parent, s.begin_op, s.end_op, s.bbox))
                .collect::<Vec<_>>(),
            [
                ("Sect", None, 0, None, Some(Rect::new(0.0, 10.0, 20.0, 40.0))),
                ("P", Some(0), 1, Some(3), Some(Rect::new(10.0, 10.0, 20.0, 20.0))),
            ]
        );
        // smoke: indices follow opening order
        assert_eq!(seqs.iter().map(|s| s.ix).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn a_form_cannot_close_the_invokers_sequence_and_its_own_close_on_leaving() {
        let mut mc = MarkedContent::default();
        mc.begin(opening("Span"), 0, PAGE);
        let mark = mc.enter_form();
        mc.begin(opening("Figure"), 2, PAGE);

        let closed_own = mc.end(3);
        let closed_outer = mc.end(4);
        mc.begin(opening("Left-open"), 5, PAGE);
        mc.leave_form(mark);

        assert_eq!((closed_own, closed_outer), (true, false));
        assert_eq!(mc.open(), [0]);
        assert!(mc.end(6), "the invoker closes its own sequence after the form");
        // smoke: the sequence left open by the form keeps no end
        assert_eq!(mc.into_sequences()[2].end_op, None);
    }
}
