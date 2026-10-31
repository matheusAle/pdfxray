//! Raw objects as the debugger shows them: every indirect object parsed once at open, a
//! reverse-reference index ("referenced by"), and the Objects tree.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::OnceLock;

use pdfxray_raw::{EntryState, IndirectObject, ObjLocation, Object, RawDocument, XrefKind};

use crate::numeric::usize_from_u32;
use crate::{Document, NodeId, ObjRef, PropGroup, Props, TreeNode};

/// Longest value shown inline before it is cut.
const VALUE_PREVIEW: usize = 160;
/// Bytes of a decoded stream shown in the inspector.
const STREAM_PREVIEW: usize = 1024;

/// All indirect objects of the newest revision plus who references whom.
#[derive(Debug, Default)]
pub(crate) struct ObjectIndex {
    objects: BTreeMap<ObjRef, IndirectObject>,
    referrers: BTreeMap<ObjRef, BTreeSet<ObjRef>>,
    /// Byte spans of the directly stored objects, for [`ObjectIndex::containing`]; built on the
    /// first lookup, which most files never make.
    spans: OnceLock<SpanIndex>,
}

/// Direct objects ordered by where they start in the file.
#[derive(Debug, Default)]
struct SpanIndex {
    /// `(start, end, object)` sorted by `start`, then object.
    by_start: Vec<(usize, usize, ObjRef)>,
    /// `reach[i]` is the largest `end` among `by_start[..=i]`: once it is not past a byte, no
    /// earlier entry can hold that byte.
    reach: Vec<usize>,
}

impl SpanIndex {
    fn new(objects: &BTreeMap<ObjRef, IndirectObject>) -> Self {
        Self::from_spans(
            objects
                .values()
                .filter(|o| matches!(o.location, ObjLocation::Direct { .. }))
                .map(|o| (o.span.start, o.span.end, o.r))
                .collect(),
        )
    }

    fn from_spans(mut by_start: Vec<(usize, usize, ObjRef)>) -> Self {
        by_start.sort_unstable_by_key(|&(start, _, r)| (start, r));
        let reach = by_start
            .iter()
            .scan(0, |top, &(_, end, _)| {
                *top = (*top).max(end);
                Some(*top)
            })
            .collect();
        Self { by_start, reach }
    }

    /// The lowest-numbered object whose span holds `offset`.
    fn containing(&self, offset: usize) -> Option<ObjRef> {
        let candidates = self.by_start.partition_point(|&(start, ..)| start <= offset);
        let mut found: Option<ObjRef> = None;
        for i in (0..candidates).rev() {
            if self.reach.get(i).is_none_or(|&reach| reach <= offset) {
                break;
            }
            if let Some(&(_, end, r)) = self.by_start.get(i)
                && offset < end
                && found.is_none_or(|best| r < best)
            {
                found = Some(r);
            }
        }
        found
    }
}

impl ObjectIndex {
    pub(crate) fn build(raw: &RawDocument) -> Self {
        let objects: BTreeMap<_, _> = raw
            .object_refs()
            .into_iter()
            .filter_map(|r| raw.object(r).map(|o| (r, o)))
            .collect();
        let mut referrers: BTreeMap<ObjRef, BTreeSet<ObjRef>> = BTreeMap::new();
        for (from, obj) in &objects {
            visit_refs(&obj.value.value, &mut |to| {
                referrers.entry(to).or_default().insert(*from);
            });
        }
        Self {
            objects,
            referrers,
            spans: OnceLock::new(),
        }
    }

    pub(crate) fn get(&self, r: ObjRef) -> Option<&IndirectObject> {
        self.objects.get(&r)
    }

