//! The right-click menu of a node: the same [`Action`]s on tree rows, the canvas and values; and
//! the value menu of a property row, which starts with the copy lines.

use std::sync::Arc;

use egui::containers::menu::menu_style;
use egui::containers::{Popup, PopupAnchor, PopupKind};
use egui::{
    Align, Color32, CursorIcon, Galley, Layout, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo, pos2, vec2,
};
use pdfxray_core::NodeId;

use super::prop_row::{self, ValueTarget};
use super::{Icon, Keycap, focus};
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
            .map(|action| row_width(ui, action.label(), action.shortcut()))
            .fold(MIN_W, f32::max);
        ui.set_min_width(width);
        for action in &offered {
            if line(ui, action.icon(), action.label(), action.shortcut(), true, width).clicked() {
                picked = Some(*action);
                ui.close();
            }
        }
    });
    if let Some(action) = picked {
        actions::run(state, &response.ctx, id, action);
    }
}

/// A line of a value menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueItem {
    CopyValue,
    CopyKeyAndValue,
    FollowLink,
}

impl ValueItem {
    const ALL: [Self; 3] = [Self::CopyValue, Self::CopyKeyAndValue, Self::FollowLink];

    const fn label(self) -> &'static str {
        match self {
            Self::CopyValue => COPY_VALUE,
            Self::CopyKeyAndValue => COPY_KEY_AND_VALUE,
            Self::FollowLink => FOLLOW_LINK,
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::CopyValue => "⌘C",
            Self::CopyKeyAndValue => "⇧⌘C",
            Self::FollowLink => "↩",
        }
    }

    const fn icon(self) -> Icon {
        match self {
            Self::CopyValue | Self::CopyKeyAndValue => Icon::Copy,
            Self::FollowLink => Icon::CornerDownRight,
        }
    }
}

/// The value menu's first line.
pub const COPY_VALUE: &str = "Copy value";
/// The value menu's second line.
pub const COPY_KEY_AND_VALUE: &str = "Copy key and value";
/// The value menu's third line, disabled when the value is not a link.
pub const FOLLOW_LINK: &str = "Follow link";

/// The value menu of a property row while it is open: Copy value, Copy key and value and Follow
/// link (disabled unless the value is a link), then, under a line, the actions of `node` when
/// the panel has one. The row opens it (right-click, `⇧F10`, the context-menu key); opened from
/// the keyboard, its first line takes the focus.
pub fn attach_value(target: &ValueTarget, node: Option<(&mut AppState, &NodeId)>) {
    let ctx = target.row.ctx.clone();
    let id = prop_row::menu_id(target.row.id);
    let Some(anchor) = prop_row::menu_anchor(&ctx, target.row.id) else {
        return;
    };
    let offered = node
        .as_ref()
        .map(|(state, id)| Action::for_node(state, id))
        .unwrap_or_default();
    let mut picked_value = None;
    let mut picked = None;
    Popup::new(
        id,
        ctx.clone(),
        PopupAnchor::Position(anchor.at),
        target.row.layer_id,
    )
    .kind(PopupKind::Menu)
    .layout(Layout::top_down_justified(Align::Min))
    .style(menu_style)
    .open_memory(None)
    .show(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ValueItem::ALL
            .iter()
            .map(|item| row_width(ui, item.label(), Some(item.key())))
            .chain(
                offered
                    .iter()
                    .map(|action| row_width(ui, action.label(), action.shortcut())),
            )
            .fold(MIN_W, f32::max);
        ui.set_min_width(width);
        for (n, item) in ValueItem::ALL.into_iter().enumerate() {
            let enabled = item != ValueItem::FollowLink || target.link;
            let response = line(ui, item.icon(), item.label(), Some(item.key()), enabled, width);
            if n == 0 && anchor.keyboard {
                response.request_focus();
            }
            if response.clicked() {
                picked_value = Some(item);
                ui.close();
            }
        }
        if !offered.is_empty() {
            ui.separator();
        }
        for action in &offered {
            if line(ui, action.icon(), action.label(), action.shortcut(), true, width).clicked() {
                picked = Some(*action);
                ui.close();
            }
        }
    });
    match picked_value {
        Some(ValueItem::CopyValue) => prop_row::copy(&ctx, target.row.id, &target.copy, target.around),
        Some(ValueItem::CopyKeyAndValue) => {
            prop_row::copy(&ctx, target.row.id, &target.key_and_value(), target.around);
        }
        Some(ValueItem::FollowLink) => prop_row::request_follow(&ctx, target.row.id),
        None => {}
    }
    if let (Some(action), Some((state, id))) = (picked, node) {
        actions::run(state, &ctx, id, action);
    }
}

/// One line of the menu, `width` wide: icon, name and, next to it, its key. The whole line is
/// the hover and click target, like the rows of the View menu. A disabled line is dimmed to
/// text-3 and does not respond.
fn line(ui: &mut Ui, icon: Icon, label: &str, key: Option<&str>, enabled: bool, width: f32) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, tokens::ROW_H), sense);
    response.widget_info(|| WidgetInfo::labeled(Role::MenuItem, enabled, label));
    let fill = if !enabled {
        Color32::TRANSPARENT
    } else if response.is_pointer_button_down_on() {
        tokens::NEUTRAL_ACTIVE
    } else if response.hovered() {
        tokens::NEUTRAL_HOVER
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, tokens::RADIUS_SM, fill);
    focus::row_ring(ui, &response, rect, tokens::RADIUS_SM);
    let (ink, icon_ink) = if enabled {
        (tokens::TEXT, tokens::TEXT_2)
    } else {
        (tokens::TEXT_3, tokens::TEXT_3)
    };
    let y = rect.center().y;
    icon.paint(
        ui,
        Rect::from_center_size(pos2(rect.min.x + ICON_X, y), Vec2::splat(ICON)),
        ICON,
        icon_ink,
    );
    let name = label_galley(ui, label);
    let name_x = rect.min.x + NAME_X;
    let after = name_x + name.size().x + KEY_GAP;
    ui.painter()
        .galley(pos2(name_x, y - name.size().y / 2.0), name, ink);
    if let Some(key) = key {
        Keycap::new(key).quiet().show_at(ui, after, y);
    }
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// `label` laid out for a line.
/// Its colour is the one it is painted with: a disabled line is dimmed.
fn label_galley(ui: &Ui, label: &str) -> Arc<Galley> {
    ui.painter().layout_no_wrap(
        label.to_owned(),
        fonts::ui(tokens::text::BODY),
        Color32::PLACEHOLDER,
    )
}

/// The width a line needs for `label` and `key`: icon, name, the gap and its keycap, and a right
/// margin.
fn row_width(ui: &Ui, label: &str, key: Option<&str>) -> f32 {
    let mut width = NAME_X + label_galley(ui, label).size().x + NAME_X - ICON_X;
    if let Some(key) = key {
        let key =
            ui.painter()
                .layout_no_wrap(key.to_owned(), fonts::mono(tokens::mono::SMALL), tokens::TEXT_2);
        width += KEY_GAP + (key.size().x + 10.0).max(16.0);
    }
    width
}
