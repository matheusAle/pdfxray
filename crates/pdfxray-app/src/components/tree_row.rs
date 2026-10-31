//! One row of a tree: chevron, kind dot, label, a dim detail (a text preview, say) and, at the
//! right, annotations.
//!
//! The detail can change when the row is selected or pointed at, so the quiet text of a row
//! (`Annual Report 2025`) gives way to the object reference (`14 0 R`) only where it is wanted.
//!
//! A content reference row (`↳ MCID 2  p1`) is the quiet variant: it points into a content
//! stream rather than being a node to select, so it has no hover, click or focus.

use egui::{
    Align, Color32, EventFilter, Id, Label, Layout, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, WidgetInfo, vec2,
};

use super::chip::Tone;
use super::focus;
use super::icon::Icon;
use super::tooltip;
use crate::theme::{fonts, tokens};

/// Horizontal indent per level.
pub const INDENT: f32 = 14.0;
const CHEVRON: f32 = 14.0;
/// Width of the target around a chevron.
const CHEVRON_HIT: f32 = 20.0;
const PAD: f32 = 6.0;
const DOT: f32 = 6.0;
/// Side of the `↳` icon of a content reference row.
const REF_ICON: f32 = 11.0;

/// Tooltip of a content reference row.
pub const CONTENT_REF_TIP: &str = "Marked content reference: select its tag or open Content";

/// What sits at the right end of a row.
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// Dim mono text, e.g. `MCID 4` or `3.8 KB`.
    Text(String),
    /// A status tag, e.g. a diagnostic code: mono text tinted by `tone`.
    Tag(String, Tone),
    /// Mono text tinted by `tone` without brackets: `untagged`.
    Toned(String, Tone),
    /// A dot with an accessible name, e.g. the marker of a validation finding.
    Dot {
        /// Dot colour.
        color: Color32,
        /// Accessible name and tooltip.
        name: String,
    },
}

/// A tree row.
#[derive(Debug, Clone)]
#[must_use = "call `show`"]
pub struct TreeRow<'a> {
    depth: u32,
    label: &'a str,
    detail: Option<&'a str>,
    hot_detail: Option<&'a str>,
    hover_text: Option<&'a str>,
    mono_label: bool,
    dot: Option<Color32>,
    children: Option<bool>,
    selected: bool,
    content_ref: bool,
    annotations: &'a [Annotation],
}

/// What happened to a row.
#[derive(Debug)]
pub struct TreeRowOutput {
    /// The row itself: `clicked()` selects, `double_clicked()` zooms, `hovered()` previews. A
    /// content reference row never reports a click; it only reports hover.
    pub row: Response,
    /// The chevron was clicked.
    pub toggled: bool,
}

impl<'a> TreeRow<'a> {
    /// A row at `depth` showing `label`.
    pub const fn new(depth: u32, label: &'a str) -> Self {
        Self {
            depth,
            label,
            detail: None,
            hot_detail: None,
            hover_text: None,
            mono_label: false,
            dot: None,
            children: None,
            selected: false,
            content_ref: false,
            annotations: &[],
        }
    }

    /// The dim id after the label.
    pub const fn detail(mut self, detail: Option<&'a str>) -> Self {
        self.detail = detail;
        self
    }

    /// The dim text that takes the place of the detail while the row is selected or pointed at:
    /// the object reference, shown only where it is wanted.
    pub const fn detail_when_hot(mut self, detail: Option<&'a str>) -> Self {
        self.hot_detail = detail;
        self
    }

    /// A tooltip for the row, e.g. what a mapped role stands for.
    pub const fn hover_text(mut self, text: Option<&'a str>) -> Self {
        self.hover_text = text;
        self
    }

    /// Sets the label in semibold mono: operator names and object headers (`23 0 obj`).
    pub const fn mono_label(mut self) -> Self {
        self.mono_label = true;
        self
    }

    /// The kind dot colour.
    pub const fn dot(mut self, color: Color32) -> Self {
        self.dot = Some(color);
        self
    }

    /// Gives the row a chevron; `open` says which way it points.
    pub const fn expandable(mut self, open: bool) -> Self {
        self.children = Some(open);
        self
    }