    /// The objects whose own value refers to `r`, in reference order.
    pub(crate) fn referrers(&self, r: ObjRef) -> impl Iterator<Item = ObjRef> + '_ {
        self.referrers.get(&r).into_iter().flatten().copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.objects.len()
    }

    /// The object whose direct span contains byte `offset`, if any.
    pub(crate) fn containing(&self, offset: usize) -> Option<ObjRef> {
        self.spans
            .get_or_init(|| SpanIndex::new(&self.objects))
            .containing(offset)
    }

    pub(crate) fn tree(&self, raw: &RawDocument) -> Vec<TreeNode> {
        let trailer = TreeNode::leaf(NodeId::Trailer, "Trailer")
            .with_detail(raw.trailer().map(|t| keys_summary(t.keys())).unwrap_or_default());
        let revisions = raw
            .revisions()
            .iter()
            .map(|rev| {
                let kind = match rev.xref.kind {
                    XrefKind::Table => "xref table".to_owned(),
                    XrefKind::Stream(r) => format!("xref stream {r}"),
                };
                TreeNode::leaf(
                    NodeId::Revision(u32::try_from(rev.index).unwrap_or(u32::MAX)),
                    format!("Revision {}", rev.index + 1),
                )
                .with_detail(format!("{kind}, {} entries", rev.xref.entries.len()))
            })
            .collect();
        let objects = self
            .objects
            .values()
            .map(|o| {
                TreeNode::leaf(NodeId::Object(o.r), format!("{} {} obj", o.r.num, o.r.generation))
                    .with_detail(summary(&o.value.value))
            })
            .collect();
        vec![
            trailer,
            TreeNode::leaf(NodeId::Group(0), "Revisions")
                .with_detail(format!("{}", raw.revisions().len()))
                .with_children(revisions),
            TreeNode::leaf(NodeId::Group(1), "Objects")
                .with_detail(format!("{}", self.objects.len()))
                .with_children(objects),
        ]
    }

    pub(crate) fn object_props(&self, raw: &RawDocument, r: ObjRef, diagnostics: &[NodeId]) -> Option<Props> {
        let obj = self.objects.get(&r)?;
        Some(self.props_of(raw, obj, true, diagnostics))
    }

    /// The inspector rows of `obj`, which may be an older definition than the one indexed. Only
    /// the newest definition has "Referenced by": the reverse index covers the newest revision.
    pub(crate) fn props_of(
        &self,
        raw: &RawDocument,
        obj: &IndirectObject,
        with_referrers: bool,
        diagnostics: &[NodeId],
    ) -> Props {
        let r = obj.r;
        let mut header = PropGroup::new("Object").row("Reference", r.to_string()).link(
            "Revision",
            obj.revision.to_string(),
            NodeId::Revision(u32::try_from(obj.revision).unwrap_or(u32::MAX)),
        );
        header = match obj.location {
            ObjLocation::Direct { offset } => header.row("Offset", offset.to_string()).row(
                "Bytes",
                format!("{}..{} ({} bytes)", obj.span.start, obj.span.end, obj.span.len()),
            ),
            ObjLocation::InObjectStream { stream, index } => header.link(
                "In object stream",
                format!("{stream} #{index}"),
                NodeId::Object(stream),
            ),
        };
        header = header.row("Type", type_name(&obj.value.value));
        let mut groups = vec![header];
        groups.push(value_group(&obj.value.value));
        if let Object::Stream(s) = &obj.value.value {
            groups.push(stream_group(raw, s));
        }
        if let Some(from) = self.referrers.get(&r).filter(|_| with_referrers) {
            let mut g = PropGroup::new("Referenced by");
            for f in from {
                g = g.link(
                    f.to_string(),
                    self.objects
                        .get(f)
                        .map(|o| summary(&o.value.value))
                        .unwrap_or_default(),
                    NodeId::Object(*f),
                );
            }
            groups.push(g);
        }
        if !diagnostics.is_empty() {
            let mut g = PropGroup::new("Diagnostics");
            for d in diagnostics {
                g = g.link(d.to_string(), "", d.clone());
            }
            groups.push(g);
        }
        Props {
            id: NodeId::Object(r),
            title: format!("{} {} obj", r.num, r.generation),
            kind: "object",
            groups,
        }
    }

    pub(crate) fn trailer_props(raw: &RawDocument) -> Option<Props> {
        let trailer = raw.trailer()?;
        let mut groups = vec![value_group(&Object::Dict(trailer.clone()))];
        if let Some(enc) = raw.encryption() {
            groups.push(
                PropGroup::new("Encryption").row("Encrypt", truncate(&Object::Dict(enc.clone()).to_string())),
            );
        }
        Some(Props {
            id: NodeId::Trailer,
            title: "Trailer".into(),
            kind: "trailer",
            groups,
        })
    }

    pub(crate) fn revision_props(raw: &RawDocument, index: u32) -> Option<Props> {
        let rev = raw.revisions().get(usize_from_u32(index))?;
        let mut xref = PropGroup::new("Cross-reference").row(
            "Kind",
            match rev.xref.kind {
                XrefKind::Table => "table".to_owned(),
                XrefKind::Stream(r) => format!("stream {r}"),
            },
        );
        xref = xref.row("Bytes", format!("{}..{}", rev.xref.span.start, rev.xref.span.end));
        if let Some(sx) = &rev.startxref {
            xref = xref.row("startxref", sx.offset.to_string());
        }
        if let Some(prev) = rev.trailer.as_ref().and_then(|t| t.get_int(b"Prev")) {
            xref = xref.row("/Prev", prev.to_string());
        }
        let mut entries = PropGroup::new("Entries");
        for e in &rev.xref.entries {
            let r = ObjRef::new(e.num, e.generation);
            entries = match e.state {
                EntryState::InUse { offset } => {
                    entries.link(r.to_string(), format!("in use @ {offset}"), NodeId::Object(r))
                }
                EntryState::Compressed { stream, index } => entries.link(
                    r.to_string(),
                    format!("in object stream {stream} #{index}"),
                    NodeId::Object(r),
                ),
                EntryState::Free { next } => entries.row(r.to_string(), format!("free, next {next}")),
            };
        }
        Some(Props {
            id: NodeId::Revision(index),
            title: format!("Revision {}", index + 1),
            kind: "revision",
            groups: vec![xref, entries],
        })
    }
}

