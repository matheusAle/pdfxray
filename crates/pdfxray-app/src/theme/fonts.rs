//! Inter for the UI and `JetBrains Mono` for values, ids and operators.
//!
//! Both are SIL OFL 1.1 and live, with their licences and checksums, in `assets/fonts`. egui's
//! built-in fonts stay behind them as fallbacks for symbols the two do not carry.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId};

use super::tokens;

const INTER_REGULAR: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Bold.ttf");
const MONO_REGULAR: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Regular.ttf");
const MONO_MEDIUM: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Medium.ttf");
const MONO_SEMIBOLD: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-SemiBold.ttf");
const MONO_BOLD: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Bold.ttf");

const MEDIUM: &str = "Inter-Medium";
const SEMIBOLD: &str = "Inter-SemiBold";
const BOLD: &str = "Inter-Bold";
const MONO_MEDIUM_NAME: &str = "JetBrainsMono-Medium";
const MONO_SEMIBOLD_NAME: &str = "JetBrainsMono-SemiBold";
const MONO_BOLD_NAME: &str = "JetBrainsMono-Bold";

/// Installs the fonts into `ctx`.
pub fn install(ctx: &Context) {
    ctx.set_fonts(definitions());
}

fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let faces: [(&str, &'static [u8]); 8] = [
        ("Inter-Regular", INTER_REGULAR),
        (MEDIUM, INTER_MEDIUM),
        (SEMIBOLD, INTER_SEMIBOLD),
        ("JetBrainsMono-Regular", MONO_REGULAR),
        (MONO_MEDIUM_NAME, MONO_MEDIUM),
        (BOLD, INTER_BOLD),
        (MONO_SEMIBOLD_NAME, MONO_SEMIBOLD),
        (MONO_BOLD_NAME, MONO_BOLD),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    let proportional_fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let monospace_fallbacks = fonts
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let chain = |first: &[&str], fallbacks: &[String]| -> Vec<String> {
        first
            .iter()
            .map(|name| (*name).to_owned())
            .chain(fallbacks.iter().cloned())
            .collect()
    };

    fonts.families.insert(
        FontFamily::Proportional,
        chain(&["Inter-Regular"], &proportional_fallbacks),
    );
    fonts.families.insert(
        FontFamily::Name(MEDIUM.into()),
        chain(&[MEDIUM, "Inter-Regular"], &proportional_fallbacks),
    );
    fonts.families.insert(
        FontFamily::Name(SEMIBOLD.into()),
        chain(&[SEMIBOLD, "Inter-Regular"], &proportional_fallbacks),
    );
    fonts.families.insert(
        FontFamily::Name(BOLD.into()),
        chain(&[BOLD, "Inter-Regular"], &proportional_fallbacks),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        chain(&["JetBrainsMono-Regular"], &monospace_fallbacks),
    );
    fonts.families.insert(
        FontFamily::Name(MONO_MEDIUM_NAME.into()),
        chain(&[MONO_MEDIUM_NAME, "JetBrainsMono-Regular"], &monospace_fallbacks),
    );
    for name in [MONO_SEMIBOLD_NAME, MONO_BOLD_NAME] {
        fonts.families.insert(
            FontFamily::Name(name.into()),
            chain(&[name, "JetBrainsMono-Regular"], &monospace_fallbacks),
        );
    }
    fonts
}

/// Inter Regular at `size`.
pub fn ui(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Inter Medium at `size`.
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// Inter `SemiBold` at `size`.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Inter Bold at `size`: tag chips, order badges, summary counts and screen titles.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// `JetBrains Mono` Regular at `size`.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// `JetBrains Mono` Medium at `size`.
pub fn mono_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_MEDIUM_NAME.into()))
}

/// `JetBrains Mono` `SemiBold` at `size`: selection labels, object headers (`23 0 obj`), operator
/// names.
pub fn mono_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_SEMIBOLD_NAME.into()))
}

/// `JetBrains Mono` Bold at `size`: diagnostic codes and validation counts.
pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_BOLD_NAME.into()))
}

// ---- Tracked text ----
//
// Tabular numerals: egui cannot switch on OpenType `tnum`. Numbers that must line up (counts,
// sizes, coordinates, object ids) are therefore set in `JetBrains Mono`, which is tabular by
// design; Inter is used for numbers only where they do not need to align.

/// `text` in `font` and `color` with `spacing` extra points between letters, as a layout job that
/// wraps at the width of the widget it is added to.
pub fn tracked(text: &str, font: FontId, color: Color32, spacing: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: spacing,
            ..TextFormat::default()
        },
    );
    job
}

/// A title at `size`: semibold, tracking [`tokens::tracking::TITLE`].
pub fn title(text: &str, size: f32, color: Color32) -> LayoutJob {
    tracked(text, semibold(size), color, tokens::tracking::TITLE)
}

/// The inspector's title: 17 px semibold, tightened, wrapping onto more lines.
pub fn inspector_title(text: &str) -> LayoutJob {
    title(text, tokens::text::INSPECTOR_TITLE, tokens::TEXT)
}

/// A section header: upper-cased, 10 px semibold, tracking [`tokens::tracking::SECTION`], text-3.
pub fn section_header(text: &str) -> LayoutJob {
    tracked(
        &text.to_uppercase(),
        semibold(tokens::text::SECTION),
        tokens::TEXT_3,
        tokens::tracking::SECTION,
    )
}
