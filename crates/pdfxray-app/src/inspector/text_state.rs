//! The TEXT STATE section: the parameters of text operators (ISO 32000-2 §9.3) in force.

use egui::Ui;
use pdfxray_core::{Matrix, NodeId};

use super::defaults::{self, DefaultValue};
use super::geometry::matrix_text;
use super::structure::Cx;
use crate::components::PropRow;

/// Title of the section.
pub const TITLE: &str = "Text state";

/// `0 · fill`: a text rendering mode and what it means.
pub fn render_mode_text(mode: u8) -> String {
    let meaning = match mode {
        0 => "fill",
        1 => "stroke",
        2 => "fill, then stroke",
        3 => "invisible",
        4 => "fill and clip",
        5 => "stroke and clip",
        6 => "fill, stroke and clip",
        7 => "clip",
        _ => "invalid",
    };
    format!("{mode} · {meaning}")
}

/// The text state values that nobody changed: character and word spacing, scaling, leading,
/// rise and the fill rendering mode.
pub fn default_values(text: &pdfxray_core::capture::TextState) -> Vec<DefaultValue> {
    let zero = |v: f64| v.abs() < f64::EPSILON;
    let mut found = Vec::new();
    if zero(text.char_spacing) {
        found.push(DefaultValue::new("Tc", "0", "Tc 0"));
    }
    if zero(text.word_spacing) {
        found.push(DefaultValue::new("Tw", "0", "Tw 0"));
    }
    if (text.horizontal_scaling - 100.0).abs() < f64::EPSILON {
        found.push(DefaultValue::new("Tz", "100%", "Tz 100%"));
    }
    if zero(text.leading) {
        found.push(DefaultValue::new("TL", "0", "TL 0"));
    }
    if zero(text.rise) {
        found.push(DefaultValue::new("Ts", "0", "Ts 0"));
    }
    if text.render_mode == 0 {
        found.push(DefaultValue::new("Tr", render_mode_text(0), "Tr fill"));
    }
    found
}

pub(super) fn show(ui: &mut Ui, cx: &Cx<'_>) {
    let Some((_, _, op)) = cx.op() else {
        return;
    };
    let text = &op.gstate.text;
    let is_text = matches!(cx.id, NodeId::Glyph { .. }) || cx.font.is_some() || text.font.is_some();
    if !is_text {
        return;
    }
    let lengths = &cx.lengths;
    let folded = default_values(text);
    let is_folded = |key: &str| folded.iter().any(|item| item.key == key);
    cx.section(ui, TITLE, |ui| {
        PropRow::new("Tf", &lengths.format(text.font_size)).show(ui);
        if !is_folded("Tc") {
            PropRow::new("Tc", &lengths.number(text.char_spacing)).show(ui);
        }
        if !is_folded("Tw") {
            PropRow::new("Tw", &lengths.number(text.word_spacing)).show(ui);
        }
        if !is_folded("Tz") {
            PropRow::new("Tz", &format!("{}%", pdfxray_core::num(text.horizontal_scaling))).show(ui);
        }
        if !is_folded("TL") {
            PropRow::new("TL", &lengths.number(text.leading)).show(ui);
        }
        if !is_folded("Ts") {
            PropRow::new("Ts", &lengths.number(text.rise)).show(ui);
        }
        if !is_folded("Tr") {
            PropRow::new("Tr", &render_mode_text(text.render_mode)).show(ui);
        }
        PropRow::new("Tm", &matrix_text(&Matrix::from(text.tm))).show(ui);
        defaults::show(ui, TITLE, &folded);
    });
}

#[cfg(test)]
mod tests {
    use pdfxray_core::capture::GState;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_fresh_text_state_folds_everything_but_the_font_and_matrix() {
        let shorts: Vec<String> = default_values(&GState::default().text)
            .into_iter()
            .map(|item| item.short)
            .collect();
        assert_eq!(shorts, ["Tc 0", "Tw 0", "Tz 100%", "TL 0", "Ts 0", "Tr fill"]);
    }

    #[test]
    fn spacing_and_a_rendering_mode_are_shown_when_they_are_set() {
        let mut text = GState::default().text;
        text.char_spacing = 2.0;
        text.render_mode = 3;
        let keys: Vec<&str> = default_values(&text).iter().map(|item| item.key).collect();
        assert_eq!(keys, ["Tw", "Tz", "TL", "Ts"]);
    }
}
