//! The Tag roles card: one row per tag role with its colour and how many elements of it the page
//! has. A role the page does not use is dimmed.
//!
//! The card is shown while the x-ray colours by tag role or while the tag boxes are on. While the
//! tag boxes are on, its rows pick which roles the canvas focuses on (see
//! [`crate::state::role_focus`]): a click focuses a role, a click on a focused row releases it,
//! Shift-click adds or removes one. A row the page does not use cannot be picked. While roles are
//! focused the header says how many are shown and offers *Show all*.
//!
//! The rows are buttons: Tab reaches them, ↑ and ↓ move between them, Space clicks and Shift+Space
//! Shift-clicks. Each is named for screen readers, e.g. `H1–H6, 1 element, focused`.

use std::collections::BTreeSet;

use egui::{
    Align, Align2, Button, Color32, CursorIcon, EventFilter, Id, Key, Layout, Modifiers, Rect, RichText,
    Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2,
};
use pdfxray_core::{PageIx, RoleClass, TagBox};

use super::cards::{Card, Column, title};
use crate::roles::{class_name, role_color};
use crate::state::AppState;
use crate::theme::{fonts, tokens};
use pdfxray_render::ColorBy;

/// Title of the card.
pub const TITLE: &str = "Tag roles";

/// Name of the button that releases the focus.
pub const SHOW_ALL: &str = "Show all";

/// Height of a row.
const ROW_H: f32 = 20.0;

/// Side of a row's colour swatch.
const SWATCH: f32 = 10.0;

/// Corner radius of a row, and of its keyboard focus ring.
const ROW_RADIUS: f32 = 5.0;

/// Width of the accent bar of a focused row, and of the keyboard focus ring.
const BAR_W: f32 = 2.0;

/// One row of the legend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleRow {
    /// The role class.
    pub class: RoleClass,
    /// How many elements of it have a box on the page; artifacts count the artifact sequences.
    pub count: usize,
}

impl RoleRow {
    /// True for a role the page does not use: its row is dimmed.
    pub const fn dimmed(&self) -> bool {
        self.count == 0
    }
}

/// How a row looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowLook {
    /// Nothing is focused: a filled swatch.
    Normal,
    /// The role is focused: tinted, with the accent bar.
    Focused,
    /// Other roles are focused: a hollow swatch and greyer text.
    Unfocused,
    /// The page does not use the role: dimmed, and it cannot be picked.
    Empty,
}

impl RowLook {
    /// The look of `row` while `focus` is focused (empty: nothing is).
    pub fn of(row: &RoleRow, focus: &BTreeSet<RoleClass>) -> Self {
        if row.dimmed() {
            Self::Empty
        } else if focus.is_empty() {
            Self::Normal
        } else if focus.contains(&row.class) {
            Self::Focused
        } else {
            Self::Unfocused
        }
    }
}

/// The accessible name of a row: `H1–H6, 1 element, focused`, `P, 2 elements, out of focus`.
pub fn row_name(row: &RoleRow, look: RowLook) -> String {
    let state = match look {
        RowLook::Focused => ", focused",
        RowLook::Unfocused => ", out of focus",
        RowLook::Normal | RowLook::Empty => "",
    };
    format!(
        "{}, {} element{}{state}",
        class_name(row.class),
        row.count,
        if row.count == 1 { "" } else { "s" }
    )
}

/// The header's readout while roles are focused: `2 of 10 shown`.
pub fn shown_text(focused: usize, rows: usize) -> String {
    format!("{focused} of {rows} shown")
}

/// True when the card is shown for the current options.
pub fn shown(state: &AppState) -> bool {
    let view = &state.canvas.view;
    view.color_by == ColorBy::TagRole || view.show.all_boxes
}

/// The rows for a page with `boxes` and `artifacts` artifact sequences, in the order of the
/// design. A role that falls in no class of the design (`Other`) only has a row when the page
/// uses one.
pub fn rows_of(boxes: &[TagBox], artifacts: usize) -> Vec<RoleRow> {
    RoleClass::ALL
        .into_iter()
        .map(|class| {
            let count = match class {
                RoleClass::Artifact => artifacts + boxes.iter().filter(|b| b.class == class).count(),
                _ => boxes.iter().filter(|b| b.class == class).count(),
            };
            RoleRow { class, count }
        })
        .filter(|row| row.class != RoleClass::Other || row.count > 0)
        .collect()
}

/// The rows for `page`.
pub fn rows(state: &mut AppState, page: PageIx) -> Vec<RoleRow> {
    let boxes = state.tag_boxes(page);
    let artifacts = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.captured(page))
        .map_or(0, |captured| {
            captured
                .marked
                .iter()
                .filter(|seq| seq.artifact.is_some())
                .count()
        });
    rows_of(&boxes, artifacts)
}

