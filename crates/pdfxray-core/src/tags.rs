//! The logical structure tree (ISO 32000-2 §14.7–14.8): structure elements, role mapping,
//! attributes, and its links to marked content, annotations and pages.
//!
//! Contract with the `document` module (keep these signatures):
//! - `TagTree::build(&Document)` runs at open after content capture and returns diagnostics.
//! - `Document::{tags_tree, tag_props, tag_hits, tag_bbox, tag_ancestors}`.
//!
//! Layout of this module:
//! - this file holds the data model and the `Document` entry points;
//! - `parse` reads `/StructTreeRoot` (elements, attributes, namespaces, number and name trees);
//! - `roles` resolves custom structure types to standard ones through `/RoleMap` and `/RoleMapNS`;
//! - `checks` cross-checks the tree against the `ParentTree` and the captured page content;
//! - `order` places the elements on a page: tag boxes, reading order, its step kinds and backtracks;
//! - `speech` lists what a screen reader announces, in order, for a reader preset;
//! - `table` builds the grid of a `Table` element and resolves each cell's header cells;
//! - `semantics` checks alternate text, role mapping and `Private` elements;
//! - `view` turns the model into navigation nodes and inspector props.
//!
//! # Direct-object structure elements
//!
//! A structure element must be an indirect object (ISO 32000-2 Table 355, entry `K`), and
//! [`NodeId::StructElem`] is keyed by [`ObjRef`]. An element written inline in its parent's
//! `/K` has no identity, so it is *folded into its nearest indirect ancestor*: its content
//! (marked-content references, object references, child elements) is read and attached to
//! that ancestor as if it were the ancestor's own `/K` entries, and a `struct-elem-direct`
//! diagnostic names the folded element.

mod checks;
mod order;
mod parse;
mod roles;
mod semantics;
mod speech;
mod table;
mod text;
mod view;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub use self::order::{OrderComparison, OrderStep, ReadingOrder, ReadingOrderSource, StepKind, TagBox};
pub use self::roles::RoleClass;
pub use self::speech::{
    LangChange, LinkFlag, LinkTarget, NoteFlag, NoteLinkSource, NoteRole, PresetRule, SkipReason, SpeechCell,
    SpeechItem, SpeechOrder, SpeechPreset, Substitution,
};
use self::table::TableAttrs;
pub use self::table::{
    HeaderRef, HeaderSource, IrregularRow, RowGroup, TableCell, TableModel, TableRules, TableScope,
};

use crate::document::DiagnosticInfo;
use crate::{Document, NodeId, ObjRef, PageIx, Props, Rect, Severity, TreeNode};

/// Group id of the `StructTreeRoot` row in the tags tree.
pub(crate) const ROOT_GROUP: u32 = 100;

/// A child entry of a structure element (or of the root), in `/K` order.
#[derive(Debug, Clone, PartialEq)]
enum Kid {
    /// A child structure element, as an index into [`TagTree::elems`].
    Elem(usize),
    /// Marked content: a bare MCID or an MCR dictionary.
    Content {
        /// Page whose content stream holds the sequence, when known.
        page: Option<PageIx>,
        /// Marked-content identifier.
        mcid: i32,
        /// `/Stm`: the form `XObject` holding the content, when it is not the page itself.
        stream: Option<ObjRef>,
        /// `/StmOwn`: the object that owns `stream`.
        stream_owner: Option<ObjRef>,
    },
    /// An object reference (`/Type /OBJR`), typically an annotation.
    Object {
        /// The referenced object.
        obj: Option<ObjRef>,
        /// `/Pg`, own or inherited from the element.
        page: Option<PageIx>,
    },
}

/// Where an attribute object came from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AttrSource {
    /// The element's own `/A`.
    Direct,
    /// Looked up through this class name in `/ClassMap`.
    Class(String),
}

