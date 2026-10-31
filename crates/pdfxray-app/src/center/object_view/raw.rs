//! The Raw bytes view: the exact bytes of the object in the file, as hex and text, with the range
//! a diagnostic pointed at marked.

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Frame, Label, Margin, Rect, RichText, ScrollArea, Sense, Ui, vec2};
use pdfxray_core::numeric::f32_from_usize;

use super::put;
use crate::object_facts::{ObjectFacts, thousands};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Bytes on one line.
pub const BYTES_PER_ROW: usize = 16;

/// Height of a line.
const ROW_H: f32 = 18.0;
/// Where the columns start, from the left edge.
const ADDRESS_X: f32 = 14.0;
const HEX_X: f32 = 100.0;
const ASCII_X: f32 = 470.0;

/// Shown for an object that has no bytes of its own.
pub const NO_BYTES: &str = "This object has no bytes of its own.";

/// The bytes the view lists: the object's, or the whole file when the highlight lies elsewhere
/// (an xref entry, the trailer).
pub fn shown_range(
    object: Option<&Range<usize>>,
    highlight: Option<&Range<usize>>,
    file_len: usize,
) -> Option<Range<usize>> {
    let object = object?;
    match highlight {
        Some(h) if h.start < object.start || h.end > object.end => Some(0..file_len),
        _ => Some(object.start..object.end.min(file_len)),
    }
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState, facts: &ObjectFacts) {
    let highlight = state.objects.highlight.clone();
    let scroll_to = std::mem::take(&mut state.objects.scroll_to_highlight);
    let Some(loaded) = &state.document else {
        return;
    };
    let file = loaded.doc.bytes();
    let Some(range) = shown_range(facts.range.as_ref(), highlight.as_ref(), file.len()) else {
        Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.label(RichText::new(NO_BYTES).color(tokens::TEXT_3));
        });
        return;
    };
    summary(
        ui,
        &range,
        highlight.as_ref(),
        range.len() == file.len() && facts.range.as_ref() != Some(&range),
    );
    let bytes = &file[range.clone()];
    let rows = bytes.len().div_ceil(BYTES_PER_ROW);
    let mut area = ScrollArea::vertical().id_salt("object-raw").auto_shrink(false);
    if scroll_to && let Some(h) = &highlight {
        let row = h.start.saturating_sub(range.start) / BYTES_PER_ROW;
        let row_h = ROW_H + ui.spacing().item_spacing.y;
        let top = f32_from_usize(row) * row_h - ui.available_height() / 3.0;
        area = area.vertical_scroll_offset(top.max(0.0));
    }
    area.show_rows(ui, ROW_H, rows, |ui, rows| {
        for row in rows {
            let from = row * BYTES_PER_ROW;
            let to = (from + BYTES_PER_ROW).min(bytes.len());
            // The highlight in the coordinates of the row's bytes.
            let marked = highlight.as_ref().map(|h| {
                h.start.saturating_sub(range.start + from)..h.end.saturating_sub(range.start + from)
            });
            line(ui, range.start + from, &bytes[from..to], marked.as_ref());
        }
    });
}

/// What range is on show.
fn summary(ui: &mut Ui, range: &Range<usize>, highlight: Option<&Range<usize>>, whole_file: bool) {
    Frame::new()
        .inner_margin(Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let scope = if whole_file { "Whole file" } else { "Bytes" };
                ui.label(
                    RichText::new(format!(
                        "{scope} {}..{} · {} bytes",
                        range.start,
                        range.end,
                        thousands(range.len())
                    ))
                    .font(fonts::mono(tokens::mono::BODY))
                    .color(tokens::TEXT_3),
                );
                if let Some(h) = highlight {
                    ui.label(
                        RichText::new(format!("highlighted {}..{}", h.start, h.end))
                            .font(fonts::mono(tokens::mono::BODY))
                            .color(tokens::ACCENT),
                    );
                }
            });
        });
}

