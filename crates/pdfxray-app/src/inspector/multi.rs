//! The inspector while three or more nodes are selected: a summary of the selection (board
//! "2.12 Canvas · Marquee multi-selection"), and the bar that leads back to it once the user
//! stepped into one of them.
//!
//! The summary has a title (`4 elements`), the Pick target and pages, chips (roles, pages,
//! problems), the nodes grouped BY ROLE with an "Only P" action that narrows the selection, and
//! the ELEMENTS, one row each, that step into the normal inspector for that node.

use egui::{
    Align, Color32, CursorIcon, Frame, Layout, Margin, RichText, ScrollArea, Sense, Ui, WidgetInfo, vec2,
};
use pdfxray_core::{Document, NodeId};

use super::{INSET, header, note, problem, summary};
use crate::components::{Icon, Tone, ToolButton, focus, section, tooltip};
use crate::modes::elements_text;
use crate::roles::role_color;
use crate::selection::{node_page, role_label};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Most characters of an element's preview.
const PREVIEW_CHARS: usize = 40;

/// Most rows of the ELEMENTS list: ⌘A on a busy page selects hundreds of operators, and the
/// summary is drawn every frame.
const MAX_ROWS: usize = 200;

/// The note under the elements.
pub const FOOTNOTE: &str = "Click an element to inspect it. “← N elements” brings you back.";

/// Title of the roles section.
pub const BY_ROLE: &str = "BY ROLE";

/// Title of the elements section.
pub const ELEMENTS: &str = "ELEMENTS";

/// One role of the selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleGroup {
    /// The role, e.g. `P`.
    pub label: String,
    /// Its colour.
    pub color: Color32,
    /// How many selected nodes have it.
    pub count: usize,
}

/// One selected node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The node.
    pub id: NodeId,
    /// Its role, e.g. `Figure`.
    pub role: String,
    /// Its role's colour.
    pub color: Color32,
    /// Its text, or what is wrong with it.
    pub preview: String,
    /// The preview is a problem.
    pub warn: bool,
}

/// What the summary shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// `4 elements`.
    pub title: String,
    /// `Pick: Tags · page 1`.
    pub line: String,
    /// `3 roles`, `page 1`, `1 problem`.
    pub chips: Vec<summary::Fact>,
    /// The roles, in the order they first appear.
    pub roles: Vec<RoleGroup>,
    /// The nodes, in selection order.
    pub elements: Vec<Element>,
}

/// The colour of a node's role: an element's role class, else its kind.
fn color_of(doc: &Document, id: &NodeId) -> Color32 {
    doc.role_class(id)
        .map_or_else(|| tokens::kind_color(id), role_color)
}

/// `page 1`, `pages 1–3`: where the nodes are.
pub fn pages_text(pages: &[u32]) -> String {
    match (pages.iter().min(), pages.iter().max()) {
        (Some(first), Some(last)) if first == last => format!("page {}", first + 1),
        (Some(first), Some(last)) => format!("pages {}–{}", first + 1, last + 1),
        _ => "no page".to_owned(),
    }
}

/// `1 role`, `3 roles`; `1 problem`, `2 problems`.
fn counted(count: usize, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
    }
}

