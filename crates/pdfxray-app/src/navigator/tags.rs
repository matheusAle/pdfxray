//! The Tags entry: the structure tree.
//!
//! Rows are quiet. Each shows the element's type and a short preview of its text (`H1 Annual
//! Report 2025`); the object reference (`14 0 R`) shows only on the selected row and the one
//! under the pointer. An `Alt` warning (the warning mark and the word) marks a figure without
//! alternative text. A type that the `/RoleMap` sends to a standard one says so in a hover
//! (`Heading1 → H1 (pdf2 ns)`). The dot in front of a row is the role colour of the element's
//! class ([`crate::roles`]).

use egui::Ui;
use pdfxray_core::{Document, NodeId, TreeKind, TreeNode};

use super::tree_body;
use crate::components::{Annotation, Tone};
use crate::roles::role_color;
use crate::state::derived::{Derived, TagIndex, split_role};
use crate::state::{AppState, NavTree};
use crate::tree_view::{Decor, TreeModel, badge_word};

/// Shown when the file has no structure tree.
pub const NO_STRUCTURE: &str = "This file has no structure tree.";

/// Text of the warning on a figure without `/Alt`; the mark in front of it is painted.
pub const ALT_MISSING: &str = "Alt";

/// Longest text preview shown after the type of an element.
pub const PREVIEW_CHARS: usize = 28;

/// The structure tree as a model: rows drawn as described above.
pub(crate) fn model(doc: &Document, derived: &Derived) -> TreeModel {
    let index = derived.tags();
    TreeModel::with_decor(doc.tree(TreeKind::Tags), |node| decor(doc, index, node))
}

/// What a mapped type says on hover: `Heading1 → H1 (pdf2 ns)`.
pub fn role_map_text(index: Option<&TagIndex>, role: &str, standard: &str) -> String {
    let namespace = index
        .and_then(|index| {
            index
                .role_map
                .iter()
                .find(|entry| entry.role == role && entry.standard == standard)
        })
        .and_then(|entry| entry.namespace.as_deref());
    match namespace {
        Some(namespace) => format!("{role} → {standard} ({namespace})"),
        None => format!("{role} → {standard}"),
    }
}

fn decor(doc: &Document, index: Option<&TagIndex>, node: &TreeNode) -> Decor {
    match &node.id {
        NodeId::StructElem(object) => {
            let (role, standard) = split_role(&node.label);
            let preview = doc.text_preview(&node.id, PREVIEW_CHARS);
            let mut decor = Decor {
                label: Some(role.to_owned()),
                detail_when_hot: Some(object.to_string()),
                hover_text: standard.map(|standard| role_map_text(index, role, standard)),
                // The dot is the role colour of the element's class, as on the canvas.
                dot: doc.role_class(&node.id).map(role_color),
                ..Decor::default()
            };
            if let Some(preview) = preview {
                decor.search.push(' ');
                decor.search.push_str(&preview);
                decor.detail = Some(preview);
            }
            if index.is_some_and(|index| index.lacks_alt(&node.id)) {
                // The diagnostic of the figure marks the row with the same warning already.
                let marked = node.badges.iter().any(|b| badge_word(&b.code) == ALT_MISSING);
                if !marked {
                    decor
                        .right
                        .push(Annotation::Tag(ALT_MISSING.to_owned(), Tone::Warn));
                }
                decor.search.push_str(" alt missing");
            }
            decor
        }
        NodeId::MarkedContent { page, .. } => Decor {
            detail: Some(format!("p{}", page + 1)),
            search: node.detail.clone().unwrap_or_default(),
            content_ref: true,
            ..Decor::default()
        },
        // The only plain objects in the tags tree are the `OBJR`s of the elements.
        NodeId::Object(_) => Decor {
            content_ref: true,
            ..Decor::default()
        },
        _ => Decor::default(),
    }
}

/// The count after the header title: `1,284 elems`.
pub(super) fn count(state: &AppState) -> Option<usize> {
    state.tag_index().map(|index| index.elements)
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    tree_body(ui, state, NavTree::Core(TreeKind::Tags), &[], NO_STRUCTURE);
}
