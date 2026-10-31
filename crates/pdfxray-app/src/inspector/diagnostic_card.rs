//! A diagnostic of the selected object, in plain language, with the buttons that help.

use std::ops::Range;

use egui::{Frame, Margin, RectAlign, RichText, Stroke, Ui};
use pdfxray_core::{DiagnosticInfo, Severity, explain};

use crate::components::{Icon, ToolButton, popover};
use crate::theme::{fonts, tokens};

/// Name of the button that jumps to the bytes.
pub const SHOW_BYTES_LABEL: &str = "Show bytes";

/// What the card's help button is called, and what it says.
fn help(code: &str) -> Option<(&'static str, &'static str)> {
    match code {
        "stream-length-wrong" => Some((
            "What is /Length?",
            "Every stream starts with a dictionary, and its /Length says how many bytes of data follow. \
             Readers use it to know where the data stops. When it is wrong they have to guess the end. \
             pdfxray measured the data up to the word endstream and shows both numbers.",
        )),
        "xref-offset-wrong" => Some((
            "What is the xref?",
            "The xref table is the file's index: it says at which byte each object starts. When an \
             entry points at the wrong byte, a reader has to search the file for the object. pdfxray \
             found it and shows where it really is.",
        )),
        _ => None,
    }
}

/// The colour of a card for a severity.
fn tint(severity: Severity) -> egui::Color32 {
    match severity {
        Severity::Info => tokens::ACCENT,
        Severity::Warning => tokens::WARN,
        Severity::Error => tokens::ERR,
    }
}

/// Draws the card; returns the byte range when "Show bytes" was pressed.
pub fn show(ui: &mut Ui, diagnostic: &DiagnosticInfo) -> Option<Range<usize>> {
    let color = tint(diagnostic.severity);
    let mut jump = None;
    Frame::new()
        .fill(color.gamma_multiply(0.08))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.35)))
        .corner_radius(tokens::RADIUS_LG)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                Icon::Warning.show(ui, 13.0, color);
                ui.label(
                    RichText::new(diagnostic.code)
                        .font(fonts::mono_medium(tokens::mono::BODY))
                        .color(color),
                );
            });
            ui.add_space(2.0);
            let text = diagnostic.explain().unwrap_or_else(|| {
                explain::diagnostic(diagnostic.code).map_or_else(|| diagnostic.message.clone(), str::to_owned)
            });
            ui.add(egui::Label::new(RichText::new(text).color(tokens::TEXT_2)).wrap());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let range = diagnostic.byte_range();
                let button = ui
                    .add_enabled_ui(range.is_some(), |ui| {
                        ToolButton::new(SHOW_BYTES_LABEL).icon(Icon::Bytes).show(ui)
                    })
                    .inner;
                if button.clicked() {
                    jump = range;
                }
                if let Some((label, text)) = help(diagnostic.code) {
                    let help_button = ToolButton::new(label).icon(Icon::Help).show(ui);
                    popover::show(&help_button, RectAlign::BOTTOM_START, 280.0, |ui| {
                        ui.add(egui::Label::new(RichText::new(text).color(tokens::TEXT_2)).wrap());
                    });
                }
            });
        });
    jump
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_exists_for_the_codes_that_have_a_button() {
        assert_eq!(help("stream-length-wrong").map(|h| h.0), Some("What is /Length?"));
        assert_eq!(help("xref-offset-wrong").map(|h| h.0), Some("What is the xref?"));
        assert_eq!(help("missing-endobj"), None);
        // Smoke: every help text says more than its title.
        assert!(help("stream-length-wrong").is_some_and(|(title, text)| text.len() > title.len() * 4));
    }

    #[test]
    fn severities_have_their_own_colours() {
        assert_ne!(tint(Severity::Warning), tint(Severity::Error));
        assert_ne!(tint(Severity::Info), tint(Severity::Warning));
    }
}
