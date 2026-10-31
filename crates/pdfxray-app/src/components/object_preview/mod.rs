//! The preview card of an object (design board "2.9 Objects · Preview cards"): under the
//! dictionary in the object view, a card with a header (`Preview`, the facts in one line, Fit ·
//! 1:1), the canvas and a footer of fact chips; in the inspector's Preview section, the same canvas
//! 160 points tall with one chip, and the facts as rows under it.
//!
//! The picture is rendered off the UI thread (see `AppState::object_preview`); a frame never
//! waits for it. A font without an embedded program is never sent to the renderer: there is
//! nothing of the file's to draw. Its card says so, and shows a dimmed specimen in the app's own
//! font, labelled as not the file's.

mod canvas;
mod facts;

use egui::{
    Align, Color32, Frame, Label, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2, pos2, vec2,
};
use pdfxray_core::{CharCode, FontProgram, ObjRef, Visual, VisualKind};

pub use self::facts::{ChipFact, Fact, Summary, facts, summary};
use crate::components::{Chip, Icon, Segment, Tone, segmented_mini, segmented_mini_width};
use crate::previews::{Picture, PreviewFrame};
use crate::state::{AppState, PreviewScale, describe};
use crate::theme::{fonts, tokens};

/// The title in the header of the centre card.
pub const PREVIEW_TITLE: &str = "Preview";

/// What the canvas says while the picture is being rendered.
pub const RENDERING: &str = "Rendering preview…";

/// The title in the frame of a font without an embedded program.
pub const NOT_EMBEDDED: &str = "Not embedded";

/// Why that matters, under the title on the centre card.
pub const NOT_EMBEDDED_WHY: &str = "Readers substitute their own font, so text can reflow or lose glyphs.";

/// The label of the specimen drawn with the app's font for a font that is not embedded.
pub const FALLBACK_LABEL: &str = "fallback specimen · not the file's font";

/// The caption over the glyph cells of a font.
pub const GLYPHS_LABEL: &str = "GLYPHS USED IN THIS FILE";

/// The title of the canvas of an object that cannot be drawn.
pub const NO_PREVIEW: &str = "No preview";

/// Height of the centre card's header.
const HEADER_H: f32 = 34.0;

/// Height of a card's footer of chips.
const FACTS_H: f32 = 34.0;

/// Height of the inspector card's canvas.
pub const INSPECTOR_CANVAS_H: f32 = 160.0;

/// The shortest the centre card's canvas gets.
const MIN_CANVAS_H: f32 = 120.0;

/// Space between a card's edge and its header or chips.
const EDGE: f32 = 12.0;

/// Space between the parts of the header.
const GAP: f32 = 8.0;

/// The accessible name of the main picture of `r` (the bitmap, the specimen, the bar, the
/// swatches): `Preview of 5 0 R`.
pub fn image_label(r: ObjRef) -> String {
    format!("Preview of {r}")
}

/// The accessible name of the cell of glyph `code`: `Glyph 48`.
pub fn glyph_label(code: &CharCode) -> String {
    format!("Glyph {}", code.hex)
}

/// The accessible name of swatch `index` painted in `color`: `Swatch 0 · #FF0000`.
pub fn swatch_label(index: usize, color: Color32) -> String {
    format!(
        "Swatch {index} · #{:02X}{:02X}{:02X}",
        color.r(),
        color.g(),
        color.b()
    )
}

/// A scale factor as the captions write it: `×90`, `×0.5`.
pub fn factor_text(factor: f32) -> String {
    let rounded = (factor * 100.0).round() / 100.0;
    format!("×{rounded}")
}

/// The caption under an image: `2 × 2 px · shown ×90 · nearest`.
pub fn caption(width: u32, height: u32, factor: f32) -> String {
    format!("{width} × {height} px · shown {} · nearest", factor_text(factor))
}

/// Which card is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variant {
    /// Under the dictionary, at the scale the user picked.
    Centre(PreviewScale),
    /// In the inspector: 160 points tall, always fitted.
    Inspector,
}

/// What the canvas holds this frame.
enum Shown {
    /// The rendered picture.
    Ready(Picture),
    /// The picture is being rendered.
    Rendering,
    /// A font without an embedded program.
    NotEmbedded,
    /// Nothing can be drawn, and why (a sentence).
    Unavailable(String),
}

/// What drawing the canvas found out.
#[derive(Debug, Default)]
struct Painted {
    /// Screen pixels per image pixel, for an image.
    factor: Option<f32>,
}

/// What the card of `visual` shows this frame; asks for the picture the first time.
fn shown(ui: &Ui, state: &mut AppState, visual: &Visual) -> Shown {
    if let VisualKind::Font(font) = &visual.kind
        && font.program == FontProgram::Missing
    {
        return Shown::NotEmbedded;
    }
    match state.object_preview(ui.ctx(), visual) {
        Some(PreviewFrame::Ready(bitmap)) => Shown::Ready(bitmap.picture),
        Some(PreviewFrame::Rendering) | None => Shown::Rendering,
        Some(PreviewFrame::Failed(reason)) => Shown::Unavailable(sentence(&describe(&reason))),
    }
}

