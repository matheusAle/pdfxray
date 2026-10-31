//! What the object view, the References drawer and the inspector know about one object.
//!
//! [`ObjectFacts`] is built once per selected object (and viewed revision) from the core's
//! answers, so the panels only draw it: no PDF logic lives in the widgets.
//!
//! This module holds the facts and [`build`] / [`build_trailer`]. How an object is named (kind,
//! title, subtitle) is in `naming`; the dictionary, operator and text lines of the decoded view
//! are in `lines`; the number, size and reference formatting the panels share is in `format`.

mod format;
mod lines;
mod naming;

use std::collections::HashMap;
use std::ops::Range;

use pdfxray_core::{DiagnosticInfo, Document, NodeId, ObjRef, PageIx, Props, UsedResource, Visual};

pub use self::format::{human_bytes, key_of, short_label, split_refs, thousands};
use self::lines::{dict_lines, op_lines, text_body};
use self::naming::{classify, subtitle_of, title_of, type_line};
use crate::state::LoadedDocument;

/// What an object is, as far as the debugger tells the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// A page's content stream.
    ContentStream,
    /// A Form `XObject`.
    Form,
    /// An image `XObject`.
    Image,
    /// Any other stream.
    Stream,
    /// A page dictionary.
    Page,
    /// A font dictionary.
    Font,
    /// A dictionary of any other kind.
    Dictionary,
    /// An array.
    Array,
    /// A number, string, name, boolean or null.
    Scalar,
    /// The trailer dictionary.
    Trailer,
}

/// A piece of a value: plain text, or a reference that navigates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    /// Text as the file writes it.
    Text(String),
    /// An indirect reference.
    Ref(ObjRef),
}

/// How a dictionary line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// `<<`, `>>`, `[`, `]`.
    Bracket,
    /// A key and its value, or an array item.
    Entry,
    /// The `stream` keyword.
    Keyword,
}

/// One line of the dictionary as the object view shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictLine {
    /// What the line is.
    pub kind: LineKind,
    /// The key with its slash (`/Length`) of a dictionary entry.
    pub key: Option<String>,
    /// The value or the bracket.
    pub pieces: Vec<Piece>,
}

/// One drawing instruction of a content stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpLine {
    /// Execution index on the page (`#5`).
    pub ix: u32,
    /// How deep the instruction sits in `q`, `BT` and marked-content nesting.
    pub indent: u8,
    /// The operator token.
    pub operator: String,
    /// The operands, in PDF syntax.
    pub operands: String,
    /// The plain-language note.
    pub note: String,
}

/// An object that refers to the one shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedBy {
    /// The referring object.
    pub obj: ObjRef,
    /// The key path of the reference: `/Contents`, `/Kids[2]`.
    pub path: String,
    /// What the referring object is: `Page 1`, `Pages`, `Font`.
    pub label: String,
}

/// Facts about the data of a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamFacts {
    /// The filters, in order, without slashes.
    pub filters: Vec<String>,
    /// Size of the stream as stored.
    pub encoded_len: usize,
    /// Size after the filters ran, when they could run.
    pub decoded_len: Option<usize>,
    /// Whether decoding went all the way.
    pub complete: bool,
    /// Where the encoded bytes sit in the file.
    pub file_range: Range<usize>,
    /// The `/Length` the dictionary declares.
    pub declared_length: Option<i64>,
}

/// Where the object is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// First byte of `n g obj`, when the object has bytes of its own.
    pub offset: Option<usize>,
    /// The object stream that holds it, when it has none.
    pub in_object_stream: Option<ObjRef>,
    /// False when the xref points somewhere else than where the object really starts.
    pub index_ok: bool,
}

/// Everything shown about one object.
#[derive(Debug, Clone)]
pub struct ObjectFacts {
    /// The object; `None` for the trailer.
    pub object: Option<ObjRef>,
    /// The revision being viewed.
    pub revision: u32,
    /// True when `revision` is not the newest: nothing here can change, and the cross-references
    /// are not tracked.
    pub read_only: bool,
    /// What it is.
    pub kind: ObjectKind,
    /// Plain name of the kind: `Content stream`.
    pub title: String,
    /// `stream · page drawing`.
    pub type_line: String,
    /// What the header says below the title.
    pub subtitle: String,
    /// `/Contents of page 1`.
    pub relation: Option<String>,
    /// The page whose content this is, when it is a page content stream.
    pub page: Option<PageIx>,
    /// The bytes from `n g obj` to `endobj`.
    pub range: Option<Range<usize>>,
    /// The stream, when it is one.
    pub stream: Option<StreamFacts>,
    /// The dictionary (or value) lines.
    pub lines: Vec<DictLine>,
    /// The drawing instructions, for streams that run operators.
    pub ops: Option<Vec<OpLine>>,
    /// The decoded text of other text streams, line by line.
    pub text: Option<TextBody>,
    /// The revision that wrote this definition.
    pub defined_in: u32,
    /// True when a later revision wrote it again.
    pub changed_later: bool,
    /// Problems the reader found in this object.
    pub diagnostics: Vec<DiagnosticInfo>,
    /// Where it is written.
    pub position: Position,
    /// The path from the trailer: the objects to follow, outermost first, ending at this one.
    pub crumbs: Vec<ObjRef>,
    /// Objects that refer to it.
    pub used_by: Vec<UsedBy>,
    /// Resources its operators select.
    pub uses: Vec<UsedResource>,
    /// What a picture of it shows, when it has one (an image, a font, a form, a colour space, a
    /// shading, a pattern, a page). Previews are of the newest revision only.
    pub visual: Option<Visual>,
}

