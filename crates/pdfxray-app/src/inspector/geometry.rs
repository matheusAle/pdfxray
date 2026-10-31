//! The GEOMETRY section: where a node is on the page, the matrix that put it there and the way
//! from its own space to the screen. Lengths follow the units switch of the status bar.

use egui::{RichText, Ui};
use pdfxray_core::capture::PaintKind;
use pdfxray_core::{Matrix, NodeId, Rect, num};

use super::structure::Cx;
use crate::components::{Chip, Icon, PropRow, Tone};
use crate::theme::{fonts, tokens};

/// Title of the section.
pub const TITLE: &str = "Geometry";

/// Value of the CTM row: the six numbers as the operator wrote them.
pub fn matrix_text(matrix: &Matrix) -> String {
    let numbers: Vec<String> = matrix.to_array().iter().map(|v| num(*v)).collect();
    format!("[{}]", numbers.join(" "))
}

/// The names of the spaces a node's coordinates pass through, from its own to the device.
fn chain(cx: &Cx<'_>) -> Vec<&'static str> {
    let mut spaces = Vec::new();
    if let Some(item) = cx.item() {
        spaces.push(match item.kind {
            PaintKind::Image { .. } => "image",
            PaintKind::Glyph { .. } => "glyph",
            PaintKind::Shading => "shading",
            PaintKind::Fill | PaintKind::Stroke | PaintKind::FillStroke | PaintKind::Clip => "path",
        });
    }
    if cx.op().is_some_and(|(_, _, op)| op.xobject.is_some()) {
        spaces.push("form");
    }
    spaces.extend(["user", "page", "device"]);
    spaces
}

pub(super) fn show(ui: &mut Ui, cx: &Cx<'_>) {
    let Some((_, rect)) = cx.doc.bbox(cx.id).filter(|_| !matches!(cx.id, NodeId::Page(_))) else {
        return;
    };
    let lengths = &cx.lengths;
    cx.section(ui, TITLE, |ui| {
        let corners = format!(
            "[{} {} {} {}]",
            lengths.number(rect.x0),
            lengths.number(rect.y0),
            lengths.number(rect.x1),
            lengths.number(rect.y1)
        );
        PropRow::new("Page bbox", &corners).show(ui);
        PropRow::new("Size", &lengths.size(rect)).show(ui);
        if let Some((_, _, op)) = cx.op() {
            let matrix = Matrix::from(op.gstate.ctm);
            let value = matrix_text(&matrix);
            // The key's hint says what a b c d e f mean; the value copies like any other.
            PropRow::new("CTM", &value).show(ui);
        }
        PropRow::new("Device", &device(cx, rect)).show(ui);
        if cx.op().is_some() {
            transform_chain(ui, &chain(cx));
        }
    });
}

/// `425 × 188 px @125%`: the box as large as it is on the canvas.
fn device(cx: &Cx<'_>, rect: Rect) -> String {
    let zoom = f64::from(cx.state.canvas.zoom);
    let width = cx.lengths.points(rect.width()) * zoom;
    let height = cx.lengths.points(rect.height()) * zoom;
    format!("{width:.0} × {height:.0} px @{:.0}%", zoom * 100.0)
}

/// `image → form → user → page → device`, the page chip highlighted.
fn transform_chain(ui: &mut Ui, spaces: &[&str]) {
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new("Transform chain")
                .font(fonts::ui(tokens::text::SMALL))
                .color(tokens::TEXT_3),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 4.0);
        // The chips start where the label above and the keys do: a space adds no item gap after it.
        ui.add_space(8.0);
        for (n, space) in spaces.iter().enumerate() {
            if n > 0 {
                Icon::ChevronRight.show(ui, 10.0, tokens::TEXT_3);
            }
            let tone = if *space == "page" {
                Tone::Kind(tokens::ACCENT)
            } else {
                Tone::Neutral
            };
            Chip::new(space).tone(tone).show(ui);
        }
    });
    ui.add_space(4.0);
}
