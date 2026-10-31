//! Reads `/Root /StructTreeRoot` into the [`TagTree`] model. Cycle-safe and depth-limited; every
//! deviation is recorded as a [`Finding`], never repaired.
//!
//! This module holds the dereferencing reader [`Rd`], the [`Builder`] and how it records findings.
//! The catalog, `/StructTreeRoot`, role map, class map and namespaces are read in [`catalog`];
//! structure elements, their kids and attributes in [`elems`]; the parent tree, the ID tree and
//! the pages' `/StructParents` in [`trees`].

mod catalog;
mod elems;
mod trees;

use std::collections::{BTreeMap, HashMap};

use pdfxray_raw::{Dict, Object};

use super::text;
use super::{Finding, TagTree};
use crate::document::DiagnosticInfo;
use crate::{Document, ObjRef, PageIx, Rect, Severity};

/// Deepest element nesting followed.
pub(super) const MAX_DEPTH: usize = 200;
/// Deepest number or name tree followed.
const MAX_TREE_DEPTH: usize = 32;
/// Longest reference chain followed when dereferencing.
const MAX_HOPS: usize = 32;
/// Longest attribute value kept.
const VALUE_MAX: usize = 200;

static NULL: Object = Object::Null;

/// Dereferencing reader over the document's object index.
#[derive(Clone, Copy)]
pub(super) struct Rd<'a> {
    pub(super) doc: &'a Document,
}

