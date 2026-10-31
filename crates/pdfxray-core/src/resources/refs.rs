//! "Used by" and "resources it uses" of an object, and the revision it last changed in.
//!
//! * `used_by` lists the objects whose value holds a reference to the object, with the key path
//!   of each reference (`/Contents`, `/Resources/Font/F1`, `/Kids[2]`). References from the
//!   trailer are not objects and are not listed.
//! * `uses` is filled for the objects that run operators: a page content stream, a Form
//!   `XObject` and a page. It lists the resources those operators select, resolved through the
//!   resources the operators resolve in, one entry per `(category, name)`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use pdfxray_raw::{Dict, Object};

use super::describe::Lens;
use super::scope::Scope;
use super::usage::{scope_owner, used_resource};
use crate::capture::{OpRecord, PaintKind, StreamRef};
use crate::numeric::usize_from_u32;
use crate::{Document, ObjRef, PageIx, ResKind};

/// Deepest object nesting followed when looking for a reference.
const MAX_PATH_DEPTH: usize = 24;

/// Who uses an object and what it uses itself. See [`Document::object_refs`].
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectRefs {
    /// Objects that refer to it, with the key path of the reference, in object order.
    pub used_by: Vec<(ObjRef, String)>,
    /// Resources its operators select: name, target, kind and a one-line summary.
    pub uses: Vec<UsedResource>,
}

/// One resource selected by the operators of a content stream.
#[derive(Debug, Clone, PartialEq)]
pub struct UsedResource {
    /// Resource name without the slash (`F1`).
    pub name: String,
    /// The object the name resolves to, when it is an indirect object.
    pub obj: Option<ObjRef>,
    /// The category (font, `XObject`, `ExtGState` ...).
    pub kind: ResKind,
    /// One line: `SourceSerif4-Bold · 18 glyphs`, `image 1360×600`, `ExtGState · alpha 0.2`.
    pub summary: String,
}

/// Which operators an object runs.
enum Selector {
    /// The `index`-th stream of the `/Contents` of a page.
    Contents { page: PageIx, index: u32 },
    /// The operators written in a Form `XObject`'s own stream.
    Form(ObjRef),
    /// Every operator of a page that resolves its names in the page's own resources.
    Page(PageIx),
}

impl Document {
    /// Who refers to `r` and which resources `r`'s operators use. Both lists are empty for an
    /// object that does not exist.
    pub fn object_refs(&self, r: ObjRef) -> ObjectRefs {
        let mut used_by = Vec::new();
        for from in self.objects.referrers(r) {
            let Some(obj) = self.objects.get(from) else {
                continue;
            };
            let mut paths = Vec::new();
            key_paths(&obj.value.value, r, &mut String::new(), 0, &mut paths);
            used_by.extend(paths.into_iter().map(|p| (from, p)));
        }
        ObjectRefs {
            used_by,
            uses: self.resources_used_by(r),
        }
    }

    /// The resources selected by the operators `r` runs.
    fn resources_used_by(&self, r: ObjRef) -> Vec<UsedResource> {
        let selectors = self.selectors_of(r);
        if selectors.is_empty() {
            return Vec::new();
        }
        let own_forms = self.forms_with_resources();
        let lens = Lens::new(&self.raw);
        let mut scopes: HashMap<ObjRef, Option<Scope>> = HashMap::new();
        // (kind, name) -> (target, summary base, glyph count)
        let mut found: BTreeMap<(ResKind, String), Found> = BTreeMap::new();
        for selector in &selectors {
            let pages: Vec<PageIx> = match selector {
                Selector::Contents { page, .. } | Selector::Page(page) => vec![*page],
                Selector::Form(_) => self.pages.iter().map(|p| p.index).collect(),
            };
            for page in pages {
                let (Some(info), Some(captured)) = (self.page(page), self.captured(page)) else {
                    continue;
                };
                let Some(page_obj) = info.obj else { continue };
                for op in captured
                    .ops
                    .iter()
                    .filter(|op| selects(selector, op, page_obj, &own_forms))
                {
                    let Some((kind, name)) = used_resource(op) else {
                        continue;
                    };
                    let owner = scope_owner(&op.stream, page_obj, &own_forms);
                    let scope = scopes.entry(owner).or_insert_with(|| self.scope_of_owner(owner));
                    let Some(entry) = scope.as_ref().and_then(|s| s.entry(kind, &name)) else {
                        continue;
                    };
                    found.entry((kind, name)).or_insert_with(|| Found {
                        target: entry.target,
                        summary: summarise(&lens, kind, &entry.value),
                        glyphs: 0,
                    });
                }
                // Glyph counts per font name for the selected operators.
                for item in &captured.items {
                    let PaintKind::Glyph { .. } = item.kind else {
                        continue;
                    };
                    let Some(op) = captured.ops.get(usize_from_u32(item.op)) else {
                        continue;
                    };
                    if !selects(selector, op, page_obj, &own_forms) {
                        continue;
                    }
                    if let Some(font) = &item.gstate.text.font
                        && let Some(slot) = found.get_mut(&(ResKind::Font, font.clone()))
                    {
                        slot.glyphs += 1;
                    }
                }
            }
        }
        found
            .into_iter()
            .map(|((kind, name), f)| UsedResource {
                summary: if kind == ResKind::Font && f.glyphs > 0 {
                    format!("{} · {} glyphs", f.summary, f.glyphs)
                } else {
                    f.summary
                },
                name,
                obj: f.target,
                kind,
            })
            .collect()
    }

