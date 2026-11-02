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
use pdfxray_core::numeric::f32_from_usize;

use crate::components::long_value::{self, Wrapped};
use crate::components::{Icon, ToolButton, focus};
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
/// Space between a line's text and the right edge.
const RIGHT_PAD: f32 = 10.0;
/// Height of each line a value wraps onto, under its first one.
const WRAP_LINE_H: f32 = 20.0;

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
    // Whether a long value was unfolded is kept per object and line.
    let salt = ui.make_persistent_id(("object-dictionary", facts.object));
    Frame::new().inner_margin(Margin::symmetric(0, 4)).show(ui, |ui| {
        ui.set_max_height(max_height);
        ScrollArea::vertical()
            .id_salt("object-dictionary")
            .auto_shrink([false, true])
            .max_height(max_height)
            .show_viewport(ui, |ui, viewport| {
                // The lines keep the pitch they had as list rows: their height and the gap.
                let gap = ui.spacing().item_spacing.y;
                let width = ui.available_width();
                // A line is as tall as its value wraps: each one is measured, and only those in
                // view are drawn.
                let laid: Vec<Option<Laid>> = rows
                    .iter()
                    .map(|row| match row {
                        Row::Loose(_) => None,
                        Row::Line(number, line, _) => lay_out(ui, salt.with(number), line, width),
                    })
                    .collect();
                let heights: Vec<f32> = laid
                    .iter()
                    .map(|laid| laid.as_ref().map_or(DICT_ROW_H, Laid::height))
                    .collect();
                let total: f32 = heights.iter().map(|h| h + gap).sum::<f32>() - gap;
                let origin = ui.max_rect().min;
                let mut top = 0.0;
                for (index, height) in heights.iter().copied().enumerate() {
                    let (from, to) = (top, top + height);
                    top = to + gap;
                    if to < viewport.min.y || from > viewport.max.y {
                        continue;
                    }
                    let rect = Rect::from_min_size(origin + vec2(0.0, from), vec2(width, height));
                    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| match &rows[index] {
                        Row::Loose(diagnostic) => loose_row(ui, facts, diagnostic, actions),
                        Row::Line(number, line, flagged) => {
                            line_row(ui, facts, *number, line, laid[index].as_ref(), *flagged, actions);
                        }
                    });
                }
                ui.allocate_rect(Rect::from_min_size(origin, vec2(width, total)), Sense::hover());
            });
    });
}

/// A line whose value wraps: the value as one text, its lines and its id.
struct Laid {
    /// The line's id: whether its value was unfolded is kept under it.
    id: egui::Id,
    /// The value, references written out.
    value: String,
    /// Where the value starts, from the left edge of the line.
    value_x: f32,
    /// The value's lines.
    wrapped: Wrapped,
}

impl Laid {
    /// The line's height: its first line in a whole row, then the lines it wraps onto and the
    /// fold line.
    fn height(&self) -> f32 {
        let more = self.wrapped.lines.len().saturating_sub(1);
        DICT_ROW_H
            + WRAP_LINE_H * f32_from_usize(more)
            + if self.wrapped.fold.is_some() {
                long_value::FOLD_LINE_H
            } else {
                0.0
            }
    }
}

/// The value of `line` as one text, references written out.
fn value_text(line: &DictLine) -> String {
    line.pieces
        .iter()
        .map(|piece| match piece {
            Piece::Text(text) => text.clone(),
            Piece::Ref(r) => r.to_string(),
        })
        .collect()
}

/// How `line` wraps in a dictionary `width` wide, when its value needs more than one line.
fn lay_out(ui: &Ui, id: egui::Id, line: &DictLine, width: f32) -> Option<Laid> {
    if line.kind != LineKind::Entry {
        return None;
    }
    let font = fonts::mono(tokens::mono::LARGE);
    let key_w = line.key.as_ref().map_or(0.0, |key| {
        ui.painter()
            .layout_no_wrap(format!("{key} "), font.clone(), Color32::WHITE)
            .size()
            .x
    });
    let value_x = TEXT_X + INDENT + key_w;
    let room = (width - RIGHT_PAD - value_x).max(20.0);
    let value = value_text(line);
    let unfolded = long_value::is_unfolded(ui.ctx(), id);
    let wrapped = long_value::wrap_in(ui, &value, &font, room, unfolded);
    (wrapped.line_count() > 1).then_some(Laid {
        id,
        value,
        value_x,
        wrapped,
    })
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
    laid: Option<&Laid>,
    flagged: Option<&DiagnosticInfo>,
    actions: &mut Actions,
) {
    let height = laid.map_or(DICT_ROW_H, Laid::height);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if flagged.is_some() {
        ui.painter()
            .rect_filled(rect, 0.0, tokens::WARN.gamma_multiply(0.10));
    }
    // One numbered line, however many lines its value wraps onto.
    let first = Rect::from_min_size(rect.min, vec2(rect.width(), DICT_ROW_H));
    let number_rect = Rect::from_min_size(first.min + vec2(14.0, 0.0), vec2(NUMBER_W, DICT_ROW_H));
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
        first.min + vec2(TEXT_X + indent, 0.0),
        egui::pos2(
            (first.max.x - RIGHT_PAD).max(first.min.x + TEXT_X + indent + 20.0),
            first.max.y,
        ),
    );
    let ink = |color: Color32| if flagged.is_some() { tokens::WARN } else { color };
    if let Some(laid) = laid {
        wrapped_value(ui, rect, line, laid, flagged.is_some(), actions);
    }
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
            // A value that wraps is drawn line by line beside the key.
            let pieces = if laid.is_some() { &[][..] } else { &line.pieces[..] };
            for piece in pieces {
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
                        focus::ring(
                            ui,
                            &target,
                            target.rect.expand2(vec2(3.0, 1.0)),
                            tokens::RADIUS_SM,
                        );
                        if target.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            actions.navigate = Some(*r);
                        }
                    }
                }
            }
        },
    );
    if let Some(diagnostic) = flagged {
        marker(ui, first, facts, diagnostic, actions);
    }
}

