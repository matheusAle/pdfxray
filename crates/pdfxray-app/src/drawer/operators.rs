//! The Operators tab: the operators of the page on the canvas as a table, in execution order.
//!
//! Columns: the index, the bytes the instruction takes in its stream, the operator, its operands
//! and the state it leaves (`depth 1`, `text object`, `font F1 26pt`, `Tlm [1 0 0 1 40 676]`,
//! `18 glyphs · 245.3 pt`). Clicking a row selects the operator; the selected one is highlighted
//! and scrolled to. Resting on a row says in plain words what the operator does.

use egui::{Color32, RichText, ScrollArea, Ui};
use pdfxray_core::capture::{CapturedPage, OpRecord, PaintKind, StreamRef};
use pdfxray_core::numeric::{f32_from_u32, usize_from_u32};
use pdfxray_core::{Document, NodeId, ObjRef, PageIx, explain, num};

use super::table::{self, Cell, row};
use crate::navigator::{select_with_modifiers, thousands};
use crate::state::AppState;
use crate::theme::tokens;

/// Shown when the page has no captured operators.
pub const NO_OPERATORS: &str = "This page has no operators.";

const WIDTHS: [f32; 3] = [34.0, 84.0, 52.0];
const STATE_W: f32 = 190.0;

/// The object a stream of operators is stored in: the form, or the page's `/Contents` entry.
pub(super) fn stream_object(doc: &Document, page: PageIx, stream: &StreamRef) -> Option<ObjRef> {
    match stream {
        StreamRef::FormXObject { obj, .. } => *obj,
        StreamRef::PageContent { index } => {
            let page_object = doc.page(page)?.obj?;
            let props = doc.node(&NodeId::Object(page_object))?;
            let contents = props.get("Value", "/Contents")?;
            references(contents).get(usize_from_u32(*index)).copied()
        }
    }
}

/// The references in a value such as `6 0 R` or `[6 0 R 7 0 R]`, in order.
fn references(value: &str) -> Vec<ObjRef> {
    let words: Vec<&str> = value
        .split(|c: char| c.is_whitespace() || c == '[' || c == ']')
        .filter(|w| !w.is_empty())
        .collect();
    words
        .windows(3)
        .filter(|w| w[2] == "R")
        .filter_map(|w| Some(ObjRef::new(w[0].parse().ok()?, w[1].parse().ok()?)))
        .collect()
}

/// `23 0 R` for a stream that is an object, `page content #0` for one that is not.
pub(super) fn stream_label(doc: &Document, page: PageIx, stream: &StreamRef) -> String {
    match (stream_object(doc, page, stream), stream) {
        (Some(object), _) => object.to_string(),
        (None, StreamRef::PageContent { index }) => format!("page content #{index}"),
        (None, StreamRef::FormXObject { name, .. }) => format!("form /{name}"),
    }
}

/// The operator the table is about: the selected one on this page, else the first.
fn current_op<'a>(state: &AppState, captured: &'a CapturedPage) -> Option<&'a OpRecord> {
    let page = state.canvas.page;
    let selected = match state.selection.primary.as_ref()? {
        NodeId::Op { page: p, ix } | NodeId::Glyph { page: p, op: ix, .. } if *p == page => Some(*ix),
        _ => None,
    };
    selected
        .and_then(|ix| captured.ops.get(usize_from_u32(ix)).filter(|op| op.ix == ix))
        .or_else(|| captured.ops.first())
}

