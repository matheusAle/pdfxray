//! The structure tree as the query API shows it: navigation nodes, inspector props, hit
//! stacks, breadcrumbs and geometry.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::parse::Rd;
use super::text;
use super::{AttrSource, Elem, Kid, ROOT_GROUP, TagTree};
use crate::{Badge, Document, NodeId, ObjRef, PageIx, PropGroup, Props, Rect, TreeNode};

/// Characters of text shown in a tree row.
const PREVIEW: usize = 60;
/// Characters of extracted text shown in the inspector.
const TEXT_MAX: usize = 300;
/// Child links listed in the inspector before the rest is summarised.
const CHILD_LINKS: usize = 50;

/// Something an element's subtree draws or references.
enum Leaf {
    Mc(PageIx, i32, Option<ObjRef>),
    Obj(ObjRef, Option<PageIx>),
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub(super) fn union(a: Rect, b: Rect) -> Rect {
    Rect::new(a.x0.min(b.x0), a.y0.min(b.y0), a.x1.max(b.x1), a.y1.max(b.y1))
}

/// Appends the words painted by the marked content `(page, mcid, stream)`, each after a space
/// unless `out` is empty, and counts the characters. Stops once `count` passes `budget`.
pub(super) fn push_words(
    doc: &Document,
    (page, mcid, stream): (PageIx, i32, Option<ObjRef>),
    out: &mut String,
    count: &mut usize,
    budget: usize,
) {
    let mut in_word = false;
    doc.mc_text_each(page, mcid, stream, &mut |glyph_text| {
        for c in glyph_text.chars() {
            if c.is_whitespace() {
                in_word = false;
                continue;
            }
            if !in_word {
                in_word = true;
                if !out.is_empty() {
                    out.push(' ');
                    *count += 1;
                }
            }
            out.push(c);
            *count += 1;
        }
        *count <= budget
    });
}

impl TagTree {
    fn diag_id(&self, local: usize) -> NodeId {
        NodeId::Diagnostic(u32::try_from(self.diag_base + local).unwrap_or(u32::MAX))
    }

    /// One badge per distinct diagnostic code among `diags`.
    fn badges(&self, diags: &[usize]) -> Vec<Badge> {
        let mut out: Vec<Badge> = Vec::new();
        for &d in diags {
            let f = &self.findings[d];
            if !out.iter().any(|b| b.code == f.code) {
                out.push(Badge {
                    code: f.code.to_owned(),
                    severity: f.severity,
                });
            }
        }
        out
    }

    fn label(e: &Elem) -> String {
        let role = if e.role.is_empty() { "(no type)" } else { &e.role };
        match &e.resolved.standard {
            Some(standard) if *standard != e.role => format!("{role} → {standard}"),
            _ => role.to_owned(),
        }
    }

    fn page_label(page: PageIx) -> String {
        format!("Page {}", page + 1)
    }

    fn kid_summary(e: &Elem) -> String {
        let mut out = String::new();
        Self::push_kid_summary(e, &mut out);
        out
    }

    /// Appends how many elements, MCIDs and OBJRs `e` holds, e.g. `2 elements, 1 MCID`.
    fn push_kid_summary(e: &Elem, out: &mut String) {
        let count = |f: fn(&Kid) -> bool| e.kids.iter().filter(|k| f(k)).count();
        let counts = [
            (count(|k| matches!(k, Kid::Elem(_))), "element", "elements"),
            (count(|k| matches!(k, Kid::Content { .. })), "MCID", "MCIDs"),
            (count(|k| matches!(k, Kid::Object { .. })), "OBJR", "OBJRs"),
        ];
        let start = out.len();
        for (n, one, many) in counts.into_iter().filter(|(n, ..)| *n > 0) {
            if out.len() > start {
                out.push_str(", ");
            }
            let _ = write!(out, "{n} {}", if n == 1 { one } else { many });
        }
    }

    /// Text of the marked content under `ei`, in reading order, whitespace squashed. Reading stops
    /// once more than `budget` characters are collected, so the result is exact up to its first
    /// `budget + 1` characters and never needs more of the page than that.
    pub(super) fn collect_text(&self, doc: &Document, ei: usize, out: &mut String, budget: usize) {
        let mut count = out.chars().count();
        self.collect_text_into(doc, ei, out, &mut count, budget);
    }

