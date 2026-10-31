//! Small mono pills: tags (`/P`), kinds (`glyph`), statuses (`Tagged · UA-2`).

use egui::{Color32, Frame, Margin, Response, RichText, Sense, Stroke, Ui, vec2};

use super::Icon;
use crate::theme::{fonts, tokens};

/// Height of a chip.
const HEIGHT: f32 = 20.0;

/// Height of a fact chip on a card ([`Chip::card`]).
const CARD_HEIGHT: f32 = 18.0;

/// Size of the icon in front of a chip's text.
const ICON: f32 = 10.0;

/// What a chip's colour says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// No meaning: names, versions.
    Neutral,
    /// A kind of node; the colour is the kind's token.
    Kind(Color32),
    /// Good.
    Ok,
    /// Needs a look.
    Warn,
    /// Broken.
    Err,
}

impl Tone {
    /// Fill, border and text colour of a chip with this tone.
    pub fn colors(self) -> (Color32, Color32, Color32) {
        let tinted = |c: Color32| (c.gamma_multiply(0.12), c.gamma_multiply(0.35), c);
        match self {
            Self::Neutral => (tokens::SURFACE_2, tokens::BORDER, tokens::TEXT_2),
            Self::Kind(c) => tinted(c),
            Self::Ok => tinted(tokens::OK),
            Self::Warn => tinted(tokens::WARN),
            Self::Err => tinted(tokens::ERR),
        }
    }
}

/// A flat pill: a name, a kind or a status. Not a control; a chip that opens something is built
/// by the panel that owns it.
#[derive(Debug, Clone)]
#[must_use = "call `show`"]
pub struct Chip<'a> {
    text: &'a str,
    tone: Tone,
    dot: Option<Color32>,
    icon: Option<Icon>,
    card: bool,
}

impl<'a> Chip<'a> {
    /// A neutral chip.
    pub const fn new(text: &'a str) -> Self {
        Self {
            text,
            tone: Tone::Neutral,
            dot: None,
            icon: None,
            card: false,
        }
    }

    /// Sets the tone.
    pub const fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// A kind dot in front of the text.
    pub const fn dot(mut self, color: Color32) -> Self {
        self.dot = Some(color);
        self
    }

    /// An icon in the tone's colour in front of the text.
    pub const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The fact chip of a preview card: an 18-point pill with a hairline, neutral or (with
    /// [`Tone::Warn`]) amber at 10 % with a 25 % hairline.
    pub const fn card(mut self) -> Self {
        self.card = true;
        self
    }

    /// Lays the chip out: a flat surface pill whose text carries the tone.
    pub fn show(self, ui: &mut Ui) -> Response {
        let (_, _, ink) = self.tone.colors();
        let (fill, stroke, height) = match (self.card, self.tone) {
            (false, _) => (tokens::SURFACE_3, Stroke::NONE, HEIGHT),
            (true, Tone::Warn) => (
                tokens::WARN_CHIP,
                Stroke::new(1.0, tokens::WARN_CHIP_HAIRLINE),
                CARD_HEIGHT,
            ),
            (true, _) => (
                tokens::FACT_CHIP,
                Stroke::new(1.0, tokens::FACT_CHIP_HAIRLINE),
                CARD_HEIGHT,
            ),
        };
        Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(4)
            .inner_margin(Margin::symmetric(7, 0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.horizontal(|ui| {
                    // The hairline is inside the frame: the row keeps the chip's height.
                    ui.set_height(height - 2.0 * stroke.width);
                    if let Some(color) = self.dot {
                        let (rect, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                    }
                    if let Some(icon) = self.icon {
                        icon.show(ui, ICON, ink);
                    }
                    ui.label(
                        RichText::new(self.text)
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(ink),
                    );
                });
            })
            .response
    }
}
