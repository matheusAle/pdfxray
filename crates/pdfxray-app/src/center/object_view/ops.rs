//! The drawing instructions of a content stream, with the Explain switch, and the decoded text
//! of other text streams.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Color32, FontId, Label, Layout, Rect, RichText, Role, ScrollArea, Sense, Ui, UiBuilder,
    WidgetInfo, vec2,
};

use super::put;
use crate::components::switch;
use crate::object_facts::{OpLine, TextBody, thousands};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Name of the switch that shows the plain-language notes.
pub const EXPLAIN_LABEL: &str = "Explain";

/// Height of an instruction row.
const OP_ROW_H: f32 = 22.0;
/// Indent per level of nesting.
const OP_INDENT: f32 = 14.0;
/// Where the instruction text starts, from the left edge.
const OP_X: f32 = 52.0;

/// The header and the list of instructions.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState, lines: &[OpLine]) {
    section_header(ui, "Drawing instructions (decoded)", |ui| {
        let mut explain = state.objects.explain;
        let operators = format!(
            "{} operator{}",
            thousands(lines.len()),
            if lines.len() == 1 { "" } else { "s" }
        );
        // Right to left: the count is rightmost, the switch left of it.
        ui.label(
            RichText::new(operators)
                .font(fonts::mono(tokens::mono::SMALL))
                .color(tokens::TEXT_3),
        );
        if switch(ui, &mut explain, EXPLAIN_LABEL).changed() {
            state.objects.explain = explain;
        }
    });
    let explain = state.objects.explain;
    let selected = state.objects.selected_op;
    let mut clicked = None;
    ScrollArea::vertical()
        .id_salt("object-ops")
        .auto_shrink(false)
        .show_rows(ui, OP_ROW_H, lines.len(), |ui, range| {
            for index in range {
                if op_row(ui, &lines[index], explain, selected == Some(lines[index].ix)) {
                    clicked = Some(lines[index].ix);
                }
            }
        });
    if clicked.is_some() {
        state.objects.selected_op = clicked;
    }
}

/// The header and the lines of a decoded text stream.
pub(super) fn text(ui: &mut Ui, body: &TextBody) {
    section_header(ui, "Stream data (decoded text)", |ui| {
        ui.label(
            RichText::new(format!("{} lines", thousands(body.total)))
                .font(fonts::mono(tokens::mono::SMALL))
                .color(tokens::TEXT_3),
        );
    });
    ScrollArea::vertical()
        .id_salt("object-text")
        .auto_shrink(false)
        .show_rows(ui, OP_ROW_H, body.lines.len(), |ui, range| {
            for index in range {
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), OP_ROW_H), Sense::hover());
                let number = Rect::from_min_size(rect.min + vec2(10.0, 0.0), vec2(32.0, rect.height()));
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(number)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        ui.label(
                            RichText::new((index + 1).to_string())
                                .font(fonts::mono(tokens::mono::BODY))
                                .color(tokens::TEXT_3.gamma_multiply(0.7)),
                        );
                    },
                );
                let text = Rect::from_min_max(rect.min + vec2(OP_X, 0.0), rect.max - vec2(10.0, 0.0));
                put(
                    ui,
                    text,
                    Label::new(
                        RichText::new(&body.lines[index])
                            .font(fonts::mono(tokens::mono::LARGE))
                            .color(tokens::TEXT),
                    )
                    .truncate()
                    .selectable(false),
                );
            }
        });
    if body.total > body.lines.len() {
        ui.label(
            RichText::new(format!(
                "… {} more lines",
                thousands(body.total - body.lines.len())
            ))
            .color(tokens::TEXT_3),
        );
    }
}

/// The header of a body section: `title` on the left, `right` laid out from the right edge.
fn section_header(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(Rect::from_min_max(
                rect.min + vec2(14.0, 0.0),
                rect.max - vec2(14.0, 0.0),
            ))
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.label(
                RichText::new(title)
                    .font(fonts::medium(tokens::text::BODY))
                    .color(tokens::TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                right(ui);
            });
        },
    );
}