impl<'a> Rd<'a> {
    /// The object stored under `r`, without following it further.
    pub(super) fn object(self, r: ObjRef) -> Option<&'a Object> {
        self.doc.objects.get(r).map(|o| &o.value.value)
    }

    /// Follows references until a direct object; dangling or looping references give `null`.
    pub(super) fn deref(self, o: &'a Object) -> &'a Object {
        let mut cur = o;
        for _ in 0..MAX_HOPS {
            match cur {
                Object::Ref(r) => match self.object(*r) {
                    Some(next) => cur = next,
                    None => return &NULL,
                },
                other => return other,
            }
        }
        &NULL
    }

    pub(super) fn dict(self, o: &'a Object) -> Option<&'a Dict> {
        self.deref(o).as_dict()
    }

    /// The dereferenced value of `key`, unless absent or `null`.
    pub(super) fn get(self, d: &'a Dict, key: &[u8]) -> Option<&'a Object> {
        let v = self.deref(&d.get(key)?.value);
        (!matches!(v, Object::Null)).then_some(v)
    }

    pub(super) fn get_ref(d: &Dict, key: &[u8]) -> Option<ObjRef> {
        d.get(key)?.value.as_ref_obj()
    }

    pub(super) fn name(self, d: &'a Dict, key: &[u8]) -> Option<String> {
        self.get(d, key)?
            .as_name()
            .map(pdfxray_raw::Name::to_string_lossy)
    }

    pub(super) fn text(self, d: &'a Dict, key: &[u8]) -> Option<String> {
        match self.get(d, key)? {
            Object::String(s) => Some(text::decode(&s.bytes)),
            _ => None,
        }
    }

    pub(super) fn int(self, d: &'a Dict, key: &[u8]) -> Option<i64> {
        self.get(d, key)?.as_int()
    }

    pub(super) fn boolean(self, d: &'a Dict, key: &[u8]) -> Option<bool> {
        match self.get(d, key)? {
            Object::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The elements of an array value, or the value itself when it is not an array. Entries
    /// keep their references.
    pub(super) fn list(self, v: &'a Object) -> Vec<&'a Object> {
        match self.deref(v) {
            Object::Array(items) => items.iter().map(|i| &i.value).collect(),
            Object::Null => Vec::new(),
            _ => vec![v],
        }
    }

    /// The references in an array (or a single reference) under `key`.
    pub(super) fn refs(self, d: &'a Dict, key: &[u8]) -> Vec<ObjRef> {
        d.get(key)
            .map(|v| self.list(&v.value))
            .unwrap_or_default()
            .into_iter()
            .filter_map(Object::as_ref_obj)
            .collect()
    }

    pub(super) fn number(self, o: &'a Object) -> Option<f64> {
        self.deref(o).as_f64()
    }

    /// A rectangle value `[x0 y0 x1 y1]`.
    pub(super) fn rect(self, o: &'a Object) -> Option<Rect> {
        let nums: Vec<f64> = self.list(o).into_iter().filter_map(|i| self.number(i)).collect();
        match nums.as_slice() {
            [x0, y0, x1, y1] => Some(Rect::new(*x0, *y0, *x1, *y1)),
            _ => None,
        }
    }
}

/// Where the element being read sits.
#[derive(Clone, Copy)]
struct Ctx {
    /// The element (or root) whose `/K` is being read.
    owner_ref: Option<ObjRef>,
    owner_idx: Option<usize>,
    /// The owner's effective `/Pg`.
    page: Option<PageIx>,
    depth: usize,
}

/// Builds a [`TagTree`] and its diagnostics.
pub(super) struct Builder<'a> {
    pub(super) rd: Rd<'a>,
    pub(super) tree: TagTree,
    pages: HashMap<ObjRef, PageIx>,
    /// Elements currently being read, outermost first.
    path: Vec<ObjRef>,
    /// Which element references each `(page, stream, MCID)`.
    pub(super) seen_content: BTreeMap<(PageIx, Option<ObjRef>, i32), usize>,
    /// Findings for role map cycles, by normalised cycle.
    pub(super) cycles: BTreeMap<Vec<String>, usize>,
}

impl<'a> Builder<'a> {
    pub(super) fn new(doc: &'a Document) -> Self {
        let pages = doc
            .pages
            .iter()
            .filter_map(|p| p.obj.map(|o| (o, p.index)))
            .collect();
        Self {
            rd: Rd { doc },
            tree: TagTree {
                diag_base: doc.diagnostics.len(),
                ..TagTree::default()
            },
            pages,
            path: Vec::new(),
            seen_content: BTreeMap::new(),
            cycles: BTreeMap::new(),
        }
    }

    pub(super) fn build(mut self) -> (TagTree, Vec<DiagnosticInfo>) {
        self.parse_catalog();
        if self.tree.root.is_some() {
            self.resolve_roles();
            self.tree.table_rules = self.table_rules();
            self.check_id_tree();
            self.check_parent_tree();
            self.check_content();
            self.check_reading_order();
            self.check_semantics();
            self.check_tables();
        }
        self.finish()
    }

    fn finish(self) -> (TagTree, Vec<DiagnosticInfo>) {
        let doc = self.rd.doc;
        let diagnostics = self
            .tree
            .findings
            .iter()
            .map(|f| {
                let at = f.object.and_then(|r| doc.objects.get(r).map(|o| (r, o.span)));
                DiagnosticInfo::core(f.code, f.severity, f.message.clone(), at)
            })
            .collect();
        (self.tree, diagnostics)
    }

    // ---- findings -------------------------------------------------------------------------

    /// Records a finding and returns its local index.
    pub(super) fn find(
        &mut self,
        code: &'static str,
        severity: Severity,
        message: String,
        object: Option<ObjRef>,
    ) -> usize {
        self.tree.findings.push(Finding {
            code,
            severity,
            message,
            object,
        });
        self.tree.findings.len() - 1
    }

    /// Records a finding that concerns element `ei` (shown as a badge on it).
    pub(super) fn find_on(
        &mut self,
        code: &'static str,
        severity: Severity,
        message: String,
        object: Option<ObjRef>,
        ei: Option<usize>,
    ) -> usize {
        let f = self.find(code, severity, message, object);
        match ei {
            Some(ei) => self.attach(f, ei),
            None => self.attach_root(f),
        }
        f
    }

    pub(super) fn attach(&mut self, finding: usize, ei: usize) {
        let diags = &mut self.tree.elems[ei].diags;
        if !diags.contains(&finding) {
            diags.push(finding);
        }
    }

    pub(super) fn attach_root(&mut self, finding: usize) {
        if let Some(root) = &mut self.tree.root
            && !root.diags.contains(&finding)
        {
            root.diags.push(finding);
        }
    }
}