/// One attribute object: owner `/O` and every other entry in PDF syntax.
#[derive(Debug, Clone, PartialEq)]
struct AttrSet {
    owner: String,
    entries: Vec<(String, String)>,
    revision: Option<i64>,
    source: AttrSource,
    /// Parsed `/BBox` entry, when present and numeric.
    bbox: Option<Rect>,
    /// The typed table attributes of an attribute object owned by `Table`.
    table: TableAttrs,
}

/// A structure type reached through `/RoleMapNS`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RoleTarget {
    role: String,
    /// Index into [`TagTree::namespaces`].
    ns: Option<usize>,
}

/// A namespace dictionary (PDF 2.0).
#[derive(Debug, Clone, PartialEq, Default)]
struct Namespace {
    obj: Option<ObjRef>,
    uri: String,
    role_map: Vec<(String, RoleTarget)>,
}

/// Result of following `/RoleMap` / `/RoleMapNS` from an element's `/S`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RoleResolution {
    /// `/S`, then every step of the mapping (the first repeated name last when cyclic).
    chain: Vec<String>,
    /// The standard type reached, if any.
    standard: Option<String>,
    /// Namespace URI of the standard type, when namespaces are in play.
    standard_ns: Option<String>,
    /// The roles on the cycle, starting from the lexicographically smallest.
    cycle: Option<Vec<String>>,
}

/// A structure element.
#[derive(Debug, Clone, PartialEq)]
struct Elem {
    obj: ObjRef,
    /// `/S` as written.
    role: String,
    /// `/NS`, as an index into [`TagTree::namespaces`].
    ns: Option<usize>,
    resolved: RoleResolution,
    parent: Option<usize>,
    /// Own `/Pg`.
    page: Option<PageIx>,
    /// Own `/Pg`, else the parent's effective page.
    eff_page: Option<PageIx>,
    kids: Vec<Kid>,
    id: Option<String>,
    title: Option<String>,
    lang: Option<String>,
    alt: Option<String>,
    actual_text: Option<String>,
    expansion: Option<String>,
    phoneme: Option<String>,
    phonetic_alphabet: Option<String>,
    revision: Option<i64>,
    refs: Vec<ObjRef>,
    associated_files: Vec<ObjRef>,
    /// Direct attributes first, then class attributes in `/C` order.
    attrs: Vec<AttrSet>,
    classes: Vec<String>,
    /// Local indices into [`TagTree::findings`].
    diags: Vec<usize>,
}

/// The structure tree root.
#[derive(Debug, Clone, PartialEq)]
struct Root {
    obj: Option<ObjRef>,
    kids: Vec<Kid>,
    /// Findings about the tree as a whole.
    diags: Vec<usize>,
}

/// An entry of the parent tree.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PtEntry {
    /// Page or form content: the owning element per MCID.
    Array(Vec<Option<ObjRef>>),
    /// An annotation or `XObject`: its owning element.
    Object(ObjRef),
}

/// What a parent tree entry says about an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PtRef {
    key: i32,
    /// The MCID slot, or `None` for an object entry.
    mcid: Option<i32>,
}

/// `/MarkInfo` of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MarkInfo {
    marked: Option<bool>,
    user_properties: Option<bool>,
    suspects: Option<bool>,
}

/// A problem found while building; becomes a [`DiagnosticInfo`].
#[derive(Debug, Clone, PartialEq)]
struct Finding {
    code: &'static str,
    severity: Severity,
    message: String,
    object: Option<ObjRef>,
}

/// The parsed structure tree.
#[derive(Debug, Default)]
pub(crate) struct TagTree {
    root: Option<Root>,
    /// Elements in document (pre-order) order.
    elems: Vec<Elem>,
    index: BTreeMap<ObjRef, usize>,
    /// Which element's `/K` references each `(page, MCID)`; the first one wins.
    owners: BTreeMap<(PageIx, i32), usize>,
    role_map: BTreeMap<String, String>,
    class_map: BTreeMap<String, Vec<AttrSet>>,
    namespaces: Vec<Namespace>,
    parent_tree: BTreeMap<i32, PtEntry>,
    parent_tree_present: bool,
    parent_tree_next_key: Option<i64>,
    /// `/StructParents` of each page that has one.
    page_keys: BTreeMap<PageIx, i32>,
    id_tree: BTreeMap<String, ObjRef>,
    mark_info: Option<MarkInfo>,
    catalog_lang: Option<String>,
    findings: Vec<Finding>,
    /// Number of diagnostics that precede ours in `Document::diagnostics`.
    diag_base: usize,
    /// For each element, what the parent tree says about it.
    pt_refs: HashMap<usize, Vec<PtRef>>,
    /// Which header-finding rules tables of this document follow.
    table_rules: TableRules,
}