/// What the user did in the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Picked {
    /// A click on the row of a role; `true` with Shift.
    Row(RoleClass, bool),
    /// *Show all*.
    ShowAll,
}

/// Adds the card to `column` when it is shown.
pub(super) fn show(column: &mut Column, state: &mut AppState) {
    if !shown(state) {
        return;
    }
    let rows = rows(state, state.canvas.page);
    let active = state.role_focus_active();
    let focus = if active {
        state.focused_roles()
    } else {
        BTreeSet::new()
    };
    let colors: Vec<Color32> = rows.iter().map(|row| role_color(row.class)).collect();
    let card = Card::new(TITLE)
        .dots(colors.iter().copied().take(4).collect())
        .wide();
    let mut picked = None;
    column.card(&card, |ui| {
        picked = header(ui, &rows, &focus);
        let ids: Vec<Id> = rows
            .iter()
            .map(|row| ui.make_persistent_id(("tag-role-row", row.class)))
            .collect();
        let mut move_to = None;
        for (at, (row, color)) in rows.iter().zip(&colors).enumerate() {
            let look = RowLook::of(row, &focus);
            let response = role_row(ui, ids[at], row, *color, look, active);
            if response.clicked() {
                let shift = ui.input(|i| i.modifiers.shift);
                picked = Some(Picked::Row(row.class, shift));
            }
            if response.has_focus() {
                move_to = arrow_target(ui, response.id, &rows, at);
            }
        }
        if let Some(at) = move_to {
            ui.memory_mut(|memory| memory.request_focus(ids[at]));
        }
    });
    match picked {
        Some(Picked::Row(class, add)) => state.click_role(class, add),
        Some(Picked::ShowAll) => state.show_all_roles(),
        None => {}
    }
}

/// The title, and while roles are focused how many are shown and *Show all*.
fn header(ui: &mut Ui, rows: &[RoleRow], focus: &BTreeSet<RoleClass>) -> Option<Picked> {
    let mut picked = None;
    ui.horizontal(|ui| {
        title(ui, TITLE);
        if focus.is_empty() {
            return;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let show_all = ui.add(
                Button::new(
                    RichText::new(SHOW_ALL)
                        .font(fonts::medium(tokens::text::SMALL))
                        .color(tokens::ACCENT),
                )
                .frame(false),
            );
            if show_all.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                picked = Some(Picked::ShowAll);
            }
            let focused = rows.iter().filter(|row| focus.contains(&row.class)).count();
            ui.label(
                RichText::new(shown_text(focused, rows.len()))
                    .font(fonts::mono(tokens::mono::TINY))
                    .color(tokens::TEXT_2),
            );
        });
    });
    picked
}

/// While the row at `at` has the keyboard focus: ↑ and ↓ go to the row before or after it that
/// can be picked. egui leaves the vertical arrows to the row.
fn arrow_target(ui: &Ui, id: Id, rows: &[RoleRow], at: usize) -> Option<usize> {
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            EventFilter {
                vertical_arrows: true,
                ..EventFilter::default()
            },
        );
    });
    let pickable = |n: &usize| !rows[*n].dimmed();
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
        return (at + 1..rows.len()).find(pickable);
    }
    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
        return (0..at).rev().find(pickable);
    }
    None
}

