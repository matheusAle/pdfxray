//! Where the hosts go.

use egui::{Color32, Context, Frame, Id, Panel, Rect, Ui};

use super::{rail, status_bar, title_bar};
use crate::state::AppState;
use crate::states::{Screen, empty, encrypted, opening};
use crate::theme::tokens;
use crate::{center, drawer, inspector, navigator};

/// A panel's frame: one tone, no border. Areas are told apart by tone, not by lines.
fn bar(fill: Color32) -> Frame {
    Frame::new().fill(fill)
}

/// Id salts of the side panels, which the user can drag wider or narrower.
const NAVIGATOR: &str = "navigator";
const INSPECTOR: &str = "inspector";

/// Where the navigator is drawn this frame (last frame before the layout runs); `None` while it
/// is not shown.
pub(crate) fn navigator_rect(ctx: &Context) -> Option<Rect> {
    ctx.data(|d| d.get_temp::<Option<Rect>>(navigator_rect_id()).flatten())
}

fn navigator_rect_id() -> Id {
    Id::unique("navigator rect")
}

/// Draws a host inside the panel's rectangle and clips it there. The host is a child of the
/// panel's ui, so a line that is too long for it cannot push the panel around. The panel's ui
/// is told the whole rectangle is used: a resizable panel takes its size from what it used, and
/// would otherwise shrink to its minimum.
fn contained(ui: &mut Ui, host: impl FnOnce(&mut Ui)) {
    let rect = ui.max_rect();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("host"));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    host(&mut child);
    ui.expand_to_include_rect(rect);
}

/// A state without a workspace: its card fills the space between the rail and the window's edge.
fn stage(ui: &mut Ui, card: impl FnOnce(&mut Ui)) {
    egui::CentralPanel::default()
        .frame(bar(tokens::CANVAS))
        .show(ui, card);
}

/// Lays the hosts out around the centre.
pub(super) fn show(ui: &mut Ui, state: &mut AppState) {
    ui.data_mut(|d| d.insert_temp(navigator_rect_id(), None::<Rect>));
    Panel::top("title_bar")
        .exact_size(tokens::TITLE_H)
        .resizable(false)
        .frame(bar(tokens::TITLE_BG))
        .show(ui, |ui| title_bar::ui(ui, state));
    Panel::bottom("status_bar")
        .exact_size(tokens::STATUS_H)
        .resizable(false)
        .frame(bar(tokens::STATUS_BG))
        .show(ui, |ui| status_bar::ui(ui, state));
    Panel::left("rail")
        .exact_size(tokens::RAIL_W)
        .resizable(false)
        .frame(bar(tokens::RAIL_BG))
        .show(ui, |ui| rail::ui(ui, state));

    match state.screen() {
        Screen::Empty => return stage(ui, |ui| empty::ui(ui, state)),
        Screen::Opening => return stage(ui, |ui| opening::ui(ui, state)),
        Screen::Encrypted => return stage(ui, |ui| encrypted::ui(ui, state)),
        Screen::Workspace => {}
    }

    // Either side panel can be dragged as wide as the room left once the other one is at its
    // narrowest and the canvas keeps its own minimum. The limit never drops below the default
    // width: egui keeps a size once it is clamped, and the first frame can be tiny.
    let room = ui.available_width() - tokens::CANVAS_MIN_W;
    let navigator = Panel::left(NAVIGATOR)
        .resizable(true)
        .default_size(tokens::NAVIGATOR_W)
        .size_range(tokens::NAVIGATOR_MIN_W..=(room - tokens::INSPECTOR_MIN_W).max(tokens::NAVIGATOR_W))
        .frame(bar(tokens::PANEL))
        .show(ui, |ui| contained(ui, |ui| navigator::ui(ui, state)));
    ui.data_mut(|d| d.insert_temp(navigator_rect_id(), Some(navigator.response.rect)));
    let room = ui.available_width() - tokens::CANVAS_MIN_W;
    Panel::right(INSPECTOR)
        .resizable(true)
        .default_size(tokens::INSPECTOR_W)
        .size_range(tokens::INSPECTOR_MIN_W..=room.max(tokens::INSPECTOR_W))
        .frame(bar(tokens::PANEL))
        .show(ui, |ui| contained(ui, |ui| inspector::ui(ui, state)));
    egui::CentralPanel::default()
        .frame(bar(tokens::CANVAS))
        .show(ui, |ui| {
            // The tab strip while collapsed, the stored height while open.
            let height = state.drawer.panel_height(ui.available_height());
            Panel::bottom("drawer")
                .exact_size(height)
                .resizable(false)
                .frame(bar(tokens::DRAWER_BG))
                .show(ui, |ui| contained(ui, |ui| drawer::ui(ui, state)));
            egui::CentralPanel::default()
                .frame(bar(tokens::CANVAS))
                .show(ui, |ui| center::ui(ui, state));
        });
}
