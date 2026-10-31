//! What the inspector shows for content and structure nodes, and the plain property sections it
//! falls back to for everything else.
//!
//! `sections` picks the sections of one node in the order of the design: paint order (while
//! stepping), geometry, graphics
//! state, image, glyph, font, text state, structure. Core's own groups that one of those
//! replaces are left out of the plain list that follows them.

use std::borrow::Cow;

use egui::Ui;
use pdfxray_core::capture::{CapturedPage, OpRecord, PaintItem, PaintKind};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, FontFacts, GlyphInfo, NodeId, ObjRef, PageIx, Props};

use super::{Action, font, geometry, glyph, graphics, paint_order, relevance, text_state};
use crate::components::{Icon, PropRow, section};
use crate::state::{AppState, RailItem};
use crate::theme::tokens;
use crate::units::{Lengths, localize};

/// Title of the section about the structure element behind a node.
pub const STRUCTURE_SECTION: &str = "Structure";

/// Title of the section about a structure element's own accessibility facts.
pub const ACCESSIBILITY_SECTION: &str = "Accessibility";

/// What the sections of one selected node are drawn from.
pub(super) struct Cx<'a> {
    pub(super) doc: &'a Document,
    pub(super) state: &'a AppState,
    pub(super) id: &'a NodeId,
    pub(super) lengths: Lengths,
    /// The glyph panel's data, for a glyph.
    pub(super) glyph: Option<GlyphInfo>,
    /// The font of the glyph, or of the first glyph a text operator painted.
    pub(super) font: Option<FontFacts>,
}

impl<'a> Cx<'a> {
    pub(super) fn new(state: &'a AppState, id: &'a NodeId) -> Option<Self> {
        let doc = &state.document.as_ref()?.doc;
        let glyph = doc.glyph_info(id);
        let font = match id {
            NodeId::Op { page, ix } => first_glyph(doc, *page, *ix)
                .and_then(|first| doc.glyph_info(&first))
                .and_then(|info| info.font),
            _ => glyph.as_ref().and_then(|info| info.font.clone()),
        };
        Some(Self {
            doc,
            state,
            id,
            lengths: state.lengths(),
            glyph,
            font,
        })
    }

    /// A section of this node: it starts open or collapsed as the rail entry asks (see
    /// [`relevance`]).
    pub(super) fn section<R>(&self, ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        section::show_with(
            ui,
            title,
            title,
            relevance::starts_open(self.state.nav.rail, title),
            body,
        )
    }

    /// The page and the operator behind the node: the operator itself, or the one that painted
    /// the glyph.
    pub(super) fn op(&self) -> Option<(PageIx, &'a CapturedPage, &'a OpRecord)> {
        let (page, ix) = match self.id {
            NodeId::Op { page, ix } | NodeId::Glyph { page, op: ix, .. } => (*page, *ix),
            _ => return None,
        };
        let captured = self.doc.captured(page)?;
        let op = captured.ops.get(usize_from_u32(ix)).filter(|op| op.ix == ix)?;
        Some((page, captured, op))
    }

