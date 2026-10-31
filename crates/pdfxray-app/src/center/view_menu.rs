//! The View control of the canvas toolbar: one button that shows the active page-wide render mode
//! and opens a menu with those modes, their keys and the options of the X-ray and the tag overlay.
//!
//! The menu has the modes that change how the whole page is painted ([`ViewTool::PAGE`]: X-ray,
//! Paint order, Clip, Alpha); Isolate and Hide act on the selection and are toolbar buttons. The
//! keys of [`crate::shell`] toggle the same modes. The menu stays open while the user toggles, so
//! a mode and its options can be set in one visit; Esc or a click outside closes it.

use egui::{
    Align, Align2, CursorIcon, Layout, Rect, RectAlign, Role, Sense, Shape, Ui, UiBuilder, WidgetInfo, pos2,
    vec2,
};

use crate::components::{Icon, Keycap, Segment, ToolButton, focus, popover, segmented, tooltip};
use crate::modes::{COLOR_MODES, OrderSource, ViewTool, color_title};
use crate::state::{AppState, DrawerTab};
use crate::theme::{fonts, tokens};

/// Name of the toggle while no mode is on.
pub const VIEW_LABEL: &str = "View";

/// Name of the ▾ beside the toggle: it opens the menu.
pub const OPTIONS_LABEL: &str = "View options";

/// Title of the colouring section of the X-ray options.
pub const COLOUR_BY_TITLE: &str = "Colour by";

/// Title of the section that says what the tag overlay shows.
pub const SHOW_TITLE: &str = "Show on page";

/// Title of the section that says where the reading order comes from.
pub const ORDER_FROM_TITLE: &str = "Reading order from";

/// Name of the option that outlines every structure element on the page; `⇧T` does the same.
pub const ALL_BOXES_LABEL: &str = "All tag boxes";

/// Name of the option that writes the tag name over each box; `L` does the same.
pub const LABELS_LABEL: &str = "Tag labels";

/// Name of the option that numbers the elements in reading order; `R` does the same.
pub const READING_ORDER_LABEL: &str = "Reading order";

/// Name of the option that draws containers around their children.
pub const NESTING_LABEL: &str = "Nesting";

/// Name of the option that includes artifacts.
pub const ARTIFACTS_LABEL: &str = "Artifacts";

/// Name of the option that also outlines items that cannot be seen.
pub const INVISIBLE_LABEL: &str = "Invisible items";

const MENU_W: f32 = 300.0;
const ROW_H: f32 = 28.0;
/// Height of a checkbox option: the line of its hover wash.
const OPTION_H: f32 = 24.0;
const ICON: f32 = 14.0;
const SECTION_GAP: f32 = 10.0;

/// The menu's modes that are on, in menu order.
fn active_tools(state: &AppState) -> Vec<ViewTool> {
    ViewTool::PAGE
        .into_iter()
        .filter(|tool| tool.active(state))
        .collect()
}

/// The name on the button: the mode that is on (`X-ray`), `X-ray +1` when more are, or `View`
/// when none is. It is also the button's accessible name.
pub fn button_label(state: &AppState) -> String {
    match active_tools(state).as_slice() {
        [] => VIEW_LABEL.to_owned(),
        [only] => only.label().to_owned(),
        [first, rest @ ..] => format!("{} +{}", first.label(), rest.len()),
    }
}

/// The mode the toggle turns on or off: the first one that is on, else the X-ray.
fn target(state: &AppState) -> ViewTool {
    active_tools(state).first().copied().unwrap_or(ViewTool::Xray)
}

/// The toggle of the active mode: icon and name, or the icon alone when `collapsed`.
fn toggle_button<'a>(label: &'a str, state: &AppState, collapsed: bool) -> ToolButton<'a> {
    let active = active_tools(state);
    ToolButton::new(label)
        .icon(active.first().map_or(Icon::Eye, |tool| tool.icon()))
        .active(!active.is_empty())
        .compact(true)
        .icon_only(collapsed)
}

/// The ▾ beside the toggle.
fn options_button(state: &AppState) -> ToolButton<'static> {
    ToolButton::new(OPTIONS_LABEL)
        .icon(Icon::ChevronDown)
        .icon_only(true)
        .active(!active_tools(state).is_empty())
        .opens_popup()
}

/// The width [`button`] takes, so the toolbar can tell whether the name fits.
pub(super) fn width(ui: &Ui, state: &AppState, collapsed: bool) -> f32 {
    toggle_button(&button_label(state), state, collapsed).width(ui) + options_button(state).width(ui)
}

/// The toolbar control: the toggle of the active mode (the X-ray while none is on) with a ▾ beside
/// it that opens the menu. `collapsed` drops the name when the toolbar is short of room: the icon
/// and the tooltip still say which mode it is.
pub(super) fn button(ui: &mut Ui, state: &mut AppState, collapsed: bool) {
    let label = button_label(state);
    let mode = target(state);
    let (toggle, options) = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let toggle = toggle_button(&label, state, collapsed).show(ui);
            let options = options_button(state).show(ui);
            (toggle, options)
        })
        .inner;
    let toggle = tooltip::named(
        toggle,
        mode.label(),
        mode.hint(),
        Some(mode.key().symbol_or_name()),
    );
    if toggle.clicked() {
        mode.toggle(state);
    }
    let options = tooltip::named(
        options,
        OPTIONS_LABEL,
        "Render modes: x-ray, paint order, clip, alpha",
        None,
    );
    let around = toggle.rect.union(options.rect);
    popover::show_beside(&options, around, RectAlign::BOTTOM_START, MENU_W, None, |ui| {
        menu(ui, state);
    });
}

