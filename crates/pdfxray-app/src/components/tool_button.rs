//! Toolbar buttons: icon and label, or icon only, with an active state.
//!
//! One accent only: an active button is neutral (a white 8 % fill, white icon and label, no
//! stroke); the accent fill with its gradient is for the primary action alone.

use egui::{
    Align2, Color32, CursorIcon, Popup, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, vec2,
};

use super::focus;
use super::icon::Icon;
use crate::theme::{effects, fonts, tokens};

const ICON: f32 = 14.0;
const PAD: f32 = 7.0;
const GAP: f32 = 5.0;
const PAD_COMPACT: f32 = 4.0;
const GAP_COMPACT: f32 = 3.0;
const HINT_GAP: f32 = 8.0;
/// The neutral active fill with the hover (white 12 %) and the press (white 16 %) laid over it,
/// so an active button reacts to the pointer too.
const ACTIVE_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x1E);
const ACTIVE_PRESSED: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x28);
/// A tall button (the title bar's Validate): height, corner radius, padding left and right, gap.
const TALL_H: f32 = 30.0;
const TALL_RADIUS: f32 = 7.0;
const TALL_PAD: (f32, f32) = (10.0, 12.0);
const TALL_GAP: f32 = 6.0;

/// How a [`ToolButton`] looks when it is not active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Transparent until hovered.
    Ghost,
    /// The one call to action: the accent gradient.
    Primary,
    /// A button that is always visible but is not the call to action: filled with a surface.
    Secondary,
}

/// A toolbar button. Active buttons get the neutral fill and white ink.
///
/// The label is always the accessible name, also when only the icon is drawn.
#[derive(Debug, Clone)]
#[must_use = "call `show`"]
pub struct ToolButton<'a> {
    label: &'a str,
    icon: Option<Icon>,
    active: bool,
    icon_only: bool,
    compact: bool,
    tall: bool,
    look: Look,
    hint: Option<&'a str>,
    min_width: f32,
    opens_popup: bool,
}

