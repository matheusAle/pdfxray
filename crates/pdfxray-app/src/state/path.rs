//! The path from the page to a selected node: what the breadcrumb lists and what the trees use
//! to find the row to highlight.

use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, NodeId};

use super::AppState;

/// The short name of a node in a breadcrumb: `Do /Im1`, `TJ #10`, `MCID 9`, `glyph 0`; other
/// nodes use the title the inspector shows.
pub fn crumb_label(doc: &Document, id: &NodeId) -> String {
    match id {
        NodeId::Op { page, ix } => doc
            .captured(*page)
            .and_then(|captured| captured.ops.get(usize_from_u32(*ix)))
            .filter(|op| op.ix == *ix)
            .map_or_else(
                || id.to_string(),
                |op| match (op.operator.as_str(), op.operands.first()) {
                    ("Do", Some(name)) => format!("Do {name}"),
                    (operator, _) => format!("{operator} #{ix}"),
                },
            ),
        NodeId::Glyph { glyph, .. } => format!("glyph {glyph}"),
        NodeId::MarkedContent { mcid, .. } => format!("MCID {mcid}"),
        _ => doc.node(id).map_or_else(|| id.to_string(), |props| props.title),
    }
}

impl AppState {
    /// The crumbs of `id`, root first, ending with `id`.
    ///
    /// Content nodes start at their page and name the structure element that owns their marked
    /// content between the page and the MCID (`Page 1 > Figure > MCID 9 > Do /Im1`); other nodes
    /// follow [`Document::breadcrumb`].
    pub fn path_to(&self, id: &NodeId) -> Vec<NodeId> {
        let Some(loaded) = &self.document else {
            return vec![id.clone()];
        };
        let doc = &loaded.doc;
        let mut path = doc.breadcrumb(id);
        if !matches!(
            id,
            NodeId::Op { .. } | NodeId::Glyph { .. } | NodeId::MarkedContent { .. } | NodeId::Artifact { .. }
        ) {
            return path;
        }
        if matches!(path.first(), Some(NodeId::Document)) {
            path.remove(0);
        }
        let owned = path.iter().enumerate().find_map(|(at, node)| match node {
            NodeId::MarkedContent { page, mcid } => self
                .tag_index()?
                .owner_of(*page, *mcid)
                .map(|owner| (at, owner.clone())),
            _ => None,
        });
        if let Some((at, owner)) = owned {
            path.insert(at, owner);
        }
        path
    }
}