/// The content of the menu.
fn menu(ui: &mut Ui, state: &mut AppState) {
    ui.spacing_mut().item_spacing.y = 0.0;
    for tool in ViewTool::PAGE {
        if tool_row(ui, state, tool) {
            tool.toggle(state);
        }
    }
    ui.add_space(SECTION_GAP);
    header(ui, COLOUR_BY_TITLE);
    let view = &mut state.canvas.view;
    let segments = COLOR_MODES.map(|by| Segment::new(by, color_title(by)));
    segmented(ui, &mut view.color_by, &segments);
    ui.add_space(SECTION_GAP);
    header(ui, SHOW_TITLE);
    let show = &mut view.show;
    option(ui, &mut show.all_boxes, ALL_BOXES_LABEL, Some("⇧T"));
    option(ui, &mut show.labels, LABELS_LABEL, Some("L"));
    option(ui, &mut show.reading_order, READING_ORDER_LABEL, Some("R"));
    option(ui, &mut show.nesting, NESTING_LABEL, None);
    option(ui, &mut show.artifacts, ARTIFACTS_LABEL, None);
    option(ui, &mut view.invisible, INVISIBLE_LABEL, None);
    ui.add_space(SECTION_GAP);
    header(ui, ORDER_FROM_TITLE);
    let sources = OrderSource::ALL.map(|source| Segment::new(source, source.title()));
    let from_changed = segmented(ui, &mut view.order_from, &sources);
    // Reading the order as a screen reader announces it: the Speech tab says which reader.
    if from_changed && view.order_from == OrderSource::Speech {
        state.drawer.show_tab(DrawerTab::Speech);
    }
}

fn header(ui: &mut Ui, text: &str) {
    ui.label(fonts::section_header(text));
    ui.add_space(4.0);
}

/// A checkbox option with the key that toggles it, if it has one, right after the name. The whole
/// line is the hover and click target, with the fills of the mode rows above it.
fn option(ui: &mut Ui, on: &mut bool, label: &str, key: Option<&str>) {
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), OPTION_H), Sense::hover());
    // The wash goes under the checkbox, which is laid out after it.
    let wash = ui.painter().add(Shape::Noop);
    let mut content = ui.new_child(
        UiBuilder::new()
            .max_rect(line)
            .layout(Layout::left_to_right(Align::Center)),
    );
    content.checkbox(on, label);
    if let Some(key) = key {
        Keycap::new(key).quiet().show(&mut content);
    }
    // Not focusable: the checkbox is the one that takes the keyboard.
    let target = ui.interact(line, ui.make_persistent_id(("option", label)), Sense::CLICK);
    let fill = if target.is_pointer_button_down_on() {
        tokens::NEUTRAL_ACTIVE
    } else if target.hovered() {
        tokens::NEUTRAL_HOVER
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter()
        .set(wash, Shape::rect_filled(line, tokens::RADIUS_SM, fill));
    if target.clicked() {
        *on = !*on;
    }
    target.on_hover_cursor(CursorIcon::PointingHand);
}

/// One mode of the menu: icon, name and its key beside the name; the one that is on has the
/// neutral active fill. True when it was clicked and can be toggled.
fn tool_row(ui: &mut Ui, state: &AppState, tool: ViewTool) -> bool {
    let on = tool.active(state);
    let enabled = tool.available(state);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let label = tool.label();
    response.widget_info(|| WidgetInfo::selected(Role::MenuItem, enabled, on, label));
    let (fill, ink) = match (enabled, on, response.hovered()) {
        (false, _, _) => (egui::Color32::TRANSPARENT, tokens::TEXT_3),
        (true, true, _) => (tokens::NEUTRAL_ACTIVE, tokens::ACTIVE_INK),
        (true, false, true) => (tokens::NEUTRAL_HOVER, tokens::TEXT),
        (true, false, false) => (egui::Color32::TRANSPARENT, tokens::TEXT),
    };
    ui.painter().rect_filled(rect, tokens::RADIUS_SM, fill);
    focus::ring(ui, &response, rect, tokens::RADIUS_SM);
    let y = rect.center().y;
    tool.icon().paint(
        ui,
        Rect::from_center_size(pos2(rect.min.x + 14.0, y), vec2(ICON, ICON)),
        ICON,
        ink,
    );
    let name = ui
        .painter()
        .layout_no_wrap(label.to_owned(), fonts::medium(tokens::text::BODY), ink);
    let name_x = rect.min.x + 30.0;
    let after = name_x + name.size().x + 8.0;
    ui.painter().galley(
        Align2::LEFT_CENTER.anchor_size(pos2(name_x, y), name.size()).min,
        name,
        ink,
    );
    Keycap::new(tool.key().symbol_or_name())
        .quiet()
        .show_at(ui, after, y);
    let response = tooltip::text(response, tool.hint());
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand).clicked()
    } else {
        false
    }
}
