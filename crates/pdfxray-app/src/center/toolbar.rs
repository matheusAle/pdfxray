//! The canvas toolbar: page navigation, the View menu, Isolate and Hide, and Pick, measure, boxes
//! and zoom.

use egui::{Align, Align2, Frame, Layout, Margin, RichText, TextEdit, Ui};
use pdfxray_core::numeric::f32_from_usize;

use super::{pick, view_menu};
use crate::components::{Icon, ToolButton, tooltip};
use crate::modes::ViewTool;
use crate::state::AppState;
use crate::theme::{fonts, tokens};
use crate::view::ZoomMode;

/// Accessible name of the previous-page button.
pub const PREV_LABEL: &str = "Prev";

/// Accessible name of the next-page button.
pub const NEXT_LABEL: &str = "Next";

/// Accessible name of the zoom-in button.
pub const ZOOM_IN_LABEL: &str = "Zoom in";

/// Accessible name of the zoom-out button.
pub const ZOOM_OUT_LABEL: &str = "Zoom out";

/// Accessible name of the fit-page button.
pub const FIT_LABEL: &str = "Fit page";

/// Name of the measure tool toggle; the `M` key does the same.
pub const MEASURE_LABEL: &str = "Measure";

/// Name of the page boxes overlay toggle; the `B` key does the same.
pub const BOXES_LABEL: &str = "Page boxes";

/// The page indicator text after the page field, e.g. `/ 3`.
pub fn page_total(count: usize) -> String {
    format!("/ {count}")
}

/// The zoom readout text, e.g. `125%`.
pub fn zoom_text(zoom: f32) -> String {
    format!("{:.0}%", zoom * 100.0)
}

/// Height of the toolbar row, margins included.
pub const HEIGHT: f32 = tokens::CONTROL_H + 12.0;

/// Space between toolbar items.
const SPACING: f32 = 2.0;

/// What Isolate and Hide say while there is nothing to isolate.
pub const NEEDS_SELECTION: &str = "Select something with content on the page first";

/// Space that sets a group of buttons apart: after the page controls, after the View menu, and
/// before the page boxes.
const GROUP_GAP: f32 = 6.0;

/// Width of the page field at least, whatever the page count is.
const PAGE_FIELD_W: f32 = 26.0;

/// Room in the page field around its digits: the field's margins and hairline, and a little air.
const PAGE_FIELD_PAD: f32 = 16.0;

/// How much of the toolbar the width allows. The names go before two controls can overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    /// Everything with its name.
    Full,
    /// The Pick menu is its icon alone.
    PickIcon,
    /// The View toggle is its icon alone too.
    Icons,
    /// Isolate and Hide leave the toolbar as well: the selection's action bar on the page and
    /// the `I` and `H` keys still have them.
    NoSelectionTools,
}

impl Fit {
    const ALL: [Self; 4] = [Self::Full, Self::PickIcon, Self::Icons, Self::NoSelectionTools];

    const fn pick_collapsed(self) -> bool {
        !matches!(self, Self::Full)
    }

    const fn view_collapsed(self) -> bool {
        matches!(self, Self::Icons | Self::NoSelectionTools)
    }

    const fn selection_tools(self) -> bool {
        !matches!(self, Self::NoSelectionTools)
    }
}

/// Horizontal margin of the toolbar on each side.
const MARGIN: i8 = 10;

/// Draws the toolbar into `ui`.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState, count: usize) {
    state.refresh_selection_ops();
    Frame::new()
        .inner_margin(Margin::symmetric(MARGIN, 6))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = SPACING;
            let room = ui.available_width();
            // The first look that fits; the narrowest when none does.
            let fit = Fit::ALL
                .into_iter()
                .find(|fit| needed(ui, state, count, *fit) <= room)
                .unwrap_or(Fit::NoSelectionTools);
            ui.horizontal(|ui| {
                pages(ui, state, count);
                ui.add_space(GROUP_GAP);
                view_menu::button(ui, state, fit.view_collapsed());
                if fit.selection_tools() {
                    ui.add_space(GROUP_GAP);
                    selection_tools(ui, state);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    zoom(ui, state);
                    pick::button(ui, state, fit.pick_collapsed());
                });
            });
        });
}

/// The width of the zoom readout's slot: as wide as the widest readout (three digits of the widest
/// digit), so the buttons to its left stay put while the zoom changes.
fn zoom_slot(ui: &Ui) -> f32 {
    (0..=9)
        .map(|digit| ToolButton::new(&format!("{digit}{digit}{digit}%")).width(ui))
        .fold(0.0, f32::max)
}

/// The width of the page field: wide enough for the digits of the last page, so that `18` of `40`
/// is not cut to `1`, but never less than [`PAGE_FIELD_W`].
fn page_field_width(ui: &Ui, count: usize) -> f32 {
    let digits = count.max(1).to_string().len();
    let widest = (0..=9)
        .map(|digit| {
            ui.painter()
                .layout_no_wrap(digit.to_string(), fonts::mono(tokens::mono::BODY), tokens::TEXT)
                .size()
                .x
        })
        .fold(0.0, f32::max);
    // At most a handful of digits.
    let text = widest * f32_from_usize(digits);
    (text + PAGE_FIELD_PAD).max(PAGE_FIELD_W)
}

