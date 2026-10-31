//! Resources (fonts, `XObjects`, colour spaces, patterns, shadings, `ExtGStates`, properties) by
//! owner, with inheritance and usage counts.
//!
//! Contract with the `document` module (keep these signatures):
//! `Document::{resources_tree, resource_props}` for `TreeKind::Resources` and
//! `NodeId::Resource`.
//!
//! # Owners
//!
//! A resource's owner (`NodeId::Resource::owner`) is the object whose content stream selects the
//! name: a page, or a Form `XObject` / tiling pattern / Type 3 font that has its own `/Resources`.
//! A page that inherits its dictionary from a `/Pages` ancestor is still its own owner, so names
//! and usage counts stay per page; `Scope::holder` records the ancestor that carries the
//! dictionary. A Form `XObject` without `/Resources` is not an owner: its names resolve in the
//! resources of whatever painted it, and its operators count for that painter.

mod describe;
mod fonts;
mod format;
mod refs;
mod scope;
mod usage;
mod visual;

pub use self::fonts::FontFacts;
pub use self::refs::{ObjectRefs, UsedResource};
pub use self::visual::{
    ColorSpaceVisual, FontProgram, FontVisual, FormVisual, ImageVisual, MAX_PREVIEW_GLYPHS, PatternVisual,
    SPECIMEN, ShadingStop, ShadingVisual, Visual, VisualKind,
};

use std::collections::{BTreeMap, BTreeSet};

use pdfxray_raw::Object;

use self::describe::Lens;
use self::scope::{Scope, ScopeKind};
use self::usage::{MAX_OP_LINKS, Usage};
use crate::numeric::usize_from_u32;
use crate::{Badge, Document, NodeId, ObjRef, PropGroup, Props, ResKind, Severity, TreeNode};

/// Id of the "Shared" group.
const SHARED_GROUP: u32 = 200;
/// First id handed out to category groups.
const FIRST_CATEGORY_GROUP: u32 = 201;

/// Names of Form `XObjects`, patterns and fonts as the first resource dictionary calls them.
type Names = BTreeMap<ObjRef, String>;

impl Document {
    /// Resources grouped by owner (page / form / pattern / Type 3 font) and category.
    pub(crate) fn resources_tree(&self) -> Vec<TreeNode> {
        let scopes = self.all_scopes();
        let usage = Usage::collect(self, None, &self.forms_with_resources());
        let names = resource_names(&scopes);
        let lens = Lens::new(&self.raw);
        let mut next_group = FIRST_CATEGORY_GROUP;
        let mut tree: Vec<TreeNode> = scopes
            .iter()
            .map(|s| scope_node(&lens, s, &usage, &names, &mut next_group))
            .collect();
        tree.extend(shared_node(&lens, &scopes, &usage, &names));
        tree
    }

    /// Props for `Resource` nodes.
    pub(crate) fn resource_props(&self, id: &NodeId) -> Option<Props> {
        let NodeId::Resource { owner, kind, name } = id else {
            return None;
        };
        let scope = self.scope_of_owner(*owner)?;
        let entry = scope.entry(*kind, name)?;
        let lens = Lens::new(&self.raw);
        let page = match scope.kind {
            ScopeKind::Page(p) => Some(p),
            _ => None,
        };
        let usage = Usage::collect(self, page, &self.forms_with_resources());

        let mut resource = PropGroup::new("Resource")
            .row("Name", format!("/{name}"))
            .row("Category", kind.key());
        resource = match scope.kind {
            ScopeKind::Page(p) => resource.link("Owner", format!("Page {}", p + 1), NodeId::Page(p)),
            other => resource.link(
                "Owner",
                format!("{} {owner}", other.word()),
                NodeId::Object(*owner),
            ),
        };
        resource = match scope.holder {
            Some(holder) if holder != *owner => resource.link(
                "Defined",
                format!("inherited from {holder}"),
                NodeId::Object(holder),
            ),
            _ => resource.row("Defined", "directly"),
        };
        resource = match entry.target {
            Some(t) => resource.link("Object", t.to_string(), NodeId::Object(t)),
            None => resource.row("Object", "direct (inline)"),
        };

        let mut groups = vec![resource];
        groups.extend(lens.details(*kind, &entry.value));
        groups.push(self.usage_group(&usage, *owner, *kind, name));
        Some(Props {
            id: id.clone(),
            title: format!("{} /{name}", kind.key()),
            kind: "resource",
            groups,
        })
    }

    /// The "Usage" group: how many operators select the resource and the first of them.
    fn usage_group(&self, usage: &Usage, owner: ObjRef, kind: ResKind, name: &str) -> PropGroup {
        let group = PropGroup::new("Usage");
        let Some(ops) = usage.ops(owner, kind, name) else {
            return group.row("Count", "unknown (the content of this owner was not interpreted)");
        };
        let mut group = group.row("Count", ops.len().to_string());
        for id in ops.iter().take(MAX_OP_LINKS) {
            let NodeId::Op { page, ix } = id else {
                continue;
            };
            let record = self
                .captured(*page)
                .and_then(|c| c.ops.get(usize_from_u32(*ix)))
                .filter(|r| r.ix == *ix);
            let (operator, text) = match record {
                Some(r) => (r.operator.clone(), r.operands.join(" ")),
                None => ("op".to_owned(), String::new()),
            };
            group = group.link(
                operator,
                format!("{text} (page {}, op {ix})", page + 1),
                id.clone(),
            );
        }
        if ops.len() > MAX_OP_LINKS {
            group = group.row("More", format!("{} more operators", ops.len() - MAX_OP_LINKS));
        }
        group
    }
}