/// The text after the tab bar: `stream 23 0 R · decoded 3,880 B`.
pub(super) fn header_note(state: &mut AppState) -> Option<String> {
    let page = state.canvas.page;
    let (label, object) = {
        let doc = &state.document.as_ref()?.doc;
        let captured = doc.captured(page)?;
        let op = current_op(state, captured)?;
        (
            stream_label(doc, page, &op.stream),
            stream_object(doc, page, &op.stream),
        )
    };
    let size = object
        .and_then(|object| state.decoded_len(object))
        .map(|bytes| format!(" · decoded {} B", thousands(bytes)));
    Some(format!("stream {label}{}", size.unwrap_or_default()))
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let page = state.canvas.page;
    let Some(doc) = state.document.as_ref().map(|loaded| &loaded.doc) else {
        return;
    };
    let Some(captured) = doc.captured(page).filter(|captured| !captured.ops.is_empty()) else {
        ui.label(RichText::new(NO_OPERATORS).color(tokens::TEXT_3));
        return;
    };
    let selected = match state.selection.primary.as_ref() {
        Some(NodeId::Op { page: p, ix } | NodeId::Glyph { page: p, op: ix, .. }) if *p == page => Some(*ix),
        _ => None,
    };
    let operands_w = (ui.available_width() - WIDTHS.iter().sum::<f32>() - STATE_W - 5.0 * 8.0).max(80.0);
    let widths = [WIDTHS[0], WIDTHS[1], WIDTHS[2], operands_w, STATE_W];
    table::header(ui, &widths, &["#", "bytes", "op", "operands", "state"]);

    let mut area = ScrollArea::vertical().id_salt("operators").auto_shrink(false);
    let memory = ui.make_persistent_id("operators-seen");
    let seen: Option<(Option<u32>, f32)> = ui.data(|d| d.get_temp(memory));
    let viewport = ui.available_height();
    if let Some((seen_ix, offset)) = seen
        && seen_ix != selected
        && let Some(ix) = selected
    {
        // Row indices are far below f32's exact integer range.
        let top = f32_from_u32(ix) * tokens::ROW_H;
        if top < offset || top + tokens::ROW_H > offset + viewport {
            area = area.vertical_scroll_offset((top - (viewport - tokens::ROW_H) / 2.0).max(0.0));
        }
    }

    let mut clicked = None;
    let mut hovered = None;
    let shown = area.show_rows(ui, tokens::ROW_H, captured.ops.len(), |ui, rows| {
        for index in rows {
            let op = &captured.ops[index];
            let id = NodeId::Op { page, ix: op.ix };
            let state_text = state_text(captured, op);
            let cells = [
                Cell::small(op.ix.to_string(), tokens::TEXT_3),
                Cell::small(
                    op.span
                        .map_or_else(String::new, |(start, end)| format!("{start}..{end}")),
                    tokens::TEXT_3,
                ),
                Cell::mono_strong(op.operator.clone(), operator_color(op)),
                Cell::mono(op.operands.join(" "), tokens::TEXT_2),
                Cell::small(state_text, tokens::TEXT_3),
            ];
            let note =
                explain::operator_with_text(&op.operator, &op.operands, text_of(captured, op).as_deref());
            let response = row(
                ui,
                ui.make_persistent_id(("operator", op.ix)),
                &op.operator,
                selected == Some(op.ix),
                &widths,
                &cells,
                Some(note),
            );
            if response.clicked() {
                clicked = Some(id.clone());
            }
            if response.hovered() {
                hovered = Some(id);
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(memory, (selected, shown.state.clamped_offset().y)));
    if hovered.is_some() {
        state.canvas.preview = hovered;
    }
    if let Some(id) = clicked {
        select_with_modifiers(ui, state, id);
    }
}

/// Marked content is purple, text blue, the rest grey or plain.
fn operator_color(op: &OpRecord) -> Color32 {
    match op.operator.as_str() {
        "BDC" | "BMC" | "EMC" => tokens::KIND_TAG,
        "BT" | "ET" | "Tf" | "Td" | "TD" | "Tm" | "T*" | "Tc" | "Tw" | "Tz" | "TL" | "Ts" | "Tr" | "Tj"
        | "TJ" | "'" | "\"" => tokens::KIND_CONTENT,
        "q" | "Q" | "cm" | "gs" => tokens::TEXT_2,
        _ if op.item_count > 0 => tokens::KIND_CONTENT,
        _ => tokens::TEXT,
    }
}

/// The text a text operator painted, for its plain-language note.
pub(crate) fn text_of(captured: &CapturedPage, op: &OpRecord) -> Option<String> {
    let first = usize::try_from(op.first_item?).ok()?;
    let text: String = captured
        .items
        .iter()
        .skip(first)
        .take(usize_from_u32(op.item_count))
        .filter_map(|item| match &item.kind {
            PaintKind::Glyph { unicode, .. } => unicode.as_deref(),
            _ => None,
        })
        .collect();
    (!text.is_empty()).then_some(text)
}

/// What the operator leaves behind, in the words of the design: `depth 1`, `text object`,
/// `font F1 26pt`, `Tlm [1 0 0 1 40 676]`, `18 glyphs · 245.3 pt`.
pub fn state_text(captured: &CapturedPage, op: &OpRecord) -> String {
    match op.operator.as_str() {
        "BDC" | "BMC" | "EMC" => format!("depth {}", op.mc_depth),
        "BT" => "text object".to_owned(),
        "Tf" => {
            let name = op.operands.first().map_or("", |n| n.trim_start_matches('/'));
            let size = op.operands.get(1).map_or("", String::as_str);
            format!("font {name} {size}pt")
        }
        "Td" | "TD" | "Tm" | "T*" | "'" | "\"" => {
            let tlm: Vec<String> = op.gstate.text.tlm.iter().map(|v| num(*v)).collect();
            format!("Tlm [{}]", tlm.join(" "))
        }
        "Tj" | "TJ" => {
            let Some(first) = op.first_item.and_then(|f| usize::try_from(f).ok()) else {
                return String::new();
            };
            let (count, advance) = captured
                .items
                .iter()
                .skip(first)
                .take(usize_from_u32(op.item_count))
                .fold((0_usize, 0.0_f64), |(count, advance), item| match item.kind {
                    PaintKind::Glyph { advance: a, .. } => (count + 1, advance + a),
                    _ => (count, advance),
                });
            format!("{count} glyphs · {advance:.1} pt")
        }
        "cm" => format!(
            "CTM [{}]",
            op.gstate
                .ctm
                .iter()
                .map(|v| num(*v))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        _ => String::new(),
    }
}