/// Reads the summary of the selection.
pub fn of(state: &mut AppState) -> Option<Summary> {
    let nodes = state.selection.nodes().to_vec();
    let mut problems = 0;
    let mut first_problem = Vec::with_capacity(nodes.len());
    for id in &nodes {
        let found = problem::of(state, id);
        problems += found.len();
        first_problem.push(found.into_iter().next().map(|p| p.title));
    }
    let doc = &state.document.as_ref()?.doc;
    let mut pages: Vec<u32> = nodes
        .iter()
        .filter_map(|id| node_page(id).or_else(|| doc.bbox(id).map(|(page, _)| page)))
        .collect();
    pages.sort_unstable();
    pages.dedup();
    let mut roles: Vec<RoleGroup> = Vec::new();
    let mut elements = Vec::with_capacity(nodes.len());
    for (id, problem) in nodes.iter().zip(first_problem) {
        let role = role_label(doc, id);
        let color = color_of(doc, id);
        match roles.iter_mut().find(|group| group.label == role) {
            Some(group) => group.count += 1,
            None => roles.push(RoleGroup {
                label: role.clone(),
                color,
                count: 1,
            }),
        }
        let text = doc
            .text_preview(id, PREVIEW_CHARS)
            .filter(|t| !t.trim().is_empty());
        let (preview, warn) = match (text, problem) {
            (Some(text), _) => (text, false),
            (None, Some(problem)) => (problem, true),
            (None, None) => (doc.node(id).map_or_else(|| id.to_string(), |p| p.title), false),
        };
        elements.push(Element {
            id: id.clone(),
            role,
            color,
            preview,
            warn,
        });
    }
    let mut chips = vec![
        summary::Fact {
            text: counted(roles.len(), "role"),
            tone: Tone::Neutral,
        },
        summary::Fact {
            text: pages_text(&pages),
            tone: Tone::Neutral,
        },
    ];
    if problems > 0 {
        chips.push(summary::Fact {
            text: counted(problems, "problem"),
            tone: Tone::Warn,
        });
    }
    Some(Summary {
        title: elements_text(nodes.len()),
        line: format!(
            "Pick: {} · {}",
            state.canvas.layer_mode.title(),
            pages_text(&pages)
        ),
        chips,
        roles,
        elements,
    })
}

/// What the user asked for in the summary.
#[derive(Debug, Default)]
struct Picked {
    only: Option<String>,
    inspect: Option<NodeId>,
}

/// The accessible name of an element row: `Inspect Figure 14 0 R`.
pub fn inspect_label(element: &Element) -> String {
    let at = match &element.id {
        NodeId::StructElem(object) | NodeId::Object(object) => object.to_string(),
        other => other.to_string(),
    };
    format!("Inspect {} {at}", element.role)
}

/// The name of the button that narrows the selection to one role: `Only P`.
pub fn only_label(role: &str) -> String {
    format!("Only {role}")
}

/// Draws the summary and applies what was clicked.
pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(summary) = of(state) else {
        return;
    };
    header::block(
        ui,
        |ui| {
            // The layers glyph, as in the design.
            Icon::Content.show(ui, 16.0, tokens::ACCENT);
        },
        &summary.title,
        &summary.line,
        None,
    );
    summary::show(ui, &summary.chips);
    let mut picked = Picked::default();
    ScrollArea::vertical()
        .id_salt("inspector-multi")
        .auto_shrink(false)
        .show(ui, |ui| {
            section::show(ui, "multi-by-role", BY_ROLE, |ui| {
                for group in &summary.roles {
                    role_row(ui, group, &mut picked);
                }
            });
            section::show(ui, "multi-elements", ELEMENTS, |ui| {
                for element in summary.elements.iter().take(MAX_ROWS) {
                    element_row(ui, element, &mut picked);
                }
                let more = summary.elements.len().saturating_sub(MAX_ROWS);
                if more > 0 {
                    note(
                        ui,
                        RichText::new(format!("and {more} more"))
                            .font(fonts::ui(tokens::text::SMALL))
                            .color(tokens::TEXT_3),
                    );
                }
            });
            note(
                ui,
                RichText::new(FOOTNOTE.replace('N', &summary.elements.len().to_string()))
                    .font(fonts::ui(tokens::text::SMALL))
                    .color(tokens::TEXT_3),
            );
        });
    if let Some(role) = picked.only {
        state.narrow_selection(&role);
    }
    if let Some(id) = picked.inspect {
        state.step_into(&id);
    }
}

/// Height of a role tag.
const TAG_H: f32 = 16.0;
/// Space between the tag, the count and "Only P" of a role row.
pub const ROLE_GROUP_GAP: f32 = 10.0;

/// A role as a small filled tag, e.g. `P` on blue.
fn role_tag(ui: &mut Ui, label: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        fonts::mono_medium(tokens::mono::SMALL),
        tokens::BG,
    );
    let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 8.0, TAG_H), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, color);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, tokens::BG);
}

