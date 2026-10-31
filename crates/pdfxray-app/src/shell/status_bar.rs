//! The status bar: cursor, file info (its hover has the counts, the open time and veraPDF), units
//! and problems.

use std::time::Duration;

use egui::{Align, Color32, Layout, Rect, Response, RichText, Role, Sense, Ui, UiBuilder, WidgetInfo, vec2};

use crate::center::canvas::RENDERING_TEXT;
use crate::components::segmented::segmented_compact;
use crate::components::{Icon, Segment, focus};
use crate::state::{AppState, DrawerTab};
use crate::states::Screen;
use crate::theme::{fonts, motion, tokens};
use crate::units::Unit;
use crate::validation::{Availability, Environment};

/// Space between the bar's edges and its content.
const PAD: f32 = 12.0;
/// Space between items.
const GAP: f32 = 18.0;
/// Width reserved for the cursor: room for its longest reading,
/// `x 297.64  y 841.89 pt · display 297.64, 0.00 · UserUnit 2` (57 mono characters of 6 pt, and
/// the icon with its gap).
const CURSOR_W: f32 = 360.0;
/// How far the diagnostics' hover pill and target reach past its text, left and right.
const HIT_PAD: f32 = 6.0;
/// Space above and below that pill inside the bar.
const HIT_INSET: f32 = 3.0;
/// Side of an item's icon.
const ICON: f32 = 11.0;
/// Space between an item's icon and its text.
const ICON_GAP: f32 = 6.0;
/// Height of a line of the hover card, which stacks three: less than the row height of a control.
const CARD_LINE_H: f32 = 18.0;

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let bar = ui.max_rect();
    super::separator(ui, bar.top());
    let content = bar.shrink2(vec2(PAD, 0.0));
    // The right cluster is drawn first; the left one gets what it left.
    let right_start = ui
        .scope_builder(
            UiBuilder::new()
                .max_rect(content)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                right(ui, state);
                ui.min_rect().min.x
            },
        )
        .inner;
    let left_rect = egui::Rect::from_min_max(
        content.min,
        egui::pos2((right_start - GAP).max(content.min.x), content.max.y),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(left_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.set_clip_rect(left_rect.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.x = GAP;
            left(ui, state);
        },
    );
}

fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text)
        .font(fonts::mono(tokens::mono::SMALL))
        .color(tokens::TEXT_3)
}

/// An icon and a mono text in `ink`, 6 pt apart, in the row `ui` is laying out.
fn icon_text(ui: &mut Ui, icon: Icon, text: impl Into<String>, ink: Color32) {
    ui.spacing_mut().item_spacing.x = ICON_GAP;
    icon.show(ui, ICON, ink);
    ui.label(
        RichText::new(text)
            .font(fonts::mono(tokens::mono::SMALL))
            .color(ink),
    );
}

/// An item of the bar: [`icon_text`] inside its row.
fn item(ui: &mut Ui, icon: Icon, text: impl Into<String>, ink: Color32) -> Response {
    ui.scope(|ui| icon_text(ui, icon, text, ink)).response
}

/// A line of the hover card: [`icon_text`] in a row of its own, since the card stacks its lines.
fn card_line(ui: &mut Ui, icon: Icon, text: impl Into<String>, ink: Color32) {
    ui.horizontal(|ui| icon_text(ui, icon, text, ink));
}

/// Name of the item whose hover tells about the file.
pub const FILE_INFO: &str = "file info";

fn left(ui: &mut Ui, state: &mut AppState) {
    // A message that does not fit is cut, never wrapped onto a second line.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    // The pointer reads page coordinates: only the workspace has a page under it.
    if state.screen() == Screen::Workspace {
        let cursor = state
            .canvas
            .cursor
            .map_or_else(|| "x –  y –".to_owned(), |c| c.text(&state.lengths()));
        // A slot of its own width: the text changes with every pointer move, and what follows
        // must not move with it.
        let slot = vec2(CURSOR_W, ui.available_height());
        ui.allocate_ui_with_layout(slot, Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(CURSOR_W);
            ui.set_max_width(CURSOR_W);
            item(ui, Icon::Pointer, cursor, tokens::TEXT_3);
        });
    }
    let page = state.canvas.page;
    let counts = state.page_stats(page).map(|stats| {
        format!(
            "{} tagged · {} artifacts · {} untagged",
            stats.coverage.tagged, stats.coverage.artifact, stats.coverage.untagged
        )
    });
    let Some(opened_in) = state.document.as_ref().map(|loaded| loaded.opened_in) else {
        return;
    };
    file_info(ui, state, counts.as_deref(), opened_in);
    if state.render_pending() {
        ui.label(dim(RENDERING_TEXT));
    }
    if state.status != crate::state::READY {
        ui.add(egui::Label::new(dim(&state.status)).truncate());
    }
}

