//! Popovers anchored to the widget that opens them. Esc and a click outside close them.
//!
//! They use the frosted frame and open with a fade and a scale from 0.97 to 1 over
//! [`motion::POPOVER`] (140 ms). egui scales a layer's shapes around a point, so the scale is
//! centred on the point of the popover nearest to its anchor, which reads as growing out of it;
//! only what is drawn is scaled, hit-testing uses the final geometry.

use egui::containers::{Popup, PopupCloseBehavior};
use egui::emath::TSTransform;
use egui::{Context, Id, LayerId, Pos2, Rect, RectAlign, Response, Ui};

use crate::theme::{effects, motion};

/// The `width` to ask [`show`] for so that the popover, frame included, is `outer` wide: the
/// width of a popover is the one of its content.
pub fn inner_width(outer: f32) -> f32 {
    outer - effects::frosted_panel().total_margin().sum().x
}

/// Shows a popover next to `anchor` while it is open. A click on `anchor` toggles it. Returns
/// what `content` returns while it is open.
pub fn show<R>(
    anchor: &Response,
    align: RectAlign,
    width: f32,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    show_with_gap(anchor, align, width, None, content)
}

/// Like [`show`], with `gap` points between the anchor and the popover instead of the default,
/// e.g. to open it beside a whole panel rather than beside the one value inside it.
pub fn show_with_gap<R>(
    anchor: &Response,
    align: RectAlign,
    width: f32,
    gap: Option<f32>,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    show_beside(anchor, anchor.rect, align, width, gap, content)
}

/// Like [`show_with_gap`] for a control made of several widgets: a click on `anchor` (one of
/// them) toggles the popover, which opens beside `around`, the rectangle of the whole control.
pub fn show_beside<R>(
    anchor: &Response,
    around: Rect,
    align: RectAlign,
    width: f32,
    gap: Option<f32>,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let mut popup = Popup::from_toggle_button_response(anchor).anchor(around);
    if let Some(gap) = gap {
        popup = popup.gap(gap);
    }
    let ctx = anchor.ctx.clone();
    let id = popup.get_id();
    // The frame is chosen before the popup runs, so the opening state is the one of the frame
    // that was drawn last: the first frame of a popup is its (invisible) sizing pass anyway.
    let opening = if popup.is_open() {
        motion::appear(&ctx, id, motion::POPOVER)
    } else {
        motion::reset(&ctx, id);
        0.0
    };
    let shown = popup
        .align(align)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .width(width)
        .frame(effects::frosted_panel_at(opening))
        .show(|ui| {
            ui.multiply_opacity(opening);
            ui.set_min_width(width);
            content(ui)
        })?;
    scale_in(
        &ctx,
        shown.response.layer_id,
        shown.response.rect,
        around,
        opening,
    );
    Some(shown.inner)
}

/// Draws the layer of an overlay at the scale of its opening state `progress` (0..=1): 0.97 at
/// the start, 1 when done, centred on the point of `rect` nearest to `anchor`.
pub(super) fn scale_in(ctx: &Context, layer: LayerId, rect: Rect, anchor: Rect, progress: f32) {
    if progress >= 1.0 {
        return;
    }
    let scale = motion::POPOVER_FROM_SCALE + (1.0 - motion::POPOVER_FROM_SCALE) * progress;
    let pivot: Pos2 = rect.clamp(anchor.center());
    ctx.transform_layer_shapes_after_rounding(
        layer,
        TSTransform::new(pivot.to_vec2() * (1.0 - scale), scale),
    );
}

/// The id the opening animation of a tooltip for `widget` is kept under.
pub(super) fn tooltip_motion_id(widget: Id) -> Id {
    widget.with("pdfxray-tooltip")
}
