//! The right-click menu of a node: the same [`Action`]s on tree rows, the canvas and values.

use std::sync::Arc;

use egui::{Color32, CursorIcon, Galley, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo, pos2, vec2};
use pdfxray_core::NodeId;

use super::{Keycap, focus};
use crate::actions::{self, Action};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Narrowest the menu is.
const MIN_W: f32 = 180.0;
const ICON: f32 = 14.0;
/// Centre of the icon and start of the name, from the left edge of a row: the same insets as the
/// rows of the View menu.
const ICON_X: f32 = 14.0;
const NAME_X: f32 = 30.0;
/// Space between the name and its keycap.
const KEY_GAP: f32 = 8.0;

/// Attaches the actions of `id` to a right-click on `response`, and runs the one picked. Nothing
/// is built unless `response` was right-clicked or its menu is open, so every row can call this.
pub fn attach(response: &Response, state: &mut AppState, id: &NodeId) {
    if !(response.secondary_clicked() || response.context_menu_opened()) {
        return;
    }
    let offered = Action::for_node(state, id);
    if offered.is_empty() {
        return;
    }
    let mut picked = None;
    response.context_menu(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = offered
            .iter()
            .map(|action| row_width(ui, *action))
            .fold(MIN_W, f32::max);
        ui.set_min_width(width);
        for action in &offered {
            if item(ui, *action, width).clicked() {
                picked = Some(*action);
                ui.close();
            }
        }
    });
    if let Some(action) = picked {
        actions::run(state, &response.ctx, id, action);
    }
}

/// One line of the menu, `width` wide: icon, name and, next to it, its key. The whole line is
/// the hover and click target, like the rows of the View menu.
fn item(ui: &mut Ui, action: Action, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, tokens::ROW_H), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::MenuItem, true, action.label()));
    let fill = if response.is_pointer_button_down_on() {
        tokens::NEUTRAL_ACTIVE
    } else if response.hovered() {
        tokens::NEUTRAL_HOVER
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, tokens::RADIUS_SM, fill);
    focus::ring(ui, &response, rect, tokens::RADIUS_SM);
    let y = rect.center().y;
    action.icon().paint(
        ui,
        Rect::from_center_size(pos2(rect.min.x + ICON_X, y), Vec2::splat(ICON)),
        ICON,
        tokens::TEXT_2,
    );
    let name = label_galley(ui, action);
    let name_x = rect.min.x + NAME_X;
    let after = name_x + name.size().x + KEY_GAP;
    ui.painter()
        .galley(pos2(name_x, y - name.size().y / 2.0), name, tokens::TEXT);
    if let Some(key) = action.shortcut() {
        Keycap::new(key).quiet().show_at(ui, after, y);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// The name of `action` laid out for a row.
fn label_galley(ui: &Ui, action: Action) -> Arc<Galley> {
    ui.painter().layout_no_wrap(
        action.label().to_owned(),
        fonts::ui(tokens::text::BODY),
        tokens::TEXT,
    )
}

/// The width a row needs for `action`: icon, name, the gap and its keycap, and a right margin.
fn row_width(ui: &Ui, action: Action) -> f32 {
    let mut width = NAME_X + label_galley(ui, action).size().x + NAME_X - ICON_X;
    if let Some(key) = action.shortcut() {
        let key =
            ui.painter()
                .layout_no_wrap(key.to_owned(), fonts::mono(tokens::mono::SMALL), tokens::TEXT_2);
        width += KEY_GAP + (key.size().x + 10.0).max(16.0);
    }
    width
}
