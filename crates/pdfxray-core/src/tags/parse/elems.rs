//! Structure elements: the `/K` kids (elements, marked-content and object references), their
//! pages, and their attributes and classes.

use pdfxray_raw::{Dict, Object};

use super::super::{AttrSet, AttrSource, Elem, Kid, RoleResolution, TableAttrs};
use super::{Builder, Ctx, MAX_DEPTH, Rd, VALUE_MAX, text};
use crate::{ObjRef, PageIx, Severity};

impl<'a> Builder<'a> {
    // ---- attributes -------------------------------------------------------------------------

    /// Attribute objects of an `/A` or `/ClassMap` value: a dictionary, or an array of
    /// dictionaries each optionally followed by its revision number.
    pub(super) fn attr_sets(&self, v: &'a Object, source: &AttrSource) -> Vec<AttrSet> {
        let rd = self.rd;
        let mut out: Vec<AttrSet> = Vec::new();
        for item in rd.list(v) {
            match rd.deref(item) {
                Object::Int(n) => {
                    if let Some(last) = out.last_mut() {
                        last.revision = Some(*n);
                    }
                }
                other => {
                    if let Some(d) = other.as_dict() {
                        out.push(self.attr_set(d, source.clone()));
                    }
                }
            }
        }
        out
    }

    fn attr_set(&self, d: &'a Dict, source: AttrSource) -> AttrSet {
        let rd = self.rd;
        let entries = d
            .entries
            .iter()
            .filter(|(k, _)| k.value.as_bytes() != b"O")
            .map(|(k, v)| {
                (
                    k.value.to_string_lossy(),
                    text::ellipsize(&v.value.to_string(), VALUE_MAX),
                )
            })
            .collect();
        let owner = rd.name(d, b"O").unwrap_or_default();
        let table = if owner == "Table" {
            TableAttrs::read(rd, d)
        } else {
            TableAttrs::default()
        };
        AttrSet {
            owner,
            entries,
            revision: None,
            source,
            bbox: rd.get(d, b"BBox").and_then(|b| rd.rect(b)),
            table,
        }
    }