/// Decoded text of a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBody {
    /// The lines, up to the display limit.
    pub lines: Vec<String>,
    /// How many lines the stream really has.
    pub total: usize,
}

impl ObjectFacts {
    /// The diagnostic about `/Length`, when the object has one.
    pub fn length_diagnostic(&self) -> Option<&DiagnosticInfo> {
        self.diagnostics.iter().find(|d| key_of(d.code).is_some())
    }

    /// The diagnostics that are not tied to a key of the dictionary.
    pub fn loose_diagnostics(&self) -> impl Iterator<Item = &DiagnosticInfo> {
        self.diagnostics.iter().filter(|d| key_of(d.code).is_none())
    }
}

/// Builds the facts of `r` as of `revision` (`None`: the newest). `None` when it does not exist.
pub fn build(loaded: &LoadedDocument, r: ObjRef, revision: Option<u32>) -> Option<ObjectFacts> {
    let doc = &loaded.doc;
    let latest = u32::try_from(loaded.info.revision_count.saturating_sub(1)).unwrap_or(u32::MAX);
    let at = revision.unwrap_or(latest);
    let read_only = at != latest;
    let props = doc.object_props_at(r, at)?;
    let type_word = props.get("Object", "Type").unwrap_or("").to_owned();
    let value_rows = value_rows(&props);
    let value_of = |key: &str| value_rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let subtype = value_of("/Subtype").map(str::to_owned);
    let object_type = value_of("/Type").map(str::to_owned);

    let range = doc.object_range_at(r, at);
    let stream_bytes = doc.stream_bytes_at(r, at);
    let declared_length = props
        .get("Stream", "Declared /Length")
        .and_then(|v| v.parse().ok());
    let (refs_used_by, refs_uses) = if read_only {
        (Vec::new(), Vec::new())
    } else {
        let refs = doc.object_refs(r);
        (refs.used_by, refs.uses)
    };
    let page_of: HashMap<ObjRef, PageIx> = (0..doc.page_count())
        .filter_map(|ix| {
            let ix = PageIx::try_from(ix).ok()?;
            Some((doc.page(ix)?.obj?, ix))
        })
        .collect();
    let contents = content_stream_of(&page_of, &refs_used_by);
    let is_stream = type_word == "stream";
    let kind = classify(
        &type_word,
        contents.is_some(),
        subtype.as_deref(),
        object_type.as_deref(),
    );

    let diagnostics: Vec<DiagnosticInfo> = if read_only {
        Vec::new()
    } else {
        loaded
            .info
            .diagnostics
            .iter()
            .filter(|d| d.object == Some(r))
            .cloned()
            .collect()
    };
    let in_object_stream = props.groups.iter().find_map(|g| {
        g.rows
            .iter()
            .find(|row| row.key == "In object stream")
            .and_then(|row| match row.link {
                Some(NodeId::Object(stream)) => Some(stream),
                _ => None,
            })
    });
    let position = Position {
        offset: range.as_ref().map(|range| range.start),
        in_object_stream,
        index_ok: !diagnostics.iter().any(|d| d.code == "xref-offset-wrong"),
    };

    let ops = (!read_only && is_stream)
        .then(|| op_lines(doc, r, contents.as_ref(), kind == ObjectKind::Form))
        .flatten();
    let text = match (&ops, &stream_bytes) {
        (None, Some(bytes)) => bytes.decoded.as_deref().and_then(text_body),
        _ => None,
    };
    let stream = stream_bytes.map(|bytes| StreamFacts {
        filters: bytes.filters,
        encoded_len: bytes.raw.len(),
        decoded_len: bytes.decoded.as_ref().map(Vec::len),
        complete: bytes.complete,
        file_range: bytes.file_range,
        declared_length,
    });

    let page = contents.as_ref().map(|(page, _, _)| *page);
    let relation = contents
        .as_ref()
        .map(|(page, _, path)| format!("{path} of page {}", page + 1))
        .or_else(|| {
            refs_used_by
                .first()
                .map(|(from, path)| format!("{path} of {from}"))
        });
    let changed_later = !read_only && doc.changed_in_later_revision(r);
    let defined_in = props
        .groups
        .iter()
        .find(|g| g.name == "Object")
        .and_then(|g| g.rows.iter().find(|row| row.key == "Revision"))
        .and_then(|row| row.value.parse().ok())
        .unwrap_or(at);

    let lines = dict_lines(&value_rows, &type_word);
    let title = title_of(kind, object_type.as_deref(), subtype.as_deref());
    let subtitle = subtitle_of(
        kind,
        page,
        ops.as_ref().map(Vec::len),
        &value_rows,
        stream.as_ref(),
        &title,
    );
    let crumbs = if read_only { vec![r] } else { crumb_path(doc, r) };
    Some(ObjectFacts {
        object: Some(r),
        revision: at,
        crumbs,
        read_only,
        type_line: type_line(&type_word, kind),
        kind,
        title,
        subtitle,
        relation,
        page,
        range,
        stream,
        lines,
        ops,
        text,
        defined_in,
        changed_later,
        diagnostics,
        position,
        used_by: refs_used_by
            .into_iter()
            .map(|(obj, path)| UsedBy {
                label: referrer_label(doc, &page_of, obj, at),
                obj,
                path,
            })
            .collect(),
        uses: refs_uses,
        visual: if read_only { None } else { doc.visual(r) },
    })
}