    /// What the node painted: the glyph, or the first item of the operator.
    pub(super) fn item(&self) -> Option<&'a PaintItem> {
        let (_, captured, op) = self.op()?;
        let first = usize::try_from(op.first_item?).ok()?;
        let items = captured
            .items
            .iter()
            .skip(first)
            .take(usize_from_u32(op.item_count));
        match self.id {
            NodeId::Glyph { glyph, .. } => items.into_iter().find(
                |item| matches!(item.kind, PaintKind::Glyph { glyph_index, .. } if glyph_index == *glyph),
            ),
            _ => items
                .into_iter()
                .find(|item| !matches!(item.kind, PaintKind::Clip)),
        }
    }

    /// The object whose `/Resources` the operator's names resolve in: the innermost form it runs
    /// in, else the page's.
    pub(super) fn resource_owner(&self) -> Option<ObjRef> {
        let (page, captured, op) = self.op()?;
        std::iter::successors(op.xobject, |&s| captured.xobjects.get(usize_from_u32(s))?.parent)
            .take(captured.xobjects.len())
            .find_map(|s| captured.xobjects.get(usize_from_u32(s))?.obj)
            .or_else(|| self.doc.page(page)?.resources_from)
    }

    /// The marked-content sequence the node sits in (innermost) and the structure element that
    /// owns it.
    pub(super) fn owner(&self) -> Option<(NodeId, i32)> {
        let path = self.state.path_to(self.id);
        let mcid = path.iter().rev().find_map(|id| match id {
            NodeId::MarkedContent { page, mcid } => Some((*page, *mcid)),
            _ => None,
        })?;
        let owner = self.state.tag_index()?.owner_of(mcid.0, mcid.1)?.clone();
        Some((owner, mcid.1))
    }
}

/// The first glyph operator `ix` of `page` painted.
fn first_glyph(doc: &Document, page: PageIx, ix: u32) -> Option<NodeId> {
    let captured = doc.captured(page)?;
    let op = captured.ops.get(usize_from_u32(ix)).filter(|op| op.ix == ix)?;
    let first = usize::try_from(op.first_item?).ok()?;
    captured
        .items
        .iter()
        .skip(first)
        .take(usize_from_u32(op.item_count))
        .find_map(|item| match item.kind {
            PaintKind::Glyph { glyph_index, .. } => Some(NodeId::Glyph {
                page,
                op: ix,
                glyph: glyph_index,
            }),
            _ => None,
        })
}

/// True for the nodes [`sections`] draws instead of the plain property list.
pub(super) fn handles(id: &NodeId) -> bool {
    matches!(
        id,
        NodeId::Op { .. }
            | NodeId::Glyph { .. }
            | NodeId::MarkedContent { .. }
            | NodeId::Artifact { .. }
            | NodeId::StructElem(_)
    )
}

/// The sections of a content or structure node, then core's remaining groups.
pub(super) fn sections(ui: &mut Ui, state: &AppState, id: &NodeId, props: &Props, action: &mut Action) {
    let Some(cx) = Cx::new(state, id) else {
        return;
    };
    // The groups of core that a section here replaces.
    let replaced: &[&str] = match id {
        NodeId::Op { .. } => &["Geometry", "Graphics state", "Text state"],
        NodeId::Glyph { .. } => &["Glyph", "Text state"],
        _ => &[],
    };
    paint_order::show(ui, &cx, action);
    geometry::show(ui, &cx);
    graphics::show(ui, &cx);
    graphics::image(ui, &cx, action);
    glyph::show(ui, &cx);
    font::show(ui, &cx, action);
    text_state::show(ui, &cx);
    structure(ui, &cx, action);
    for group in props
        .groups
        .iter()
        .filter(|g| !replaced.contains(&g.name.as_str()))
    {
        group_section(ui, group, &cx.lengths, cx.state.nav.rail, action);
    }
}

/// The structure element behind a content node: its owner, language, alternative text and where
/// the parent tree points back at it. For a structure element, the same facts about itself.
fn structure(ui: &mut Ui, cx: &Cx<'_>, action: &mut Action) {
    let (element, mcid, title) = match cx.id {
        NodeId::StructElem(_) => (cx.id.clone(), None, ACCESSIBILITY_SECTION),
        _ => match cx.owner() {
            Some((owner, mcid)) => (owner, Some(mcid), STRUCTURE_SECTION),
            None => return,
        },
    };
    let Some(props) = cx.doc.node(&element) else {
        return;
    };
    let NodeId::StructElem(object) = element else {
        return;
    };
    cx.section(ui, title, |ui| {
        let role = props.get("Element", "Type").unwrap_or_default();
        if !matches!(cx.id, NodeId::StructElem(_)) {
            let value = format!("/{role}  {object}");
            if PropRow::new("Owner", &value).link(true).show(ui).value.clicked() {
                action.navigate = Some(NodeId::StructElem(object));
            }
        }
        if let Some(lang) = language(&props) {
            PropRow::new("Lang", &lang).show(ui);
        }
        match props.get("Element", "Alt") {
            Some(alt) => {
                PropRow::new("/Alt", alt).show(ui);
            }
            None if cx
                .state
                .tag_index()
                .is_some_and(|tags| tags.lacks_alt(&NodeId::StructElem(object))) =>
            {
                PropRow::new("/Alt", "missing").warn(true).show(ui);
            }
            None => {}
        }
        parent_tree(ui, &props, mcid, object);
    });
}

