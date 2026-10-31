//! Tooltips: shown after the delay set in [`crate::theme::style::TOOLTIP_DELAY`] (400 ms) once
//! the pointer rests on the widget. They are frosted like popovers (the window fill of the
//! style) and open with the same fade and scale.

use egui::{Order, Rect, RectAlign, Response, RichText, Tooltip, Ui, WidgetText};

use super::keycap::Keycap;
use super::popover;
use crate::theme::{effects, fonts, motion, tokens};

/// Points between the edge of a menu (or button) and a tooltip that opens beside it.
const MENU_GAP: f32 = 8.0;

/// Where a tooltip beside its anchor opens, in order: the right of it, else the left.
const BESIDE: [RectAlign; 1] = [RectAlign::LEFT_START];

/// Where a tooltip opens against its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Below the widget.
    Below,
    /// Beside the menu the widget is a row of, level with the row.
    BesideMenu,
    /// Beside the widget, centred on it: for a column of buttons, where below would cover the
    /// next one.
    BesideWidget,
}

/// The rectangle a tooltip of `response` opens around, and where it opens. A widget of the page
/// gets its own rectangle (the tooltip opens below it). A row of a menu or popover gets the width
/// of the whole panel at the height of the row: the tooltip then opens beside the panel, level
/// with the row, instead of on top of the rows below it.
fn anchor_of(response: &Response) -> (Rect, Placement) {
    let panel = (response.layer_id.order == Order::Foreground)
        .then(|| response.ctx.memory(|m| m.area_rect(response.layer_id.id)))
        .flatten();
    match panel {
        Some(panel) => (
            Rect::from_x_y_ranges(panel.x_range(), response.rect.y_range()),
            Placement::BesideMenu,
        ),
        None => (response.rect, Placement::Below),
    }
}

/// Shows `content` as the tooltip of `response`, fading and scaling in while it opens. For a
/// tooltip that is more than a line or a name with a detail. Opens below the widget, or beside
/// the menu the widget is a row of.
pub fn custom(response: Response, content: impl FnOnce(&mut Ui)) -> Response {
    let (anchor, placement) = anchor_of(&response);
    place(response, anchor, placement, content)
}

fn place(response: Response, anchor: Rect, placement: Placement, content: impl FnOnce(&mut Ui)) -> Response {
    let ctx = response.ctx.clone();
    let id = popover::tooltip_motion_id(response.id);
    // A disabled control still explains itself, e.g. why it cannot be used yet.
    let mut tooltip = if response.enabled() {
        Tooltip::for_enabled(&response)
    } else {
        Tooltip::for_disabled(&response)
    };
    // The frame is chosen before the tooltip runs, like the one of a popover, so it fades
    // together with its content.
    let opening = if tooltip.popup.is_open() {
        motion::appear(&ctx, id, motion::POPOVER)
    } else {
        0.0
    };
    tooltip.popup = tooltip.popup.frame(effects::frosted_tooltip_at(opening));
    let beside = match placement {
        Placement::Below => None,
        Placement::BesideMenu => Some(RectAlign::RIGHT_START),
        Placement::BesideWidget => Some(RectAlign::RIGHT),
    };
    if let Some(align) = beside {
        tooltip.popup = tooltip
            .popup
            .anchor(anchor)
            .align(align)
            .align_alternatives(&BESIDE)
            .gap(MENU_GAP);
    }
    let shown = tooltip.show(|ui| {
        ui.multiply_opacity(opening);
        content(ui);
    });
    match shown {
        Some(shown) => popover::scale_in(
            &ctx,
            shown.response.layer_id,
            shown.response.rect,
            anchor,
            opening,
        ),
        None => motion::reset(&ctx, id),
    }
    response
}

/// Adds a tooltip with `text` to `response`.
pub fn text(response: Response, text: impl Into<WidgetText>) -> Response {
    let text = text.into();
    custom(response, |ui| {
        ui.label(text);
    })
}

/// Adds a tooltip with `text` that opens to the right of `response`, level with it: for the
/// buttons of the rail, where a tooltip below one would hide the next.
pub fn text_beside(response: Response, text: impl Into<WidgetText>) -> Response {
    let text = text.into();
    let anchor = response.rect;
    place(response, anchor, Placement::BesideWidget, |ui| {
        ui.label(text);
    })
}

/// Adds a tooltip with `text` and, as a keycap right after it, the key that does the same.
pub fn with_shortcut(response: Response, text: &str, shortcut: &str) -> Response {
    custom(response, |ui| {
        ui.horizontal(|ui| {
            ui.label(text);
            Keycap::new(shortcut).show(ui);
        });
    })
}

/// Widest a named tooltip grows before its description wraps.
const NAMED_W: f32 = 260.0;

/// A tooltip for a button that may show only its icon: the `name` in semibold with the
/// `shortcut` as a keycap right beside it, then what the button does.
pub fn named(response: Response, name: &str, detail: &str, shortcut: Option<&str>) -> Response {
    custom(response, |ui| {
        ui.set_max_width(NAMED_W);
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(name)
                    .font(fonts::semibold(tokens::text::BODY))
                    .color(tokens::TEXT),
            );
            if let Some(shortcut) = shortcut {
                Keycap::new(shortcut).show(ui);
            }
        });
        ui.label(RichText::new(detail).color(tokens::TEXT_2));
    })
}
