//! A file is opening: a card with the stages, what each found, and a way out.

use std::f32::consts::TAU;
use std::time::Duration;

use egui::{
    Align, Label, Layout, Margin, Painter, Pos2, Rect, RichText, Role, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, WidgetInfo, pos2, vec2,
};
use pdfxray_core::numeric::f32_from_f64;

use super::card;
use crate::brand;
use crate::components::Icon;
use crate::open::{OpenProgress, STAGES, StageState, stage_title};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Note under the progress bar.
pub const PAGES_NOTE: &str = "Pages appear as soon as they're ready";

/// Name of the button that stops opening.
pub const CANCEL_LABEL: &str = "Cancel";

/// What the card's headline says about `name`.
pub fn title(name: &str) -> String {
    format!("Opening {name}")
}

/// Size of the logo above the headline.
const LOGO: f32 = 48.0;
/// Space between the logo and the headline.
const LOGO_BELOW: f32 = 14.0;

const WIDTH: f32 = 440.0;
const ROW_H: f32 = 25.0;
/// Height of the card, to centre it before it is laid out.
const HEIGHT: f32 = 343.0;
const BAR_H: f32 = 6.0;

/// Draws the card for the job in flight.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let (Some(name), Some(progress)) = (state.opening_name(), state.open_progress().cloned()) else {
        return;
    };
    let turns = state.opening.spinner_turns;
    let mut cancel = false;
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add_space(((ui.available_height() - HEIGHT) / 2.0).max(24.0));
        card(tokens::SURFACE, Margin::same(28)).show(ui, |ui| {
            ui.set_width(WIDTH - 56.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                brand::logo(ui, LOGO);
                ui.add_space(LOGO_BELOW);
                ui.add(
                    Label::new(
                        RichText::new(title(&name))
                            .font(fonts::semibold(tokens::text::TITLE))
                            .color(tokens::TEXT),
                    )
                    .truncate(),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(progress.facts())
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                );
                ui.add_space(12.0);
                for stage in STAGES {
                    stage_row(ui, &progress, stage, turns);
                }
                ui.add_space(10.0);
                bar(ui, progress.fraction());
                ui.add_space(14.0);
                cancel = footer(ui);
            });
        });
    });
    if cancel {
        state.cancel_open();
    }
}

/// One stage: its mark, its name, and what it found.
fn stage_row(ui: &mut Ui, progress: &OpenProgress, stage: pdfxray_core::OpenStage, turns: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let stage_state = progress.state(stage);
    let mark = Rect::from_center_size(pos2(rect.min.x + 7.0, rect.center().y), vec2(14.0, 14.0));
    match stage_state {
        StageState::Done => {
            ui.painter()
                .circle_stroke(mark.center(), 6.5, Stroke::new(1.25, tokens::OK));
            Icon::Check.paint(ui, mark, 8.0, tokens::OK);
        }
        StageState::Running => spinner(ui, mark.center(), turns),
        StageState::Pending => {
            ui.painter()
                .circle_stroke(mark.center(), 6.5, Stroke::new(1.25, tokens::BORDER_STRONG));
        }
    }
    let ink = if stage_state == StageState::Pending {
        tokens::TEXT_3
    } else {
        tokens::TEXT
    };
    let name = Rect::from_min_max(rect.min + vec2(25.0, 0.0), rect.max);
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(name)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.label(
                RichText::new(stage_title(stage))
                    .font(fonts::ui(tokens::text::BODY))
                    .color(ink),
            );
        },
    );
    if let Some(detail) = progress.detail(stage) {
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.label(
                    RichText::new(detail)
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                );
            },
        );
    }
}

/// A ring with a quarter turned in the accent. It turns while `turns`; the repaints it asks for
/// are timed ones, so a screen that holds it can still settle.
fn spinner(ui: &Ui, center: Pos2, turns: bool) {
    let painter = ui.painter();
    painter.circle_stroke(center, 6.5, Stroke::new(1.25, tokens::BORDER_STRONG));
    let start = if turns {
        ui.ctx().request_repaint_after(Duration::from_millis(33));
        // One turn a second.
        (f32_from_f64(ui.input(|input| input.time)) * TAU) % TAU
    } else {
        0.0
    };
    arc(
        painter,
        center,
        6.5,
        start,
        TAU / 3.0,
        Stroke::new(1.25, tokens::ACCENT),
    );
}

fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
    const STEPS: u8 = 12;
    let points: Vec<Pos2> = (0..=STEPS)
        .map(|step| {
            let angle = start + sweep * f32::from(step) / f32::from(STEPS);
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

/// The progress bar.
fn bar(ui: &mut Ui, fraction: f32) {
    let (track, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_H), Sense::hover());
    ui.painter().rect_filled(track, BAR_H / 2.0, tokens::SURFACE_3);
    let mut filled = track;
    filled.set_width(track.width() * fraction.clamp(0.0, 1.0));
    if filled.width() > 0.0 {
        ui.painter().rect(
            filled,
            BAR_H / 2.0,
            tokens::ACCENT,
            Stroke::NONE,
            StrokeKind::Inside,
        );
    }
}

/// The note and the Cancel button; true when Cancel was pressed.
fn footer(ui: &mut Ui) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.label(
                RichText::new(PAGES_NOTE)
                    .font(fonts::ui(tokens::text::SMALL))
                    .color(tokens::TEXT_3),
            );
        },
    );
    let galley = ui.painter().layout_no_wrap(
        CANCEL_LABEL.to_owned(),
        fonts::ui(tokens::text::SMALL),
        tokens::TEXT_2,
    );
    let button = Rect::from_min_max(pos2(rect.max.x - galley.size().x - 8.0, rect.min.y), rect.max);
    let response = ui.interact(button, ui.make_persistent_id("cancel-open"), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, CANCEL_LABEL));
    let ink = if response.hovered() {
        tokens::TEXT
    } else {
        tokens::TEXT_2
    };
    ui.painter().text(
        button.right_center() - vec2(4.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        CANCEL_LABEL,
        fonts::ui(tokens::text::SMALL),
        ink,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}