    fn collect_text_into(
        &self,
        doc: &Document,
        ei: usize,
        out: &mut String,
        count: &mut usize,
        budget: usize,
    ) {
        for kid in &self.elems[ei].kids {
            if *count >= budget {
                return;
            }
            match kid {
                Kid::Content {
                    page: Some(page),
                    mcid,
                    stream,
                    ..
                } => push_words(doc, (*page, *mcid, *stream), out, count, budget),
                Kid::Elem(child) => self.collect_text_into(doc, *child, out, count, budget),
                _ => {}
            }
        }
    }

    /// A preview of what a screen reader announces for the element, at most `max` characters,
    /// whitespace collapsed, ending in `…` when cut. Acrobat exposes an element's `/Alt`,
    /// `/ActualText` or `/E` instead of its content ("the child count is always zero"), so they
    /// win over the painted text, in that order; a substitute that is blank is ignored. Without
    /// one, the text painted by the element's marked content (descendants included) is shown.
    pub(super) fn preview(&self, doc: &Document, ei: usize, max: usize) -> Option<String> {
        if max == 0 {
            return None;
        }
        let e = &self.elems[ei];
        let spoken = [&e.alt, &e.actual_text, &e.expansion]
            .into_iter()
            .flatten()
            .map(|s| text::squash(s))
            .find(|s| !s.is_empty());
        let out = spoken.unwrap_or_else(|| {
            let mut painted = String::new();
            self.collect_text(doc, ei, &mut painted, max);
            painted
        });
        (!out.is_empty()).then(|| text::ellipsize(&out, max))
    }

    fn object_subtype(doc: &Document, o: ObjRef) -> String {
        let rd = Rd { doc };
        rd.object(o)
            .and_then(pdfxray_raw::Object::as_dict)
            .and_then(|d| rd.name(d, b"Subtype").or_else(|| rd.name(d, b"Type")))
            .unwrap_or_else(|| "object".to_owned())
    }

    // ---- tree -------------------------------------------------------------------------------

    pub(super) fn tree(&self, doc: &Document) -> Vec<TreeNode> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let marked = match self.mark_info.and_then(|m| m.marked) {
            Some(true) => "/Marked true",
            Some(false) => "/Marked false",
            None => "no /Marked",
        };
        let mut detail = format!("{}, {marked}", plural(self.elems.len(), "element", "elements"));
        if let Some(info) = self.mark_info {
            if info.suspects == Some(true) {
                detail.push_str(", /Suspects true");
            }
            if info.user_properties == Some(true) {
                detail.push_str(", /UserProperties true");
            }
        }
        let mut node = TreeNode::leaf(NodeId::Group(ROOT_GROUP), "StructTreeRoot")
            .with_detail(detail)
            .with_children(self.kid_nodes(doc, &root.kids));
        node.badges = self.badges(&root.diags);
        vec![node]
    }

    fn kid_nodes(&self, doc: &Document, kids: &[Kid]) -> Vec<TreeNode> {
        let mut nodes = Vec::with_capacity(kids.len());
        nodes.extend(kids.iter().filter_map(|kid| match kid {
            Kid::Elem(ei) => Some(self.elem_node(doc, *ei)),
            Kid::Content {
                page: Some(page),
                mcid,
                stream,
                ..
            } => {
                let mut detail = Self::page_label(*page);
                if let Some(s) = stream {
                    let _ = write!(detail, " · in form {s}");
                }
                Some(
                    TreeNode::leaf(
                        NodeId::MarkedContent {
                            page: *page,
                            mcid: *mcid,
                        },
                        format!("MCID {mcid}"),
                    )
                    .with_detail(detail),
                )
            }
            Kid::Content { page: None, .. } | Kid::Object { obj: None, .. } => None,
            Kid::Object { obj: Some(o), .. } => {
                Some(TreeNode::leaf(NodeId::Object(*o), "OBJR").with_detail(Self::object_subtype(doc, *o)))
            }
        }));
        nodes
    }