impl TagTree {
    /// Number of structure elements found.
    pub(crate) fn element_count(&self) -> usize {
        self.elems.len()
    }

    /// Reads `/Root /StructTreeRoot` and cross-checks it against the page content. Never fails:
    /// whatever is wrong with the tree becomes a diagnostic.
    pub(crate) fn build(doc: &Document) -> (Self, Vec<DiagnosticInfo>) {
        parse::Builder::new(doc).build()
    }

    /// Every page-content marked-content sequence under element `obj`, recursively, in `/K`
    /// order and without duplicates.
    fn marked_content_of(&self, obj: ObjRef) -> Vec<(PageIx, i32)> {
        /// What the walk has seen so far; `elems` guards against cycles, `content` against repeats.
        #[derive(Default)]
        struct Seen {
            elems: BTreeSet<usize>,
            content: HashSet<(PageIx, i32)>,
        }
        fn walk(tree: &TagTree, i: usize, seen: &mut Seen, out: &mut Vec<(PageIx, i32)>) {
            if !seen.elems.insert(i) {
                return;
            }
            for kid in &tree.elems[i].kids {
                match kid {
                    Kid::Elem(child) => walk(tree, *child, seen, out),
                    Kid::Content {
                        page: Some(page),
                        mcid,
                        ..
                    } if seen.content.insert((*page, *mcid)) => {
                        out.push((*page, *mcid));
                    }
                    Kid::Content { .. } | Kid::Object { .. } => {}
                }
            }
        }
        let mut out = Vec::new();
        if let Some(&i) = self.index.get(&obj) {
            walk(self, i, &mut Seen::default(), &mut out);
        }
        out
    }
}

impl Document {
    /// The structure tree from `/StructTreeRoot`.
    pub(crate) fn tags_tree(&self) -> Vec<TreeNode> {
        self.tags.tree(self)
    }

    /// Props for `StructElem` nodes.
    pub(crate) fn tag_props(&self, id: &NodeId) -> Option<Props> {
        let NodeId::StructElem(r) = id else {
            return None;
        };
        self.tags.props(self, *r)
    }

    /// Structure elements owning the marked content in `content_hits`, innermost first.
    pub(crate) fn tag_hits(&self, content_hits: &[NodeId]) -> Vec<NodeId> {
        self.tags.hits(content_hits)
    }

    /// Page and bounding box of a structure element (its content on the first page it touches).
    pub(crate) fn tag_bbox(&self, id: &NodeId) -> Option<(PageIx, Rect)> {
        let NodeId::StructElem(r) = id else {
            return None;
        };
        self.tags.bbox(self, *r)
    }

    /// The marked-content sequences a node stands for: a structure element's MCIDs
    /// (recursively), or the node itself when it is already marked content. Renderers use this
    /// to isolate or hide a tag.
    pub fn marked_content(&self, id: &NodeId) -> Vec<NodeId> {
        match id {
            NodeId::StructElem(r) => self
                .tags
                .marked_content_of(*r)
                .into_iter()
                .map(|(page, mcid)| NodeId::MarkedContent { page, mcid })
                .collect(),
            NodeId::MarkedContent { .. } => vec![id.clone()],
            _ => Vec::new(),
        }
    }

    /// Structure ancestors of `id`, root first, excluding `id`.
    pub(crate) fn tag_ancestors(&self, id: &NodeId) -> Vec<NodeId> {
        let NodeId::StructElem(r) = id else {
            return Vec::new();
        };
        self.tags.ancestors(*r)
    }
}