/// The bytes of a stream object: as stored in the file and after its filters ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamBytes {
    /// The encoded bytes, exactly as the file holds them (`Document::bytes()[file_range]`).
    pub raw: Vec<u8>,
    /// The bytes after the filter chain ran. `None` when decoding could not start (the data lies
    /// outside the file, or `/Filter` is not a name or an array of names); otherwise whatever was
    /// produced, see [`StreamBytes::complete`].
    pub decoded: Option<Vec<u8>>,
    /// The filters the dictionary declares, in order, as written and without the slash
    /// (`FlateDecode`, `Fl`, ...).
    pub filters: Vec<String>,
    /// False when decoding stopped early (an unsupported or damaged filter, an encrypted file,
    /// the output limit) or could not start; `decoded` is then partial or absent.
    pub complete: bool,
    /// Where `raw` sits in the file: the bytes between `stream` and `endstream`, using the
    /// actual length rather than the declared `/Length`.
    pub file_range: Range<usize>,
}

impl Document {
    /// The stream data of `r` in the newest revision; `None` when `r` is not a stream.
    pub fn stream_bytes(&self, r: ObjRef) -> Option<StreamBytes> {
        self.stream_bytes_at(r, self.latest_revision()?)
    }

    /// The stream data of `r` as of `revision` (later revisions are ignored).
    pub fn stream_bytes_at(&self, r: ObjRef, revision: u32) -> Option<StreamBytes> {
        let obj = self.raw.object_in_revision(r, usize_from_u32(revision))?;
        let Object::Stream(stream) = &obj.value.value else {
            return None;
        };
        let file_range = stream.data.start..stream.data.end;
        let raw = self.raw.stream_data(stream)?.to_vec();
        let (decoded, filters, complete) = match self.raw.decode_stream(stream) {
            Ok(d) => (
                Some(d.bytes),
                d.filters.iter().map(pdfxray_raw::Name::to_string_lossy).collect(),
                d.complete,
            ),
            Err(_) => (None, Vec::new(), false),
        };
        Some(StreamBytes {
            raw,
            decoded,
            filters,
            complete,
            file_range,
        })
    }

    /// Where `r` is written in the file, from `num gen obj` to `endobj`, in the newest revision.
    /// `None` when the object does not exist or lives inside an object stream, which has no bytes
    /// of its own.
    pub fn object_range(&self, r: ObjRef) -> Option<Range<usize>> {
        self.object_range_at(r, self.latest_revision()?)
    }

    /// Like [`Document::object_range`], for the definition of `r` as of `revision`.
    pub fn object_range_at(&self, r: ObjRef, revision: u32) -> Option<Range<usize>> {
        let obj = self.raw.object_in_revision(r, usize_from_u32(revision))?;
        matches!(obj.location, ObjLocation::Direct { .. }).then_some(obj.span.start..obj.span.end)
    }

