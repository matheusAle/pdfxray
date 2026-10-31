//! The Tag roles card: one row per tag role with its colour and how many elements of it the page
//! has. A role the page does not use is dimmed.
//!
//! The card is shown while the x-ray colours by tag role or while the tag boxes are on.

use egui::{Color32, Grid, Label, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use pdfxray_core::{PageIx, RoleClass, TagBox};

use super::cards::{Card, Column, title};
use crate::roles::{class_name, role_color};
use crate::state::AppState;
use crate::theme::{fonts, tokens};
use pdfxray_render::ColorBy;

/// Title of the card.
pub const TITLE: &str = "Tag roles";

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

/// Adds the card to `column` when it is shown.
pub(super) fn show(column: &mut Column, state: &mut AppState) {
    if !shown(state) {
        return;
    }
    let rows = rows(state, state.canvas.page);
    let colors: Vec<Color32> = rows.iter().map(|row| role_color(row.class)).collect();
    let card = Card::new(TITLE).dots(colors.iter().copied().take(4).collect());
    column.card(&card, |ui| {
        title(ui, TITLE);
        Grid::new("tag-roles-legend").spacing([10.0, 4.0]).show(ui, |ui| {
            for (row, color) in rows.iter().zip(&colors) {
                // A disabled label is dimmed, and says so to assistive technology.
                let on = !row.dimmed();
                swatch(ui, *color, row.dimmed());
                ui.add_enabled(
                    on,
                    Label::new(RichText::new(class_name(row.class)).color(if on {
                        tokens::TEXT
                    } else {
                        tokens::TEXT_3
                    })),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled(
                        on,
                        Label::new(
                            RichText::new(row.count.to_string())
                                .font(fonts::mono(tokens::mono::SMALL))
                                .color(tokens::TEXT_3),
                        ),
                    );
                });
                ui.end_row();
            }
        });
    });
}

/// A square outlined in the role colour, faint for a role the page does not use.
fn swatch(ui: &mut Ui, color: Color32, dimmed: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    let color = if dimmed { color.gamma_multiply(0.45) } else { color };
    ui.painter().rect(
        rect.shrink(1.0),
        2.0,
        color.gamma_multiply(0.18),
        Stroke::new(1.5, color),
        StrokeKind::Inside,
    );
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
}
