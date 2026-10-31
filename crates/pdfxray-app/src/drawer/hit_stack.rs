//! The Hit stack tab: everything the last canvas click found, topmost first, one row each.
//!
//! A row is: its position, an icon for the kind of node, the node's name and, dim, where it
//! lives: `op #214 · stream 23 0 R · bytes 4410..4422`, `/Figure <</MCID 9>> BDC … EMC`,
//! `22 0 R · /Alt missing`, `3 0 R · MediaBox [0 0 612 792]`.

use egui::{RichText, ScrollArea, Ui};
use pdfxray_core::capture::{CapturedPage, MarkedSeq, OpRecord};
use pdfxray_core::numeric::usize_from_u32;
use pdfxray_core::{Document, LayerMode, NodeId, fmt_rect};

use super::operators::stream_label;
use super::table::{Cell, row};
use crate::components::Icon;
use crate::inspector::header::icon_of;
use crate::navigator::select_with_modifiers;
use crate::state::{AppState, HitEntry};
use crate::theme::tokens;

/// Shown before the first canvas click.
pub const HIT_STACK_HINT: &str = "Click the page to list everything under the pointer.";

/// Text of the warning on a figure without `/Alt`.
pub const ALT_MISSING: &str = "/Alt missing";

const WIDTHS: [f32; 3] = [16.0, 18.0, 150.0];

/// One row of the table, as it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitLine {
    /// The icon of the kind of node.
    icon: Icon,
    /// The node's name, e.g. `Do /Im1`.
    pub label: String,
    /// Where it lives, e.g. `op #214 · stream 23 0 R · bytes 4410..4422`.
    pub detail: String,
    /// The detail is a warning (`/Alt missing`).
    pub warn: bool,
}

/// The text after the tab bar: `click @ 184.2, 512.6 pt`, led by the Pick mode when it is not
/// All (`picking Tags · click @ …`), so a short stack explains itself.
pub fn header_note(state: &AppState) -> Option<String> {
    let picking = (state.canvas.layer_mode != LayerMode::All)
        .then(|| format!("picking {}", state.canvas.layer_mode.title()));
    let click = state.drawer.hit_stack.as_ref().map(|stack| {
        let lengths = state.lengths();
        format!(
            "click @ {}, {} {}",
            lengths.number(stack.point.x),
            lengths.number(stack.point.y),
            lengths.unit.label()
        )
    });
    match (picking, click) {
        (Some(picking), Some(click)) => Some(format!("{picking} · {click}")),
        (picking, click) => picking.or(click),
    }
}

/// The rows of the table for the last canvas click, topmost first.
pub fn lines(state: &AppState) -> Vec<HitLine> {
    let (Some(stack), Some(loaded)) = (&state.drawer.hit_stack, &state.document) else {
        return Vec::new();
    };
    stack
        .entries
        .iter()
        .map(|entry| {
            let lacking_alt = state.tag_index().is_some_and(|tags| tags.lacks_alt(&entry.id));
            line(&loaded.doc, entry, lacking_alt)
        })
        .collect()
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(stack) = state.drawer.hit_stack.clone() else {
        ui.label(RichText::new(HIT_STACK_HINT).color(tokens::TEXT_3));
        return;
    };
    let lines = lines(state);
    let selected = state.selection.primary.clone();
    let mut clicked = None;
    let mut hovered = None;
    ScrollArea::vertical()
        .id_salt("hit-stack")
        .auto_shrink(false)
        .show(ui, |ui| {
            for (index, (entry, line)) in stack.entries.iter().zip(&lines).enumerate() {
                let detail_color = if line.warn { tokens::WARN } else { tokens::TEXT_3 };
                let cells = [
                    Cell::small(index.to_string(), tokens::TEXT_3),
                    Cell::Icon(line.icon, tokens::kind_color(&entry.id)),
                    Cell::mono(line.label.clone(), tokens::TEXT),
                    Cell::small(line.detail.clone(), detail_color),
                ];
                let response = row(
                    ui,
                    ui.make_persistent_id(("hit", index)),
                    &line.label,
                    selected.as_ref() == Some(&entry.id),
                    &WIDTHS,
                    &cells,
                    None,
                );
                if response.clicked() {
                    clicked = Some(entry.id.clone());
                }
                if response.hovered() {
                    hovered = Some(entry.id.clone());
                }
            }
        });
    if hovered.is_some() {
        state.canvas.preview = hovered;
    }
    if let Some(id) = clicked {
        select_with_modifiers(ui, state, id);
    }
}

