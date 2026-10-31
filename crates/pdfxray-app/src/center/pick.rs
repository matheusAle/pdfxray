//! The Pick menu of the canvas toolbar: what a click on the page may select (the layer mode).
//!
//! It filters the hit stack, not the render, so it lives with the page: the toolbar shows it
//! and the drawer's Hit stack says which mode picked the rows. The keys `1` to `6` do the same.

use egui::{Color32, CursorIcon, Key, RectAlign, Role, Sense, Ui, WidgetInfo, vec2};
use pdfxray_core::LayerMode;

use crate::components::{Icon, ToolButton, focus, popover, tooltip};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// The modes in menu order, with their key and kind colour.
pub const MODES: [(LayerMode, Key, Option<Color32>); 6] = [
    (LayerMode::All, Key::Num1, None),
    (LayerMode::Tags, Key::Num2, Some(tokens::KIND_TAG)),
    (LayerMode::Content, Key::Num3, Some(tokens::KIND_CONTENT)),
    (LayerMode::Annotations, Key::Num4, Some(tokens::KIND_ANNOT)),
    (LayerMode::Boxes, Key::Num5, Some(tokens::OK)),
    (LayerMode::Artifacts, Key::Num6, Some(tokens::KIND_ARTIFACT)),
];

/// Shown in place of the key of a mode that is not built yet.
pub const LATER: &str = "v1.1";

/// Width of the menu.
const MENU_W: f32 = 220.0;
/// Height of a menu row.
const ROW_H: f32 = tokens::ROW_H;

/// Whether the app can pick in `mode`. Annotations are not hit-tested before v1.1, so picking
/// them would always find just the page.
pub fn available(mode: LayerMode) -> bool {
    mode != LayerMode::Annotations
}

/// Accessible name and text of the toolbar button, e.g. `Pick: Tags`.
pub fn label(mode: LayerMode) -> String {
    format!("Pick: {}", mode.title())
}

/// The toolbar button: it reads out the mode, which an icon alone cannot. `collapsed` keeps the
/// icon alone for a toolbar that is short of room; the tooltip then reads the mode out.
fn trigger(text: &str, mode: LayerMode, collapsed: bool) -> ToolButton<'_> {
    ToolButton::new(text)
        .icon(Icon::Pointer)
        .active(mode != LayerMode::All)
        .compact(true)
        .icon_only(collapsed)
        .opens_popup()
        .hint("▾")
}

/// The width [`button`] takes, so the toolbar can tell whether the name fits.
pub(super) fn width(ui: &Ui, state: &AppState, collapsed: bool) -> f32 {
    let mode = state.canvas.layer_mode;
    trigger(&label(mode), mode, collapsed).width(ui)
}

/// The toolbar button and its menu.
pub(super) fn button(ui: &mut Ui, state: &mut AppState, collapsed: bool) {
    let current = state.canvas.layer_mode;
    let text = label(current);
    let response = trigger(&text, current, collapsed).show(ui);
    let name = if collapsed { text.as_str() } else { "Pick" };
    let response = tooltip::named(response, name, "What a click on the page selects", Some("1–6"));
    let mut picked = None;
    popover::show(&response, RectAlign::BOTTOM_END, MENU_W, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (mode, key, dot) in MODES {
            if row(ui, mode, key, dot, mode == current) {
                picked = Some(mode);
            }
        }
    });
    if let Some(mode) = picked {
        state.canvas.layer_mode = mode;
        egui::Popup::close_all(ui.ctx());
    }
}

/// One mode of the menu; true when it was clicked.
fn row(ui: &mut Ui, mode: LayerMode, key: Key, dot: Option<Color32>, selected: bool) -> bool {
    let enabled = available(mode);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let title = mode.title();
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, title));
    let hovered = enabled && response.hovered();
    if selected {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::ACCENT_DIM);
    } else if hovered {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    let ink = match (enabled, selected) {
        (false, _) => tokens::TEXT_3,
        (true, true) => tokens::ACCENT,
        (true, false) => tokens::TEXT,
    };
    let y = rect.center().y;
    if let Some(color) = dot {
        let square = egui::Rect::from_center_size(egui::pos2(rect.min.x + 13.0, y), vec2(6.0, 6.0));
        ui.painter().rect_filled(
            square,
            2.0,
            if enabled { color } else { color.gamma_multiply(0.4) },
        );
    }
    ui.painter().text(
        egui::pos2(rect.min.x + 26.0, y),
        egui::Align2::LEFT_CENTER,
        title,
        fonts::ui(tokens::text::BODY),
        ink,
    );
    let note = if enabled { key.symbol_or_name() } else { LATER };
    ui.painter().text(
        egui::pos2(rect.max.x - 10.0, y),
        egui::Align2::RIGHT_CENTER,
        note,
        fonts::mono(tokens::mono::SMALL),
        tokens::TEXT_3,
    );
    focus::ring(ui, &response, rect, tokens::RADIUS_SM);
    if !enabled {
        tooltip::text(
            response,
            "Annotations are not hit-tested yet: picking them comes in v1.1.",
        );
        return false;
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}