    /// One line describing `r` as of `revision`, as the Objects tree shows it: `/Type /Page`,
    /// `stream /Subtype /Image`, `[4 items]`. `None` when the object does not exist then.
    pub fn object_summary_at(&self, r: ObjRef, revision: u32) -> Option<String> {
        let obj = self.raw.object_in_revision(r, usize_from_u32(revision))?;
        Some(summary(&obj.value.value))
    }

    /// Inspector properties of `r` as of `revision`: what the file said after that revision was
    /// written. The newest revision gives the same as [`Document::node`]; an older one has no
    /// "Referenced by" and no diagnostics, which are computed for the newest file only.
    pub fn object_props_at(&self, r: ObjRef, revision: u32) -> Option<Props> {
        if Some(revision) == self.latest_revision() {
            return self.node(&NodeId::Object(r));
        }
        let obj = self.raw.object_in_revision(r, usize_from_u32(revision))?;
        Some(self.objects.props_of(&self.raw, &obj, false, &[]))
    }

    fn latest_revision(&self) -> Option<u32> {
        u32::try_from(self.raw.revisions().len().checked_sub(1)?).ok()
    }
}

fn visit_refs(obj: &Object, f: &mut impl FnMut(ObjRef)) {
    match obj {
        Object::Ref(r) => f(*r),
        Object::Array(items) => items.iter().for_each(|i| visit_refs(&i.value, f)),
        Object::Dict(d) => d.entries.iter().for_each(|(_, v)| visit_refs(&v.value, f)),
        Object::Stream(s) => s.dict.entries.iter().for_each(|(_, v)| visit_refs(&v.value, f)),
        _ => {}
    }
}

fn type_name(obj: &Object) -> &'static str {
    match obj {
        Object::Null => "null",
        Object::Bool(_) => "boolean",
        Object::Int(_) => "integer",
        Object::Real(_) => "real",
        Object::String(_) => "string",
        Object::Name(_) => "name",
        Object::Array(_) => "array",
        Object::Dict(_) => "dictionary",
        Object::Stream(_) => "stream",
        Object::Ref(_) => "reference",
    }
}

/// One-line description used in trees: `/Type /Page`, `stream /Subtype /Image`, `[4 items]`.
pub(crate) fn summary(obj: &Object) -> String {
    let (prefix, dict) = match obj {
        Object::Dict(d) => ("", d),
        Object::Stream(s) => ("stream ", &s.dict),
        Object::Array(a) => return format!("[{} items]", a.len()),
        other => return truncate(&other.to_string()),
    };
    let ty = dict.get_name(b"Type").map(|n| format!("/Type {n}"));
    let sub = dict
        .get_name(b"Subtype")
        .or_else(|| dict.get_name(b"S"))
        .map(ToString::to_string);
    let parts: Vec<String> = [ty, sub].into_iter().flatten().collect();
    if parts.is_empty() {
        format!("{prefix}<< {} keys >>", dict.len())
    } else {
        format!("{prefix}{}", parts.join(" "))
    }
}

fn value_group(obj: &Object) -> PropGroup {
    let dict = match obj {
        Object::Dict(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    };
    let mut g = PropGroup::new("Value");
    match (dict, obj) {
        (Some(d), _) => {
            for (k, v) in &d.entries {
                g = add_value_row(g, k.value.to_string(), &v.value);
            }
        }
        (None, Object::Array(items)) => {
            for (i, v) in items.iter().enumerate() {
                g = add_value_row(g, format!("[{i}]"), &v.value);
            }
        }
        (None, other) => g = g.row("Value", truncate(&other.to_string())),
    }
    g
}

fn add_value_row(g: PropGroup, key: String, v: &Object) -> PropGroup {
    match v {
        Object::Ref(r) => g.link(key, r.to_string(), NodeId::Object(*r)),
        other => g.row(key, truncate(&other.to_string())),
    }
}

fn stream_group(raw: &RawDocument, s: &pdfxray_raw::Stream) -> PropGroup {
    let mut g = PropGroup::new("Stream")
        .row("Encoded bytes", s.data.len().to_string())
        .row(
            "Declared /Length",
            s.declared_length
                .map_or_else(|| "missing".to_owned(), |l| l.to_string()),
        );
    match raw.decode_stream(s) {
        Ok(d) => {
            let filters: Vec<String> = d.filters.iter().map(ToString::to_string).collect();
            g = g
                .row(
                    "Filters",
                    if filters.is_empty() {
                        "none".to_owned()
                    } else {
                        filters.join(" ")
                    },
                )
                .row("Decoded bytes", d.bytes.len().to_string())
                .row("Complete", if d.complete { "yes" } else { "no" }.to_owned());
            if d.complete && is_texty(&d.bytes) {
                g = g.row("Preview", preview(&d.bytes));
            }
        }
        Err(e) => g = g.row("Decode error", e.to_string()),
    }
    g
}

fn is_texty(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(STREAM_PREVIEW)];
    let printable = sample
        .iter()
        .filter(|b| b.is_ascii_graphic() || b.is_ascii_whitespace())
        .count();
    !sample.is_empty() && printable * 10 >= sample.len() * 9
}

