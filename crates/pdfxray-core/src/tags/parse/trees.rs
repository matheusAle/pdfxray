//! The number and name trees: the parent tree, the ID tree, and each page's `/StructParents`.

use std::collections::BTreeSet;

use pdfxray_raw::{Dict, Object};

use super::super::PtEntry;
use super::{Builder, MAX_TREE_DEPTH, text};
use crate::ObjRef;

impl<'a> Builder<'a> {
    // ---- number and name trees --------------------------------------------------------------

    pub(super) fn parse_parent_tree(&mut self, root: &'a Dict) {
        let rd = self.rd;
        self.tree.parent_tree_next_key = rd.int(root, b"ParentTreeNextKey");
        let Some(node) = root.get(b"ParentTree") else {
            return;
        };
        self.tree.parent_tree_present = true;
        let mut leaves = Vec::new();
        self.number_tree(&node.value, &mut leaves, &mut BTreeSet::new(), 0);
        for (key, value) in leaves {
            let Ok(key) = i32::try_from(key) else { continue };
            let entry = match value {
                Object::Ref(r) => match rd.object(*r) {
                    Some(Object::Array(items)) => Some(PtEntry::Array(array_slots(items))),
                    Some(Object::Dict(_)) => Some(PtEntry::Object(*r)),
                    _ => None,
                },
                Object::Array(items) => Some(PtEntry::Array(array_slots(items))),
                _ => None,
            };
            if let Some(entry) = entry {
                self.tree.parent_tree.entry(key).or_insert(entry);
            }
        }
    }

    /// Collects the `(key, value)` leaves of a number tree in tree order.
    fn number_tree(
        &self,
        node: &'a Object,
        out: &mut Vec<(i64, &'a Object)>,
        seen: &mut BTreeSet<ObjRef>,
        depth: usize,
    ) {
        let rd = self.rd;
        if depth > MAX_TREE_DEPTH {
            return;
        }
        if let Object::Ref(r) = node
            && !seen.insert(*r)
        {
            return;
        }
        let Some(d) = rd.dict(node) else { return };
        if let Some(Object::Array(nums)) = rd.get(d, b"Nums") {
            for pair in nums.as_chunks::<2>().0 {
                if let Some(key) = rd.deref(&pair[0].value).as_int() {
                    out.push((key, &pair[1].value));
                }
            }
        }
        if let Some(kids) = d.get(b"Kids") {
            for kid in rd.list(&kids.value) {
                self.number_tree(kid, out, seen, depth + 1);
            }
        }
    }

    pub(super) fn parse_id_tree(&mut self, root: &'a Dict) {
        let rd = self.rd;
        let Some(node) = root.get(b"IDTree") else {
            return;
        };
        let mut leaves = Vec::new();
        self.name_tree(&node.value, &mut leaves, &mut BTreeSet::new(), 0);
        for (key, value) in leaves {
            if let Some(r) = value.as_ref_obj() {
                self.tree.id_tree.entry(key).or_insert(r);
            }
        }
        let _ = rd;
    }

    /// Collects the `(key, value)` leaves of a name tree in tree order.
    fn name_tree(
        &self,
        node: &'a Object,
        out: &mut Vec<(String, &'a Object)>,
        seen: &mut BTreeSet<ObjRef>,
        depth: usize,
    ) {
        let rd = self.rd;
        if depth > MAX_TREE_DEPTH {
            return;
        }
        if let Object::Ref(r) = node
            && !seen.insert(*r)
        {
            return;
        }
        let Some(d) = rd.dict(node) else { return };
        if let Some(Object::Array(names)) = rd.get(d, b"Names") {
            for pair in names.as_chunks::<2>().0 {
                if let Object::String(s) = rd.deref(&pair[0].value) {
                    out.push((text::decode(&s.bytes), &pair[1].value));
                }
            }
        }
        if let Some(kids) = d.get(b"Kids") {
            for kid in rd.list(&kids.value) {
                self.name_tree(kid, out, seen, depth + 1);
            }
        }
    }

    pub(super) fn parse_page_keys(&mut self) {
        let rd = self.rd;
        for page in &rd.doc.pages {
            let key = page
                .obj
                .and_then(|o| rd.object(o))
                .and_then(Object::as_dict)
                .and_then(|d| rd.int(d, b"StructParents"))
                .and_then(|k| i32::try_from(k).ok());
            if let Some(key) = key {
                self.tree.page_keys.insert(page.index, key);
            }
        }
    }
}

fn array_slots(items: &[pdfxray_raw::Spanned<Object>]) -> Vec<Option<ObjRef>> {
    items.iter().map(|i| i.value.as_ref_obj()).collect()
}
