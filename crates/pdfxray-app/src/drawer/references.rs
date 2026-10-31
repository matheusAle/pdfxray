//! The References tab: what the selected object points to, and what points to it.

use egui::{
    Align, Color32, Label, Layout, Rect, RichText, Role, ScrollArea, Sense, Ui, UiBuilder, WidgetInfo, vec2,
};
use pdfxray_core::{NodeId, ObjRef, ResKind, UsedResource};

use super::table;
use crate::components::Icon;
use crate::object_facts::ObjectFacts;
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Shown when no object is selected.
pub const PICK_AN_OBJECT: &str = "Select an object to see what it points to and what points to it.";

/// Shown in place of the lists while an older revision is on show.
pub const NOT_PER_REVISION: &str = "References are only tracked for the newest revision.";

/// Width of the "Used by" column.
const USED_BY_W: f32 = 330.0;

/// The count on the tab: the objects that use this one plus the resources it uses.
pub fn count(state: &AppState) -> usize {
    state
        .object_facts()
        .map_or(0, |facts| facts.used_by.len() + facts.uses.len())
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(facts) = state.object_facts().cloned() else {
        ui.label(RichText::new(PICK_AN_OBJECT).color(tokens::TEXT_3));
        return;
    };
    if facts.read_only {
        ui.label(RichText::new(NOT_PER_REVISION).color(tokens::TEXT_3));
        return;
    }
    let mut target = None;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let height = ui.available_height();
        let top_down = Layout::top_down(Align::Min);
        ui.allocate_ui_with_layout(
            vec2(USED_BY_W.min(ui.available_width() * 0.45), height),
            top_down,
            |ui| {
                used_by(ui, &facts, &mut target);
            },
        );
        let (rule, _) = ui.allocate_exact_size(vec2(1.0, height), Sense::hover());
        ui.painter().vline(
            rule.center().x,
            rule.y_range(),
            egui::Stroke::new(1.0, tokens::BORDER),
        );
        ui.add_space(14.0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), height), top_down, |ui| {
            uses(ui, &facts, &mut target);
        });
    });
    if let Some(r) = target {
        state.select(NodeId::Object(r));
    }
}

fn heading(ui: &mut Ui, title: &str, count: usize, noun: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            RichText::new(title)
                .font(fonts::semibold(tokens::text::BODY))
                .color(tokens::TEXT_2),
        );
        ui.label(
            RichText::new(format!("{count} {noun}"))
                .font(fonts::mono(tokens::mono::SMALL))
                .color(tokens::TEXT_3),
        );
    });
}

fn used_by(ui: &mut Ui, facts: &ObjectFacts, target: &mut Option<ObjRef>) {
    let n = facts.used_by.len();
    heading(ui, "Used by", n, if n == 1 { "object" } else { "objects" });
    if n == 0 {
        ui.label(RichText::new("Nothing refers to this object.").color(tokens::TEXT_3));
        return;
    }
    ScrollArea::vertical()
        .id_salt("references-used-by")
        .auto_shrink(false)
        .show_rows(ui, tokens::ROW_H, n, |ui, range| {
            for index in range {
                let item = &facts.used_by[index];
                let reference = item.obj.to_string();
                let cells = [
                    Cell::label(&item.label, 62.0),
                    Cell::link(&reference, 70.0),
                    Cell::dim(&item.path),
                ];
                let id = ui.make_persistent_id(("used-by", index));
                if row(ui, id, Icon::File, tokens::TEXT_2, &reference, &cells) {
                    *target = Some(item.obj);
                }
            }
        });
}

fn uses(ui: &mut Ui, facts: &ObjectFacts, target: &mut Option<ObjRef>) {
    let n = facts.uses.len();
    heading(ui, "Resources it uses", n, if n == 1 { "name" } else { "names" });
    if n == 0 {
        ui.label(RichText::new("It selects no resources.").color(tokens::TEXT_3));
        return;
    }
    ScrollArea::vertical()
        .id_salt("references-uses")
        .auto_shrink(false)
        .show_rows(ui, tokens::ROW_H, n, |ui, range| {
            for index in range {
                let item = &facts.uses[index];
                let name = format!("/{}", item.name);
                let reference = item.obj.map(|r| r.to_string());
                let cells = [
                    Cell::label(&name, 52.0),
                    match &reference {
                        Some(text) => Cell::link(text, 70.0),
                        None => Cell::dim("inline"),
                    },
                    Cell::dim(&item.summary),
                ];
                let id = ui.make_persistent_id(("uses", index));
                let (icon, tint) = resource_icon(item);
                let name_for_access = reference.clone().unwrap_or_else(|| name.clone());
                if row(ui, id, icon, tint, &name_for_access, &cells)
                    && let Some(r) = item.obj
                {
                    *target = Some(r);
                }
            }
        });
}

/// The icon of a resource, by what the name selects.
fn resource_icon(item: &UsedResource) -> (Icon, Color32) {
    match item.kind {
        ResKind::Font => (Icon::Font, tokens::KIND_CONTENT),
        ResKind::XObject if item.summary.starts_with("image") => (Icon::Image, tokens::KIND_CONTENT),
        ResKind::XObject => (Icon::Content, tokens::KIND_CONTENT),
        ResKind::ColorSpace => (Icon::Palette, tokens::OK),
        ResKind::Pattern | ResKind::Shading => (Icon::Shapes, tokens::OK),
        ResKind::ExtGState => (Icon::Alpha, tokens::KIND_ANNOT),
        ResKind::Properties => (Icon::Hash, tokens::KIND_ANNOT),
    }
}

/// One cell of a row.
struct Cell<'a> {
    text: &'a str,
    width: Option<f32>,
    font: egui::FontId,
    color: Color32,
}

impl<'a> Cell<'a> {
    fn label(text: &'a str, width: f32) -> Self {
        Self {
            text,
            width: Some(width),
            font: fonts::ui(tokens::text::BODY),
            color: tokens::TEXT,
        }
    }

    fn link(text: &'a str, width: f32) -> Self {
        Self {
            text,
            width: Some(width),
            font: fonts::mono(tokens::mono::BODY),
            color: tokens::ACCENT,
        }
    }

    fn dim(text: &'a str) -> Self {
        Self {
            text,
            width: None,
            font: fonts::mono(tokens::mono::BODY),
            color: tokens::TEXT_3,
        }
    }
}

/// Draws a clickable row: an icon and cells. `name` is the accessible name of the link; returns
/// true when the row was clicked.
fn row(ui: &mut Ui, id: egui::Id, icon: Icon, tint: Color32, name: &str, cells: &[Cell<'_>]) -> bool {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, tokens::ROW_H), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Link, true, name));
    let response = table::row_background(ui, rect, response, false);
    icon.paint(
        ui,
        Rect::from_min_size(rect.min + vec2(6.0, 0.0), vec2(14.0, rect.height())),
        14.0,
        tint,
    );
    let mut x = rect.min.x + 28.0;
    for cell in cells {
        let w = cell.width.unwrap_or_else(|| (rect.max.x - x - 6.0).max(10.0));
        let cell_rect = Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(w, rect.height()));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(cell_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.add(
                    Label::new(RichText::new(cell.text).font(cell.font.clone()).color(cell.color))
                        .truncate()
                        .selectable(false),
                );
            },
        );
        x += w + 6.0;
    }
    response.clicked()
}
