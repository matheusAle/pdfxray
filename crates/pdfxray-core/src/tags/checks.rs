//! Cross-checks between the structure tree, the `ParentTree`, the `IDTree` and the captured
//! page content. Every disagreement becomes a finding; nothing is repaired.

use std::collections::{BTreeMap, HashMap};

use pdfxray_raw::Object;

use super::parse::Builder;
use super::{Kid, PtEntry, PtRef};
use crate::capture::{CapturedPage, MarkedSeq, PaintKind, StreamRef};
use crate::numeric::usize_from_u32;
use crate::{ObjRef, PageIx, Severity};

impl Builder<'_> {
    /// Element `/ID`s against the `/IDTree`.
    pub(super) fn check_id_tree(&mut self) {
        let mut by_id: HashMap<String, usize> = HashMap::new();
        for ei in 0..self.tree.elems.len() {
            let Some(id) = self.tree.elems[ei].id.clone() else {
                continue;
            };
            let obj = self.tree.elems[ei].obj;
            if let Some(&first) = by_id.get(&id) {
                let other = self.tree.elems[first].obj;
                self.find_on(
                    "duplicate-element-id",
                    Severity::Error,
                    format!("/ID ({id}) of {obj} is already used by {other}"),
                    Some(obj),
                    Some(ei),
                );
            } else {
                by_id.insert(id.clone(), ei);
            }
            if !self.tree.id_tree.contains_key(&id) {
                self.find_on(
                    "id-tree-mismatch",
                    Severity::Warning,
                    format!("/ID ({id}) of {obj} is not listed in the /IDTree"),
                    Some(obj),
                    Some(ei),
                );
            }
        }
        let entries: Vec<(String, ObjRef)> = self.tree.id_tree.iter().map(|(k, v)| (k.clone(), *v)).collect();
        for (id, r) in entries {
            match self.tree.index.get(&r).copied() {
                None => {
                    self.find_on(
                        "id-tree-mismatch",
                        Severity::Warning,
                        format!(
                            "the /IDTree maps ({id}) to {r}, which is not a structure element of the tree"
                        ),
                        Some(r),
                        None,
                    );
                }
                Some(ei) if self.tree.elems[ei].id.as_deref() != Some(id.as_str()) => {
                    self.find_on(
                        "id-tree-mismatch",
                        Severity::Warning,
                        format!("the /IDTree maps ({id}) to {r}, whose own /ID differs"),
                        Some(r),
                        Some(ei),
                    );
                }
                Some(_) => {}
            }
        }
    }

    /// `/K` ownership of MCIDs and annotations against the `ParentTree`.
    pub(super) fn check_parent_tree(&mut self) {
        self.index_parent_tree();
        let has_content = !self.seen_content.is_empty()
            || self
                .tree
                .elems
                .iter()
                .any(|e| e.kids.iter().any(|k| matches!(k, Kid::Object { .. })));
        if !self.tree.parent_tree_present {
            if has_content {
                let root = self.tree.root.as_ref().and_then(|r| r.obj);
                self.find_on(
                    "parent-tree-missing",
                    Severity::Error,
                    "the structure tree references content but has no usable /ParentTree".into(),
                    root,
                    None,
                );
            }
            return;
        }
        self.check_page_mappings();
        self.check_object_mappings();
    }

    /// Records, per element, which parent tree entries name it.
    fn index_parent_tree(&mut self) {
        let mut refs: HashMap<usize, Vec<PtRef>> = HashMap::new();
        for (&key, entry) in &self.tree.parent_tree {
            match entry {
                PtEntry::Array(slots) => {
                    for (mcid, slot) in (0..).zip(slots) {
                        if let Some(&ei) = slot.as_ref().and_then(|r| self.tree.index.get(r)) {
                            refs.entry(ei).or_default().push(PtRef {
                                key,
                                mcid: Some(mcid),
                            });
                        }
                    }
                }
                PtEntry::Object(r) => {
                    if let Some(&ei) = self.tree.index.get(r) {
                        refs.entry(ei).or_default().push(PtRef { key, mcid: None });
                    }
                }
            }
        }
        self.tree.pt_refs = refs;
    }

    fn check_page_mappings(&mut self) {
        let doc = self.rd.doc;
        let mut by_page: BTreeMap<PageIx, Vec<(i32, usize)>> = BTreeMap::new();
        for (&(page, stream, mcid), &ei) in &self.seen_content {
            // Content in a form XObject is mapped by the form's own /StructParents.
            if stream.is_none() {
                by_page.entry(page).or_default().push((mcid, ei));
            }
        }
        for (page, owned) in by_page {
            let page_obj = doc.pages.get(usize_from_u32(page)).and_then(|p| p.obj);
            let label = page + 1;
            let Some(&key) = self.tree.page_keys.get(&page) else {
                self.find(
                    "struct-parent-missing",
                    Severity::Warning,
                    format!(
                        "page {label} has tagged content but no /StructParents, \
                         so the ParentTree cannot map its MCIDs to elements"
                    ),
                    page_obj,
                );
                continue;
            };
            let slots = match self.tree.parent_tree.get(&key).cloned() {
                Some(PtEntry::Array(slots)) => slots,
                Some(PtEntry::Object(_)) => {
                    self.find(
                        "parent-tree-mismatch",
                        Severity::Error,
                        format!("/StructParents {key} of page {label} points at an object entry, not an MCID array"),
                        page_obj,
                    );
                    continue;
                }
                None => {
                    self.find(
                        "struct-parent-missing",
                        Severity::Warning,
                        format!("/StructParents {key} of page {label} has no entry in the ParentTree"),
                        page_obj,
                    );
                    continue;
                }
            };
            let in_content = self.content_mcids(page);
            for (mcid, ei) in owned {
                if in_content
                    .as_ref()
                    .is_some_and(|c| c.binary_search(&mcid).is_err())
                {
                    continue;
                }
                let slot = usize::try_from(mcid)
                    .ok()
                    .and_then(|i| slots.get(i))
                    .copied()
                    .flatten();
                let owner = self.tree.elems[ei].obj;
                if slot != Some(owner) {
                    let says = slot.map_or_else(|| "no element".to_owned(), |r| r.to_string());
                    self.find_on(
                        "parent-tree-mismatch",
                        Severity::Error,
                        format!(
                            "ParentTree key {key} names {says} for MCID {mcid} on page {label}, \
                             but {owner} references it in /K"
                        ),
                        Some(owner),
                        Some(ei),
                    );
                }
            }
            for (mcid, slot) in (0..).zip(&slots) {
                let Some(named) = *slot else { continue };
                if self.tree.owners.contains_key(&(page, mcid)) {
                    continue;
                }
                let at = self.tree.index.get(&named).copied();
                self.find_on(
                    "parent-tree-mismatch",
                    Severity::Error,
                    format!(
                        "ParentTree key {key} names {named} for MCID {mcid} on page {label}, \
                         but no element references that MCID in /K"
                    ),
                    Some(named),
                    at,
                );
            }
        }
    }

    /// Annotations and other objects referenced by `OBJR`s against `/StructParent`.
    fn check_object_mappings(&mut self) {
        let rd = self.rd;
        let mut targets: Vec<(usize, ObjRef)> = Vec::new();
        for (ei, elem) in self.tree.elems.iter().enumerate() {
            for kid in &elem.kids {
                if let Kid::Object { obj: Some(o), .. } = kid {
                    targets.push((ei, *o));
                }
            }
        }
        for (ei, target) in targets {
            let owner = self.tree.elems[ei].obj;
            let Some(dict) = rd.object(target).and_then(Object::as_dict) else {
                self.find_on(
                    "objr-target-missing",
                    Severity::Warning,
                    format!("{owner} has an OBJR for {target}, which is not a dictionary object"),
                    Some(owner),
                    Some(ei),
                );
                continue;
            };
            let Some(key) = rd.int(dict, b"StructParent").and_then(|k| i32::try_from(k).ok()) else {
                self.find_on(
                    "struct-parent-missing",
                    Severity::Warning,
                    format!("{target} is referenced by an OBJR of {owner} but has no /StructParent"),
                    Some(target),
                    Some(ei),
                );
                continue;
            };
            match self.tree.parent_tree.get(&key) {
                Some(PtEntry::Object(r)) if *r == owner => {}
                Some(PtEntry::Object(r)) => {
                    let named = *r;
                    self.find_on(
                        "parent-tree-mismatch",
                        Severity::Error,
                        format!(
                            "ParentTree key {key} names {named} as the parent of {target}, \
                             but {owner} references it with an OBJR"
                        ),
                        Some(target),
                        Some(ei),
                    );
                }
                Some(PtEntry::Array(_)) => {
                    self.find_on(
                        "parent-tree-mismatch",
                        Severity::Error,
                        format!(
                            "/StructParent {key} of {target} points at an MCID array, not an object entry"
                        ),
                        Some(target),
                        Some(ei),
                    );
                }
                None => {
                    self.find_on(
                        "struct-parent-missing",
                        Severity::Warning,
                        format!("/StructParent {key} of {target} has no entry in the ParentTree"),
                        Some(target),
                        Some(ei),
                    );
                }
            }
        }
    }

    /// The MCIDs found in a page's content, or `None` when the page could not be captured.
    fn content_mcids(&self, page: PageIx) -> Option<Vec<i32>> {
        let doc = self.rd.doc;
        doc.captured(page)?;
        Some(doc.mcids_on_page(page))
    }

    /// Structure against content: referenced-but-absent MCIDs, orphan MCIDs and untagged runs.
    pub(super) fn check_content(&mut self) {
        let doc = self.rd.doc;
        let contents: Vec<Option<Vec<i32>>> = (0..doc.pages.len())
            .map(|p| self.content_mcids(PageIx::try_from(p).unwrap_or(PageIx::MAX)))
            .collect();
        let referenced: Vec<(PageIx, i32, usize)> = self
            .seen_content
            .iter()
            .filter(|((_, stream, _), _)| stream.is_none())
            .map(|(&(page, _, mcid), &ei)| (page, mcid, ei))
            .collect();
        for (page, mcid, ei) in referenced {
            let Some(Some(mcids)) = contents.get(usize_from_u32(page)) else {
                continue;
            };
            if mcids.binary_search(&mcid).is_err() {
                let owner = self.tree.elems[ei].obj;
                self.find_on(
                    "mcid-not-in-content",
                    Severity::Error,
                    format!(
                        "{owner} references MCID {mcid} on page {}, but that page's content has no such sequence",
                        page + 1
                    ),
                    Some(owner),
                    Some(ei),
                );
            }
        }
        for (page, mcids) in (0..).zip(&contents) {
            let Some(mcids) = mcids else { continue };
            let page_obj = doc.pages.get(usize_from_u32(page)).and_then(|p| p.obj);
            let mut unique = mcids.clone();
            unique.sort_unstable();
            unique.dedup();
            for mcid in unique {
                if !self.tree.owners.contains_key(&(page, mcid)) {
                    self.find(
                        "mcid-not-in-structure",
                        Severity::Error,
                        format!(
                            "MCID {mcid} on page {} is not referenced by any structure element (orphan content)",
                            page + 1
                        ),
                        page_obj,
                    );
                }
            }
            if let Some(captured) = doc.captured(page) {
                for (mcid, count) in duplicate_mcids(captured) {
                    self.find(
                        "mcid-duplicate-in-content",
                        Severity::Warning,
                        format!(
                            "MCID {mcid} is used by {count} marked-content sequences in the content of \
                             page {}",
                            page + 1
                        ),
                        page_obj,
                    );
                }
                for (op, tag) in nested_in_artifact(captured) {
                    self.find(
                        "marked-content-in-artifact",
                        Severity::Warning,
                        format!(
                            "operator {op} on page {} opens /{tag} inside an artifact; content inside an \
                             artifact is not part of the structure",
                            page + 1
                        ),
                        page_obj,
                    );
                }
                for (first, end) in untagged_runs(captured) {
                    self.find(
                        "untagged-content",
                        Severity::Warning,
                        format!(
                            "operators {first}..{end} on page {} paint outside any tagged or artifact \
                             marked-content sequence",
                            page + 1
                        ),
                        page_obj,
                    );
                }
            }
        }
    }
}

