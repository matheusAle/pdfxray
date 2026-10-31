//! egui visuals and spacing from the design tokens. The app is dark only.

use egui::style::{KineticScrollStyle, ScrollAnimation};
use egui::{Context, CornerRadius, Margin, Stroke, Style, TextStyle, Vec2, Visuals, vec2};
use pdfxray_core::numeric::u8_from_f32_round;

use super::effects::POPOVER_SHADOW;
use super::tokens::{
    ACCENT, ACCENT_DIM, CANVAS, ERR, HAIRLINE, NEUTRAL_ACTIVE, NEUTRAL_HOVER, PANEL, POPOVER_FILL,
    POPOVER_HAIRLINE, RADIUS_LG, RADIUS_MD, ROW_H, TEXT, TEXT_2, TEXT_3, WARN, mono, text,
};
use super::{fonts, motion};

/// How long the pointer must rest on a widget before its tooltip shows.
pub const TOOLTIP_DELAY: f32 = 0.4;

/// Applies the dark visuals and spacing to every theme of `ctx`, so a light OS setting cannot
/// change the look.
pub fn install(ctx: &Context) {
    ctx.all_styles_mut(|style| {
        apply(style);
    });
}

fn radius(r: f32) -> CornerRadius {
    // Radii are tiny whole numbers.
    CornerRadius::same(u8_from_f32_round(r))
}

fn apply(style: &mut Style) {
    style.visuals = visuals();
    style.text_styles = [
        (TextStyle::Small, fonts::ui(text::SMALL)),
        (TextStyle::Body, fonts::ui(text::BODY)),
        (TextStyle::Button, fonts::ui(text::BODY)),
        (TextStyle::Heading, fonts::semibold(text::TITLE)),
        (TextStyle::Monospace, fonts::mono(mono::BODY)),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = vec2(6.0, 4.0);
    spacing.button_padding = vec2(8.0, 4.0);
    spacing.interact_size = Vec2::new(24.0, ROW_H);
    spacing.window_margin = Margin::same(10);
    spacing.menu_margin = Margin::same(8);
    spacing.indent = 14.0;
    spacing.icon_width = 14.0;
    spacing.icon_width_inner = 8.0;
    spacing.slider_rail_height = 4.0;
    spacing.scroll.bar_width = 6.0;
    spacing.scroll.floating_width = 4.0;
    spacing.scroll.foreground_color = false;

    // egui's own scroll motion is motion too: with "Reduce motion" a scroll-to-widget jumps, and
    // a list does not coast on after a drag or spring back when its content shrinks.
    if motion::reduce_motion() {
        style.scroll_animation = ScrollAnimation::none();
        style.spacing.scroll.kinetic = KineticScrollStyle {
            decay_time: 0.0,
            rubber_band: false,
            ..KineticScrollStyle::default()
        };
    }

    style.interaction.tooltip_delay = TOOLTIP_DELAY;
    style.interaction.show_tooltips_only_when_still = true;
    // Values are copied with their own icon, never by selecting text.
    style.interaction.selectable_labels = false;
    style.interaction.multi_widget_text_select = false;
}

/// Every fill and stroke of egui's own widgets comes from the v2 tokens, so nothing egui draws
/// by itself (text edits, sliders, separators, popups) is lighter than the design. Areas are
/// told apart by tone: the only strokes left are hairlines and the accent of a focus ring.
fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = None;
    v.weak_text_color = Some(TEXT_3);
    v.hyperlink_color = ACCENT;
    v.faint_bg_color = NEUTRAL_HOVER;
    v.extreme_bg_color = CANVAS;
    v.text_edit_bg_color = Some(CANVAS);
    v.code_bg_color = NEUTRAL_ACTIVE;
    v.warn_fg_color = WARN;
    v.error_fg_color = ERR;
    v.panel_fill = PANEL;
    // Popups and tooltips are built from the window fill, stroke and shadow: the frosted look.
    v.window_fill = POPOVER_FILL;
    v.window_stroke = Stroke::new(1.0, POPOVER_HAIRLINE);
    v.window_corner_radius = radius(RADIUS_LG);
    v.menu_corner_radius = radius(RADIUS_LG);
    v.window_shadow = POPOVER_SHADOW;
    v.popup_shadow = POPOVER_SHADOW;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.text_cursor.stroke = Stroke::new(1.5, ACCENT);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.disabled_alpha = 0.45;

    let corner = radius(RADIUS_MD);
    let w = &mut v.widgets;
    // Separators and frames of plain egui widgets use this stroke: a hairline, not a border.
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = corner;

    w.inactive.bg_fill = NEUTRAL_HOVER;
    w.inactive.weak_bg_fill = NEUTRAL_HOVER;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_2);
    w.inactive.corner_radius = corner;

    w.hovered.bg_fill = NEUTRAL_ACTIVE;
    w.hovered.weak_bg_fill = NEUTRAL_HOVER;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.corner_radius = corner;
    w.hovered.expansion = 0.0;

    // Pressed and open controls are neutral too: the accent is for the selection only.
    w.active.bg_fill = NEUTRAL_ACTIVE;
    w.active.weak_bg_fill = NEUTRAL_ACTIVE;
    w.active.bg_stroke = Stroke::new(1.0, POPOVER_HAIRLINE);
    w.active.fg_stroke = Stroke::new(1.0, TEXT);
    w.active.corner_radius = corner;
    w.active.expansion = 0.0;

    w.open.bg_fill = NEUTRAL_ACTIVE;
    w.open.weak_bg_fill = NEUTRAL_ACTIVE;
    w.open.bg_stroke = Stroke::NONE;
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = corner;
    v
}
