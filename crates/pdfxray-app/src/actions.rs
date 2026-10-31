//! What can be done with a node, wherever it is shown: the canvas selection bar, right-click
//! menus on tree rows, the canvas and inspector values all list [`Action::for_node`] and run
//! [`run`], so the same node offers the same actions everywhere.

use std::ops::Range;

use egui::Context;
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{NodeId, ObjRef};

use crate::components::Icon;
use crate::modes::IsolateMode;
use crate::state::{AppState, RailItem};

/// Something to do with a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Paint only the node's content.
    Isolate,
    /// Paint everything except the node's content.
    Hide,
    /// Select the node in the navigator that lists it.
    ShowInTree,
    /// Open the object behind the node in the object view.
    Object,
    /// The object's bytes as they are in the file.
    RawBytes,
    /// The raw view with the stream's data marked.
    StreamBytes,
    /// Copy the node's id (`struct:14:0`).
    CopyId,
    /// Copy everything the inspector shows about the node, as JSON.
    CopyJson,
}

impl Action {
    /// Every action, in menu order.
    pub const ALL: [Self; 8] = [
        Self::Isolate,
        Self::Hide,
        Self::ShowInTree,
        Self::Object,
        Self::RawBytes,
        Self::StreamBytes,
        Self::CopyId,
        Self::CopyJson,
    ];

    /// Name in menus and the accessible name of buttons.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Isolate => "Isolate",
            Self::Hide => "Hide",
            Self::ShowInTree => "Show in tree",
            Self::Object => "Object",
            Self::RawBytes => "Raw bytes",
            Self::StreamBytes => "Stream bytes",
            Self::CopyId => "Copy id",
            Self::CopyJson => "Copy JSON",
        }
    }

    /// Icon in menus and on the selection bar.
    pub const fn icon(self) -> Icon {
        match self {
            Self::Isolate => Icon::Isolate,
            Self::Hide => Icon::Hide,
            Self::ShowInTree => Icon::Tags,
            Self::Object => Icon::Braces,
            Self::RawBytes | Self::StreamBytes => Icon::Bytes,
            Self::CopyId | Self::CopyJson => Icon::Copy,
        }
    }

    /// The key that does the same, shown as a keycap.
    pub const fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::Isolate => Some("I"),
            Self::Hide => Some("H"),
            _ => None,
        }
    }

    /// The actions that make sense for `id`, in menu order.
    pub fn for_node(state: &AppState, id: &NodeId) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|action| match action {
                Self::Isolate | Self::Hide => paints(id),
                Self::ShowInTree => tree_of(id).is_some(),
                Self::Object | Self::RawBytes => object_of(state, id).is_some(),
                Self::StreamBytes => stream_range(state, id).is_some(),
                Self::CopyId => true,
                Self::CopyJson => has_props(state, id),
            })
            .collect()
    }
}

/// True for nodes whose content can be isolated or hidden on the canvas.
fn paints(id: &NodeId) -> bool {
    matches!(
        id,
        NodeId::StructElem(_)
            | NodeId::MarkedContent { .. }
            | NodeId::Artifact { .. }
            | NodeId::Op { .. }
            | NodeId::Glyph { .. }
    )
}

/// Can the inspector show `id`? Decided from the id and the sizes the app already holds, as the
/// actions of a node are listed every frame the action bar is up: building its properties is for
/// [`run`] to do when the user asks for the JSON.
fn has_props(state: &AppState, id: &NodeId) -> bool {
    let Some(loaded) = &state.document else {
        return false;
    };
    match id {
        NodeId::Page(page) | NodeId::PageBox { page, .. } => loaded.doc.page(*page).is_some(),
        NodeId::Diagnostic(ix) => usize_from_u32(*ix) < loaded.info.diagnostics.len(),
        NodeId::Group(_) => false,
        _ => true,
    }
}

/// Where the stream data of the object `id` sits in the file, when `id` is the object the
/// object view shows and that is a stream.
fn stream_range(state: &AppState, id: &NodeId) -> Option<Range<usize>> {
    let NodeId::Object(object) = id else {
        return None;
    };
    let facts = state.object_facts()?;
    if facts.object != Some(*object) {
        return None;
    }
    facts.stream.as_ref().map(|stream| stream.file_range.clone())
}

