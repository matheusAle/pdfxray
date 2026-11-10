//! A small on/off switch with its label to the right.

use egui::{Rect, Response, Role, Sense, Ui, WidgetInfo, vec2};

use super::focus;
use crate::theme::{fonts, tokens};

/// Height of the area that takes the click.
const HIT_H: f32 = tokens::CONTROL_H - 6.0;

/// Size of the track the knob slides in.
const TRACK: egui::Vec2 = vec2(26.0, 14.0);

/// Space between the track and the label.
const LABEL_GAP: f32 = 6.0;

/// The width [`switch`] takes with `label`, to lay out a row before drawing it.
pub fn switch_width(ui: &Ui, label: &str) -> f32 {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), fonts::ui(tokens::text::BODY), tokens::TEXT_2);
    TRACK.x + LABEL_GAP + galley.size().x
}

/// Draws a switch that flips `on` when clicked. The response of the whole control is
/// [`Response::changed`] after a flip; its accessible name is `label`.
pub fn switch(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    switch_named(ui, on, label, label)
}

/// A [`switch`] that shows `label`, a shortened one, and is named `name` for assistive tech.
pub fn switch_named(ui: &mut Ui, on: &mut bool, label: &str, name: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), fonts::ui(tokens::text::BODY), tokens::TEXT_2);
    let track = TRACK;
    // Taller than the track, so the pointer does not have to find 14 points.
    let size = vec2(track.x + LABEL_GAP + galley.size().x, HIT_H.max(galley.size().y));
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(Role::Switch, true, *on, name));
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let track_rect = Rect::from_min_size(egui::pos2(rect.min.x, rect.center().y - track.y / 2.0), track);
    let hovered = response.hovered();
    let (fill, knob_x) = if *on {
        let accent = if hovered {
            tokens::ACCENT
        } else {
            tokens::ACCENT.gamma_multiply(0.85)
        };
        (accent, track_rect.max.x - 7.0)
    } else {
        let off = if hovered {
            tokens::BORDER_STRONG
        } else {
            tokens::SURFACE_3
        };
        (off, track_rect.min.x + 7.0)
    };
    let ink = if hovered { tokens::TEXT } else { tokens::TEXT_2 };
    ui.painter().rect_filled(track_rect, track.y / 2.0, fill);
    focus::ring(ui, &response, track_rect, track.y / 2.0);
    ui.painter()
        .circle_filled(egui::pos2(knob_x, track_rect.center().y), 5.0, tokens::BG);
    ui.painter().galley_with_override_text_color(
        egui::pos2(
            track_rect.max.x + LABEL_GAP,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        ink,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
