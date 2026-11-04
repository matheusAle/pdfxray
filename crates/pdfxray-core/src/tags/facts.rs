//! Read-only facts of the structure elements for the diff, which builds its keys from them in
//! one bottom-up pass instead of asking per element (the per-element queries re-walk subtrees).

use super::{Kid, TagTree};
use crate::{Document, ObjRef, PageIx, PropGroup, Rect};

/// What the diff reads of one structure element.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ElemFacts<'a> {
    /// The element object.
    pub(crate) obj: ObjRef,
    /// The parent element, by index; `None` under the structure tree root.
    pub(crate) parent: Option<usize>,
    /// `/S` as written.
    pub(crate) declared: &'a str,
    /// The standard type `/S` resolves to.
    pub(crate) standard: Option<&'a str>,
    /// `/C`, the class names.
    pub(crate) classes: &'a [String],
}

/// A child entry of an element that the diff walks: a child element or marked content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KidFact {
    /// A child element, by index. Indices of children are greater than their parent's: the
    /// elements are kept in pre-order.
    Elem(usize),
    /// Marked content on a known page.
    Content {
        /// The page.
        page: PageIx,
        /// The MCID.
        mcid: i32,
        /// The Form `XObject` holding it, `None` for the page's own content.
        stream: Option<ObjRef>,
    },
}

impl Document {
    /// Number of structure elements, indexed `0..n` in document pre-order.
    pub(crate) fn tag_elem_count(&self) -> usize {
        self.tags.elems.len()
    }

    /// The facts of element `ei`.
    pub(crate) fn tag_elem_facts(&self, ei: usize) -> ElemFacts<'_> {
        let e = &self.tags.elems[ei];
        ElemFacts {
            obj: e.obj,
            parent: e.parent,
            declared: &e.role,
            standard: e.resolved.standard.as_deref(),
            classes: &e.classes,
        }
    }

    /// The child elements and marked content of element `ei`, in `/K` order.
    pub(crate) fn tag_kid_facts(&self, ei: usize) -> impl Iterator<Item = KidFact> + '_ {
        self.tags.elems[ei].kids.iter().filter_map(|kid| match kid {
            Kid::Elem(child) => Some(KidFact::Elem(*child)),
            Kid::Content {
                page: Some(page),
                mcid,
                stream,
                ..
            } => Some(KidFact::Content {
                page: *page,
                mcid: *mcid,
                stream: *stream,
            }),
            Kid::Content { page: None, .. } | Kid::Object { .. } => None,
        })
    }

    /// The inspector groups that describe element `ei` itself, as [`Document::tag_props`] shows
    /// them: `Element` and `Attributes`. The other groups describe its content, its place in the
    /// tree and its object, which the diff compares elsewhere or not at all.
    pub(crate) fn tag_own_groups(&self, ei: usize) -> Vec<PropGroup> {
        let mut groups = vec![self.tags.element_group(ei)];
        groups.extend(TagTree::attributes_group(&self.tags.elems[ei]));
        groups
    }

    /// The box of an element whose subtree paints nothing: its `/BBox` attribute, else the
    /// `/Rect` of an object it references (see [`Document::tag_bbox`]).
    pub(crate) fn tag_unpainted_bbox(&self, ei: usize) -> Option<(PageIx, Rect)> {
        self.tags.bbox(self, self.tags.elems[ei].obj)
    }
}
