//! The depth-first walk over `/K`: one item per run of marked content, per substituted element,
//! per object reference and per empty table cell.

use pdfxray_raw::Object;

use super::super::parse::Rd;
use super::super::text;
use super::super::{Elem, Kid, RoleClass};
use super::{
    LinkFlag, LinkTarget, McRef, PAINTED_MAX, SkipReason, Speaker, SpeechItem, SpeechPreset, Substitution,
    language,
};
use crate::{Document, NodeId, ObjRef, PageIx};

/// Standard types that are announced even when they paint no text: a graphic, a control.
const ANNOUNCED_WITHOUT_TEXT: [&str; 3] = ["Figure", "Formula", "Form"];

/// What a run of marked content reads as.
#[derive(Default)]
struct RunText {
    text: String,
    painted: Option<String>,
    by: Option<Substitution>,
    lang: Option<String>,
    in_artifact: bool,
    invisible: bool,
    first_op: Option<u32>,
}

/// A substitute text for an element.
struct Substitute {
    by: Substitution,
    text: String,
    also_actual_text: Option<String>,
}

/// The first of `texts` that is not blank, squashed.
fn first_text(texts: &[&Option<String>]) -> Option<(usize, String)> {
    texts.iter().enumerate().find_map(|(i, t)| {
        let squashed = text::squash(t.as_deref()?);
        (!squashed.is_empty()).then_some((i, squashed))
    })
}

/// The level of `H1`..`Hn`.
fn heading_level(role: &str) -> Option<u8> {
    role.strip_prefix('H')?.parse().ok()
}

