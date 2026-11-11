//! The status bar's display item: which pixels pages are drawn on, this Mac's or an emulated
//! screen's, and the popover that sets the emulated screen: its viewer, size, scaling and zoom.

use egui::{Align, DragValue, Layout, RectAlign, RichText, Ui};
use pdfxray_render::{Display, Viewer, ViewerZoom};

use crate::components::segmented::segmented_compact;
use crate::components::{Icon, Segment, popover, switch};
use crate::emulation::{
    DPI_RANGE, OFF_TEXT, SCALINGS, SCREENS, SIDE_RANGE, ZOOM_RANGE, dpi_of, rounding_text, scale_text,
    summary,
};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Name of the status bar item that opens the popover.
pub const DISPLAY_LABEL: &str = "Display";

/// The switch that draws pages on the emulated screen.
pub const EMULATE_LABEL: &str = "Emulate a Windows screen";

/// The row that picks the viewer.
pub const VIEWER_LABEL: &str = "Viewer";

/// The row that sets the screen's size.
pub const SCREEN_LABEL: &str = "Screen";

/// The row that sets its scaling.
pub const SCALING_LABEL: &str = "Scaling";

/// The row that sets the viewer's zoom.
pub const ZOOM_LABEL: &str = "Zoom";

/// What the popover says while comparing, whose pages are drawn on this Mac's pixels.
pub const COMPARING_NOTE: &str = "A comparison is drawn on this Mac's pixels.";

/// Outer width of the popover.
const POPOVER_W: f32 = 360.0;

/// Width of the labels in front of the rows.
const LABEL_W: f32 = 58.0;

/// What the status bar item says: the emulated screen, or that pages are on this Mac's pixels.
pub fn item_text(state: &AppState) -> String {
    state
        .emulation()
        .map_or_else(|| OFF_TEXT.to_owned(), |e| summary(&e))
}

/// The item, and its popover while open.
pub(super) fn item(ui: &mut Ui, state: &mut AppState) {
    let ink = |hovered: bool| match (state.screen.on, hovered) {
        (true, _) => tokens::ACCENT,
        (false, true) => tokens::TEXT_2,
        (false, false) => tokens::TEXT_3,
    };
    let response = super::status_bar::button(
        ui,
        "display",
        Icon::Monitor,
        &item_text(state),
        DISPLAY_LABEL,
        ink,
    );
    popover::show(
        &response,
        RectAlign::TOP_END,
        popover::inner_width(POPOVER_W),
        |ui| body(ui, state),
    );
}

fn body(ui: &mut Ui, state: &mut AppState) {
    ui.spacing_mut().item_spacing.y = 8.0;
    switch(ui, &mut state.screen.on, EMULATE_LABEL);
    let on = state.screen.on;
    ui.add_enabled_ui(on, |ui| {
        let settings = &mut state.screen.settings;
        row(ui, VIEWER_LABEL, |ui| viewer(ui, &mut settings.viewer));
        row(ui, SCREEN_LABEL, |ui| screen(ui, &mut settings.display));
        row(ui, SCALING_LABEL, |ui| scaling(ui, &mut settings.display));
        row(ui, ZOOM_LABEL, |ui| zoom(ui, &mut settings.zoom));
    });
    let settings = state.screen.settings;
    let page = state.canvas.page;
    if on
        && let Some(text) = state
            .document
            .as_ref()
            .and_then(|loaded| scale_text(&settings, &loaded.doc, page))
    {
        note(ui, &text, tokens::TEXT_2, true);
    }
    note(ui, rounding_text(settings.viewer), tokens::TEXT_3, false);
    if on && state.in_diff() {
        note(ui, COMPARING_NOTE, tokens::WARN, false);
    }
}

/// A row: its label, then what `add` puts after it.
fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_W, tokens::CONTROL_H),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(LABEL_W);
                ui.label(
                    RichText::new(label)
                        .font(fonts::ui(tokens::text::SMALL))
                        .color(tokens::TEXT_3),
                );
            },
        );
        ui.vertical(add);
    });
}

/// A line of text under the rows, wrapped to the popover's width.
fn note(ui: &mut Ui, text: &str, ink: egui::Color32, mono: bool) {
    let font = if mono {
        fonts::mono(tokens::mono::SMALL)
    } else {
        fonts::ui(tokens::text::SMALL)
    };
    ui.add(egui::Label::new(RichText::new(text).font(font).color(ink)).wrap());
}

fn viewer(ui: &mut Ui, viewer: &mut Option<Viewer>) {
    let mut segments = vec![Segment::new(None, "pdfxray")];
    segments.extend(Viewer::ALL.map(|v| Segment::new(Some(v), v.name())));
    segmented_compact(ui, viewer, &segments);
}

fn screen(ui: &mut Ui, display: &mut Display) {
    let labels = SCREENS.map(|(w, h)| format!("{w}×{h}"));
    let segments: Vec<Segment<'_, (u32, u32)>> = SCREENS
        .iter()
        .zip(&labels)
        .map(|(size, label)| Segment::new(*size, label))
        .collect();
    let mut size = (display.width, display.height);
    if segmented_compact(ui, &mut size, &segments) {
        (display.width, display.height) = size;
    }
    ui.horizontal(|ui| {
        ui.add(DragValue::new(&mut display.width).range(SIDE_RANGE).suffix(" px"));
        ui.label(RichText::new("×").color(tokens::TEXT_3));
        ui.add(
            DragValue::new(&mut display.height)
                .range(SIDE_RANGE)
                .suffix(" px"),
        );
    });
}

fn scaling(ui: &mut Ui, display: &mut Display) {
    let labels = SCALINGS.map(|p| format!("{p}%"));
    let segments: Vec<Segment<'_, u32>> = SCALINGS
        .iter()
        .zip(&labels)
        .map(|(percent, label)| Segment::new(dpi_of(*percent), label))
        .collect();
    segmented_compact(ui, &mut display.dpi, &segments);
    ui.add(DragValue::new(&mut display.dpi).range(DPI_RANGE).suffix(" DPI"));
}

fn zoom(ui: &mut Ui, zoom: &mut ViewerZoom) {
    let segments = [
        Segment::new(ViewerZoom::FitWidth, "Fit width"),
        Segment::new(ViewerZoom::FitPage, "Fit page"),
        Segment::new(ViewerZoom::Percent(100), "100%"),
    ];
    segmented_compact(ui, zoom, &segments);
    let mut percent = match *zoom {
        ViewerZoom::Percent(p) => p,
        ViewerZoom::FitWidth | ViewerZoom::FitPage => 100,
    };
    let field = ui.add(DragValue::new(&mut percent).range(ZOOM_RANGE).suffix("%"));
    if field.changed() {
        *zoom = ViewerZoom::Percent(percent);
    }
}