/// The centre card of `visual`, at most `max` points and never wider than the room left: the
/// header, a canvas filling the height left, and the chips.
pub fn centre(ui: &mut Ui, state: &mut AppState, visual: &Visual, max: Vec2) {
    let shown = shown(ui, state, visual);
    let words = summary(visual);
    let mut scale = state.objects.preview_scale;
    let width = max.x.min(ui.available_width());
    let canvas_h = (max.y - HEADER_H - FACTS_H - 4.0).max(MIN_CANVAS_H);
    card(ui, width, |ui| {
        header(ui, &words.meta, &mut scale);
        hairline(ui);
        canvas::show(
            ui,
            &shown,
            visual,
            vec2(ui.available_width(), canvas_h),
            Variant::Centre(scale),
        );
        hairline(ui);
        footer(ui, &words.chips);
    });
    state.objects.preview_scale = scale;
}

/// The inspector card of `visual`: the canvas, 160 points tall, and one chip. The rows of
/// [`facts`] go under it.
pub fn inspector(ui: &mut Ui, state: &mut AppState, visual: &Visual) {
    let shown = shown(ui, state, visual);
    let words = summary(visual);
    card(ui, ui.available_width(), |ui| {
        let painted = canvas::show(
            ui,
            &shown,
            visual,
            vec2(ui.available_width(), INSPECTOR_CANVAS_H),
            Variant::Inspector,
        );
        hairline(ui);
        let chip = painted.factor.map_or(words.chip, |factor| ChipFact {
            text: format!("{} · nearest", factor_text(factor)),
            warn: false,
        });
        footer(ui, &[chip]);
    });
}

/// The card: its surface, hairline and corners; the parts go edge to edge inside.
fn card(ui: &mut Ui, width: f32, add_contents: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(tokens::PREVIEW_CARD)
        .stroke(Stroke::new(1.0, tokens::PREVIEW_CARD_HAIRLINE))
        .corner_radius(tokens::RADIUS_XL)
        .show(ui, |ui| {
            ui.set_width((width - 2.0).max(16.0));
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            add_contents(ui);
        });
}

/// A hairline across the card between two of its parts.
fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, tokens::HAIRLINE);
}

/// `Preview` on the left; the facts in one line and the Fit · 1:1 control on the right.
fn header(ui: &mut Ui, meta: &str, scale: &mut PreviewScale) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER_H), Sense::hover());
    let inner = rect.shrink2(vec2(EDGE, 0.0));
    let row = |rect: Rect, layout: Layout| UiBuilder::new().max_rect(rect).layout(layout);
    let title = ui
        .scope_builder(row(inner, Layout::left_to_right(Align::Center)), |ui| {
            ui.label(
                RichText::new(PREVIEW_TITLE)
                    .font(fonts::semibold(tokens::text::BODY))
                    .color(tokens::TEXT),
            )
            .rect
        })
        .inner;
    let segments = PreviewScale::ALL.map(|s| Segment::new(s, s.title()));
    let control = Rect::from_min_max(
        pos2(inner.max.x - segmented_mini_width(ui, &segments), inner.min.y),
        inner.max,
    );
    ui.scope_builder(row(control, Layout::left_to_right(Align::Center)), |ui| {
        segmented_mini(ui, scale, &segments);
    });
    let between = Rect::from_min_max(
        pos2(title.max.x + GAP, inner.min.y),
        pos2(control.min.x - GAP, inner.max.y),
    );
    if between.width() > 0.0 {
        ui.scope_builder(row(between, Layout::right_to_left(Align::Center)), |ui| {
            ui.add(
                Label::new(
                    RichText::new(meta)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                )
                .truncate(),
            );
        });
    }
}

/// The chips, on one line; what does not fit is cut at the card's edge.
fn footer(ui: &mut Ui, chips: &[ChipFact]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), FACTS_H), Sense::hover());
    let clip = rect.intersect(ui.clip_rect());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(EDGE, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.set_clip_rect(clip);
            ui.spacing_mut().item_spacing.x = 6.0;
            for fact in chips {
                let neutral = Chip::new(&fact.text).card();
                if fact.warn {
                    neutral.tone(Tone::Warn).icon(Icon::Warning).show(ui);
                } else {
                    neutral.show(ui);
                }
            }
        },
    );
}

/// `text` as a label with its top-left corner at `at`, at most `width` wide; wrapped or cut.
fn text_at(ui: &mut Ui, at: egui::Pos2, width: f32, text: RichText, wrap: bool) -> Rect {
    let room = Rect::from_min_size(at, vec2(width.max(1.0), f32::MAX / 4.0));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(room)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            let label = Label::new(text);
            let label = if wrap { label.wrap() } else { label.truncate() };
            ui.add(label).rect
        },
    )
    .inner
}

/// `reason` as a sentence: a capital first letter and a full stop.
fn sentence(reason: &str) -> String {
    let mut chars = reason.chars();
    let mut out: String = chars
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    out.push_str(chars.as_str());
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn renderer_reasons_read_as_sentences() {
        assert_eq!(
            sentence("the image data cannot be decoded"),
            "The image data cannot be decoded."
        );
        // Smoke: a sentence stays as it is.
        assert_eq!(sentence("Done."), "Done.");
    }

    #[test]
    fn captions_state_the_factor() {
        assert_eq!(caption(2, 2, 90.0), "2 × 2 px · shown ×90 · nearest");
        assert_eq!(factor_text(0.4567), "×0.46");
        assert_eq!(
            swatch_label(3, Color32::from_rgb(0xFF, 0x80, 0)),
            "Swatch 3 · #FF8000"
        );
        // Smoke: a whole factor has no decimals.
        assert_eq!(factor_text(1.0), "×1");
    }
}
