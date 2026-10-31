//! Collapsible section headers; whether a section is open is remembered per key.
//!
//! No divider lines: a section is told apart by its 14 px of padding above and below and by its
//! 10 px header, tracked +0.9 in text-3 with a dimmed chevron.

use egui::{Id, Role, Sense, Ui, WidgetInfo, vec2};

use super::icon::Icon;
use crate::theme::{fonts, tokens};

/// Space above and below a section.
const PAD: f32 = 14.0;
const HEADER_H: f32 = 18.0;
/// What the header's hit area reaches above and below it, into the padding.
const HIT_GROW: f32 = 4.0;
/// Space between rows: they are 28 px tall at a 29 px pitch, so the hover of one meets the next
/// and the pointer never falls between them.
const ROW_GAP: f32 = 1.0;
/// Gap between the header and the first row.
const BODY_GAP: f32 = 4.0;

fn state_id(key: &str) -> Id {
    Id::unique(("pdfxray-section", key))
}

/// True when the section with `key` is open: what the user chose, else `initially_open`. Only a
/// choice is remembered, so a section that was never touched follows the default of the moment.
pub fn is_open_or(ui: &Ui, key: &str, initially_open: bool) -> bool {
    ui.data_mut(|data| data.get_persisted::<bool>(state_id(key)))
        .unwrap_or(initially_open)
}

/// Sets whether the section with `key` is open.
pub fn set_open(ui: &Ui, key: &str, open: bool) {
    ui.data_mut(|data| data.insert_persisted(state_id(key), open));
}

/// A header with `title` and, while open, the `body` below it. `key` names the section for the
/// remembered state. Returns what `body` returns when it ran.
pub fn show<R>(ui: &mut Ui, key: &str, title: &str, body: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    show_with(ui, key, title, true, body)
}

/// Like [`show`], for a section that starts open or collapsed (`initially_open`) until the user
/// opens or closes it: the inspector collapses what the current mode does not need.
pub fn show_with<R>(
    ui: &mut Ui,
    key: &str,
    title: &str,
    initially_open: bool,
    body: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let open = is_open_or(ui, key, initially_open);
    let width = ui.available_width();
    ui.add_space(PAD);
    let (rect, slot) = ui.allocate_exact_size(vec2(width, HEADER_H), Sense::hover());
    // A bigger target than the 10 px text: the padding around the header is part of it.
    let response = ui.interact(
        rect.expand2(vec2(0.0, HIT_GROW)),
        slot.id.with("hit"),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, open, title));
    if ui.is_rect_visible(rect) {
        let chevron = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let hovered = response.hovered();
        if hovered {
            // The rows' wash: the same extent and corners as theirs.
            ui.painter()
                .rect_filled(response.rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
        }
        let dimmed = if hovered {
            tokens::TEXT_2
        } else {
            tokens::TEXT_3.gamma_multiply(0.7)
        };
        chevron.paint(
            ui,
            egui::Rect::from_min_size(rect.min + vec2(2.0, 0.0), vec2(16.0, rect.height())),
            12.0,
            dimmed,
        );
        let galley = ui.painter().layout_job(fonts::section_header(title));
        let pos = egui::pos2(rect.min.x + 20.0, rect.center().y - galley.size().y / 2.0);
        let ink = if hovered { tokens::TEXT_2 } else { tokens::TEXT_3 };
        ui.painter().galley(pos, galley, ink);
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        set_open(ui, key, !open);
    }
    let result = open.then(|| {
        ui.add_space(BODY_GAP);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            body(ui)
        })
        .inner
    });
    ui.add_space(PAD);
    result
}