    fn class_names(&self, d: &'a Dict) -> Vec<String> {
        let rd = self.rd;
        d.get(b"C")
            .map(|c| rd.list(&c.value))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| rd.deref(c).as_name().map(pdfxray_raw::Name::to_string_lossy))
            .collect()
    }

    // ---- structure elements -----------------------------------------------------------------

    pub(super) fn parse_kids(&mut self, k: Option<&'a Object>, ctx: Ctx) -> Vec<Kid> {
        let mut out = Vec::new();
        let Some(k) = k else { return out };
        for item in self.rd.list(k) {
            self.parse_kid(item, ctx, &mut out);
        }
        out
    }

    fn parse_kid(&mut self, item: &'a Object, ctx: Ctx, out: &mut Vec<Kid>) {
        match self.rd.deref(item) {
            Object::Int(n) => match i32::try_from(*n) {
                Ok(mcid) => self.content_kid(ctx, ctx.page, mcid, (None, None), out),
                Err(_) => self.invalid_kid(ctx, &format!("MCID {n} is out of range")),
            },
            Object::Dict(d) => self.classify(d, item.as_ref_obj(), ctx, out),
            Object::Null => {
                if let Object::Ref(r) = item {
                    self.invalid_kid(ctx, &format!("{r} does not exist"));
                }
            }
            other => self.invalid_kid(
                ctx,
                &format!("an entry of type {} is not a structure kid", kind(other)),
            ),
        }
    }

    fn classify(&mut self, d: &'a Dict, r: Option<ObjRef>, ctx: Ctx, out: &mut Vec<Kid>) {
        let rd = self.rd;
        let ty = rd.name(d, b"Type");
        let typeless = ty.is_none() && d.get(b"S").is_none();
        if ty.as_deref() == Some("MCR") || (typeless && d.get(b"MCID").is_some()) {
            self.mcr(d, ctx, out);
        } else if ty.as_deref() == Some("OBJR") || (typeless && d.get(b"Obj").is_some()) {
            self.objr(d, ctx, out);
        } else if let Some(r) = r {
            if let Some(ei) = self.parse_elem(r, ctx) {
                out.push(Kid::Elem(ei));
            }
        } else {
            self.fold_direct(d, ctx, out);
        }
    }

    fn mcr(&mut self, d: &'a Dict, ctx: Ctx, out: &mut Vec<Kid>) {
        let rd = self.rd;
        let page = self.page_of(d, ctx).or(ctx.page);
        let Some(mcid) = rd.int(d, b"MCID").and_then(|v| i32::try_from(v).ok()) else {
            self.invalid_kid(ctx, "a marked-content reference has no usable /MCID");
            return;
        };
        let streams = (Rd::get_ref(d, b"Stm"), Rd::get_ref(d, b"StmOwn"));
        self.content_kid(ctx, page, mcid, streams, out);
    }

    fn objr(&mut self, d: &'a Dict, ctx: Ctx, out: &mut Vec<Kid>) {
        let page = self.page_of(d, ctx).or(ctx.page);
        let Some(obj) = Rd::get_ref(d, b"Obj") else {
            self.invalid_kid(ctx, "an object reference has no indirect /Obj");
            return;
        };
        out.push(Kid::Object { obj: Some(obj), page });
    }

    fn content_kid(
        &mut self,
        ctx: Ctx,
        page: Option<PageIx>,
        mcid: i32,
        (stream, stream_owner): (Option<ObjRef>, Option<ObjRef>),
        out: &mut Vec<Kid>,
    ) {
        match page {
            Some(page) => self.register(ctx, page, mcid, stream),
            None => {
                self.find_on(
                    "pg-missing",
                    Severity::Warning,
                    format!("MCID {mcid} has no /Pg on its MCR or on the element, so its page is unknown"),
                    ctx.owner_ref,
                    ctx.owner_idx,
                );
            }
        }
        out.push(Kid::Content {
            page,
            mcid,
            stream,
            stream_owner,
        });
    }

    /// Notes that the owner references `(page, mcid)`.
    fn register(&mut self, ctx: Ctx, page: PageIx, mcid: i32, stream: Option<ObjRef>) {
        let Some(owner) = ctx.owner_idx else { return };
        match self.seen_content.get(&(page, stream, mcid)) {
            Some(&first) => {
                let (a, b) = (self.tree.elems[first].obj, self.tree.elems[owner].obj);
                self.find_on(
                    "mcid-referenced-twice",
                    Severity::Error,
                    format!(
                        "MCID {mcid} on page {} is referenced by both {a} and {b}",
                        page + 1
                    ),
                    Some(b),
                    Some(owner),
                );
            }
            None => {
                self.seen_content.insert((page, stream, mcid), owner);
            }
        }
        self.tree.owners.entry((page, mcid)).or_insert(owner);
    }

    fn invalid_kid(&mut self, ctx: Ctx, why: &str) {
        self.find_on(
            "struct-kid-invalid",
            Severity::Warning,
            format!("unusable /K entry: {why}"),
            ctx.owner_ref,
            ctx.owner_idx,
        );
    }

    /// `/Pg` of `d` as a page index, and the reference when it is not a page.
    fn page_lookup(&self, d: &Dict) -> (Option<PageIx>, Option<ObjRef>) {
        match Rd::get_ref(d, b"Pg") {
            Some(r) => match self.pages.get(&r) {
                Some(&p) => (Some(p), None),
                None => (None, Some(r)),
            },
            None => (None, None),
        }
    }

    /// `/Pg` of an MCR, OBJR or inline element; a `/Pg` that is not a page is reported on the
    /// owner.
    fn page_of(&mut self, d: &Dict, ctx: Ctx) -> Option<PageIx> {
        let (page, bad) = self.page_lookup(d);
        if let Some(r) = bad {
            self.not_a_page(r, ctx.owner_ref, ctx.owner_idx);
        }
        page
    }

    fn not_a_page(&mut self, pg: ObjRef, object: Option<ObjRef>, ei: Option<usize>) {
        self.find_on(
            "pg-not-a-page",
            Severity::Warning,
            format!("/Pg {pg} is not a page of the document"),
            object,
            ei,
        );
    }

    /// A structure element written inline in `/K` has no object identity (see the module
    /// documentation), so its content is folded into the owner.
    fn fold_direct(&mut self, d: &'a Dict, ctx: Ctx, out: &mut Vec<Kid>) {
        let role = self.rd.name(d, b"S").unwrap_or_default();
        self.find_on(
            "struct-elem-direct",
            Severity::Warning,
            format!(
                "structure element /{role} is a direct object, but elements must be indirect; \
                 its content is folded into the nearest indirect ancestor"
            ),
            ctx.owner_ref,
            ctx.owner_idx,
        );
        if ctx.depth >= MAX_DEPTH {
            self.too_deep(ctx);
            return;
        }
        let inner = Ctx {
            page: self.page_of(d, ctx).or(ctx.page),
            depth: ctx.depth + 1,
            ..ctx
        };
        let kids = self.parse_kids(d.get(b"K").map(|v| &v.value), inner);
        out.extend(kids);
    }

    fn too_deep(&mut self, ctx: Ctx) {
        self.find_on(
            "struct-tree-too-deep",
            Severity::Error,
            format!("structure elements are nested deeper than {MAX_DEPTH} levels; the rest is not followed"),
            ctx.owner_ref,
            ctx.owner_idx,
        );
    }

    /// Reads the indirect element `r` and everything below it. `None` when it is not read
    /// (cycle, second parent, not a dictionary, too deep).
    fn parse_elem(&mut self, r: ObjRef, ctx: Ctx) -> Option<usize> {
        let rd = self.rd;
        if ctx.depth >= MAX_DEPTH {
            self.too_deep(ctx);
            return None;
        }
        if let Some(&existing) = self.tree.index.get(&r) {
            if self.path.contains(&r) {
                self.find_on(
                    "struct-elem-cycle",
                    Severity::Error,
                    format!("{r} is its own ancestor: a /K chain leads back to it"),
                    Some(r),
                    Some(existing),
                );
            } else {
                self.find_on(
                    "struct-elem-multiple-parents",
                    Severity::Error,
                    format!("{r} is referenced from more than one /K; only its first position is shown"),
                    Some(r),
                    Some(existing),
                );
            }
            return None;
        }
        let Some(dict) = rd.object(r).and_then(Object::as_dict) else {
            self.invalid_kid(ctx, &format!("{r} is not a dictionary"));
            return None;
        };
        let ei = self.tree.elems.len();
        let (own_page, bad_page) = self.page_lookup(dict);
        let role = rd.name(dict, b"S");
        let ns = Rd::get_ref(dict, b"NS").map(|n| self.ns_index(n));
        let classes = self.class_names(dict);
        let mut attrs = dict
            .get(b"A")
            .map(|a| self.attr_sets(&a.value, &AttrSource::Direct))
            .unwrap_or_default();
        for class in &classes {
            match self.tree.class_map.get(class) {
                Some(sets) => attrs.extend(sets.iter().map(|s| AttrSet {
                    source: AttrSource::Class(class.clone()),
                    ..s.clone()
                })),
                None => {
                    self.find(
                        "class-not-found",
                        Severity::Warning,
                        format!("{r} uses class /{class}, which is not in the /ClassMap"),
                        Some(r),
                    );
                }
            }
        }
        let eff_page = own_page.or(ctx.page);
        self.tree.elems.push(Elem {
            obj: r,
            role: role.clone().unwrap_or_default(),
            ns,
            resolved: RoleResolution::default(),
            parent: ctx.owner_idx,
            page: own_page,
            eff_page,
            kids: Vec::new(),
            id: rd.text(dict, b"ID"),
            title: rd.text(dict, b"T"),
            lang: rd.text(dict, b"Lang"),
            alt: rd.text(dict, b"Alt"),
            actual_text: rd.text(dict, b"ActualText"),
            expansion: rd.text(dict, b"E"),
            phoneme: rd.text(dict, b"Phoneme"),
            phonetic_alphabet: rd.name(dict, b"PhoneticAlphabet"),
            revision: rd.int(dict, b"R"),
            refs: rd.refs(dict, b"Ref"),
            associated_files: rd.refs(dict, b"AF"),
            attrs,
            classes,
            diags: Vec::new(),
        });
        self.tree.index.insert(r, ei);
        if let Some(pg) = bad_page {
            self.not_a_page(pg, Some(r), Some(ei));
        }
        if role.is_none() {
            self.find_on(
                "struct-elem-no-type",
                Severity::Error,
                format!("{r} has no /S structure type"),
                Some(r),
                Some(ei),
            );
        }
        if let Some(parent_ref) = ctx.owner_ref
            && Rd::get_ref(dict, b"P") != Some(parent_ref)
        {
            let said =
                Rd::get_ref(dict, b"P").map_or_else(|| "has no /P".to_owned(), |p| format!("has /P {p}"));
            self.find_on(
                "struct-elem-parent-mismatch",
                Severity::Warning,
                format!("{r} {said} but is a child of {parent_ref}"),
                Some(r),
                Some(ei),
            );
        }
        self.path.push(r);
        let kids = self.parse_kids(
            dict.get(b"K").map(|v| &v.value),
            Ctx {
                owner_ref: Some(r),
                owner_idx: Some(ei),
                page: eff_page,
                depth: ctx.depth + 1,
            },
        );
        self.path.pop();
        self.tree.elems[ei].kids = kids;
        Some(ei)
    }
}

fn kind(o: &Object) -> &'static str {
    match o {
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
