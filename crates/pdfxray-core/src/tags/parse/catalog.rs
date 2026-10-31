//! The catalog's `/StructTreeRoot`, `/MarkInfo` and `/Lang`, the role map, the class map and the
//! namespaces.

use pdfxray_raw::{Dict, Object};

use super::super::{AttrSource, MarkInfo, Namespace, RoleTarget, Root};
use super::{Builder, Ctx};
use crate::{ObjRef, Severity};

impl<'a> Builder<'a> {
    // ---- catalog and root -------------------------------------------------------------------

    pub(super) fn parse_catalog(&mut self) {
        let rd = self.rd;
        let catalog_ref = rd
            .doc
            .raw
            .trailer()
            .and_then(|t| t.get(b"Root"))
            .and_then(|v| v.value.as_ref_obj());
        let Some(catalog) = catalog_ref.and_then(|r| rd.object(r)).and_then(Object::as_dict) else {
            return;
        };
        self.tree.catalog_lang = rd.text(catalog, b"Lang");
        self.tree.mark_info = rd
            .get(catalog, b"MarkInfo")
            .and_then(Object::as_dict)
            .map(|m| MarkInfo {
                marked: rd.boolean(m, b"Marked"),
                user_properties: rd.boolean(m, b"UserProperties"),
                suspects: rd.boolean(m, b"Suspects"),
            });
        let marked = self.tree.mark_info.and_then(|m| m.marked);
        let root_value = catalog.get(b"StructTreeRoot").map(|v| &v.value);
        let Some(root) = root_value.and_then(|v| rd.dict(v)) else {
            if marked == Some(true) {
                self.find(
                    "marked-true-no-struct-tree",
                    Severity::Warning,
                    "/MarkInfo /Marked is true but the catalog has no usable /StructTreeRoot".into(),
                    catalog_ref,
                );
            }
            return;
        };
        let root_ref = root_value.and_then(Object::as_ref_obj);
        self.tree.root = Some(Root {
            obj: root_ref,
            kids: Vec::new(),
            diags: Vec::new(),
        });
        if marked != Some(true) {
            let state = match marked {
                Some(false) => "false",
                _ => "missing",
            };
            self.find_on(
                "marked-false-but-tagged",
                Severity::Warning,
                format!("the document has a /StructTreeRoot but /MarkInfo /Marked is {state}"),
                catalog_ref,
                None,
            );
        }
        self.parse_role_map(root);
        self.parse_class_map(root);
        self.parse_namespaces(root);
        let ctx = Ctx {
            owner_ref: root_ref,
            owner_idx: None,
            page: None,
            depth: 0,
        };
        let kids = self.parse_kids(root.get(b"K").map(|v| &v.value), ctx);
        if let Some(r) = &mut self.tree.root {
            r.kids = kids;
        }
        self.parse_id_tree(root);
        self.parse_parent_tree(root);
        self.parse_page_keys();
    }

    fn parse_role_map(&mut self, root: &'a Dict) {
        let rd = self.rd;
        let Some(map) = rd.get(root, b"RoleMap").and_then(Object::as_dict) else {
            return;
        };
        for (k, v) in &map.entries {
            if let Some(target) = rd.deref(&v.value).as_name() {
                self.tree
                    .role_map
                    .entry(k.value.to_string_lossy())
                    .or_insert_with(|| target.to_string_lossy());
            }
        }
    }

    fn parse_class_map(&mut self, root: &'a Dict) {
        let rd = self.rd;
        let Some(map) = rd.get(root, b"ClassMap").and_then(Object::as_dict) else {
            return;
        };
        for (k, v) in &map.entries {
            let sets = self.attr_sets(&v.value, &AttrSource::Direct);
            self.tree
                .class_map
                .entry(k.value.to_string_lossy())
                .or_insert(sets);
        }
    }

    fn parse_namespaces(&mut self, root: &'a Dict) {
        for r in self.rd.refs(root, b"Namespaces") {
            self.ns_index(r);
        }
    }

    /// The index of the namespace dictionary `r`, reading it on first use.
    pub(super) fn ns_index(&mut self, r: ObjRef) -> usize {
        if let Some(i) = self.tree.namespaces.iter().position(|n| n.obj == Some(r)) {
            return i;
        }
        let rd = self.rd;
        let i = self.tree.namespaces.len();
        // Placeholder first so a `RoleMapNS` that names its own namespace terminates.
        self.tree.namespaces.push(Namespace {
            obj: Some(r),
            ..Namespace::default()
        });
        let Some(d) = rd.object(r).and_then(Object::as_dict) else {
            return i;
        };
        let uri = rd.text(d, b"NS").unwrap_or_default();
        let mut role_map = Vec::new();
        if let Some(map) = rd.get(d, b"RoleMapNS").and_then(Object::as_dict) {
            for (k, v) in &map.entries {
                let items = rd.list(&v.value);
                let target = match rd.deref(&v.value) {
                    Object::Array(_) => items.first().and_then(|n| rd.deref(n).as_name()),
                    other => other.as_name(),
                };
                let Some(target) = target else { continue };
                let ns = match rd.deref(&v.value) {
                    Object::Array(_) => items.get(1).and_then(|n| n.as_ref_obj()),
                    _ => None,
                };
                let ns = ns.map(|n| self.ns_index(n));
                role_map.push((
                    k.value.to_string_lossy(),
                    RoleTarget {
                        role: target.to_string_lossy(),
                        ns,
                    },
                ));
            }
        }
        let ns = &mut self.tree.namespaces[i];
        ns.uri = uri;
        ns.role_map = role_map;
        i
    }
}
