//! The GLYPH section: the character code, what Unicode it stands for and where that came from,
//! `ActualText`, the width and advance, and a tile showing the glyph with its font metrics.

use egui::{Galley, Pos2, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use pdfxray_core::numeric::f32_from_f64;
use pdfxray_core::{GlyphInfo, UnicodeSource, num};

use super::structure::Cx;
use crate::components::PropRow;
use crate::theme::{fonts, tokens};
use crate::units::Lengths;

/// Title of the section.
pub const TITLE: &str = "Glyph";

/// `<41>  1 byte`.
pub fn char_code_text(info: &GlyphInfo) -> String {
    if !info.char_code.known {
        return "unknown".to_owned();
    }
    let bytes = info.char_code.bytes;
    format!(
        "<{}>  {bytes} byte{}",
        info.char_code.hex,
        if bytes == 1 { "" } else { "s" }
    )
}

/// `U+0041 'A'  via ToUnicode`.
pub fn unicode_text(info: &GlyphInfo) -> String {
    let Some(text) = info.unicode.as_deref().filter(|t| !t.is_empty()) else {
        return "none".to_owned();
    };
    let points: Vec<String> = info.code_points.iter().map(|c| format!("U+{c:04X}")).collect();
    let source = match info.unicode_source {
        UnicodeSource::ToUnicode => "via ToUnicode",
        UnicodeSource::Encoding => "via encoding",
        UnicodeSource::None => "",
    };
    format!("{} '{text}'  {source}", points.join(" "))
        .trim_end()
        .to_owned()
}

/// `722 /1000 · 18.77 pt`.
pub fn width_text(info: &GlyphInfo, lengths: &Lengths) -> String {
    format!(
        "{} /1000 · {}",
        num(info.width_units),
        lengths.format(info.width_pt)
    )
}

/// `[18.77 0] pt`.
pub fn advance_text(info: &GlyphInfo, lengths: &Lengths) -> String {
    format!(
        "[{} {}] {}",
        lengths.number(info.advance[0]),
        lengths.number(info.advance[1]),
        lengths.unit.label()
    )
}

pub(super) fn show(ui: &mut Ui, cx: &Cx<'_>) {
    let Some(info) = &cx.glyph else {
        return;
    };
    cx.section(ui, TITLE, |ui| {
        PropRow::new("Char code", &char_code_text(info)).show(ui);
        PropRow::new("Unicode", &unicode_text(info)).show(ui);
        match &info.actual_text {
            Some(actual) => {
                let suffix = if actual.inherited { "  (inherited)" } else { "" };
                PropRow::new("ActualText", &format!("{}{suffix}", actual.text)).show(ui);
            }
            None => {
                PropRow::new("ActualText", "—  (inherits none)").show(ui);
            }
        }
        PropRow::new("Width", &width_text(info, &cx.lengths)).show(ui);
        PropRow::new("Advance", &advance_text(info, &cx.lengths)).show(ui);
        tile(ui, info);
    });
}

/// Where to put `galley` so the baseline of its first line lies on `baseline` and the text is
/// centred on `centre_x`: the line box reaches below the baseline by the font's descent, so
/// aligning its bottom would lift the text by that much.
fn on_baseline(galley: &Galley, centre_x: f32, baseline: f32) -> Pos2 {
    let ascent = galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .map_or_else(|| galley.size().y, |glyph| glyph.pos.y);
    pos2(centre_x - galley.size().x / 2.0, baseline - ascent)
}

/// The glyph drawn large between its ascent and descent lines, with the metrics beside it.
fn tile(ui: &mut Ui, info: &GlyphInfo) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        ui.painter().rect(
            rect,
            tokens::RADIUS_MD,
            tokens::BG,
            Stroke::new(1.0, tokens::BORDER_STRONG),
            StrokeKind::Inside,
        );
        let ascent = info.font.as_ref().and_then(|f| f.ascent_em).unwrap_or(0.8);
        let descent = info.font.as_ref().and_then(|f| f.descent_em).unwrap_or(-0.2);
        let span = (ascent - descent).max(0.1);
        let inner = rect.shrink(8.0);
        let baseline = inner.min.y + inner.height() * f32_from_f64(ascent / span);
        for (y, color) in [
            (inner.min.y, tokens::TEXT_3.gamma_multiply(0.5)),
            (baseline, tokens::PINK.gamma_multiply(0.7)),
            (inner.max.y, tokens::TEXT_3.gamma_multiply(0.5)),
        ] {
            ui.painter().hline(rect.x_range(), y, Stroke::new(1.0, color));
        }
        let shown = info.unicode.as_deref().filter(|t| !t.is_empty()).unwrap_or("?");
        let galley = ui
            .painter()
            .layout_no_wrap(shown.to_owned(), fonts::semibold(34.0), tokens::TEXT);
        let origin = on_baseline(&galley, rect.center().x, baseline);
        ui.painter().galley(origin, galley, tokens::TEXT);
        ui.add_space(6.0);
        // The metrics are grouped beside the tile and centred on it.
        let bbox = info.font.as_ref().and_then(|f| f.font_bbox);
        let line_h = ui
            .painter()
            .layout_no_wrap(
                "ascent".to_owned(),
                fonts::mono(tokens::mono::SMALL),
                tokens::TEXT_3,
            )
            .size()
            .y;
        let lines = if bbox.is_some() { 3.0 } else { 2.0 };
        let gap = 2.0;
        let metrics_h = lines * line_h + (lines - 1.0) * gap;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = gap;
            // Each metric is as tall as its text, so they sit together.
            ui.spacing_mut().interact_size.y = 0.0;
            ui.add_space(((rect.height() - metrics_h) / 2.0).max(0.0));
            let line = |ui: &mut Ui, key: &str, value: String| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(key)
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(tokens::TEXT_3),
                    );
                    ui.label(
                        RichText::new(value)
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(tokens::TEXT_2),
                    );
                });
            };
            line(ui, "ascent", format!("{ascent:+.2} em"));
            line(ui, "descent", format!("{descent:+.2} em"));
            if let Some(b) = bbox {
                line(
                    ui,
                    "bbox",
                    format!("[{} {} {} {}]", num(b[0]), num(b[1]), num(b[2]), num(b[3])),
                );
            }
        });
    });
    ui.add_space(4.0);
}