/// MCIDs that more than one sequence of the page's own content uses, with their counts, in
/// ascending MCID order. Sequences inside form `XObject`s are not compared: a form drawn twice
/// legitimately repeats its MCIDs, and the capture does not tell the invocations apart.
fn duplicate_mcids(page: &CapturedPage) -> Vec<(i32, usize)> {
    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for seq in &page.marked {
        if let (Some(mcid), StreamRef::PageContent { .. }) = (seq.mcid, &seq.stream) {
            *counts.entry(mcid).or_default() += 1;
        }
    }
    counts.into_iter().filter(|&(_, count)| count > 1).collect()
}

/// Marked-content sequences that start inside an artifact: `(opening operator, tag)`.
fn nested_in_artifact(page: &CapturedPage) -> Vec<(u32, &str)> {
    let inside_artifact = |seq: &MarkedSeq| {
        let mut cur = seq.parent;
        // The parent chain cannot loop (a sequence opens inside an earlier one), but bound it anyway.
        for _ in 0..page.marked.len() {
            let Some(parent) = cur.and_then(|i| page.marked.get(usize_from_u32(i))) else {
                return false;
            };
            if parent.artifact.is_some() {
                return true;
            }
            cur = parent.parent;
        }
        false
    };
    page.marked
        .iter()
        .filter(|seq| inside_artifact(seq))
        .map(|seq| (seq.begin_op, seq.tag.as_str()))
        .collect()
}