/// One instruction: its number, the operator, the operands and, with Explain on, the note.
fn op_row(ui: &mut Ui, line: &OpLine, explain: bool, selected: bool) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), OP_ROW_H), Sense::hover());
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("object-op", line.ix)),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, format!("#{}", line.ix)));
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, tokens::ACCENT.gamma_multiply(0.08));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            tokens::ACCENT,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, tokens::SURFACE_2);
    }
    let number = Rect::from_min_size(rect.min + vec2(10.0, 0.0), vec2(32.0, rect.height()));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(number)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.label(
                RichText::new(format!("#{}", line.ix))
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
        },
    );

    // The note sits at the right edge; what is left is for the instruction.
    let mut right = rect.max.x - 14.0;
    let note = (explain && !line.note.is_empty()).then(|| {
        let width = ui
            .painter()
            .layout_no_wrap(line.note.clone(), fonts::ui(tokens::text::SMALL), tokens::TEXT_3)
            .size()
            .x
            + 4.0;
        let note_rect = Rect::from_min_max(
            egui::pos2(right - width, rect.min.y),
            egui::pos2(right, rect.max.y),
        );
        right -= width + 16.0;
        note_rect
    });
    let left = rect.min.x + OP_X + f32::from(line.indent) * OP_INDENT;
    let operator_w = ui
        .painter()
        .layout_no_wrap(
            line.operator.clone(),
            fonts::mono_medium(tokens::mono::LARGE),
            tokens::TEXT,
        )
        .size()
        .x;
    // Operands come first in the file, so they come first on the row too.
    let mut x = left;
    if !line.operands.is_empty() {
        let operands = operands_job(&line.operands);
        let wanted = ui.painter().layout_job(operands.clone()).size().x;
        let room = (right - x - operator_w - 12.0).max(24.0);
        let width = wanted.min(room);
        let cell = Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(width, rect.height()));
        put(ui, cell, Label::new(operands).truncate().selectable(false));
        x += width + 8.0;
    }
    let operator_rect = Rect::from_min_max(
        egui::pos2(x, rect.min.y),
        egui::pos2(x + operator_w + 4.0, rect.max.y),
    );
    put(
        ui,
        operator_rect,
        Label::new(
            RichText::new(&line.operator)
                .font(fonts::mono_medium(tokens::mono::LARGE))
                .color(tokens::KIND_CONTENT),
        )
        .selectable(false),
    );
    if let Some(note_rect) = note {
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(note_rect)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.add(
                    Label::new(
                        RichText::new(&line.note)
                            .font(fonts::ui(tokens::text::SMALL))
                            .color(tokens::TEXT_3),
                    )
                    .selectable(false),
                );
            },
        );
    }
    response.clicked()
}

/// The operands coloured by what they are: names, numbers, strings, the rest.
fn operands_job(text: &str) -> LayoutJob {
    let font = fonts::mono(tokens::mono::LARGE);
    let mut job = LayoutJob::default();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let (len, color) = token(rest, c);
        push(&mut job, &rest[..len], &font, color);
        rest = &rest[len..];
    }
    job
}

fn push(job: &mut LayoutJob, text: &str, font: &FontId, color: Color32) {
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            ..TextFormat::default()
        },
    );
}

/// The length of the token at the start of `rest` and its colour.
fn token(rest: &str, first: char) -> (usize, Color32) {
    let delimiter = |c: char| c.is_whitespace() || matches!(c, '/' | '[' | ']' | '<' | '>' | '(' | ')');
    match first {
        '/' => {
            let len = 1 + rest[1..].find(delimiter).unwrap_or(rest.len() - 1);
            (len, tokens::ACCENT)
        }
        '(' => (string_len(rest), tokens::KIND_CONTENT),
        c if c.is_ascii_digit() || matches!(c, '-' | '+' | '.') => {
            let len = rest.find(delimiter).unwrap_or(rest.len());
            (len.max(1), tokens::WARN)
        }
        c => (c.len_utf8(), tokens::TEXT_2),
    }
}

/// The length of the literal string at the start of `rest`, honouring nesting and escapes.
fn string_len(rest: &str) -> usize {
    let mut depth = 0_u32;
    let mut escaped = false;
    for (i, c) in rest.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    rest.len()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn runs(text: &str) -> Vec<(String, Color32)> {
        operands_job(text)
            .sections
            .iter()
            .map(|s| {
                (
                    text[s.byte_range.start.0..s.byte_range.end.0].to_owned(),
                    s.format.color,
                )
            })
            .collect()
    }

    #[test]
    fn operands_are_coloured_by_what_they_are() {
        let shown = runs("/F1 26");

        assert_eq!(shown[0], ("/F1".to_owned(), tokens::ACCENT));
        assert_eq!(shown[2], ("26".to_owned(), tokens::WARN));
        // Smoke: every byte of the text is in a run.
        assert_eq!(
            shown.iter().map(|(t, _)| t.as_str()).collect::<String>(),
            "/F1 26"
        );
    }

    #[test]
    fn strings_keep_their_nesting_and_escapes() {
        let shown = runs(r"[(A \) (b)) -20 (B)]");

        let strings: Vec<&str> = shown
            .iter()
            .filter(|(_, c)| *c == tokens::KIND_CONTENT)
            .map(|(t, _)| t.as_str())
            .collect();
        assert_eq!(strings, [r"(A \) (b))", "(B)"]);
        assert_eq!(
            shown.iter().map(|(t, _)| t.as_str()).collect::<String>(),
            r"[(A \) (b)) -20 (B)]"
        );
    }
}