/// One row: swatch, name and count. While `active` it is a button, unless the page does not use
/// the role.
fn role_row(
    ui: &mut Ui,
    id: Id,
    row: &RoleRow,
    color: Color32,
    look: RowLook,
    active: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    let pickable = active && look != RowLook::Empty;
    let sense = if pickable { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, id, sense);
    let response = if pickable {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };
    let name = row_name(row, look);
    response.widget_info(|| {
        if active {
            WidgetInfo::selected(Role::Button, pickable, look == RowLook::Focused, name.clone())
        } else {
            WidgetInfo::labeled(Role::Label, look != RowLook::Empty, name.clone())
        }
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let painter = ui.painter();
    if look == RowLook::Focused {
        painter.rect_filled(rect, ROW_RADIUS, tokens::ROLE_FOCUS);
        let bar = Rect::from_min_size(
            pos2(rect.min.x + 1.0, rect.min.y + 3.0),
            vec2(BAR_W, rect.height() - 6.0),
        );
        painter.rect_filled(bar, 1.0, tokens::ACCENT);
    }
    if pickable && response.is_pointer_button_down_on() {
        painter.rect_filled(rect, ROW_RADIUS, tokens::NEUTRAL_ACTIVE);
    } else if pickable && response.hovered() {
        painter.rect_filled(rect, ROW_RADIUS, tokens::NEUTRAL_HOVER);
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            ROW_RADIUS,
            Stroke::new(BAR_W, tokens::ACCENT),
            StrokeKind::Inside,
        );
    }
    let swatch = Rect::from_center_size(
        pos2(rect.min.x + 10.0 + SWATCH / 2.0, rect.center().y),
        vec2(SWATCH, SWATCH),
    );
    // Out of focus or unused, the swatch is hollow: the state is not colour alone.
    match look {
        RowLook::Normal | RowLook::Focused => {
            painter.rect_filled(swatch, 2.0, color);
        }
        RowLook::Unfocused | RowLook::Empty => {
            let strength = if look == RowLook::Unfocused { 0.5 } else { 0.45 };
            painter.rect_stroke(
                swatch,
                2.0,
                Stroke::new(1.5, color.gamma_multiply(strength)),
                StrokeKind::Inside,
            );
        }
    }
    let (font, ink) = match look {
        RowLook::Normal => (fonts::ui(tokens::text::BODY), tokens::ROLE_ROW_TEXT),
        RowLook::Focused => (fonts::semibold(tokens::text::BODY), tokens::TEXT),
        RowLook::Unfocused => (fonts::ui(tokens::text::BODY), tokens::ROLE_UNFOCUSED_TEXT),
        RowLook::Empty => (fonts::ui(tokens::text::BODY), tokens::TEXT_3),
    };
    painter.text(
        pos2(swatch.max.x + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        class_name(row.class),
        font,
        ink,
    );
    let count_ink = match look {
        RowLook::Normal | RowLook::Focused => tokens::TEXT_3,
        RowLook::Unfocused | RowLook::Empty => tokens::TEXT_3.gamma_multiply(0.7),
    };
    painter.text(
        pos2(rect.max.x - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        row.count.to_string(),
        fonts::mono(tokens::mono::SMALL),
        count_ink,
    );
    response
}

#[cfg(test)]
mod tests {
    use pdfxray_core::{NodeId, Rect};
    use pretty_assertions::assert_eq;

    use super::*;

    fn tag(class: RoleClass) -> TagBox {
        TagBox {
            id: NodeId::Document,
            role: String::new(),
            standard: String::new(),
            class,
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            depth: 0,
        }
    }

    #[test]
    fn rows_count_each_class_and_keep_the_design_order() {
        let boxes = [
            tag(RoleClass::Paragraph),
            tag(RoleClass::Heading),
            tag(RoleClass::Paragraph),
            tag(RoleClass::Group),
        ];

        let rows = rows_of(&boxes, 3);

        let counts: Vec<(RoleClass, usize)> = rows.iter().map(|row| (row.class, row.count)).collect();
        assert_eq!(
            counts,
            [
                (RoleClass::Heading, 1),
                (RoleClass::Paragraph, 2),
                (RoleClass::Figure, 0),
                (RoleClass::Caption, 0),
                (RoleClass::Table, 0),
                (RoleClass::List, 0),
                (RoleClass::Link, 0),
                (RoleClass::Inline, 0),
                (RoleClass::Group, 1),
                (RoleClass::Artifact, 3),
            ]
        );
        assert_eq!(rows.iter().filter(|row| row.dimmed()).count(), 6);
    }

    #[test]
    fn other_has_a_row_only_when_the_page_uses_it() {
        assert!(
            rows_of(&[tag(RoleClass::Other)], 0)
                .iter()
                .any(|r| r.class == RoleClass::Other)
        );
        assert!(!rows_of(&[], 0).iter().any(|r| r.class == RoleClass::Other));
    }

    #[test]
    fn a_row_looks_and_is_named_for_the_focus() {
        let heading = RoleRow {
            class: RoleClass::Heading,
            count: 1,
        };
        let paragraph = RoleRow {
            class: RoleClass::Paragraph,
            count: 2,
        };
        let table = RoleRow {
            class: RoleClass::Table,
            count: 0,
        };
        let focus = BTreeSet::from([RoleClass::Heading]);

        assert_eq!(
            [&heading, &paragraph, &table].map(|row| RowLook::of(row, &focus)),
            [RowLook::Focused, RowLook::Unfocused, RowLook::Empty]
        );
        assert_eq!(RowLook::of(&paragraph, &BTreeSet::new()), RowLook::Normal);
        assert_eq!(row_name(&heading, RowLook::Focused), "H1–H6, 1 element, focused");
        assert_eq!(
            row_name(&paragraph, RowLook::Unfocused),
            "P, 2 elements, out of focus"
        );
        assert_eq!(row_name(&table, RowLook::Empty), "Table, 0 elements");
        assert_eq!(shown_text(2, 9), "2 of 9 shown");
    }
}