fn preview(bytes: &[u8]) -> String {
    let cut = &bytes[..bytes.len().min(STREAM_PREVIEW)];
    let mut s = String::from_utf8_lossy(cut).into_owned();
    if bytes.len() > STREAM_PREVIEW {
        s.push('…');
    }
    s
}

fn keys_summary<'a>(keys: impl Iterator<Item = &'a pdfxray_raw::Name>) -> String {
    keys.map(ToString::to_string).collect::<Vec<_>>().join(" ")
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= VALUE_PREVIEW {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(VALUE_PREVIEW).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pdfxray_raw::RawDocument;
    use pretty_assertions::assert_eq;

    use super::*;

    /// The lookup the index replaced: first direct object in reference order that holds `offset`.
    fn linear(spans: &[(usize, usize, ObjRef)], offset: usize) -> Option<ObjRef> {
        let mut sorted = spans.to_vec();
        sorted.sort_by_key(|&(_, _, r)| r);
        sorted
            .iter()
            .find(|&&(start, end, _)| start <= offset && offset < end)
            .map(|&(_, _, r)| r)
    }

    fn r(num: u32) -> ObjRef {
        ObjRef::new(num, 0)
    }

    #[test]
    fn containing_agrees_with_a_linear_scan_over_overlapping_spans() {
        let spans = vec![
            (0, 10, r(7)),
            (5, 8, r(3)),
            (8, 8, r(1)),
            (12, 40, r(9)),
            (15, 20, r(2)),
            (15, 16, r(5)),
            (60, 70, r(4)),
        ];
        let index = SpanIndex::from_spans(spans.clone());

        for offset in 0..80 {
            assert_eq!(
                index.containing(offset),
                linear(&spans, offset),
                "offset {offset}"
            );
        }
        assert_eq!(
            index.containing(6),
            Some(r(3)),
            "the lower-numbered of two holders wins"
        );
        assert_eq!(index.containing(8), Some(r(7)), "an empty span holds nothing");
        assert_eq!(index.containing(11), None);
        assert_eq!(index.containing(39), Some(r(9)));
    }

    #[test]
    fn containing_finds_the_object_around_every_byte_of_real_files() {
        for name in [
            "minimal-one-page",
            "incremental-update",
            "xref-stream-objstm",
            "struct-rich",
        ] {
            let bytes = std::fs::read(pdfxray_fixtures::pdf_path(name)).unwrap();
            let len = bytes.len();
            let raw = RawDocument::parse(Arc::new(bytes)).unwrap();
            let objects = ObjectIndex::build(&raw);
            let spans: Vec<_> = objects
                .objects
                .values()
                .filter(|o| matches!(o.location, ObjLocation::Direct { .. }))
                .map(|o| (o.span.start, o.span.end, o.r))
                .collect();

            let mismatches: Vec<_> = (0..len + 8)
                .filter(|&at| objects.containing(at) != linear(&spans, at))
                .collect();

            assert_eq!(mismatches, Vec::<usize>::new(), "{name}");
            assert!(!spans.is_empty(), "{name} has direct objects");
            let (start, end, object) = spans[0];
            assert_eq!(objects.containing(start), Some(object), "{name}: first byte");
            assert_eq!(
                objects.containing(end),
                linear(&spans, end),
                "{name}: one past the end"
            );
        }
    }
}