    /// The ways `r` runs operators; empty when it is not a page, a page content stream or a form.
    fn selectors_of(&self, r: ObjRef) -> Vec<Selector> {
        let mut out = Vec::new();
        if let Some(page) = self.pages.iter().find(|p| p.obj == Some(r)) {
            out.push(Selector::Page(page.index));
        }
        let Some(obj) = self.objects.get(r) else {
            return out;
        };
        if let Object::Stream(s) = &obj.value.value
            && s.dict
                .get_name(b"Subtype")
                .is_some_and(|n| n.as_bytes() == b"Form")
        {
            out.push(Selector::Form(r));
        }
        for page in &self.pages {
            let Some(contents) = page.obj.and_then(|p| self.contents_of(p)) else {
                continue;
            };
            out.extend(
                contents
                    .into_iter()
                    .zip(0..)
                    .filter(|(c, _)| *c == r)
                    .map(|(_, index)| Selector::Contents {
                        page: page.index,
                        index,
                    }),
            );
        }
        out
    }

    /// The streams of a page's `/Contents`, in order.
    fn contents_of(&self, page: ObjRef) -> Option<Vec<ObjRef>> {
        let dict: &Dict = self.objects.get(page)?.value.value.as_dict()?;
        let value = &dict.get(b"Contents")?.value;
        Some(match self.raw.resolve(value).as_ref() {
            Object::Array(items) => items.iter().filter_map(|i| i.value.as_ref_obj()).collect(),
            _ => value.as_ref_obj().into_iter().collect(),
        })
    }
}

/// What is known about a resource while collecting.
struct Found {
    target: Option<ObjRef>,
    summary: String,
    glyphs: usize,
}

/// True when `op` belongs to the operators `selector` names.
fn selects(selector: &Selector, op: &OpRecord, page_obj: ObjRef, own_forms: &BTreeSet<ObjRef>) -> bool {
    match selector {
        Selector::Contents { index, .. } => {
            matches!(&op.stream, StreamRef::PageContent { index: i } if i == index)
        }
        Selector::Form(form) => {
            matches!(&op.stream, StreamRef::FormXObject { obj: Some(o), .. } if o == form)
        }
        Selector::Page(_) => scope_owner(&op.stream, page_obj, own_forms) == page_obj,
    }
}

/// Every place in `obj` that holds a reference to `target`, as key paths.
fn key_paths(obj: &Object, target: ObjRef, path: &mut String, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_PATH_DEPTH {
        return;
    }
    match obj {
        Object::Ref(r) if *r == target => {
            out.push(if path.is_empty() {
                "(value)".to_owned()
            } else {
                path.clone()
            });
        }
        Object::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                let keep = path.len();
                let _ = write!(path, "[{i}]");
                key_paths(&item.value, target, path, depth + 1, out);
                path.truncate(keep);
            }
        }
        Object::Dict(d) => dict_paths(d, target, path, depth, out),
        Object::Stream(s) => dict_paths(&s.dict, target, path, depth, out),
        _ => {}
    }
}

fn dict_paths(d: &Dict, target: ObjRef, path: &mut String, depth: usize, out: &mut Vec<String>) {
    for (key, value) in &d.entries {
        let keep = path.len();
        path.push('/');
        path.push_str(&key.value.to_string_lossy());
        key_paths(&value.value, target, path, depth + 1, out);
        path.truncate(keep);
    }
}

/// One line for a resource, without the glyph count (added once the operators are counted).
fn summarise(lens: &Lens<'_>, kind: ResKind, value: &Object) -> String {
    let resolved = lens.resolve(value);
    match (kind, &resolved) {
        (ResKind::Font, Object::Dict(d)) => lens
            .name(d, "BaseFont")
            .or_else(|| lens.name(d, "Subtype"))
            .unwrap_or_else(|| "font".to_owned()),
        (ResKind::XObject, Object::Stream(s)) => match lens.name(&s.dict, "Subtype").as_deref() {
            Some("Image") => {
                let dim = |k: &str| {
                    lens.int(&s.dict, k)
                        .map_or_else(|| "?".to_owned(), |v| v.to_string())
                };
                format!("image {}×{}", dim("Width"), dim("Height"))
            }
            Some("Form") => "form XObject".to_owned(),
            _ => lens.summary(kind, value),
        },
        (ResKind::ExtGState, Object::Dict(d)) => {
            ext_gstate(lens, d).unwrap_or_else(|| lens.summary(kind, value))
        }
        _ => lens.summary(kind, value),
    }
}

/// `ExtGState · alpha 0.2 · blend Multiply`, when the dictionary has settings worth naming.
fn ext_gstate(lens: &Lens<'_>, d: &Dict) -> Option<String> {
    let mut parts = Vec::new();
    let number = |k: &str| lens.obj(d, k).and_then(|v| v.as_f64());
    match (number("ca"), number("CA")) {
        (Some(a), Some(b)) if (a - b).abs() < 1e-9 => parts.push(format!("alpha {}", crate::num(a))),
        (a, b) => {
            parts.extend(a.map(|a| format!("fill alpha {}", crate::num(a))));
            parts.extend(b.map(|b| format!("stroke alpha {}", crate::num(b))));
        }
    }
    if let Some(bm) = lens.name(d, "BM") {
        parts.push(format!("blend {bm}"));
    }
    if lens
        .obj(d, "SMask")
        .is_some_and(|v| !matches!(&v, Object::Name(n) if n == "None"))
    {
        parts.push("soft mask".to_owned());
    }
    (!parts.is_empty()).then(|| format!("ExtGState · {}", parts.join(" · ")))
}