    /// Draws the accent outline and fill.
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Makes this a content reference row: `↳` instead of the chevron, no kind dot, the label
    /// dim and in mono, no hover tint, not clickable or focusable, with an explanatory tooltip.
    pub const fn content_ref(mut self) -> Self {
        self.content_ref = true;
        self
    }

    /// Things to show at the right end.
    pub const fn annotations(mut self, annotations: &'a [Annotation]) -> Self {
        self.annotations = annotations;
        self
    }

    /// Lays the row out across the available width. `id` must be unique among the rows.
    pub fn show(self, ui: &mut Ui, id: Id) -> TreeRowOutput {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, tokens::ROW_H), Sense::hover());
        let sense = if self.content_ref {
            Sense::hover()
        } else {
            Sense::click()
        };
        let row = ui.interact(rect, id, sense);
        let row = if self.content_ref {
            row
        } else {
            row.on_hover_cursor(egui::CursorIcon::PointingHand)
        };
        if row.has_focus() {
            // The arrow keys move the selection, not the focus.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..EventFilter::default()
                    },
                );
            });
        }
        let label = self.label;
        let selected = self.selected;
        if !self.content_ref {
            row.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, label));
        }

        if ui.is_rect_visible(rect) {
            if self.selected {
                ui.painter().rect(
                    rect.shrink2(vec2(2.0, 0.5)),
                    tokens::RADIUS_SM,
                    tokens::ACCENT_DIM,
                    Stroke::new(1.0, tokens::ACCENT),
                    StrokeKind::Inside,
                );
            } else if row.hovered() && !self.content_ref {
                ui.painter().rect_filled(
                    rect.shrink2(vec2(2.0, 0.5)),
                    tokens::RADIUS_SM,
                    tokens::NEUTRAL_HOVER,
                );
            }
            // The row the keyboard is on, selected or not.
            focus::ring(ui, &row, rect.shrink2(vec2(2.0, 0.5)), tokens::RADIUS_SM);
        }

        let mut toggled = false;
        let mut x = rect.min.x + PAD + f32::from(u16::try_from(self.depth).unwrap_or(u16::MAX)) * INDENT;
        if self.content_ref {
            let slot = Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(CHEVRON, tokens::ROW_H));
            Icon::CornerDownRight.paint(ui, slot, REF_ICON, tokens::TEXT_3);
        } else if let Some(open) = self.children {
            let slot = Rect::from_min_size(egui::pos2(x, rect.min.y), vec2(CHEVRON, tokens::ROW_H));
            // The glyph is 12 px; the target around it is wider, so a click near it still toggles.
            let chevron = ui
                .interact(
                    Rect::from_center_size(slot.center(), vec2(CHEVRON_HIT, tokens::ROW_H)),
                    id.with("chevron"),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let verb = if open { "Collapse" } else { "Expand" };
            chevron.widget_info(|| WidgetInfo::labeled(Role::Button, true, format!("{verb} {label}")));
            // Chevrons are dimmed until pointed at.
            let ink = if chevron.hovered() {
                tokens::TEXT
            } else {
                tokens::TEXT_3.gamma_multiply(0.7)
            };
            let icon = if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            };
            icon.paint(ui, slot, 12.0, ink);
            toggled = chevron.clicked();
        }
        x += CHEVRON + 2.0;

        if let Some(color) = self.dot.filter(|_| !self.content_ref) {
            ui.painter()
                .circle_filled(egui::pos2(x + DOT / 2.0, rect.center().y), DOT / 2.0, color);
        }
        x += DOT + 6.0;

        // Annotations are laid out from the right edge inwards; the label gets what is left.
        let mut right = rect.max.x - PAD;
        for (index, annotation) in self.annotations.iter().enumerate().rev() {
            right = place_annotation(ui, id.with(index), rect, right, annotation);
        }

        let (font, ink) = if self.content_ref {
            (fonts::mono(tokens::mono::BODY), tokens::TEXT_3)
        } else if self.mono_label {
            (fonts::mono_semibold(tokens::mono::BODY), tokens::TEXT)
        } else {
            (fonts::ui(tokens::text::BODY), tokens::TEXT)
        };
        let label_w = ui
            .painter()
            .layout_no_wrap(self.label.to_owned(), font.clone(), ink)
            .size()
            .x
            + 2.0;
        let label_max = (right - x - 4.0).max(10.0);
        let label_rect = Rect::from_min_size(
            egui::pos2(x, rect.min.y),
            vec2(label_w.min(label_max), tokens::ROW_H),
        );
        put(
            ui,
            label_rect,
            Label::new(RichText::new(self.label).font(font).color(ink)).truncate(),
        );
        let hot = self.selected || row.hovered();
        let detail = if hot {
            self.hot_detail.or(self.detail)
        } else {
            self.detail
        };
        if let Some(detail) = detail {
            let from = label_rect.max.x + 6.0;
            if right - from > 12.0 {
                let detail_rect =
                    Rect::from_min_max(egui::pos2(from, rect.min.y), egui::pos2(right - 4.0, rect.max.y));
                put(
                    ui,
                    detail_rect,
                    Label::new(
                        RichText::new(detail)
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(tokens::TEXT_3),
                    )
                    .truncate(),
                );
            }
        }
        if self.content_ref {
            tooltip::text(row.clone(), CONTENT_REF_TIP);
        } else if let Some(text) = self.hover_text {
            tooltip::text(row.clone(), text);
        }
        TreeRowOutput { row, toggled }
    }
}