/// The first name each Form `XObject`, pattern and font is known by.
fn resource_names(scopes: &[Scope]) -> Names {
    let mut names = Names::new();
    for scope in scopes {
        for (kind, entries) in &scope.categories {
            if !matches!(kind, ResKind::XObject | ResKind::Pattern | ResKind::Font) {
                continue;
            }
            for e in entries {
                if let Some(t) = e.target {
                    names.entry(t).or_insert_with(|| e.name.clone());
                }
            }
        }
    }
    names
}

/// `Form /Fm1` or `Form 12 0 R` for owners that are not pages, `Page 2` for pages.
fn owner_label(scope: &Scope, names: &Names) -> String {
    match scope.kind {
        ScopeKind::Page(p) => format!("Page {}", p + 1),
        other => match names.get(&scope.owner) {
            Some(n) => format!("{} /{n}", other.word()),
            None => format!("{} {}", other.word(), scope.owner),
        },
    }
}

fn scope_node(
    lens: &Lens<'_>,
    scope: &Scope,
    usage: &Usage,
    names: &Names,
    next_group: &mut u32,
) -> TreeNode {
    let (id, label, detail) = match scope.kind {
        ScopeKind::Page(p) => {
            let detail = match scope.holder {
                Some(h) if h != scope.owner => Some(format!("inherited from {h}")),
                Some(_) => None,
                None => Some("no /Resources".to_owned()),
            };
            (NodeId::Page(p), format!("Page {} resources", p + 1), detail)
        }
        other => {
            let label = match names.get(&scope.owner) {
                Some(n) => format!("{} /{n} ({})", other.word(), scope.owner),
                None => format!("{} ({})", other.word(), scope.owner),
            };
            (NodeId::Object(scope.owner), label, None)
        }
    };
    let children = scope
        .categories
        .iter()
        .map(|(kind, entries)| {
            let group = *next_group;
            *next_group += 1;
            let nodes = entries
                .iter()
                .map(|e| entry_node(lens, scope, *kind, e, usage))
                .collect();
            TreeNode::leaf(NodeId::Group(group), kind.key())
                .with_detail(entries.len().to_string())
                .with_children(nodes)
        })
        .collect();
    let node = TreeNode::leaf(id, label).with_children(children);
    match detail {
        Some(d) => node.with_detail(d),
        None => node,
    }
}

fn entry_node(
    lens: &Lens<'_>,
    scope: &Scope,
    kind: ResKind,
    entry: &scope::Entry,
    usage: &Usage,
) -> TreeNode {
    let id = NodeId::Resource {
        owner: scope.owner,
        kind,
        name: entry.name.clone(),
    };
    let mut detail = lens.summary(kind, &entry.value);
    let mut badges = Vec::new();
    if let Some(ops) = usage.ops(scope.owner, kind, &entry.name) {
        match ops.len() {
            0 => badges.push(Badge {
                code: "unused-resource".to_owned(),
                severity: Severity::Info,
            }),
            1 => detail = format!("{detail} · 1 use"),
            n => detail = format!("{detail} · {n} uses"),
        }
    }
    let mut node = TreeNode::leaf(id, format!("/{}", entry.name)).with_detail(detail);
    node.badges = badges;
    node
}

/// Resource objects named by more than one owner, as a "Shared" group.
fn shared_node(lens: &Lens<'_>, scopes: &[Scope], usage: &Usage, names: &Names) -> Option<TreeNode> {
    struct Sharing<'s> {
        kind: ResKind,
        owners: Vec<&'s Scope>,
        uses: Vec<Option<usize>>,
    }
    let mut by_target: BTreeMap<ObjRef, Sharing<'_>> = BTreeMap::new();
    for scope in scopes {
        for (kind, entries) in &scope.categories {
            for e in entries {
                let Some(target) = e.target else { continue };
                let sharing = by_target.entry(target).or_insert_with(|| Sharing {
                    kind: *kind,
                    owners: Vec::new(),
                    uses: Vec::new(),
                });
                if !sharing.owners.iter().any(|o| o.owner == scope.owner) {
                    sharing.owners.push(scope);
                }
                sharing
                    .uses
                    .push(usage.ops(scope.owner, *kind, &e.name).map(<[NodeId]>::len));
            }
        }
    }
    let children: Vec<TreeNode> = by_target
        .into_iter()
        .filter(|(_, s)| s.owners.iter().map(|o| o.owner).collect::<BTreeSet<_>>().len() > 1)
        .map(|(target, s)| {
            let users: Vec<String> = s.owners.iter().map(|o| owner_label(o, names)).collect();
            let mut detail = format!(
                "{}; used by {}: {}",
                lens.summary(s.kind, &Object::Ref(target)),
                users.len(),
                users.join(", ")
            );
            if let Some(total) = s.uses.iter().copied().sum::<Option<usize>>() {
                detail = format!("{detail} · {total} uses");
            }
            TreeNode::leaf(NodeId::Object(target), format!("{} {target}", s.kind.key())).with_detail(detail)
        })
        .collect();
    if children.is_empty() {
        return None;
    }
    Some(
        TreeNode::leaf(NodeId::Group(SHARED_GROUP), "Shared")
            .with_detail(children.len().to_string())
            .with_children(children),
    )
}