    fn elem_node(&self, doc: &Document, ei: usize) -> TreeNode {
        let e = &self.elems[ei];
        let mut detail = String::new();
        let quoted = |detail: &mut String, prefix: &str, text: &str| {
            detail.push_str(prefix);
            detail.push('"');
            text::push_ellipsized(detail, text, PREVIEW);
            detail.push('"');
        };
        if let Some(alt) = &e.alt {
            quoted(&mut detail, "Alt: ", &text::squash(alt));
        } else if let Some(actual) = &e.actual_text {
            quoted(&mut detail, "ActualText: ", &text::squash(actual));
        } else {
            let mut preview = String::new();
            self.collect_text(doc, ei, &mut preview, PREVIEW);
            // The preview is already squashed, one word at a time.
            if !preview.is_empty() {
                quoted(&mut detail, "", &preview);
            }
        }
        let before_summary = detail.len();
        if before_summary > 0 {
            detail.push_str(" · ");
        }
        Self::push_kid_summary(e, &mut detail);
        if detail.len() == before_summary + if before_summary > 0 { " · ".len() } else { 0 } {
            // Nothing was added by the summary: drop the separator again.
            detail.truncate(before_summary);
        }
        let mut node = TreeNode::leaf(NodeId::StructElem(e.obj), Self::label(e))
            .with_children(self.kid_nodes(doc, &e.kids));
        if !detail.is_empty() {
            node = node.with_detail(detail);
        }
        node.badges = self.badges(&e.diags);
        node
    }

    // ---- props ------------------------------------------------------------------------------

    pub(super) fn props(&self, doc: &Document, r: ObjRef) -> Option<Props> {
        let ei = *self.index.get(&r)?;
        let e = &self.elems[ei];
        let mut groups = vec![self.element_group(ei)];
        groups.extend(Self::attributes_group(e));
        groups.extend(self.content_group(doc, ei));
        groups.push(self.structure_group(ei));
        groups.push(Self::object_group(e));
        groups.extend(self.diagnostics_group(e));
        Some(Props {
            id: NodeId::StructElem(r),
            title: Self::label(e),
            kind: "struct-elem",
            groups,
        })
    }

    /// The element's `/Lang`, or the nearest ancestor's, or the catalog's.
    pub(super) fn effective_lang(&self, ei: usize) -> Option<(String, Option<usize>)> {
        let mut cur = Some(ei);
        while let Some(i) = cur {
            if let Some(lang) = &self.elems[i].lang {
                return Some((lang.clone(), Some(i)));
            }
            cur = self.elems[i].parent;
        }
        self.catalog_lang.clone().map(|l| (l, None))
    }

    fn elem_link(&self, ei: usize) -> (String, NodeId) {
        let e = &self.elems[ei];
        (Self::label(e), NodeId::StructElem(e.obj))
    }

    fn element_group(&self, ei: usize) -> PropGroup {
        let e = &self.elems[ei];
        let opt = |g: PropGroup, key: &str, v: &Option<String>| match v {
            Some(v) => g.row(key, v),
            None => g,
        };
        let role = if e.role.is_empty() { "(none)" } else { &e.role };
        let mut g = PropGroup::new("Element").row("Type", role);
        g = match (&e.resolved.standard, &e.resolved.cycle) {
            (Some(standard), _) => g.row("Standard type", standard),
            (None, Some(cycle)) => g.row(
                "Standard type",
                format!("none (role map cycle: {})", cycle.join(" → ")),
            ),
            (None, None) => g.row("Standard type", "none (not mapped to a standard type)"),
        };
        if e.resolved.chain.len() > 1 {
            g = g.row("Role chain", e.resolved.chain.join(" → "));
        }
        if let Some(ns) = e.ns.and_then(|i| self.namespaces.get(i)) {
            g = g.row("Namespace", &ns.uri);
        }
        if let Some(standard_ns) = &e.resolved.standard_ns
            && e.ns.and_then(|i| self.namespaces.get(i)).map(|n| &n.uri) != Some(standard_ns)
        {
            g = g.row("Standard namespace", standard_ns);
        }
        g = opt(g, "ID", &e.id);
        g = opt(g, "Title", &e.title);
        g = opt(g, "Lang", &e.lang);
        if let Some((lang, from)) = self.effective_lang(ei) {
            g = match from {
                Some(i) if i == ei => g,
                Some(i) => {
                    let (label, id) = self.elem_link(i);
                    g.link("Effective lang", format!("{lang} (from {label})"), id)
                }
                None => g.row("Effective lang", format!("{lang} (from the catalog)")),
            };
        }
        g = opt(g, "Alt", &e.alt);
        g = opt(g, "ActualText", &e.actual_text);
        g = opt(g, "Expansion", &e.expansion);
        g = opt(g, "Phoneme", &e.phoneme);
        g = opt(g, "Phonetic alphabet", &e.phonetic_alphabet);
        if let Some(rev) = e.revision {
            g = g.row("Revision", rev.to_string());
        }
        for r in &e.refs {
            g = match self.index.get(r) {
                Some(&i) => {
                    let (label, id) = self.elem_link(i);
                    g.link("Ref", label, id)
                }
                None => g.link("Ref", r.to_string(), NodeId::Object(*r)),
            };
        }
        for f in &e.associated_files {
            g = g.link("Associated file", f.to_string(), NodeId::Object(*f));
        }
        g
    }