/// "file info": nothing but the name until the pointer rests on it, then a card with the page's
/// tagged, artifact and untagged counts, how long opening took and what is known of veraPDF.
fn file_info(ui: &mut Ui, state: &AppState, counts: Option<&str>, opened_in: Duration) {
    let response = item(ui, Icon::Info, FILE_INFO, tokens::TEXT_3);
    let response = ui.interact(response.rect, response.id.with(FILE_INFO), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, FILE_INFO));
    let (verapdf, dot) = verapdf_status(&state.validation.environment);
    let ctx = ui.ctx().clone();
    let id = response.id.with("hover-card");
    let mut opening = 1.0;
    let shown = egui::Tooltip::for_enabled(&response).show(|ui| {
        opening = motion::appear(&ctx, id, motion::POPOVER);
        ui.multiply_opacity(opening);
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.spacing_mut().interact_size.y = CARD_LINE_H;
        if let Some(counts) = counts {
            card_line(ui, Icon::Content, counts, tokens::TEXT_2);
        }
        card_line(
            ui,
            Icon::Timer,
            format!("opened in {:.2} s", opened_in.as_secs_f64()),
            tokens::TEXT_2,
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ICON_GAP;
            let (rect, _) = ui.allocate_exact_size(vec2(ICON, ICON), Sense::hover());
            ui.painter().circle_filled(rect.center(), 3.0, dot);
            ui.label(
                RichText::new(verapdf)
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_2),
            );
        });
    });
    if shown.is_none() {
        motion::reset(&ctx, id);
    }
}

fn right(ui: &mut Ui, state: &mut AppState) {
    // Right to left: the diagnostics are rightmost, then the units.
    if state.document.is_some() {
        diagnostics(ui, state);
    }
    let mut unit = state.unit;
    // This row runs right to left, and so does the control inside it: hand it the units backwards
    // and they read pt, mm, in, px.
    let mut segments = Unit::ALL.map(|u| Segment::new(u, u.label()));
    segments.reverse();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        if segmented_compact(ui, &mut unit, &segments) {
            state.unit = unit;
        }
    });
}

fn diagnostics(ui: &mut Ui, state: &mut AppState) {
    let count = state
        .document
        .as_ref()
        .map_or(0, |loaded| loaded.info.diagnostics.len());
    let text = format!("{count} diagnostics");
    let font = fonts::mono(tokens::mono::SMALL);
    let galley = ui.painter().layout_no_wrap(text.clone(), font, tokens::TEXT);
    let (rect, _) = ui.allocate_exact_size(
        vec2(ICON + ICON_GAP + galley.size().x, galley.size().y),
        Sense::hover(),
    );
    // The target is the bar's whole height, and a little wider than the text.
    let target = Rect::from_x_y_ranges(rect.x_range().expand(HIT_PAD), ui.max_rect().y_range());
    let response = ui.interact(target, ui.make_persistent_id("diagnostics"), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, text.clone()));
    if response.hovered() {
        ui.painter().rect_filled(
            target.shrink2(vec2(0.0, HIT_INSET)),
            tokens::RADIUS_SM,
            tokens::NEUTRAL_HOVER,
        );
    }
    let ink = match (count > 0, response.hovered()) {
        (true, _) => tokens::WARN,
        (false, true) => tokens::TEXT_2,
        (false, false) => tokens::TEXT_3,
    };
    focus::ring(
        ui,
        &response,
        target.shrink2(vec2(0.0, HIT_INSET)),
        tokens::RADIUS_SM,
    );
    Icon::Warning.paint(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(rect.min.x + ICON / 2.0, rect.center().y),
            vec2(ICON, ICON),
        ),
        ICON,
        ink,
    );
    ui.painter().galley_with_override_text_color(
        egui::pos2(rect.min.x + ICON + ICON_GAP, rect.min.y),
        galley,
        ink,
    );
    if response.clicked() {
        state.drawer.show_tab(DrawerTab::Diagnostics);
        state.explainer = false;
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
}

/// What the hover card says about veraPDF, and the colour of the dot in front of it.
fn verapdf_status(environment: &Environment) -> (String, Color32) {
    match environment {
        Environment::Known(Availability::Ready { version }) => (format!("veraPDF {version}"), tokens::OK),
        Environment::Known(_) => ("veraPDF missing".to_owned(), tokens::WARN),
        Environment::Checking => ("veraPDF · looking for it".to_owned(), tokens::TEXT_3),
        Environment::Unchecked => ("veraPDF · not checked yet".to_owned(), tokens::TEXT_3),
    }
}