/// Runs of painting operators outside every tagged or artifact marked-content sequence, as
/// half-open operator index ranges. A run ends at any `BMC`, `BDC` or `EMC`.
fn untagged_runs(page: &CapturedPage) -> Vec<(u32, u32)> {
    let covered = |mc_stack: &[u32]| {
        mc_stack.iter().any(|&i| {
            page.marked
                .get(usize_from_u32(i))
                .is_some_and(|m| m.mcid.is_some() || m.artifact.is_some())
        })
    };
    let marked_content_between = |from: u32, to: u32| {
        page.ops
            .get(usize_from_u32(from) + 1..usize_from_u32(to))
            .is_some_and(|ops| {
                ops.iter()
                    .any(|o| matches!(o.operator.as_str(), "BMC" | "BDC" | "EMC"))
            })
    };
    let mut runs = Vec::new();
    let mut open: Option<(u32, u32)> = None;
    for item in page.items.iter().filter(|i| !matches!(i.kind, PaintKind::Clip)) {
        if covered(&item.mc_stack) {
            runs.extend(open.take().map(|(a, b)| (a, b + 1)));
            continue;
        }
        open = match open {
            Some((first, last)) if !marked_content_between(last, item.op) => Some((first, item.op)),
            Some((first, last)) => {
                runs.push((first, last + 1));
                Some((item.op, item.op))
            }
            None => Some((item.op, item.op)),
        };
    }
    runs.extend(open.map(|(a, b)| (a, b + 1)));
    runs
}
