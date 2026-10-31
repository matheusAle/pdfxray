//! Driving the View menu of the canvas toolbar the way a user does: open it from its button,
//! choose, close it again. Include it with `#[path = "common/view_tools.rs"] mod view_tools;`
//! next to `mod common;`.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_app::center::view_menu;

use crate::common::{Region, the};

/// The name of the View button now: the mode that is on, or `View`.
pub(crate) fn trigger(harness: &Harness<'_, PdfxrayApp>) -> String {
    view_menu::button_label(&harness.state().state)
}

/// Clicks the ▾ beside the View button, which opens and closes the menu.
fn click_trigger(harness: &mut Harness<'_, PdfxrayApp>) {
    the(harness, Region::Center, Role::Button, view_menu::OPTIONS_LABEL).click();
    harness.run_steps(4);
}

/// True while the menu is showing: its modes are menu items.
pub(crate) fn is_open(harness: &Harness<'_, PdfxrayApp>) -> bool {
    harness
        .query_all_by_role_and_label(Role::MenuItem, "X-ray")
        .next()
        .is_some()
}

/// Opens the menu from its ▾; does nothing when it is open.
pub(crate) fn open(harness: &mut Harness<'_, PdfxrayApp>) {
    if !is_open(harness) {
        click_trigger(harness);
    }
}

/// Closes the menu with a click on its ▾; does nothing when it is closed.
pub(crate) fn close(harness: &mut Harness<'_, PdfxrayApp>) {
    if is_open(harness) {
        click_trigger(harness);
    }
}

/// Opens the menu, runs `act` on it and closes it.
pub(crate) fn in_menu(harness: &mut Harness<'_, PdfxrayApp>, act: impl FnOnce(&mut Harness<'_, PdfxrayApp>)) {
    open(harness);
    act(harness);
    harness.run_steps(2);
    close(harness);
}

/// Toggles the mode named `label`: `X-ray`, `Paint order`, `Clip` and `Alpha` from the menu,
/// `Isolate` and `Hide` with their toolbar buttons.
pub(crate) fn toggle(harness: &mut Harness<'_, PdfxrayApp>, label: &str) {
    if ["Isolate", "Hide"].contains(&label) {
        toolbar_button(harness, label).click();
        harness.run_steps(2);
    } else {
        in_menu(harness, |h| the(h, Region::Center, Role::MenuItem, label).click());
    }
}

/// The toolbar button named `label`: the topmost one, as the selection's action bar on the page
/// can have the same name.
pub(crate) fn toolbar_button<'h>(
    harness: &'h Harness<'_, PdfxrayApp>,
    label: &'h str,
) -> egui_kittest::Node<'h> {
    harness
        .query_all_by_role_and_label(Role::Button, label)
        .min_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .unwrap_or_else(|| panic!("a toolbar button {label}"))
}

/// Picks the option of a segmented control of the menu (`Artifact`, `Paint order` under
/// "Reading order from").
pub(crate) fn pick(harness: &mut Harness<'_, PdfxrayApp>, label: &str) {
    in_menu(harness, |h| the(h, Region::Center, Role::Button, label).click());
}

/// Ticks or unticks the checkbox named `label` of the menu.
pub(crate) fn check(harness: &mut Harness<'_, PdfxrayApp>, label: &str) {
    in_menu(harness, |h| the(h, Region::Center, Role::CheckBox, label).click());
}
