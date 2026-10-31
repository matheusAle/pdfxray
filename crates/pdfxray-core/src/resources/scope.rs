//! Resource owners ("scopes"): the pages, Form `XObjects`, tiling patterns and Type 3 fonts
//! that carry a `/Resources` dictionary, with every named entry of its categories.

use std::collections::BTreeSet;

use pdfxray_raw::{Dict, Object};

use crate::page::PageInfo;
use crate::{Document, ObjRef, PageIx, ResKind};

/// What kind of object owns a `/Resources` dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScopeKind {
    /// A page.
    Page(PageIx),
    /// A Form `XObject` with its own `/Resources`.
    Form,
    /// A tiling pattern with its own `/Resources`.
    Pattern,
    /// A Type 3 font with its own `/Resources`.
    Type3Font,
}

impl ScopeKind {
    /// Word used in labels.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Page(_) => "Page",
            Self::Form => "Form",
            Self::Pattern => "Pattern",
            Self::Type3Font => "Type 3 font",
        }
    }
}

/// One named entry of a category dictionary.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    /// Resource name without the slash.
    pub(super) name: String,
    /// The value exactly as written: a reference or a direct object.
    pub(super) value: Object,
    /// The referenced object, when the value is an indirect reference.
    pub(super) target: Option<ObjRef>,
}

/// The effective resources of one owner.
#[derive(Debug, Clone)]
pub(super) struct Scope {
    /// The object whose content stream uses these resources (page, form, pattern, font).
    pub(super) owner: ObjRef,
    /// The object that carries the `/Resources` entry: the owner itself or, for a page, an
    /// ancestor `/Pages` node. `None` when no `/Resources` applies at all.
    pub(super) holder: Option<ObjRef>,
    /// What the owner is.
    pub(super) kind: ScopeKind,
    /// Non-empty categories in [`ResKind::ALL`] order.
    pub(super) categories: Vec<(ResKind, Vec<Entry>)>,
}

impl Scope {
    /// The entry `name` of `kind`.
    pub(super) fn entry(&self, kind: ResKind, name: &str) -> Option<&Entry> {
        self.entries(kind).iter().find(|e| e.name == name)
    }

    /// The entries of one category.
    pub(super) fn entries(&self, kind: ResKind) -> &[Entry] {
        self.categories
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(&[], |(_, e)| e.as_slice())
    }
}

/// Classifies an object that can own `/Resources` besides pages.
fn classify(obj: &Object) -> Option<ScopeKind> {
    let is_name = |d: &Dict, key: &[u8], want: &[u8]| d.get_name(key).is_some_and(|n| n.as_bytes() == want);
    match obj {
        Object::Stream(s) if is_name(&s.dict, b"Subtype", b"Form") => Some(ScopeKind::Form),
        Object::Stream(s) if s.dict.get_int(b"PatternType") == Some(1) => Some(ScopeKind::Pattern),
        Object::Dict(d) if is_name(d, b"Subtype", b"Type3") => Some(ScopeKind::Type3Font),
        _ => None,
    }
}

impl Document {
    /// The `/Resources` dictionary written on `holder`, following one indirection.
    fn resources_dict(&self, holder: ObjRef) -> Option<Dict> {
        let obj = self.objects.get(holder)?;
        let value = obj.value.value.as_dict()?.get(b"Resources")?;
        self.raw.resolve(&value.value).as_dict().cloned()
    }

    fn read_scope(&self, owner: ObjRef, holder: Option<ObjRef>, kind: ScopeKind) -> Scope {
        let dict = holder.and_then(|h| self.resources_dict(h));
        let mut categories = Vec::new();
        if let Some(dict) = dict {
            for res_kind in ResKind::ALL {
                let Some(value) = dict.get(res_kind.key().as_bytes()) else {
                    continue;
                };
                let resolved = self.raw.resolve(&value.value);
                let Some(entries) = resolved.as_dict() else {
                    continue;
                };
                let mut seen = BTreeSet::new();
                let list: Vec<Entry> = entries
                    .entries
                    .iter()
                    .filter(|(k, _)| seen.insert(k.value.as_bytes()))
                    .map(|(k, v)| Entry {
                        name: k.value.to_string_lossy(),
                        value: v.value.clone(),
                        target: v.value.as_ref_obj(),
                    })
                    .collect();
                if !list.is_empty() {
                    categories.push((res_kind, list));
                }
            }
        }
        Scope {
            owner,
            holder,
            kind,
            categories,
        }
    }

    /// The resources a page sees. `None` when the raw page tree has no object for it.
    pub(super) fn page_scope(&self, page: &PageInfo) -> Option<Scope> {
        let owner = page.obj?;
        Some(self.read_scope(owner, page.resources_from, ScopeKind::Page(page.index)))
    }

    /// Every Form `XObject`, tiling pattern and Type 3 font with its own `/Resources`, in object
    /// order.
    pub(super) fn nested_scope_owners(&self) -> Vec<(ObjRef, ScopeKind)> {
        self.raw
            .object_refs()
            .into_iter()
            .filter_map(|r| {
                let obj = self.objects.get(r)?;
                let kind = classify(&obj.value.value)?;
                self.resources_dict(r)?;
                Some((r, kind))
            })
            .collect()
    }

    /// The Form `XObjects` that own their `/Resources`; operators inside any other form resolve
    /// names in the resources of whatever painted it.
    pub(super) fn forms_with_resources(&self) -> BTreeSet<ObjRef> {
        self.nested_scope_owners()
            .into_iter()
            .filter(|(_, k)| *k == ScopeKind::Form)
            .map(|(r, _)| r)
            .collect()
    }

    /// All owners in document order: pages, then forms / patterns / Type 3 fonts.
    pub(super) fn all_scopes(&self) -> Vec<Scope> {
        let pages = self.pages.iter().filter_map(|p| self.page_scope(p));
        let nested = self
            .nested_scope_owners()
            .into_iter()
            .map(|(r, kind)| self.read_scope(r, Some(r), kind));
        pages.chain(nested).collect()
    }

    /// The scope whose owner object is `owner`.
    pub(super) fn scope_of_owner(&self, owner: ObjRef) -> Option<Scope> {
        if let Some(page) = self.pages.iter().find(|p| p.obj == Some(owner)) {
            return self.page_scope(page);
        }
        let kind = classify(&self.objects.get(owner)?.value.value)?;
        self.resources_dict(owner)?;
        Some(self.read_scope(owner, Some(owner), kind))
    }
}