    fn attributes_group(e: &Elem) -> Option<PropGroup> {
        if e.attrs.is_empty() && e.classes.is_empty() {
            return None;
        }
        let mut g = PropGroup::new("Attributes");
        if !e.classes.is_empty() {
            g = g.row("Classes", e.classes.join(", "));
        }
        for a in &e.attrs {
            let owner = if a.owner.is_empty() {
                "(no owner)"
            } else {
                &a.owner
            };
            let origin = match &a.source {
                AttrSource::Direct => String::new(),
                AttrSource::Class(c) => format!(" (from class {c})"),
            };
            if a.entries.is_empty() {
                g = g.row(owner, format!("(no entries){origin}"));
            }
            for (k, v) in &a.entries {
                g = g.row(format!("{owner}: {k}"), format!("{v}{origin}"));
            }
            if let Some(rev) = a.revision {
                g = g.row(format!("{owner}: revision"), format!("{rev}{origin}"));
            }
        }
        Some(g)
    }

    fn content_group(&self, doc: &Document, ei: usize) -> Option<PropGroup> {
        let e = &self.elems[ei];
        let mut g = PropGroup::new("Content");
        let mut any = false;
        if let Some(page) = e.eff_page {
            g = g.link("Page", Self::page_label(page), NodeId::Page(page));
            any = true;
        }
        for kid in &e.kids {
            any = true;
            g = match kid {
                Kid::Elem(_) => {
                    any = e.eff_page.is_some() || any;
                    g
                }
                Kid::Content {
                    page: Some(page),
                    mcid,
                    stream,
                    ..
                } => {
                    let mut value = format!("{mcid} · {}", Self::page_label(*page));
                    if let Some(s) = stream {
                        let _ = write!(value, " · in form {s}");
                    }
                    g.link(
                        "MCID",
                        value,
                        NodeId::MarkedContent {
                            page: *page,
                            mcid: *mcid,
                        },
                    )
                }
                Kid::Content { page: None, mcid, .. } => g.row("MCID", format!("{mcid} · page unknown")),
                Kid::Object { obj: Some(o), page } => {
                    let mut value = format!("{o} · {}", Self::object_subtype(doc, *o));
                    if let Some(p) = page {
                        let _ = write!(value, " · {}", Self::page_label(*p));
                    }
                    g.link("OBJR", value, NodeId::Object(*o))
                }
                Kid::Object { obj: None, .. } => g,
            };
        }
        let mut extracted = String::new();
        self.collect_text(doc, ei, &mut extracted, TEXT_MAX);
        if !extracted.is_empty() {
            g = g.row("Text", text::ellipsize(&extracted, TEXT_MAX));
            any = true;
        }
        any.then_some(g)
    }

