//! The activity rail: the logo, then one icon per way into the file.

use egui::{Align, CursorIcon, Layout, Rect, RectAlign, RichText, Role, Sense, Ui, WidgetInfo, vec2};

use crate::components::{Icon, focus, popover, tooltip};
use crate::state::{AppState, DrawerTab, RailItem};
use crate::theme::{fonts, tokens};

const BUTTON: f32 = 36.0;
/// Size of the logo at the top of the rail.
const LOGO: f32 = 28.0;
/// Width of the hairline under the logo.
const LOGO_RULE_W: f32 = 24.0;
/// Space between the logo, its hairline and the first entry.
const LOGO_GAP: f32 = 4.0;

/// Width of the settings popover: its sentence fits on one line.
const SETTINGS_W: f32 = 272.0;

/// What hovering the rail's logo says.
fn logo_tooltip() -> String {
    format!("{} {}", crate::brand::NAME, crate::brand::VERSION)
}

fn icon_of(item: RailItem) -> Icon {
    match item {
        RailItem::Tags => Icon::Tags,
        RailItem::Content => Icon::Content,
        RailItem::Artifacts => Icon::Artifacts,
        RailItem::Pages => Icon::Pages,
        RailItem::Objects => Icon::Objects,
        RailItem::Revisions => Icon::Revisions,
        RailItem::Validation => Icon::Validation,
    }
}

/// The rail. While there is nothing to look into (no file, or one still opening) the entries are
/// shown but do nothing.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let has_document = state.screen().has_workspace();
    ui.add_space(10.0);
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        logo(ui);
        for item in RailItem::TOP {
            let active = state.nav.rail == item;
            let entry = button(
                ui,
                icon_of(item),
                item.title(),
                active,
                has_document,
                tokens::ACCENT,
            );
            if entry.clicked() && has_document {
                state.set_rail(item);
            }
        }
    });
    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 4.0;
        let settings = button(ui, Icon::Settings, "Settings", false, true, tokens::ACCENT);
        popover::show(&settings, RectAlign::RIGHT_END, SETTINGS_W, |ui| {
            ui.label(RichText::new("pdfxray").font(fonts::semibold(tokens::text::LARGE)));
            ui.label(RichText::new(concat!("version ", env!("CARGO_PKG_VERSION"))).color(tokens::TEXT_3));
            ui.add_space(4.0);
            ui.label("Reads PDFs only: it never changes your file.");
        });
        let validation = state.nav.rail == RailItem::Validation;
        let shield = button(
            ui,
            Icon::Validation,
            RailItem::Validation.title(),
            validation,
            has_document,
            tokens::ACCENT,
        );
        if shield.clicked() && has_document {
            state.set_rail(RailItem::Validation);
        }
        let diagnostics = state
            .document
            .as_ref()
            .map_or(0, |loaded| loaded.info.diagnostics.len());
        let ink = if diagnostics > 0 {
            tokens::WARN
        } else {
            tokens::TEXT_3
        };
        let warning = button(ui, Icon::Warning, "Diagnostics", false, has_document, ink);
        if warning.clicked() && has_document {
            state.drawer.show_tab(DrawerTab::Diagnostics);
            state.explainer = false;
        }
    });
}

/// The logo and a short hairline under it. Decorative: not a button; hovering names the app and
/// its version.
fn logo(ui: &mut Ui) {
    let logo = crate::brand::logo(ui, LOGO);
    tooltip::text_beside(logo, logo_tooltip());
    ui.add_space(LOGO_GAP);
    let (rule, _) = ui.allocate_exact_size(vec2(LOGO_RULE_W, 1.0), Sense::hover());
    ui.painter().hline(
        rule.x_range(),
        rule.center().y,
        egui::Stroke::new(1.0, tokens::HAIRLINE),
    );
    ui.add_space(LOGO_GAP);
}

/// One rail button: a rounded square with the icon, neutral and white when active. `accent` is
/// the ink of an entry that carries a status colour (the diagnostics warning).
fn button(
    ui: &mut Ui,
    icon: Icon,
    name: &str,
    active: bool,
    enabled: bool,
    accent: egui::Color32,
) -> egui::Response {
    let (cell, _) = ui.allocate_exact_size(vec2(tokens::RAIL_W, BUTTON), Sense::hover());
    let rect = Rect::from_center_size(cell.center(), vec2(BUTTON, BUTTON));
    let response = ui.interact(rect, ui.make_persistent_id(("rail", name)), Sense::click());
    // An entry that does nothing is not selected, whatever the rail remembers.
    let active = active && enabled;
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, active, name));
    let hovered = response.hovered() && enabled;
    // The active item is neutral (a white 8 % fill, white icon): the accent is for the selection.
    if active {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_LG, tokens::NEUTRAL_ACTIVE);
    } else if hovered {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_LG, tokens::NEUTRAL_HOVER);
    }
    focus::ring(ui, &response, rect, tokens::RADIUS_LG);
    let ink = if active {
        tokens::ACTIVE_INK
    } else if enabled && !hovered && accent != tokens::ACCENT {
        accent
    } else if hovered {
        tokens::TEXT
    } else {
        tokens::TEXT_2
    };
    icon.paint(ui, rect, 18.0, ink);
    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };
    tooltip::text_beside(response, name)
}