impl Speaker<'_> {
    /// The structure type the preset announces for element `ei`.
    fn announced(&self, ei: usize) -> Option<String> {
        let resolved = &self.tree.elems[ei].resolved;
        let one_hop = self.preset == SpeechPreset::FirefoxPdfJs;
        if one_hop && resolved.chain.len() > 2 {
            return None;
        }
        resolved.standard.clone()
    }

    /// An empty item for element `ei`.
    fn base(&self, ei: usize) -> SpeechItem {
        let e = &self.tree.elems[ei];
        let role = self.announced(ei);
        let class = if role == e.resolved.standard {
            self.tree.class(ei)
        } else {
            RoleClass::Other
        };
        SpeechItem {
            index: 0,
            node: self.tree.id_of(ei),
            marked_content: Vec::new(),
            page: e.eff_page,
            declared_role: e.role.clone(),
            level: role.as_deref().and_then(heading_level),
            role,
            class,
            text: String::new(),
            painted: None,
            substituted_by: None,
            also_actual_text: None,
            lang: self.tree.effective_lang(ei).and_then(|(l, _)| language(&l)),
            lang_change: None,
            cell: None,
            note: None,
            link: None,
            invisible: false,
            skipped: None,
        }
    }

    // ---- text -------------------------------------------------------------------------------

    /// The text a run of marked content reads as. A sequence's own `/ActualText` or `/Alt`
    /// replaces its glyphs (ISO 32000-1 §14.9.3–14.9.4).
    fn run_text(&self, run: &[McRef]) -> RunText {
        let mut out = RunText::default();
        let mut parts: Vec<String> = Vec::new();
        let mut painted_parts: Vec<String> = Vec::new();
        let mut all_invisible = !run.is_empty();
        for &mc in run {
            let facts = self.doc.mc_facts(mc.0, mc.1, mc.2).unwrap_or_default();
            let mut words = String::new();
            super::super::view::push_words(self.doc, mc, &mut words, &mut 0, usize::MAX);
            let replaced = first_text(&[&facts.alt, &facts.actual_text]);
            match replaced {
                Some((which, text)) => {
                    out.by.get_or_insert(if which == 0 {
                        Substitution::Alt
                    } else {
                        Substitution::ActualText
                    });
                    parts.push(text);
                }
                None => parts.push(words.clone()),
            }
            painted_parts.push(words);
            out.in_artifact |= facts.in_artifact;
            all_invisible &= facts.invisible;
            if out.lang.is_none() {
                out.lang = facts.lang.as_deref().and_then(language);
            }
            out.first_op = match (out.first_op, facts.first_op) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        out.invisible = all_invisible;
        out.text = text::squash(&parts.join(" "));
        let painted = text::squash(&painted_parts.join(" "));
        out.painted = (out.by.is_some() && painted != out.text && !painted.is_empty()).then_some(painted);
        out
    }

    /// The text painted by everything below `ei`.
    fn painted_below(&self, ei: usize) -> Option<String> {
        let mut painted = String::new();
        self.tree.collect_text(self.doc, ei, &mut painted, PAINTED_MAX);
        (!painted.is_empty()).then_some(painted)
    }

    /// The first operator that paints anything below `ei`.
    fn first_op_below(&self, ei: usize) -> Option<u32> {
        self.tree
            .marked_content_of(self.tree.elems[ei].obj)
            .into_iter()
            .filter_map(|(page, mcid)| self.doc.mc_first_op(page, mcid, None))
            .min()
    }

    /// The first page that has content below `ei`.
    fn page_below(&self, ei: usize) -> Option<PageIx> {
        self.tree.elems[ei].eff_page.or_else(|| {
            self.tree
                .marked_content_of(self.tree.elems[ei].obj)
                .first()
                .map(|(page, _)| *page)
        })
    }

    // ---- rules ------------------------------------------------------------------------------

    /// What replaces element `ei`, by the preset's rules.
    fn substitute(&self, ei: usize) -> Option<Substitute> {
        let e = &self.tree.elems[ei];
        match self.preset {
            SpeechPreset::ChromiumDefault => None,
            SpeechPreset::FirefoxPdfJs => {
                let figure_path = e.resolved.standard.as_deref() == Some("Figure")
                    && e.attrs.iter().any(|a| a.bbox.is_some())
                    && matches!(e.kids.first(), Some(Kid::Content { .. }));
                if !figure_path {
                    return None;
                }
                let (which, text) = first_text(&[&e.alt, &e.actual_text])?;
                Some(Substitute {
                    by: if which == 0 {
                        Substitution::Alt
                    } else {
                        Substitution::ActualText
                    },
                    text,
                    also_actual_text: None,
                })
            }
            SpeechPreset::Structure | SpeechPreset::AcrobatWindows => {
                let (which, text) = first_text(&[&e.alt, &e.actual_text, &e.expansion])?;
                let by = [
                    Substitution::Alt,
                    Substitution::ActualText,
                    Substitution::Expansion,
                ][which];
                let also_actual_text = (by == Substitution::Alt && self.preset == SpeechPreset::Structure)
                    .then(|| first_text(&[&e.actual_text]).map(|(_, t)| t))
                    .flatten();
                Some(Substitute {
                    by,
                    text,
                    also_actual_text,
                })
            }
        }
    }

    // ---- the walk ---------------------------------------------------------------------------

    pub(super) fn visit(&mut self, ei: usize, inherited: Option<SkipReason>) {
        let tree = self.tree;
        let e: &Elem = &tree.elems[ei];
        let start = self.items.len();
        let standard = e.resolved.standard.as_deref();
        let mut skip = inherited;
        if skip.is_none() {
            skip = match standard {
                Some("Private") => Some(SkipReason::Private),
                Some("Artifact") => Some(SkipReason::Artifact),
                _ => None,
            };
        }
        let mut substituted = false;
        if skip.is_none()
            && let Some(sub) = self.substitute(ei)
        {
            let mut item = self.base(ei);
            item.page = self.page_below(ei);
            item.painted = self.painted_below(ei).filter(|p| *p != sub.text);
            item.text = sub.text;
            item.substituted_by = Some(sub.by);
            item.also_actual_text = sub.also_actual_text;
            let first_op = self.first_op_below(ei);
            self.emit(item, first_op);
            skip = Some(SkipReason::CoveredBy(tree.id_of(ei)));
            substituted = true;
        }
        let mut run: Vec<McRef> = Vec::new();
        for kid in &e.kids {
            match kid {
                Kid::Content {
                    page: Some(page),
                    mcid,
                    stream,
                    ..
                } => {
                    // The content of a substituted element is its `painted` text, not an item.
                    if !substituted {
                        run.push((*page, *mcid, *stream));
                    }
                }
                Kid::Content { page: None, .. } | Kid::Object { obj: None, .. } => {}
                Kid::Elem(child) => {
                    self.flush(ei, &mut run, skip.as_ref());
                    self.visit(*child, skip.clone());
                }
                Kid::Object { obj: Some(obj), page } => {
                    self.object(ei, *obj, *page, &mut run, skip.as_ref());
                }
            }
        }
        self.flush(ei, &mut run, skip.as_ref());
        if self.items.len() == start && (self.cells.contains_key(&ei) || e.kids.is_empty()) {
            // An empty cell is announced; any other element with nothing in it is not.
            let mut item = self.base(ei);
            item.skipped = skip
                .clone()
                .or_else(|| (!self.cells.contains_key(&ei)).then_some(SkipReason::Empty));
            self.emit(item, None);
        }
        if self.items.len() > start {
            if let Some(cell) = self.cells.get(&ei)
                && self.items[start].cell.is_none()
            {
                self.items[start].cell = Some(cell.clone());
            }
            self.first_item.insert(ei, start);
        }
    }

    /// Emits the text of the pending run of marked content as an item of element `ei`.
    fn flush(&mut self, ei: usize, run: &mut Vec<McRef>, skip: Option<&SkipReason>) {
        if run.is_empty() {
            return;
        }
        let mcs = std::mem::take(run);
        let rt = self.run_text(&mcs);
        let mut item = self.base(ei);
        item.page = Some(mcs[0].0);
        item.marked_content = mcs
            .iter()
            .map(|&(page, mcid, _)| NodeId::MarkedContent { page, mcid })
            .collect();
        item.lang = rt.lang.or(item.lang);
        item.text = rt.text;
        item.painted = rt.painted;
        item.substituted_by = rt.by;
        item.invisible = rt.invisible;
        let announced_empty = item
            .role
            .as_deref()
            .is_some_and(|r| ANNOUNCED_WITHOUT_TEXT.contains(&r));
        if item.text.is_empty() && (skip.is_some() || rt.in_artifact) {
            // Nothing to say and not said: an image under a substituted element, an artifact rule.
            return;
        }
        item.skipped = skip.cloned().or_else(|| {
            if rt.in_artifact {
                Some(SkipReason::Artifact)
            } else if item.text.is_empty() && !announced_empty && !self.cells.contains_key(&ei) {
                Some(SkipReason::Empty)
            } else {
                None
            }
        });
        self.emit(item, rt.first_op);
    }

    /// Emits the annotation `obj` at its place among the `/K` entries of `ei`. The text pending
    /// in `run` is its name.
    fn object(
        &mut self,
        ei: usize,
        obj: ObjRef,
        page: Option<PageIx>,
        run: &mut Vec<McRef>,
        skip: Option<&SkipReason>,
    ) {
        let mcs = std::mem::take(run);
        let rt = self.run_text(&mcs);
        let annot = read_annotation(self.doc, obj);
        let mut item = self.base(ei);
        item.node = NodeId::Object(obj);
        item.page = page.or(item.page);
        item.marked_content = mcs
            .iter()
            .map(|&(page, mcid, _)| NodeId::MarkedContent { page, mcid })
            .collect();
        let named_by_content = matches!(item.role.as_deref(), Some("Link" | "Reference"));
        item.text = if rt.text.is_empty() && !named_by_content {
            annot
                .tu
                .clone()
                .or_else(|| annot.contents.clone())
                .unwrap_or_default()
        } else {
            rt.text
        };
        item.painted = rt.painted;
        item.substituted_by = rt.by;
        item.invisible = rt.invisible;
        item.lang = rt.lang.or(item.lang);
        item.link = Some(LinkFlag {
            subtype: annot.subtype,
            target: annot.target,
            contents: annot.contents,
        });
        item.skipped = skip.cloned();
        self.emit(item, rt.first_op);
    }
}