    fn structure_group(&self, ei: usize) -> PropGroup {
        let e = &self.elems[ei];
        let mut g = PropGroup::new("Structure");
        g = match e.parent {
            Some(p) => {
                let (label, id) = self.elem_link(p);
                g.link("Parent", label, id)
            }
            None => g.row("Parent", "StructTreeRoot"),
        };
        g = g.row("Depth", self.depth_of(ei).to_string());
        let summary = Self::kid_summary(e);
        g = g.row(
            "Children",
            if summary.is_empty() {
                "none".into()
            } else {
                summary
            },
        );
        let children: Vec<usize> = e
            .kids
            .iter()
            .filter_map(|k| match k {
                Kid::Elem(c) => Some(*c),
                _ => None,
            })
            .collect();
        for (n, &c) in children.iter().take(CHILD_LINKS).enumerate() {
            let (label, id) = self.elem_link(c);
            g = g.link(format!("Child {}", n + 1), label, id);
        }
        if children.len() > CHILD_LINKS {
            g = g.row(
                "More children",
                format!("{} not listed", children.len() - CHILD_LINKS),
            );
        }
        for pt in self.pt_refs.get(&ei).map(Vec::as_slice).unwrap_or_default() {
            g = match pt.mcid {
                Some(mcid) => {
                    let page = self
                        .page_keys
                        .iter()
                        .find(|(_, k)| **k == pt.key)
                        .map(|(p, _)| *p);
                    match page {
                        Some(page) => g.link(
                            "ParentTree",
                            format!("key {} · MCID {mcid} · {}", pt.key, Self::page_label(page)),
                            NodeId::MarkedContent { page, mcid },
                        ),
                        None => g.row("ParentTree", format!("key {} · MCID {mcid}", pt.key)),
                    }
                }
                None => g.row("ParentTree", format!("key {} · object entry", pt.key)),
            };
        }
        g
    }

    fn object_group(e: &Elem) -> PropGroup {
        let mut g = PropGroup::new("Object").link("Reference", e.obj.to_string(), NodeId::Object(e.obj));
        if let Some(page) = e.page {
            g = g.link("Pg", Self::page_label(page), NodeId::Page(page));
        }
        g
    }

    fn diagnostics_group(&self, e: &Elem) -> Option<PropGroup> {
        if e.diags.is_empty() {
            return None;
        }
        let mut local: Vec<usize> = e.diags.clone();
        local.sort_unstable();
        let mut g = PropGroup::new("Diagnostics");
        for d in local {
            let f = &self.findings[d];
            g = g.link(f.code, &f.message, self.diag_id(d));
        }
        Some(g)
    }

    // ---- hits, ancestors, geometry ----------------------------------------------------------