/// How many objects the search for the path from the trailer looks at before it gives up.
const CRUMB_SEARCH_LIMIT: usize = 200;

/// The shortest chain of references from an object the trailer names (`/Root`, `/Info`) down to
/// `r`, outermost first; just `[r]` when there is none within reach.
fn crumb_path(doc: &Document, r: ObjRef) -> Vec<ObjRef> {
    let named: Vec<ObjRef> = doc
        .node(&NodeId::Trailer)
        .iter()
        .flat_map(|props| props.groups.iter().flat_map(|g| g.rows.iter()))
        .filter_map(|row| match row.link {
            Some(NodeId::Object(target)) => Some(target),
            _ => None,
        })
        .collect();
    // Breadth first, from `r` towards whoever refers to it; `via` remembers where each came from.
    let mut via: HashMap<ObjRef, ObjRef> = HashMap::new();
    let mut queue = std::collections::VecDeque::from([r]);
    let mut looked_at = 0;
    while let Some(current) = queue.pop_front() {
        if named.contains(&current) {
            let mut path = vec![current];
            let mut at = current;
            while let Some(&next) = via.get(&at) {
                path.push(next);
                at = next;
            }
            return path;
        }
        looked_at += 1;
        if looked_at > CRUMB_SEARCH_LIMIT {
            break;
        }
        for (from, _) in doc.object_refs(current).used_by {
            if from != r && !via.contains_key(&from) {
                via.insert(from, current);
                queue.push_back(from);
            }
        }
    }
    vec![r]
}

/// The trailer's facts: its dictionary, no bytes of its own.
pub fn build_trailer(loaded: &LoadedDocument) -> Option<ObjectFacts> {
    let props = loaded.doc.node(&NodeId::Trailer)?;
    let rows = value_rows(&props);
    let latest = u32::try_from(loaded.info.revision_count.saturating_sub(1)).unwrap_or(u32::MAX);
    Some(ObjectFacts {
        object: None,
        revision: latest,
        read_only: false,
        kind: ObjectKind::Trailer,
        title: "Trailer".to_owned(),
        type_line: "dictionary · trailer".to_owned(),
        subtitle: format!("The newest trailer · {} keys", rows.len()),
        relation: None,
        page: None,
        range: None,
        stream: None,
        lines: dict_lines(&rows, "dictionary"),
        ops: None,
        text: None,
        defined_in: latest,
        changed_later: false,
        diagnostics: Vec::new(),
        position: Position {
            offset: None,
            in_object_stream: None,
            index_ok: true,
        },
        crumbs: Vec::new(),
        used_by: Vec::new(),
        uses: Vec::new(),
        visual: None,
    })
}

/// The rows of the "Value" group as `(key, value)`.
fn value_rows(props: &Props) -> Vec<(String, String)> {
    props
        .groups
        .iter()
        .find(|g| g.name == "Value")
        .map(|g| g.rows.iter().map(|r| (r.key.clone(), r.value.clone())).collect())
        .unwrap_or_default()
}

/// The page, the position in `/Contents` and the key path, when `used_by` shows that the object
/// is a page content stream.
fn content_stream_of(
    page_of: &HashMap<ObjRef, PageIx>,
    used_by: &[(ObjRef, String)],
) -> Option<(PageIx, u32, String)> {
    used_by.iter().find_map(|(from, path)| {
        let rest = path.strip_prefix("/Contents")?;
        let index = match rest {
            "" => 0,
            _ => rest.strip_prefix('[')?.strip_suffix(']')?.parse().ok()?,
        };
        Some((*page_of.get(from)?, index, "/Contents".to_owned()))
    })
}

/// What a referring object is, in a word or two: `Page 1`, `Pages`, `Font`, `stream`.
fn referrer_label(doc: &Document, page_of: &HashMap<ObjRef, PageIx>, from: ObjRef, at: u32) -> String {
    if let Some(page) = page_of.get(&from) {
        return format!("Page {}", page + 1);
    }
    let summary = doc.object_summary_at(from, at).unwrap_or_default();
    if let Some(typed) = summary.strip_prefix("/Type /") {
        return typed.split_whitespace().next().unwrap_or("object").to_owned();
    }
    if summary.starts_with("stream") {
        "stream".to_owned()
    } else if summary.starts_with('[') {
        "array".to_owned()
    } else {
        "dictionary".to_owned()
    }
}