/// What the dictionary of an annotation says.
struct Annotation {
    subtype: String,
    target: Option<LinkTarget>,
    contents: Option<String>,
    tu: Option<String>,
}

/// Reads the annotation `obj`.
fn read_annotation(doc: &Document, obj: ObjRef) -> Annotation {
    let rd = Rd { doc };
    let Some(d) = rd.object(obj).and_then(Object::as_dict) else {
        return Annotation {
            subtype: "object".to_owned(),
            target: None,
            contents: None,
            tu: None,
        };
    };
    let destination = |value: &Object| match rd.deref(value) {
        Object::Array(items) => items
            .first()
            .and_then(|first| first.value.as_ref_obj())
            .and_then(|page| doc.pages.iter().find(|p| p.obj == Some(page)))
            .map(|p| LinkTarget::Page(p.index)),
        Object::Name(n) => Some(LinkTarget::Other(format!(
            "named destination {}",
            n.to_string_lossy()
        ))),
        Object::String(s) => Some(LinkTarget::Other(format!(
            "named destination {}",
            text::decode(&s.bytes)
        ))),
        _ => None,
    };
    let target = match rd.get(d, b"A").and_then(Object::as_dict) {
        Some(action) => match rd.name(action, b"S").as_deref() {
            Some("URI") => rd.text(action, b"URI").map(LinkTarget::Uri),
            Some("GoTo") => rd.get(action, b"D").and_then(destination),
            Some(other) => Some(LinkTarget::Other(format!("{other} action"))),
            None => None,
        },
        None => rd.get(d, b"Dest").and_then(destination),
    };
    Annotation {
        subtype: rd.name(d, b"Subtype").unwrap_or_else(|| "object".to_owned()),
        target,
        contents: rd.text(d, b"Contents").filter(|c| !c.trim().is_empty()),
        tu: rd.text(d, b"TU").filter(|c| !c.trim().is_empty()),
    }
}