    pub(super) fn hits(&self, content_hits: &[NodeId]) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for hit in content_hits {
            let NodeId::MarkedContent { page, mcid } = hit else {
                continue;
            };
            let mut cur = self.owners.get(&(*page, *mcid)).copied();
            while let Some(i) = cur {
                let id = NodeId::StructElem(self.elems[i].obj);
                if seen.insert(id.clone()) {
                    out.push(id);
                }
                cur = self.elems[i].parent;
            }
        }
        out
    }

    pub(super) fn ancestors(&self, r: ObjRef) -> Vec<NodeId> {
        let Some(&ei) = self.index.get(&r) else {
            return Vec::new();
        };
        let mut chain = Vec::new();
        let mut cur = self.elems[ei].parent;
        while let Some(p) = cur {
            chain.push(NodeId::StructElem(self.elems[p].obj));
            cur = self.elems[p].parent;
        }
        chain.reverse();
        chain
    }

    fn leaves(&self, ei: usize, out: &mut Vec<Leaf>) {
        for kid in &self.elems[ei].kids {
            match kid {
                Kid::Elem(c) => self.leaves(*c, out),
                Kid::Content {
                    page: Some(page),
                    mcid,
                    stream,
                    ..
                } => out.push(Leaf::Mc(*page, *mcid, *stream)),
                Kid::Object { obj: Some(o), page } => out.push(Leaf::Obj(*o, *page)),
                _ => {}
            }
        }
    }

    /// Where the element's subtree draws, per page: the boxes of its marked content; without
    /// any, its `/BBox` attribute, then the `/Rect` of the first object it references.
    fn placed_boxes(&self, doc: &Document, ei: usize) -> Vec<(PageIx, Rect)> {
        let e = &self.elems[ei];
        let mut leaves = Vec::new();
        self.leaves(ei, &mut leaves);
        let boxes: Vec<(PageIx, Rect)> = leaves
            .iter()
            .filter_map(|l| match l {
                Leaf::Mc(page, mcid, stream) => doc.mc_bbox_in(*page, *mcid, *stream).map(|b| (*page, b)),
                Leaf::Obj(..) => None,
            })
            .collect();
        if !boxes.is_empty() {
            return boxes;
        }
        if let Some(b) = e.attrs.iter().find_map(|a| a.bbox) {
            let page = e.eff_page.or_else(|| {
                leaves.iter().find_map(|l| match l {
                    Leaf::Mc(p, ..) => Some(*p),
                    Leaf::Obj(..) => None,
                })
            });
            return page.map(|p| (p, b)).into_iter().collect();
        }
        leaves
            .iter()
            .find_map(|l| {
                let Leaf::Obj(o, page) = l else { return None };
                Some(((*page).or(e.eff_page)?, Self::object_rect(doc, *o)?))
            })
            .into_iter()
            .collect()
    }

    /// The `/Rect` of an annotation-like object.
    pub(super) fn object_rect(doc: &Document, o: ObjRef) -> Option<Rect> {
        let rd = Rd { doc };
        rd.object(o)
            .and_then(pdfxray_raw::Object::as_dict)
            .and_then(|d| rd.get(d, b"Rect"))
            .and_then(|v| rd.rect(v))
    }

    /// The union of the bounding boxes of the element's marked content on the first page that
    /// has any; without content, its `/BBox` attribute, then the `/Rect` of an object it
    /// references.
    pub(super) fn bbox(&self, doc: &Document, r: ObjRef) -> Option<(PageIx, Rect)> {
        let ei = *self.index.get(&r)?;
        let boxes = self.placed_boxes(doc, ei);
        let &(first, _) = boxes.first()?;
        let merged = boxes
            .iter()
            .filter(|(p, _)| *p == first)
            .map(|(_, b)| *b)
            .reduce(union)?;
        Some((first, merged))
    }

    /// The union of the element's boxes (see [`TagTree::bbox`]) that lie on `page`.
    pub(super) fn bbox_on(&self, doc: &Document, ei: usize, page: PageIx) -> Option<Rect> {
        self.placed_boxes(doc, ei)
            .into_iter()
            .filter(|(p, _)| *p == page)
            .map(|(_, b)| b)
            .reduce(union)
    }

    /// The union of the boxes of the content the element itself holds on `page` (marked content
    /// and the objects it references, not its child elements).
    pub(super) fn own_box(&self, doc: &Document, ei: usize, page: PageIx) -> Option<Rect> {
        let e = &self.elems[ei];
        e.kids
            .iter()
            .filter_map(|kid| match kid {
                Kid::Content {
                    page: Some(p),
                    mcid,
                    stream,
                    ..
                } if *p == page => doc.mc_bbox_in(page, *mcid, *stream),
                Kid::Object {
                    obj: Some(o),
                    page: p,
                } if p.or(e.eff_page) == Some(page) => Self::object_rect(doc, *o),
                _ => None,
            })
            .reduce(union)
    }

    /// Number of ancestors of the element: 0 for a child of the structure tree root.
    pub(super) fn depth_of(&self, ei: usize) -> u32 {
        let mut depth = 0;
        let mut cur = self.elems[ei].parent;
        while let Some(p) = cur {
            depth += 1;
            cur = self.elems[p].parent;
        }
        depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plural_and_union_helpers() {
        assert_eq!(plural(1, "MCID", "MCIDs"), "1 MCID");
        assert_eq!(plural(0, "MCID", "MCIDs"), "0 MCIDs");
        let u = union(Rect::new(0.0, 0.0, 2.0, 2.0), Rect::new(1.0, -1.0, 5.0, 1.0));
        assert_eq!(u, Rect::new(0.0, -1.0, 5.0, 2.0));
        assert_eq!(
            TagTree::default().marked_content_of(ObjRef::new(1, 0)),
            Vec::new()
        );
    }
}