/// The colour of a piece of a value's text: names in the accent, the rest as text, all of it
/// in the warning colour on a flagged line.
fn piece_ink(text: &str, flagged: bool) -> Color32 {
    if flagged {
        tokens::WARN
    } else if text.trim_start().starts_with('/') {
        tokens::ACCENT
    } else {
        tokens::TEXT
    }
}

/// The lines of a value that wraps, from `laid.value_x` in the line's `rect`: the first one
/// beside the key, the others under the value's first item, then the fold line. References stay
/// links.
fn wrapped_value(
    ui: &mut Ui,
    rect: Rect,
    line: &DictLine,
    laid: &Laid,
    flagged: bool,
    actions: &mut Actions,
) {
    let font = fonts::mono(tokens::mono::LARGE);
    // Each piece's byte range in the value text, and the reference it is.
    let mut spans = Vec::with_capacity(line.pieces.len());
    let mut at = 0;
    for piece in &line.pieces {
        let (len, target, ink) = match piece {
            Piece::Text(text) => (text.len(), None, piece_ink(text, flagged)),
            Piece::Ref(r) => (r.to_string().len(), Some(*r), tokens::ACCENT),
        };
        spans.push((at..at + len, target, ink));
        at += len;
    }
    let left = rect.min.x + laid.value_x;
    for (n, range) in laid.wrapped.lines.iter().enumerate() {
        let y = rect.min.y + DICT_ROW_H / 2.0 + WRAP_LINE_H * f32_from_usize(n);
        let mut x = left + if n == 0 { 0.0 } else { laid.wrapped.indent };
        for (piece, (span, target, ink)) in spans.iter().enumerate() {
            let from = span.start.max(range.start);
            let to = span.end.min(range.end);
            if from >= to {
                continue;
            }
            let text = &laid.value[from..to];
            let drawn = ui.painter().text(
                egui::pos2(x, y),
                egui::Align2::LEFT_CENTER,
                text,
                font.clone(),
                *ink,
            );
            if let Some(r) = target {
                let hit = ui.interact(drawn, laid.id.with((n, piece)), Sense::click());
                let name = r.to_string();
                hit.widget_info(|| WidgetInfo::labeled(Role::Link, true, name.clone()));
                ui.painter()
                    .hline(drawn.x_range(), drawn.max.y - 1.0, egui::Stroke::new(1.0, *ink));
                focus::ring(ui, &hit, drawn.expand2(vec2(3.0, 1.0)), tokens::RADIUS_SM);
                if hit.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    actions.navigate = Some(*r);
                }
            }
            x = drawn.max.x;
        }
    }
    if let Some(fold) = &laid.wrapped.fold {
        let shown = f32_from_usize(laid.wrapped.lines.len());
        let y = rect.min.y
            + DICT_ROW_H / 2.0
            + WRAP_LINE_H * shown
            + (long_value::FOLD_LINE_H - WRAP_LINE_H) / 2.0;
        let closer_ink = piece_ink(fold.closer, flagged);
        long_value::fold_line(ui, laid.id, left + laid.wrapped.indent, y, fold, font, closer_ink);
    }
    // Assistive tech reads the whole value, however it is folded.
    let value = ui.interact(
        Rect::from_min_max(egui::pos2(left, rect.min.y), rect.max),
        laid.id.with("value"),
        Sense::hover(),
    );
    let text = laid.value.clone();
    value.widget_info(|| WidgetInfo::labeled(Role::Label, true, text.clone()));
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
