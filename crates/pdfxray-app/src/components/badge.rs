//! Count pills, e.g. the number of findings on a drawer tab.

use egui::{Label, Response, RichText, Sense, Stroke, StrokeKind, Ui, vec2};

use super::chip::Tone;
use crate::theme::{fonts, tokens};

/// Height of the pill.
const HEIGHT: f32 = 16.0;
/// Space between the digits and the ends of the pill.
const PAD: f32 = 5.0;

/// A count in a pill.
#[derive(Debug, Clone, Copy)]
#[must_use = "call `show`"]
pub struct Badge {
    count: usize,
    tone: Tone,
}

impl Badge {
    /// A neutral pill showing `count`.
    pub const fn new(count: usize) -> Self {
        Self {
            count,
            tone: Tone::Neutral,
        }
    }

    /// Tints the pill.
    pub const fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// The width of the pill: as wide as its digits, never narrower than tall.
    pub fn width(self, ui: &Ui) -> f32 {
        let digits = ui
            .painter()
            .layout_no_wrap(
                self.count.to_string(),
                fonts::mono_semibold(tokens::mono::SMALL),
                tokens::TEXT_2,
            )
            .size()
            .x;
        (digits + 2.0 * PAD).max(HEIGHT)
    }

    /// Lays the pill out: `HEIGHT` high, as wide as its digits (never narrower than tall, so
    /// one digit makes a circle), and not as tall as a control.
    pub fn show(self, ui: &mut Ui) -> Response {
        let (fill, border, ink) = match self.tone {
            Tone::Neutral => (tokens::SURFACE_3, tokens::SURFACE_3, tokens::TEXT_2),
            other => other.colors(),
        };
        let text = RichText::new(self.count.to_string())
            .font(fonts::mono_semibold(tokens::mono::SMALL))
            .color(ink);
        let (rect, response) = ui.allocate_exact_size(vec2(self.width(ui), HEIGHT), Sense::hover());
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, fill);
        if border != fill {
            ui.painter()
                .rect_stroke(rect, radius, Stroke::new(1.0, border), StrokeKind::Inside);
        }
        ui.put(rect, Label::new(text).selectable(false));
        response
    }
}