/// One line: the offset, the hex bytes and the text, with `marked` (indices in the row) tinted.
fn line(ui: &mut Ui, offset: usize, bytes: &[u8], marked: Option<&Range<usize>>) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let cell = |x: f32| Rect::from_min_size(rect.min + vec2(x, 0.0), vec2(ASCII_X - x - 10.0, rect.height()));
    put(
        ui,
        Rect::from_min_size(
            rect.min + vec2(ADDRESS_X, 0.0),
            vec2(HEX_X - ADDRESS_X, rect.height()),
        ),
        Label::new(
            RichText::new(format!("{offset:08x}"))
                .font(fonts::mono(tokens::mono::BODY))
                .color(tokens::TEXT_3),
        )
        .selectable(false),
    );
    put(
        ui,
        cell(HEX_X),
        Label::new(hex_job(bytes, marked)).selectable(false),
    );
    put(
        ui,
        Rect::from_min_max(rect.min + vec2(ASCII_X, 0.0), rect.max),
        Label::new(ascii_job(bytes, marked)).selectable(false),
    );
}

fn format(font: &FontId, color: Color32, marked: bool) -> TextFormat {
    TextFormat {
        font_id: font.clone(),
        color: if marked { tokens::TEXT } else { color },
        background: if marked {
            tokens::ACCENT.gamma_multiply(0.30)
        } else {
            Color32::TRANSPARENT
        },
        ..TextFormat::default()
    }
}

/// `33 20 30 20 6f 62 6a 0a  ...`: a wider gap in the middle of the line.
fn hex_job(bytes: &[u8], marked: Option<&Range<usize>>) -> LayoutJob {
    let font = fonts::mono(tokens::mono::BODY);
    let mut job = LayoutJob::default();
    let is_marked = |i: usize| marked.is_some_and(|m| m.contains(&i));
    for (i, byte) in bytes.iter().enumerate() {
        job.append(
            &format!("{byte:02x}"),
            0.0,
            format(&font, tokens::TEXT_2, is_marked(i)),
        );
        if i + 1 < bytes.len() {
            let gap = if i + 1 == BYTES_PER_ROW / 2 { "  " } else { " " };
            job.append(
                gap,
                0.0,
                format(&font, tokens::TEXT_2, is_marked(i) && is_marked(i + 1)),
            );
        }
    }
    job
}

/// The text column: printable characters as they are, the rest as dots.
fn ascii_job(bytes: &[u8], marked: Option<&Range<usize>>) -> LayoutJob {
    let font = fonts::mono(tokens::mono::BODY);
    let mut job = LayoutJob::default();
    for (i, byte) in bytes.iter().enumerate() {
        let c = if byte.is_ascii_graphic() || *byte == b' ' {
            char::from(*byte)
        } else {
            '.'
        };
        job.append(
            &c.to_string(),
            0.0,
            format(&font, tokens::TEXT_3, marked.is_some_and(|m| m.contains(&i))),
        );
    }
    job
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_object_range_is_shown_unless_the_highlight_lies_outside_it() {
        let object = 121..246;

        assert_eq!(shown_range(Some(&object), None, 667), Some(121..246));
        assert_eq!(shown_range(Some(&object), Some(&(130..140)), 667), Some(121..246));
        assert_eq!(shown_range(Some(&object), Some(&(477..495)), 667), Some(0..667));
        assert_eq!(shown_range(None, Some(&(1..2)), 667), None);
        // A range that runs past the file is cut to it.
        assert_eq!(shown_range(Some(&(600..900)), None, 667), Some(600..667));
    }

    #[test]
    fn marked_bytes_get_a_background_and_the_text_stays_whole() {
        let job = hex_job(b"3 0 obj\n", Some(&(0..3)));

        assert_eq!(job.text, "33 20 30 20 6f 62 6a 0a");
        let tinted: Vec<&str> = job
            .sections
            .iter()
            .filter(|s| s.format.background != Color32::TRANSPARENT)
            .map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0])
            .collect();
        assert_eq!(tinted, ["33 20 30"]);
        assert_eq!(ascii_job(b"3 0 obj\n", None).text, "3 0 obj.");
    }
}
