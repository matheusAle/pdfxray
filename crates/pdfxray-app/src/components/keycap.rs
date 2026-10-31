//! A key drawn as a keycap: `X`, `⇧T`, `⌘O`. Tooltips and menus show the shortcut with it, next to
//! the name and never at the far edge. Menus use the [`Keycap::quiet`] look: the key is a hint
//! there, not the subject.

use egui::{Label, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, vec2};

use crate::theme::{fonts, tokens};

/// Height of a keycap.
const HEIGHT: f32 = 16.0;
/// Smallest width of a keycap, so `X` is a key and not a sliver.
const MIN_W: f32 = 16.0;
/// Space left and right of the text.
const PAD: f32 = 5.0;
const RADIUS: f32 = 4.0;

/// A small rounded key: mono 10 on a neutral surface with a hairline.
#[derive(Debug, Clone, Copy)]
#[must_use = "call `show`"]
pub struct Keycap<'a> {
    key: &'a str,
    quiet: bool,
}

impl<'a> Keycap<'a> {
    /// A keycap showing `key`. The text is its accessible name.
    pub const fn new(key: &'a str) -> Self {
        Self { key, quiet: false }
    }

    /// The look for menu rows: no fill, only a faint outline, and dim text, so the key stays
    /// readable without competing with the name beside it.
    pub const fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Fill, outline and text colour of the key.
    const fn colors(self) -> (egui::Color32, egui::Color32, egui::Color32) {
        if self.quiet {
            (egui::Color32::TRANSPARENT, tokens::HAIRLINE, tokens::TEXT_3)
        } else {
            (tokens::NEUTRAL_ACTIVE, tokens::POPOVER_HAIRLINE, tokens::TEXT_2)
        }
    }

    /// Lays the keycap out inline, `HEIGHT` points tall whatever the row it sits in.
    pub fn show(self, ui: &mut Ui) -> Response {
        let font = fonts::mono(tokens::mono::SMALL);
        let (fill, outline, ink) = self.colors();
        let text_size = ui
            .painter()
            .layout_no_wrap(self.key.to_owned(), font.clone(), ink)
            .size();
        let (rect, response) =
            ui.allocate_exact_size(vec2((text_size.x + PAD * 2.0).max(MIN_W), HEIGHT), Sense::hover());
        ui.painter()
            .rect(rect, RADIUS, fill, Stroke::new(1.0, outline), StrokeKind::Inside);
        ui.put(
            Rect::from_center_size(rect.center(), text_size),
            Label::new(RichText::new(self.key).font(font).color(ink)).selectable(false),
        );
        response
    }

    /// Draws the keycap with its left edge at `left` and its middle at height `y`, without
    /// taking part in the layout of `ui`: the cursor stays where it was, so a row painted by hand
    /// is as tall as it was allocated.
    pub fn show_at(self, ui: &mut Ui, left: f32, y: f32) -> Response {
        let room = Rect::from_min_size(Pos2::new(left, y - HEIGHT / 2.0), vec2(64.0, HEIGHT));
        let mut child = ui.new_child(UiBuilder::new().max_rect(room));
        self.show(&mut child)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_quiet_key_has_no_fill_a_faint_outline_and_dim_text() {
        let quiet = Keycap::new("X").quiet();

        assert_eq!(
            quiet.colors(),
            (egui::Color32::TRANSPARENT, tokens::HAIRLINE, tokens::TEXT_3)
        );
        // Smoke: the tooltip key keeps its surface, so the two looks differ.
        assert_eq!(
            Keycap::new("X").colors(),
            (tokens::NEUTRAL_ACTIVE, tokens::POPOVER_HAIRLINE, tokens::TEXT_2)
        );
    }
}
