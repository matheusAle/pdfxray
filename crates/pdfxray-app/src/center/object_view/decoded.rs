//! The Decoded view: the dictionary with line numbers, diagnostics on the line they are about,
//! and below it what the data means: the drawing instructions, the text, or the picture of the
//! object.

use std::ops::Range;

use egui::{
    Align, Color32, Frame, Label, Layout, Margin, Rect, RichText, Role, ScrollArea, Sense, Ui, UiBuilder,
    WidgetInfo, vec2,
};
use pdfxray_core::{DiagnosticInfo, ObjRef};

use super::{ops, preview};
use crate::components::{Icon, ToolButton};
use crate::object_facts::{DictLine, LineKind, ObjectFacts, Piece, human_bytes, key_of, thousands};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Height of a dictionary line: code reads denser than tree rows (design: ~24).
const DICT_ROW_H: f32 = 24.0;

/// Name of the button on a diagnostic that jumps to the bytes.
pub const SHOW_BYTES_LABEL: &str = "Show bytes";

/// Width of the line number column.
const NUMBER_W: f32 = 26.0;
/// Where the text of a line starts, from the left edge.
const TEXT_X: f32 = 44.0;
/// Indent of the entries inside brackets.
const INDENT: f32 = 16.0;

/// What the user did in the view this frame.
#[derive(Debug, Default)]
struct Actions {
    navigate: Option<ObjRef>,
    show_bytes: Option<Range<usize>>,
}

/// One line of the dictionary block.
enum Row<'a> {
    /// A diagnostic that is not about a key, above the dictionary.
    Loose(&'a DiagnosticInfo),
    /// A line of the dictionary, with its number (from 1) and the diagnostic on it.
    Line(usize, &'a DictLine, Option<&'a DiagnosticInfo>),
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState, facts: &ObjectFacts) {
    let mut actions = Actions::default();
    // Instructions and text say more than a picture; the inspector still shows the picture.
    let picture = facts
        .visual
        .as_ref()
        .filter(|_| facts.ops.is_none() && facts.text.is_none());
    let body = picture.is_some() || facts.ops.is_some() || facts.text.is_some() || facts.stream.is_some();
    let room = ui.available_height();
    let block_h = if body { (room * 0.36).max(120.0) } else { room };
    dictionary(ui, facts, block_h, &mut actions);
    if body {
        ui.separator();
        if let Some(op_lines) = &facts.ops {
            ops::ui(ui, state, op_lines);
        } else if let Some(text) = &facts.text {
            ops::text(ui, text);
        } else if let Some(visual) = picture {
            preview::ui(ui, state, visual);
        } else if let Some(stream) = &facts.stream {
            binary(ui, facts, stream);
        }
    }
    if let Some(r) = actions.navigate {
        state.select(pdfxray_core::NodeId::Object(r));
    }
    if let Some(range) = actions.show_bytes {
        state.show_bytes(range);
    }
}

fn dictionary(ui: &mut Ui, facts: &ObjectFacts, max_height: f32, actions: &mut Actions) {
    let length = facts.length_diagnostic();
    let mut rows: Vec<Row<'_>> = facts.loose_diagnostics().map(Row::Loose).collect();
    rows.extend(facts.lines.iter().enumerate().map(|(i, line)| {
        let flagged = length.filter(|d| line.key.as_deref() == key_of(d.code));
        Row::Line(i + 1, line, flagged)
    }));
    Frame::new().inner_margin(Margin::symmetric(0, 4)).show(ui, |ui| {
        ui.set_max_height(max_height);
        ScrollArea::vertical()
            .id_salt("object-dictionary")
            .auto_shrink([false, true])
            .max_height(max_height)
            .show_rows(ui, DICT_ROW_H, rows.len(), |ui, range| {
                for index in range {
                    match &rows[index] {
                        Row::Loose(diagnostic) => loose_row(ui, facts, diagnostic, actions),
                        Row::Line(number, line, flagged) => {
                            line_row(ui, facts, *number, line, *flagged, actions);
                        }
                    }
                }
            });
    });
}

/// The text a diagnostic shows on its line.
fn marker_text(facts: &ObjectFacts, diagnostic: &DiagnosticInfo) -> String {
    if diagnostic.code == "stream-length-wrong"
        && let Some(stream) = &facts.stream
    {
        let declared = stream
            .declared_length
            .and_then(|n| usize::try_from(n).ok())
            .map_or_else(|| "missing".to_owned(), |n| format!("{} B", thousands(n)));
        return format!("Declared {declared}, actual {} B", thousands(stream.encoded_len));
    }
    diagnostic.message.clone()
}