/// One role: its tag, its count and the button that keeps only it, read as one group from the
/// tag on, [`ROLE_GROUP_GAP`] apart; the free space is on the right.
fn role_row(ui: &mut Ui, group: &RoleGroup, picked: &mut Picked) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), tokens::ROW_H),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(f32::from(INSET));
            // The gaps are between what is drawn: the frameless button keeps no side padding.
            ui.spacing_mut().item_spacing.x = ROLE_GROUP_GAP;
            ui.spacing_mut().button_padding.x = 0.0;
            role_tag(ui, &group.label, group.color);
            ui.label(
                RichText::new(group.count.to_string())
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_2),
            );
            let label = only_label(&group.label);
            if link(ui, &label).clicked() {
                picked.only = Some(group.label.clone());
            }
        },
    );
}

/// One selected node: its role, its preview and a chevron. Clicking it steps into the node.
fn element_row(ui: &mut Ui, element: &Element, picked: &mut Picked) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), tokens::ROW_H), Sense::click());
    let name = inspect_label(element);
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, &name));
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, tokens::RADIUS_SM, tokens::NEUTRAL_HOVER);
    }
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(f32::from(INSET), 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    role_tag(&mut row, &element.role, element.color);
    let ink = if element.warn { tokens::WARN } else { tokens::TEXT };
    row.add(
        egui::Label::new(
            RichText::new(&element.preview)
                .font(fonts::ui(tokens::text::BODY))
                .color(ink),
        )
        .truncate()
        .selectable(false),
    );
    Icon::ChevronRight.paint(
        ui,
        egui::Rect::from_min_size(rect.right_top() - vec2(24.0, 0.0), vec2(16.0, rect.height())),
        12.0,
        tokens::TEXT_3,
    );
    focus::row_ring(ui, &response, rect, tokens::RADIUS_SM);
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        picked.inspect = Some(element.id.clone());
    }
}

/// A frameless button in the accent: `Only P`, `← 4 elements`.
fn link(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(fonts::ui(tokens::text::BODY))
                .color(tokens::ACCENT),
        )
        .frame(false),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

/// Name of the button that goes back to the summary: `← 4 elements`.
pub fn back_label(count: usize) -> String {
    format!("← {}", elements_text(count))
}

/// Accessible name of the button to the previous selected node.
pub const PREVIOUS: &str = "Previous element ([)";

/// Accessible name of the button to the next selected node.
pub const NEXT: &str = "Next element (])";

/// `2 of 4`: where the primary is among the selected nodes.
pub fn position_text(at: usize, count: usize) -> String {
    format!("{} of {count}", at + 1)
}

/// The bar over the inspector of one node of a multi-selection: back to the summary, which one
/// of how many, previous and next.
pub(super) fn stepped_bar(ui: &mut Ui, state: &mut AppState) {
    let count = state.selection.len();
    let Some(at) = state.selection.primary_index() else {
        return;
    };
    let mut back = false;
    let mut step = None;
    Frame::new()
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let button = link(ui, &back_label(count));
                back = button.clicked();
                tooltip::text(button, "Back to the summary of the selection");
                ui.label(RichText::new("·").color(tokens::TEXT_3));
                ui.label(
                    RichText::new(position_text(at, count))
                        .font(fonts::mono(tokens::mono::SMALL))
                        .color(tokens::TEXT_3),
                );
                for (name, icon, forward) in [
                    (PREVIOUS, Icon::ChevronLeft, false),
                    (NEXT, Icon::ChevronRight, true),
                ] {
                    let button = ToolButton::new(name)
                        .icon(icon)
                        .icon_only(true)
                        .compact(true)
                        .show(ui);
                    if button.clicked() {
                        step = Some(forward);
                    }
                }
            });
        });
    if back {
        state.step_out();
    }
    if let Some(forward) = step {
        state.step_selection(forward);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn pages_and_counts_read_as_words() {
        assert_eq!(pages_text(&[0]), "page 1");
        assert_eq!(pages_text(&[0, 2]), "pages 1–3");
        assert_eq!(counted(1, "role"), "1 role");
        assert_eq!(counted(3, "problem"), "3 problems");
        assert_eq!(position_text(1, 4), "2 of 4");
        assert_eq!(back_label(4), "← 4 elements");
        // Smoke: no page at all says so.
        assert_eq!(pages_text(&[]), "no page");
    }
}
