//! What the table overlay and the footnote connectors share: lines between two boxes that leave
//! them at their edges, three line styles, arrow heads and the small label chips that sit on a
//! line and are buttons.

use egui::{
    Color32, CursorIcon, Id, Painter, Pos2, Rect as UiRect, Role, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, pos2, vec2,
};

use super::tag_overlay::clear_of;
use crate::theme::{fonts, tokens};

/// Width of a connector.
pub(super) const WIDTH: f32 = 1.5;

/// Length of an arrow head.
const ARROW: f32 = 7.0;

/// Dash and gap of a dashed connector.
const DASH: (f32, f32) = (6.0, 4.0);

/// Dot spacing of a dotted connector.
const DOT: (f32, f32) = (1.5, 4.0);

/// How a connector is drawn: what kind of tie it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineStyle {
    /// An explicit tie.
    Solid,
    /// A tie by a rule that reaches over several cells.
    Dashed,
    /// A tie by position alone.
    Dotted,
}

/// The point where the line from the centre of `rect` towards `to` leaves `rect`. `to` inside the
/// rectangle gives its centre.
pub(super) fn edge_toward(rect: UiRect, to: Pos2) -> Pos2 {
    let centre = rect.center();
    let delta = to - centre;
    if rect.contains(to) || (delta.x == 0.0 && delta.y == 0.0) {
        return centre;
    }
    let half = rect.size() / 2.0;
    let sx = if delta.x == 0.0 {
        f32::INFINITY
    } else {
        half.x / delta.x.abs()
    };
    let sy = if delta.y == 0.0 {
        f32::INFINITY
    } else {
        half.y / delta.y.abs()
    };
    centre + delta * sx.min(sy)
}

/// Draws a line from `from` to `to`.
pub(super) fn line(painter: &Painter, from: Pos2, to: Pos2, color: Color32, style: LineStyle) {
    let stroke = Stroke::new(WIDTH, color);
    match style {
        LineStyle::Solid => {
            painter.line_segment([from, to], stroke);
        }
        LineStyle::Dashed => painter.extend(Shape::dashed_line(&[from, to], stroke, DASH.0, DASH.1)),
        LineStyle::Dotted => painter.extend(Shape::dashed_line(&[from, to], stroke, DOT.0, DOT.1)),
    }
}

/// An arrow head with its tip at `tip`, for a line that comes from `from`.
pub(super) fn arrow_head(painter: &Painter, tip: Pos2, from: Pos2, color: Color32) {
    let back = (from - tip).normalized();
    if !back.is_finite() {
        return;
    }
    let side = vec2(-back.y, back.x) * (ARROW * 0.5);
    let base = tip + back * ARROW;
    painter.add(Shape::convex_polygon(
        vec![tip, base + side, base - side],
        color,
        Stroke::NONE,
    ));
}

/// How far a label sits off the line it belongs to, so a short connector stays visible.
const BESIDE: f32 = 11.0;

/// The middle of the line from `from` to `to`, moved to one side of it.
pub(super) fn beside(from: Pos2, to: Pos2) -> Pos2 {
    let mid = from.lerp(to, 0.5);
    let along = (to - from).normalized();
    if along.is_finite() {
        mid + vec2(-along.y, along.x) * BESIDE
    } else {
        mid
    }
}

/// The middle of the line from `from` to `to`, moved to one side of it by as much as a box of
/// `size` needs to leave the line clear: a wide label by a steep line moves sideways by half its
/// width, not by [`BESIDE`].
pub(super) fn clear_beside(from: Pos2, to: Pos2, size: Vec2) -> Pos2 {
    let mid = from.lerp(to, 0.5);
    let along = (to - from).normalized();
    if !along.is_finite() {
        return mid;
    }
    let normal = vec2(-along.y, along.x);
    let reach = normal.x.abs() * size.x / 2.0 + normal.y.abs() * size.y / 2.0;
    mid + normal * (reach + BESIDE / 2.0)
}

/// What a [`label`] says and is called.
#[derive(Debug, Clone, Copy)]
pub(super) struct Label<'a> {
    /// The text on the chip.
    pub(super) text: &'a str,
    /// The colour of the text and the border.
    pub(super) ink: Color32,
    /// The widget's id.
    pub(super) id: Id,
    /// The widget's accessible name.
    pub(super) name: &'a str,
}

/// A label chip centred on `at`, clear of the chips in `placed`: mono text on a dark fill with a
/// border of the same colour, as a button.
pub(super) fn label(
    ui: &Ui,
    painter: &Painter,
    placed: &mut Vec<UiRect>,
    at: Pos2,
    label: &Label<'_>,
) -> egui::Response {
    let galley = painter.layout_no_wrap(
        label.text.to_owned(),
        fonts::mono_medium(tokens::mono::SMALL),
        label.ink,
    );
    let size = galley.size() + vec2(10.0, 5.0);
    let rect = clear_of(placed, UiRect::from_center_size(at, size));
    placed.push(rect);
    let response = ui
        .interact(rect, label.id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.name));
    // Under the pointer the chip takes a wash of its colour and a full-strength border.
    let hovered = response.hovered();
    painter.rect_filled(rect, tokens::RADIUS_SM, tokens::BG.gamma_multiply(0.92));
    if hovered {
        painter.rect_filled(rect, tokens::RADIUS_SM, label.ink.gamma_multiply(0.18));
    }
    painter.rect_stroke(
        rect,
        tokens::RADIUS_SM,
        Stroke::new(1.0, label.ink.gamma_multiply(if hovered { 1.0 } else { 0.6 })),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(5.0, 2.5), galley, label.ink);
    response
}

/// A small dot at `at`, where a connector starts.
pub(super) fn anchor(painter: &Painter, at: Pos2, color: Color32) {
    painter.circle_filled(at, 2.5, color);
}

/// The point `inset` inside the page rectangle's right edge at the height of `y`: where a
/// connector to another page leaves, and arrives.
pub(super) fn margin_point(page: UiRect, y: f32, inset: f32) -> Pos2 {
    pos2(
        page.max.x - inset,
        y.clamp(page.min.y + inset, page.max.y - inset),
    )
}