/// The warn-tinted marker at the right end of a line: the problem and the button to the bytes.
fn marker(ui: &mut Ui, rect: Rect, facts: &ObjectFacts, diagnostic: &DiagnosticInfo, actions: &mut Actions) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let range = diagnostic.byte_range();
            let button = ui
                .add_enabled_ui(range.is_some(), |ui| {
                    ToolButton::new(SHOW_BYTES_LABEL).icon(Icon::Bytes).show(ui)
                })
                .inner;
            if button.clicked() {
                actions.show_bytes = range;
            }
            ui.label(
                RichText::new(marker_text(facts, diagnostic))
                    .font(fonts::ui(tokens::text::BODY))
                    .color(tokens::WARN),
            );
            Icon::Warning.show(ui, 13.0, tokens::WARN);
        },
    );
}

fn loose_row(ui: &mut Ui, facts: &ObjectFacts, diagnostic: &DiagnosticInfo, actions: &mut Actions) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), DICT_ROW_H), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, tokens::WARN.gamma_multiply(0.10));
    marker(ui, rect, facts, diagnostic, actions);
}

fn line_row(
    ui: &mut Ui,
    facts: &ObjectFacts,
    number: usize,
    line: &DictLine,
    flagged: Option<&DiagnosticInfo>,
    actions: &mut Actions,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), DICT_ROW_H), Sense::hover());
    if flagged.is_some() {
        ui.painter()
            .rect_filled(rect, 0.0, tokens::WARN.gamma_multiply(0.10));
    }
    let number_rect = Rect::from_min_size(rect.min + vec2(14.0, 0.0), vec2(NUMBER_W, rect.height()));
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(number_rect)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.label(
                RichText::new(number.to_string())
                    .font(fonts::mono(tokens::mono::BODY))
                    .color(tokens::TEXT_3.gamma_multiply(0.7)),
            );
        },
    );
    let indent = if line.kind == LineKind::Entry { INDENT } else { 0.0 };
    let text_rect = Rect::from_min_max(
        rect.min + vec2(TEXT_X + indent, 0.0),
        egui::pos2(
            (rect.max.x - 10.0).max(rect.min.x + TEXT_X + indent + 20.0),
            rect.max.y,
        ),
    );
    let ink = |color: Color32| if flagged.is_some() { tokens::WARN } else { color };
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if let Some(key) = &line.key {
                ui.label(
                    RichText::new(format!("{key} "))
                        .font(fonts::mono(tokens::mono::LARGE))
                        .color(ink(tokens::ACCENT)),
                );
            }
            for piece in &line.pieces {
                match piece {
                    Piece::Text(text) => {
                        let color = match line.kind {
                            LineKind::Entry if text.trim_start().starts_with('/') => ink(tokens::ACCENT),
                            LineKind::Entry => ink(tokens::TEXT),
                            LineKind::Bracket | LineKind::Keyword => tokens::TEXT_3,
                        };
                        ui.add(
                            Label::new(
                                RichText::new(text)
                                    .font(fonts::mono(tokens::mono::LARGE))
                                    .color(color),
                            )
                            .selectable(false),
                        );
                    }
                    Piece::Ref(r) => {
                        let text = r.to_string();
                        let target = ui.add(
                            Label::new(
                                RichText::new(&text)
                                    .font(fonts::mono(tokens::mono::LARGE))
                                    .color(tokens::ACCENT)
                                    .underline(),
                            )
                            .sense(Sense::click())
                            .selectable(false),
                        );
                        target.widget_info(|| WidgetInfo::labeled(Role::Link, true, text.clone()));
                        if target.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            actions.navigate = Some(*r);
                        }
                    }
                }
            }
        },
    );
    if let Some(diagnostic) = flagged {
        marker(ui, rect, facts, diagnostic, actions);
    }
}

/// What to say about a stream whose data is neither operators nor text.
fn binary(ui: &mut Ui, facts: &ObjectFacts, stream: &crate::object_facts::StreamFacts) {
    Frame::new()
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Binary data")
                    .font(fonts::semibold(tokens::text::BODY))
                    .color(tokens::TEXT),
            );
            ui.add_space(4.0);
            let decoded = match stream.decoded_len {
                Some(n) if stream.complete => format!("{} after decoding", human_bytes(n)),
                _ => "not fully decoded".to_owned(),
            };
            let filters = if stream.filters.is_empty() {
                "no filter".to_owned()
            } else {
                stream.filters.join(" + ")
            };
            let line = format!(
                "{} stored with {filters}, {decoded}. Bytes are not listed here; switch to Raw bytes to see them.",
                human_bytes(stream.encoded_len)
            );
            put_wrapped(ui, &line);
            if matches!(facts.kind, crate::object_facts::ObjectKind::Image) {
                ui.add_space(2.0);
                put_wrapped(ui, &facts.subtitle);
            }
        });
}

fn put_wrapped(ui: &mut Ui, text: &str) {
    ui.add(Label::new(RichText::new(text).color(tokens::TEXT_2)).wrap());
}