/// The navigator entry that lists `id`.
pub fn tree_of(id: &NodeId) -> Option<RailItem> {
    match id {
        NodeId::StructElem(_) | NodeId::MarkedContent { .. } => Some(RailItem::Tags),
        NodeId::Op { .. } | NodeId::Glyph { .. } => Some(RailItem::Content),
        NodeId::Artifact { .. } => Some(RailItem::Artifacts),
        NodeId::Page(_) | NodeId::PageBox { .. } => Some(RailItem::Pages),
        NodeId::Object(_) | NodeId::Trailer | NodeId::Revision(_) | NodeId::Resource { .. } => {
            Some(RailItem::Objects)
        }
        _ => None,
    }
}

/// The indirect object behind `id`: the element, page or object itself, or a resource's owner.
pub fn object_of(state: &AppState, id: &NodeId) -> Option<ObjRef> {
    let doc = &state.document.as_ref()?.doc;
    match id {
        NodeId::Object(r) | NodeId::StructElem(r) => Some(*r),
        NodeId::Page(page) | NodeId::PageBox { page, .. } => doc.page(*page)?.obj,
        NodeId::Resource { owner, .. } => Some(*owner),
        _ => None,
    }
}

/// Does `action` with `id`. Isolating and hiding select the node first, as they work on the
/// selection.
pub fn run(state: &mut AppState, ctx: &Context, id: &NodeId, action: Action) {
    match action {
        Action::Isolate | Action::Hide => {
            if state.selection.primary.as_ref() != Some(id) {
                state.select(id.clone());
            }
            let mode = if action == Action::Isolate {
                IsolateMode::Only
            } else {
                IsolateMode::Hide
            };
            state.toggle_isolation(mode);
        }
        Action::ShowInTree => {
            if let Some(rail) = tree_of(id) {
                state.set_rail(rail);
            }
            state.select(id.clone());
        }
        Action::Object | Action::RawBytes => {
            if let Some(object) = object_of(state, id) {
                state.select(NodeId::Object(object));
                state.set_rail(RailItem::Objects);
                if action == Action::RawBytes {
                    state.show_raw(None);
                }
            }
        }
        Action::StreamBytes => {
            if let Some(range) = stream_range(state, id) {
                state.show_bytes(range);
            }
        }
        Action::CopyId => ctx.copy_text(id.to_string()),
        Action::CopyJson => {
            if let Some(json) = state
                .document
                .as_ref()
                .and_then(|loaded| loaded.doc.node(id))
                // Plain data with string keys: serialising it to a string cannot fail.
                .and_then(|props| serde_json::to_string_pretty(&props).ok())
            {
                ctx.copy_text(json);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn each_node_lists_the_tree_that_shows_it() {
        let elem = NodeId::StructElem(ObjRef::new(14, 0));
        assert_eq!(tree_of(&elem), Some(RailItem::Tags));
        assert_eq!(tree_of(&NodeId::Op { page: 0, ix: 3 }), Some(RailItem::Content));
        assert_eq!(
            tree_of(&NodeId::Artifact { page: 0, seq: 1 }),
            Some(RailItem::Artifacts)
        );
        assert_eq!(tree_of(&NodeId::Page(2)), Some(RailItem::Pages));
        assert_eq!(
            tree_of(&NodeId::Object(ObjRef::new(5, 0))),
            Some(RailItem::Objects)
        );
        // Smoke: a diagnostic lives in a drawer, not a tree.
        assert_eq!(tree_of(&NodeId::Diagnostic(0)), None);
    }

    #[test]
    fn only_content_can_be_isolated_and_every_action_has_a_label() {
        assert!(paints(&NodeId::Glyph {
            page: 0,
            op: 1,
            glyph: 0
        }));
        assert!(!paints(&NodeId::Page(0)));
        let labels: Vec<&str> = Action::ALL.iter().map(|a| a.label()).collect();
        assert_eq!(
            labels,
            [
                "Isolate",
                "Hide",
                "Show in tree",
                "Object",
                "Raw bytes",
                "Stream bytes",
                "Copy id",
                "Copy JSON"
            ]
        );
    }
}
