//! The object view that replaces the canvas for the Objects entry of the rail: the header with
//! history and the Decoded | Raw bytes switch, and the body of the active view.

pub mod decoded;
pub mod ops;
pub mod preview;
pub mod raw;

use egui::{Align, Frame, Label, Layout, Margin, Rect, RichText, Sense, Ui, UiBuilder, vec2};

use crate::components::{Chip, Icon, Segment, Tone, ToolButton, segmented, segmented_width, tooltip};
use crate::object_facts::{ObjectFacts, human_bytes};
use crate::state::{AppState, ObjectMode};
use crate::theme::{fonts, tokens};

/// Shown before an object is selected.
pub const PICK_AN_OBJECT: &str = "Pick an object in the navigator to see it here.";

/// Name of the history button that goes back.
pub const BACK_LABEL: &str = "Back";

/// Name of the history button that goes forward.
pub const FORWARD_LABEL: &str = "Forward";

/// Draws the object view.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(facts) = state.object_facts().cloned() else {
        placeholder(ui, state);
        return;
    };
    header(ui, state, &facts);
    match state.objects.mode {
        ObjectMode::Decoded => decoded::ui(ui, state, &facts),
        ObjectMode::Raw => raw::ui(ui, state, &facts),
    }
}

/// What to say when there is nothing to show.
fn placeholder(ui: &mut Ui, state: &AppState) {
    let text = match (&state.selection.primary, state.viewed_revision()) {
        (Some(pdfxray_core::NodeId::Object(r)), Some(rev)) => {
            format!("{r} does not exist in revision {}.", rev + 1)
        }
        _ => PICK_AN_OBJECT.to_owned(),
    };
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.35).max(24.0));
        ui.label(RichText::new(text).color(tokens::TEXT_3));
    });
}

/// History arrows, the title, the chips, and the view switch; the subtitle below.
fn header(ui: &mut Ui, state: &mut AppState, facts: &ObjectFacts) {
    Frame::new()
        .inner_margin(Margin {
            left: 14,
            right: 14,
            top: 10,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                history_buttons(ui, state);
                let title = facts.object.map_or_else(
                    || facts.title.clone(),
                    |r| format!("{} {} obj", r.num, r.generation),
                );
                ui.label(
                    RichText::new(title)
                        .font(fonts::mono_medium(tokens::mono::LARGE + 3.0))
                        .color(tokens::TEXT),
                );
                chips(ui, facts);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // The control sits at the row's right end and reads left to right: its room
                    // is reserved first, then it is drawn into that room.
                    let segments = ObjectMode::ALL.map(|mode| Segment::new(mode, mode.title()));
                    let size = vec2(segmented_width(ui, &segments), ui.available_height());
                    let (room, _) = ui.allocate_exact_size(size, Sense::hover());
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(room)
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| {
                            let mut mode = state.objects.mode;
                            if segmented(ui, &mut mode, &segments) {
                                state.objects.mode = mode;
                            }
                        },
                    );
                });
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(52.0);
                ui.label(
                    RichText::new(&facts.subtitle)
                        .font(fonts::ui(tokens::text::SMALL))
                        .color(tokens::TEXT_3),
                );
            });
        });
    ui.separator();
}

fn history_buttons(ui: &mut Ui, state: &mut AppState) {
    ui.spacing_mut().item_spacing.x = 2.0;
    let back = ui
        .add_enabled_ui(state.history.can_go_back(), |ui| {
            ToolButton::new(BACK_LABEL)
                .icon(Icon::ChevronLeft)
                .icon_only(true)
                .show(ui)
        })
        .inner;
    let forward = ui
        .add_enabled_ui(state.history.can_go_forward(), |ui| {
            ToolButton::new(FORWARD_LABEL)
                .icon(Icon::ChevronRight)
                .icon_only(true)
                .show(ui)
        })
        .inner;
    let (went_back, went_forward) = (back.clicked(), forward.clicked());
    tooltip::with_shortcut(back, "Back", "⌘[");
    tooltip::with_shortcut(forward, "Forward", "⌘]");
    if went_back {
        state.history_back();
    }
    if went_forward {
        state.history_forward();
    }
    ui.spacing_mut().item_spacing.x = 6.0;
}

fn chips(ui: &mut Ui, facts: &ObjectFacts) {
    if let Some(stream) = &facts.stream {
        let filters = if stream.filters.is_empty() {
            "no filter".to_owned()
        } else {
            stream.filters.join(" + ")
        };
        let sizes = match stream.decoded_len {
            Some(decoded) if stream.complete => {
                format!("{} → {}", human_bytes(stream.encoded_len), human_bytes(decoded))
            }
            Some(_) | None => format!("{} → partial", human_bytes(stream.encoded_len)),
        };
        let text = if stream.filters.is_empty() {
            format!("{filters} · {}", human_bytes(stream.encoded_len))
        } else {
            format!("{filters} · {sizes}")
        };
        Chip::new(&text).show(ui);
    }
    if facts.changed_later {
        Chip::new(&format!("changed in rev {}", facts.defined_in + 1))
            .tone(Tone::Warn)
            .show(ui);
    }
    if let Some(page) = facts.page {
        Chip::new(&format!("Page {} /Contents", page + 1))
            .tone(Tone::Kind(tokens::ACCENT))
            .show(ui);
    }
}

/// A label with `rect` as its box, vertically centred.
fn put(ui: &mut Ui, rect: Rect, label: Label) -> egui::Response {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| ui.add(label),
    )
    .inner
}