impl<'a> ToolButton<'a> {
    /// A button named `label`.
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            icon: None,
            active: false,
            icon_only: false,
            compact: false,
            tall: false,
            look: Look::Ghost,
            hint: None,
            min_width: 0.0,
            opens_popup: false,
        }
    }

    /// Puts `icon` in front of the label.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Draws the neutral active fill with white ink.
    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Draws only the icon; the label stays the accessible name and the tooltip.
    pub const fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only && self.icon.is_some();
        self
    }

    /// Tighter padding, for toolbars that have little room.
    pub const fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// The title bar's size: taller, rounder and with a semibold label.
    pub const fn tall(mut self) -> Self {
        self.tall = true;
        self
    }

    /// Fills the button with the accent gradient.
    pub const fn primary(mut self) -> Self {
        self.look = Look::Primary;
        self
    }

    /// Fills the button with a surface colour: visible at rest, but not the call to action.
    pub const fn secondary(mut self) -> Self {
        self.look = Look::Secondary;
        self
    }

    /// A dim note after the label, e.g. the shortcut (`⌘O`). Not part of the accessible name.
    pub const fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    /// Keeps the button at least `width` wide, its content centred, so a label that changes
    /// (the zoom readout) does not move the buttons beside it.
    pub const fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Draws the pressed look while the popup that a click on this button toggles is open.
    pub const fn opens_popup(mut self) -> Self {
        self.opens_popup = true;
        self
    }

    fn hint_font() -> egui::FontId {
        fonts::mono(tokens::mono::SMALL)
    }

    /// Space left and right of the content.
    fn pad(&self) -> (f32, f32) {
        if self.tall {
            TALL_PAD
        } else if self.compact {
            (PAD_COMPACT, PAD_COMPACT)
        } else {
            (PAD, PAD)
        }
    }

    fn gap(&self) -> f32 {
        if self.tall {
            TALL_GAP
        } else if self.compact {
            GAP_COMPACT
        } else {
            GAP
        }
    }

    fn height(&self) -> f32 {
        if self.tall { TALL_H } else { tokens::CONTROL_H }
    }

    fn font(&self) -> egui::FontId {
        if self.tall {
            fonts::semibold(tokens::text::BODY)
        } else {
            fonts::medium(tokens::text::BODY)
        }
    }

    /// The width [`Self::show`] will take, so a toolbar can decide what fits.
    pub fn width(&self, ui: &Ui) -> f32 {
        if self.icon_only {
            return self.height();
        }
        self.content_width(ui).max(self.min_width)
    }

    /// The width of padding, icon, label and hint.
    fn content_width(&self, ui: &Ui) -> f32 {
        let galley = ui
            .painter()
            .layout_no_wrap(self.label.to_owned(), self.font(), Color32::WHITE);
        let (left, right) = self.pad();
        let mut width = left + right + galley.size().x;
        if self.icon.is_some() {
            width += ICON + self.gap();
        }
        if let Some(hint) = self.hint {
            let galley = ui
                .painter()
                .layout_no_wrap(hint.to_owned(), Self::hint_font(), Color32::WHITE);
            width += HINT_GAP + galley.size().x;
        }
        width
    }

    /// Lays the button out and returns its response; `clicked()` is the action.
    pub fn show(self, ui: &mut Ui) -> Response {
        let enabled = ui.is_enabled();
        let font = self.font();
        let galley = (!self.icon_only).then(|| {
            ui.painter()
                .layout_no_wrap(self.label.to_owned(), font.clone(), Color32::WHITE)
        });
        let ((pad, _), gap) = (self.pad(), self.gap());

        let content = if self.icon_only {
            0.0
        } else {
            self.content_width(ui)
        };
        let width = self.width(ui);
        let (rect, response) = ui.allocate_exact_size(vec2(width, self.height()), Sense::click());
        let label = self.label;
        let active = self.active;
        response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, active, label));

        if ui.is_rect_visible(rect) {
            let hovered = response.hovered() && enabled;
            let open = self.opens_popup && Popup::is_id_open(ui.ctx(), Popup::default_response_id(&response));
            let pressed = (response.is_pointer_button_down_on() && enabled) || open;
            let (fill, stroke, ink) = self.colors(hovered, pressed);
            let radius = if self.tall { TALL_RADIUS } else { tokens::RADIUS_MD };
            if self.look == Look::Primary {
                effects::primary_gradient(ui.painter(), rect, radius);
                // Hover lightens, a press darkens, over the gradient.
                let shade = if response.is_pointer_button_down_on() {
                    Color32::from_black_alpha(0x28)
                } else if hovered {
                    Color32::from_white_alpha(0x1C)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, radius, shade);
            } else {
                ui.painter().rect(rect, radius, fill, stroke, StrokeKind::Inside);
            }
            focus::ring(ui, &response, rect, radius);
            let ink = if enabled { ink } else { tokens::TEXT_3 };
            let mut x = rect.min.x
                + if self.icon_only {
                    (rect.width() - ICON) / 2.0
                } else {
                    pad + (rect.width() - content) / 2.0
                };
            if let Some(icon) = self.icon {
                let slot =
                    egui::Rect::from_min_size(egui::pos2(x, rect.center().y - ICON / 2.0), Vec2::splat(ICON));
                icon.paint(ui, slot, ICON, ink);
                x += ICON + gap;
            }
            if let Some(galley) = galley {
                let size = galley.size();
                let pos = Align2::LEFT_CENTER.anchor_size(egui::pos2(x, rect.center().y), size);
                ui.painter().galley_with_override_text_color(pos.min, galley, ink);
                x += size.x;
            }
            if let (Some(hint), false) = (self.hint, self.icon_only) {
                ui.painter().text(
                    egui::pos2(x + HINT_GAP, rect.center().y),
                    Align2::LEFT_CENTER,
                    hint,
                    Self::hint_font(),
                    ink.gamma_multiply(0.6),
                );
            }
        }
        if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }

    /// Fill, stroke and ink for the state. Primary draws its own gradient: its fill is unused.
    fn colors(&self, hovered: bool, pressed: bool) -> (Color32, Stroke, Color32) {
        match (self.look, self.active) {
            (Look::Primary, _) => (tokens::PRIMARY_BOTTOM, Stroke::NONE, tokens::ON_ACCENT),
            (Look::Secondary, _) => {
                let fill = if hovered || pressed {
                    tokens::BORDER_STRONG
                } else {
                    tokens::SURFACE_3
                };
                (fill, Stroke::NONE, tokens::TEXT)
            }
            // Active is neutral: the accent belongs to the selection and the primary action.
            (Look::Ghost, true) => {
                let fill = if pressed {
                    ACTIVE_PRESSED
                } else if hovered {
                    ACTIVE_HOVER
                } else {
                    tokens::NEUTRAL_ACTIVE
                };
                (fill, Stroke::NONE, tokens::ACTIVE_INK)
            }
            (Look::Ghost, false) if pressed => (tokens::NEUTRAL_ACTIVE, Stroke::NONE, tokens::ACTIVE_INK),
            (Look::Ghost, false) if hovered => (tokens::NEUTRAL_HOVER, Stroke::NONE, tokens::TEXT),
            (Look::Ghost, false) => (Color32::TRANSPARENT, Stroke::NONE, tokens::TEXT_2),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fill(button: &ToolButton<'_>, hovered: bool, pressed: bool) -> Color32 {
        button.colors(hovered, pressed).0
    }

    #[test]
    fn a_ghost_button_reacts_to_hover_and_to_a_press() {
        let button = ToolButton::new("Fit");

        assert_eq!(fill(&button, false, false), Color32::TRANSPARENT);
        assert_eq!(fill(&button, true, false), tokens::NEUTRAL_HOVER);
        assert_eq!(fill(&button, true, true), tokens::NEUTRAL_ACTIVE);
        assert_eq!(button.colors(true, true).2, tokens::ACTIVE_INK);
        // Smoke: three different states, three different fills.
        assert_ne!(fill(&button, false, false), fill(&button, true, false));
        assert_ne!(fill(&button, true, false), fill(&button, true, true));
    }

    #[test]
    fn an_active_button_still_reacts_to_hover_and_to_a_press() {
        let button = ToolButton::new("Fit").active(true);

        let (rest, hover, press) = (
            fill(&button, false, false),
            fill(&button, true, false),
            fill(&button, true, true),
        );

        assert_eq!(rest, tokens::NEUTRAL_ACTIVE);
        assert_eq!(hover, ACTIVE_HOVER);
        assert_eq!(press, ACTIVE_PRESSED);
        // Each state is a little brighter than the one before it, over the same white.
        assert!(rest.a() < hover.a() && hover.a() < press.a());
        assert_eq!(button.colors(false, false).2, tokens::ACTIVE_INK);
    }

    #[test]
    fn a_secondary_button_shows_a_clear_step_when_hovered() {
        let button = ToolButton::new("Try a sample").secondary();

        let (rest, hover) = (fill(&button, false, false), fill(&button, true, false));

        assert_eq!(rest, tokens::SURFACE_3);
        assert_eq!(hover, tokens::BORDER_STRONG);
        // Smoke: a step of more than ten levels in every channel, and a press keeps it.
        assert!(hover.g().abs_diff(rest.g()) > 10 && hover.b().abs_diff(rest.b()) > 10);
        assert_eq!(fill(&button, true, true), hover);
    }
}