fn put(ui: &mut Ui, rect: Rect, label: Label) {
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(label);
        },
    );
}

/// Draws `annotation` ending at `right`; returns where the next one ends.
fn place_annotation(ui: &mut Ui, id: Id, row: Rect, right: f32, annotation: &Annotation) -> f32 {
    match annotation {
        Annotation::Dot { color, name } => {
            let rect = Rect::from_center_size(egui::pos2(right - 5.0, row.center().y), vec2(10.0, 10.0));
            let response = ui.interact(rect, id.with("annotation"), Sense::hover());
            response.widget_info(|| WidgetInfo::labeled(Role::Image, true, name.clone()));
            ui.painter().circle_filled(rect.center(), 4.0, *color);
            tooltip::text(response, name.clone());
            right - 14.0
        }
        Annotation::Text(text) => {
            let galley =
                ui.painter()
                    .layout_no_wrap(text.clone(), fonts::mono(tokens::mono::SMALL), tokens::TEXT_3);
            let w = galley.size().x;
            let rect = Rect::from_min_max(
                egui::pos2(right - w - 2.0, row.min.y),
                egui::pos2(right, row.max.y),
            );
            put(
                ui,
                rect,
                Label::new(
                    RichText::new(text.clone())
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                ),
            );
            right - w - 10.0
        }
        Annotation::Toned(text, tone) => {
            let (_, _, ink) = tone.colors();
            let galley = ui
                .painter()
                .layout_no_wrap(text.clone(), fonts::mono(tokens::mono::SMALL), ink);
            let w = galley.size().x;
            let rect = Rect::from_min_max(
                egui::pos2(right - w - 2.0, row.min.y),
                egui::pos2(right, row.max.y),
            );
            put(
                ui,
                rect,
                Label::new(
                    RichText::new(text.clone())
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(ink),
                ),
            );
            right - w - 10.0
        }
        Annotation::Tag(text, tone) => {
            // A warning mark and the word, in the tone of the problem.
            let (_, _, ink) = tone.colors();
            let font = fonts::mono(tokens::mono::SMALL);
            let w = ui
                .painter()
                .layout_no_wrap(text.clone(), font.clone(), ink)
                .size()
                .x;
            let mark = 11.0;
            let rect = Rect::from_min_max(
                egui::pos2(right - w - 2.0, row.min.y),
                egui::pos2(right, row.max.y),
            );
            let icon = Rect::from_center_size(
                egui::pos2(rect.min.x - 4.0 - mark / 2.0, row.center().y),
                vec2(mark, mark),
            );
            Icon::Warning.paint(ui, icon, mark, ink);
            put(
                ui,
                rect,
                Label::new(RichText::new(text.clone()).font(font).color(ink)),
            );
            icon.min.x - 8.0
        }
    }
}