/// The width the toolbar takes at `fit`: the page controls, the View menu, Isolate and Hide, the
/// zoom group and the Pick menu, and the space between them.
fn needed(ui: &Ui, state: &AppState, count: usize, fit: Fit) -> f32 {
    let square = tokens::CONTROL_H;
    let total = ui
        .painter()
        .layout_no_wrap(page_total(count), fonts::mono(tokens::mono::BODY), tokens::TEXT_3)
        .size()
        .x;
    // Prev, the field, the total and Next.
    let pages = square * 2.0 + page_field_width(ui, count) + total;
    // Zoom out and in, fit, boxes and measure around the readout.
    let zoom = square * 5.0 + zoom_slot(ui);
    // Five gaps up to the View menu, one to the zoom group, seven inside it (eight items, the
    // group gap among them), and the two group gaps; Isolate and Hide add two of each kind.
    let selection = if fit.selection_tools() {
        square * 2.0 + SPACING * 2.0 + GROUP_GAP
    } else {
        0.0
    };
    let buttons = pages
        + view_menu::width(ui, state, fit.view_collapsed())
        + zoom
        + pick::width(ui, state, fit.pick_collapsed());
    buttons + selection + SPACING * 13.0 + GROUP_GAP * 2.0
}

fn pages(ui: &mut Ui, state: &mut AppState, count: usize) {
    let last = usize::try_from(state.canvas.page).is_ok_and(|p| p + 1 >= count);
    ui.add_enabled_ui(state.canvas.page > 0, |ui| {
        let prev = ToolButton::new(PREV_LABEL)
            .icon(Icon::ChevronLeft)
            .icon_only(true)
            .show(ui);
        if prev.clicked() {
            state.prev_page();
        }
        tooltip::named(prev, PREV_LABEL, "Previous page", None);
    });
    let field = ui.add(
        TextEdit::singleline(&mut state.canvas.page_field)
            .desired_width(page_field_width(ui, count))
            .font(fonts::mono(tokens::mono::BODY))
            .align(Align2::CENTER_CENTER)
            .hint_text("Go to page"),
    );
    if field.lost_focus() {
        state.commit_page_field();
    } else if !field.has_focus() {
        state.canvas.page_field = (state.canvas.page + 1).to_string();
    }
    ui.label(
        RichText::new(page_total(count))
            .font(fonts::mono(tokens::mono::BODY))
            .color(tokens::TEXT_3),
    );
    ui.add_enabled_ui(!last, |ui| {
        let next = ToolButton::new(NEXT_LABEL)
            .icon(Icon::ChevronRight)
            .icon_only(true)
            .show(ui);
        if next.clicked() {
            state.next_page();
        }
        tooltip::named(next, NEXT_LABEL, "Next page", None);
    });
}

/// Isolate and Hide: they act on the selection, so they are disabled until there is one with
/// content, and say so.
fn selection_tools(ui: &mut Ui, state: &mut AppState) {
    for tool in ViewTool::SELECTION {
        let available = tool.available(state);
        let response = ui
            .add_enabled_ui(available, |ui| {
                ToolButton::new(tool.label())
                    .icon(tool.icon())
                    .icon_only(true)
                    .active(tool.active(state))
                    .show(ui)
            })
            .inner;
        let detail = if available { tool.hint() } else { NEEDS_SELECTION };
        let response = tooltip::named(response, tool.label(), detail, Some(tool.key().symbol_or_name()));
        if response.clicked() {
            tool.toggle(state);
        }
    }
}

/// The right-hand group, added right to left: zoom in, readout, zoom out, fit, boxes, measure;
/// the Pick menu follows it.
fn zoom(ui: &mut Ui, state: &mut AppState) {
    let zoom_in = ToolButton::new(ZOOM_IN_LABEL)
        .icon(Icon::ZoomIn)
        .icon_only(true)
        .show(ui);
    if zoom_in.clicked() {
        state.zoom_in();
    }
    tooltip::named(zoom_in, ZOOM_IN_LABEL, "Magnify the page", Some("⌘="));
    let readout = zoom_text(state.canvas.zoom);
    let response = ToolButton::new(&readout).min_width(zoom_slot(ui)).show(ui);
    if response.clicked() {
        state.set_zoom(1.0);
    }
    tooltip::named(response, "Actual size", "Reset the zoom to 100%", Some("⌘0"));
    let zoom_out = ToolButton::new(ZOOM_OUT_LABEL)
        .icon(Icon::ZoomOut)
        .icon_only(true)
        .show(ui);
    if zoom_out.clicked() {
        state.zoom_out();
    }
    tooltip::named(zoom_out, ZOOM_OUT_LABEL, "Shrink the page", Some("⌘-"));
    let fit = ToolButton::new(FIT_LABEL)
        .icon(Icon::Fit)
        .icon_only(true)
        .active(state.canvas.zoom_mode == ZoomMode::Fit)
        .show(ui);
    if fit.clicked() {
        state.fit_page();
    }
    tooltip::named(fit, FIT_LABEL, "Fit the whole page in the canvas", None);
    ui.add_space(GROUP_GAP);
    let boxes = ToolButton::new(BOXES_LABEL)
        .icon(Icon::Boxes)
        .icon_only(true)
        .active(state.canvas.show_boxes)
        .show(ui);
    if boxes.clicked() {
        state.canvas.show_boxes = !state.canvas.show_boxes;
    }
    tooltip::named(
        boxes,
        BOXES_LABEL,
        "Draw all five page boxes; dashed ones are defaults",
        Some("B"),
    );
    let measure = ToolButton::new(MEASURE_LABEL)
        .icon(Icon::Measure)
        .icon_only(true)
        .active(state.canvas.measure.active)
        .show(ui);
    if measure.clicked() {
        state.canvas.measure.toggle();
    }
    tooltip::named(
        measure,
        MEASURE_LABEL,
        "Measure distances: drag, or click two points",
        Some("M"),
    );
}