fn line(doc: &Document, entry: &HitEntry, lacking_alt: bool) -> HitLine {
    let plain = |detail: String| HitLine {
        icon: icon_of(&entry.id),
        label: entry.label.clone(),
        detail,
        warn: false,
    };
    match &entry.id {
        NodeId::Op { page, ix } => {
            let Some(captured) = doc.captured(*page) else {
                return plain(String::new());
            };
            let Some(op) = captured.ops.get(usize_from_u32(*ix)).filter(|op| op.ix == *ix) else {
                return plain(String::new());
            };
            let mut line = plain(op_detail(doc, captured, op));
            if op.operator == "Do"
                && let Some(name) = op.operands.first()
            {
                line.label = format!("Do {name}");
            }
            line
        }
        NodeId::Glyph { page, op, glyph } => {
            let info = doc.glyph_info(&NodeId::Glyph {
                page: *page,
                op: *op,
                glyph: *glyph,
            });
            let shown = info
                .as_ref()
                .and_then(|i| i.unicode.clone())
                .filter(|u| !u.is_empty());
            let points = info.as_ref().map(|i| {
                i.code_points
                    .iter()
                    .map(|c| format!("U+{c:04X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            HitLine {
                icon: Icon::Font,
                label: shown.map_or_else(|| format!("glyph {glyph}"), |s| format!("'{s}'")),
                detail: format!("glyph {glyph} · {} · op #{op}", points.unwrap_or_default()),
                warn: false,
            }
        }
        NodeId::MarkedContent { page, mcid } => plain(
            doc.captured(*page)
                .and_then(|c| c.marked.iter().find(|m| m.mcid == Some(*mcid)))
                .map_or_else(String::new, sequence_detail),
        ),
        NodeId::Artifact { page, seq } => plain(
            doc.captured(*page)
                .and_then(|c| {
                    c.marked
                        .iter()
                        .filter(|m| m.artifact.is_some())
                        .nth(usize_from_u32(*seq))
                })
                .map_or_else(String::new, sequence_detail),
        ),
        NodeId::StructElem(object) => HitLine {
            warn: lacking_alt,
            detail: if lacking_alt {
                format!("{object} · {ALT_MISSING}")
            } else {
                object.to_string()
            },
            ..plain(String::new())
        },
        NodeId::Page(page) => {
            let info = doc.page(*page);
            let object = info
                .and_then(|i| i.obj)
                .map_or_else(String::new, |o| format!("{o} · "));
            let media = info.map_or_else(String::new, |i| format!("MediaBox {}", fmt_rect(i.media_box)));
            plain(format!("{object}{media}"))
        }
        NodeId::PageBox { page, kind } => plain(
            doc.page(*page)
                .and_then(|info| info.boxes.iter().find(|b| b.kind == *kind))
                .map_or_else(String::new, |b| fmt_rect(b.rect)),
        ),
        _ => plain(String::new()),
    }
}

/// `op #214 · stream 23 0 R · bytes 4410..4422`.
fn op_detail(doc: &Document, captured: &CapturedPage, op: &OpRecord) -> String {
    let mut detail = format!(
        "op #{} · stream {}",
        op.ix,
        stream_label(doc, captured.page, &op.stream)
    );
    if let Some((start, end)) = op.span {
        detail = format!("{detail} · bytes {start}..{end}");
    }
    detail
}

/// `/Figure <</MCID 9>> BDC … EMC`.
fn sequence_detail(seq: &MarkedSeq) -> String {
    let properties: Vec<String> = seq.properties.iter().map(|(k, v)| format!("/{k} {v}")).collect();
    if properties.is_empty() {
        format!("/{} BMC … EMC", seq.tag)
    } else {
        format!("/{} <<{}>> BDC … EMC", seq.tag, properties.join(" "))
    }
}