/// `en-US`, or `en-US (inherited)` when the element takes it from an ancestor.
fn language(props: &Props) -> Option<String> {
    if let Some(own) = props.get("Element", "Lang") {
        return Some(own.to_owned());
    }
    let effective = props.get("Element", "Effective lang")?;
    let lang = effective.split(" (from ").next().unwrap_or(effective);
    Some(format!("{lang} (inherited)"))
}

/// The parent tree's entry for the element: `[0] → 22 0 R ✓` when it points back at the
/// element (for the MCID of a content node, or any entry of a structure element itself).
fn parent_tree(ui: &mut Ui, props: &Props, mcid: Option<i32>, object: ObjRef) {
    let entries: Vec<&str> = props
        .groups
        .iter()
        .filter(|g| g.name == "Structure")
        .flat_map(|g| g.rows.iter())
        .filter(|row| row.key == "ParentTree")
        .map(|row| row.value.as_str())
        .collect();
    let wanted = mcid.map(|m| format!("MCID {m} "));
    let found = entries.iter().find_map(|value| {
        if let Some(wanted) = &wanted
            && !format!("{value} ").contains(wanted.as_str())
        {
            return None;
        }
        let key = value.strip_prefix("key ")?.split(' ').next()?;
        Some(format!("[{key}] → {object}"))
    });
    match found {
        Some(value) => {
            PropRow::new("ParentTree", &value)
                .trailing(Icon::Check, tokens::OK)
                .show(ui);
        }
        None if mcid.is_some() => {
            PropRow::new("ParentTree", "missing").warn(true).show(ui);
        }
        None => {}
    }
}

/// The value as it is shown: blank values (a painted space, an empty string) are quoted so the
/// row has something to see and to click.
fn visible(value: &str) -> Cow<'_, str> {
    if value.trim().is_empty() {
        Cow::Owned(format!("{value:?}"))
    } else {
        Cow::Borrowed(value)
    }
}

fn group_section(
    ui: &mut Ui,
    group: &pdfxray_core::PropGroup,
    lengths: &Lengths,
    rail: RailItem,
    action: &mut Action,
) {
    let open = relevance::starts_open(rail, &group.name);
    section::show_with(ui, &group.name, &group.name, open, |ui| {
        for row in &group.rows {
            let shown = localize(&row.value, lengths);
            let text = visible(&shown);
            let output = PropRow::new(&row.key, &text)
                .copy_text(&shown)
                .link(row.link.is_some())
                .show(ui);
            if output.value.clicked() {
                action.navigate.clone_from(&row.link);
            }
        }
    });
}

/// One section per property group. Lengths are shown in the unit chosen in the status bar.
pub(super) fn props(ui: &mut Ui, props: &Props, lengths: &Lengths, rail: RailItem, action: &mut Action) {
    for group in &props.groups {
        group_section(ui, group, lengths, rail, action);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn blank_values_are_quoted_and_others_untouched() {
        assert_eq!(visible(" "), "\" \"");
        assert_eq!(visible(""), "\"\"");
        assert_eq!(visible("Hello"), "Hello");
        assert_eq!(visible(" x "), " x ");
    }
}
