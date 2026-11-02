//! The navigator: a header, the filter and the list of the active rail entry.
//!
//! Each entry has its own file; they share the tree body drawn here.

pub mod content;
pub mod objects;
pub mod pages;
pub mod revisions;
pub mod tags;
pub mod validation;

use egui::{Align, Frame, Id, Layout, Margin, RichText, TextEdit, Ui};
use pdfxray_core::NodeId;

use crate::components::{Icon, ToolButton, context_menu, tooltip};
use crate::state::{AppState, NavTree, RailItem};
use crate::theme::{fonts, tokens};

/// Name of the header button that collapses every row.
pub const COLLAPSE_ALL_LABEL: &str = "Collapse all";

/// Name of the header button that expands every row.
pub const EXPAND_ALL_LABEL: &str = "Expand all";

/// Shown in a list that has no rows.
pub const EMPTY_TREE: &str = "Nothing to show yet.";

/// Accessible name of the filter field; its hint is only an example of what it takes.
pub const FILTER_LABEL: &str = "Filter";

/// Name of the button under a filtered list that matches nothing: it empties the filter.
pub const CLEAR_FILTER_LABEL: &str = "Clear filter";

/// Id of the filter field, so the frame around it can tell when it has the focus.
const FILTER_ID: &str = "pdfxray-nav-filter";

/// What a list says when the filter matches none of its `things` (`rows`, `pages`): the query,
/// and the way back to the whole list. Returns true when the user asked for it.
fn no_match(ui: &mut Ui, filter: &str, things: &str) -> bool {
    ui.label(RichText::new(format!("No {things} match “{}”.", filter.trim())).color(tokens::TEXT_3));
    ui.add_space(4.0);
    clear_filter_button(ui)
}

/// The button that empties the filter; true when clicked.
fn clear_filter_button(ui: &mut Ui) -> bool {
    ToolButton::new(CLEAR_FILTER_LABEL)
        .icon(Icon::Close)
        .show(ui)
        .clicked()
}

/// [`no_match`], emptying the filter when the button is clicked.
fn no_match_in(ui: &mut Ui, state: &mut AppState, things: &str) {
    if no_match(ui, &state.nav.filter, things) {
        state.nav.filter.clear();
    }
}

/// Draws the navigator host.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    Frame::new()
        .inner_margin(Margin {
            left: 10,
            right: 10,
            top: 12,
            bottom: 0,
        })
        .show(ui, |ui| {
            header(ui, state);
            ui.add_space(8.0);
            filter(ui, state);
            ui.add_space(6.0);
            match state.nav.rail {
                RailItem::Tags => tags::ui(ui, state),
                RailItem::Content | RailItem::Artifacts => content::ui(ui, state),
                RailItem::Pages => pages::ui(ui, state),
                RailItem::Objects => objects::ui(ui, state),
                RailItem::Revisions => revisions::ui(ui, state),
                RailItem::Validation => validation::ui(ui, state),
            }
        });
}

/// `1284` as `1,284`.
pub(crate) fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The unit that follows the row count in the header.
const fn count_unit(rail: RailItem) -> &'static str {
    match rail {
        RailItem::Tags => "elems",
        RailItem::Content | RailItem::Artifacts | RailItem::Objects => "nodes",
        RailItem::Revisions => "revisions",
        RailItem::Pages | RailItem::Validation => "",
    }
}

/// What follows the title in the header: `1,284 elems`, `377 ops · 6 streams`.
fn header_stats(state: &mut AppState, tree: Option<NavTree>) -> Option<String> {
    match state.nav.rail {
        RailItem::Tags => {
            tags::count(state).map(|n| format!("{} {}", thousands(n), count_unit(RailItem::Tags)))
        }
        RailItem::Content | RailItem::Artifacts => content::stats_text(state),
        RailItem::Pages => pages::header_text(state),
        RailItem::Objects => objects::header_text(state),
        rail => {
            let count = state.tree_model(tree?).map(|model| model.len())?;
            Some(format!("{} {}", thousands(count), count_unit(rail)))
        }
    }
}

fn header(ui: &mut Ui, state: &mut AppState) {
    let rail = state.nav.rail;
    let tree = state.nav_tree();
    let summary = header_stats(state, tree);
    ui.horizontal(|ui| {
        ui.label(fonts::title(rail.heading(), tokens::text::TITLE, tokens::TEXT));
        if let Some(summary) = summary {
            ui.label(
                RichText::new(summary)
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
        }
        let Some(tree) = tree else {
            if rail == RailItem::Validation {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    validation::header_action(ui, state);
                });
            }
            return;
        };
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let expand = ToolButton::new(EXPAND_ALL_LABEL)
                .icon(Icon::ExpandAll)
                .icon_only(true)
                .show(ui);
            let collapse = ToolButton::new(COLLAPSE_ALL_LABEL)
                .icon(Icon::CollapseAll)
                .icon_only(true)
                .show(ui);
            let (expand_clicked, collapse_clicked) = (expand.clicked(), collapse.clicked());
            tooltip::text(expand, EXPAND_ALL_LABEL);
            tooltip::text(collapse, COLLAPSE_ALL_LABEL);
            if let Some(model) = state.tree_model(tree) {
                if expand_clicked {
                    model.expand_all();
                }
                if collapse_clicked {
                    model.collapse_all();
                }
            }
        });
    });
}

fn filter(ui: &mut Ui, state: &mut AppState) {
    let hint = state.nav.rail.filter_hint();
    let focus = std::mem::take(&mut state.nav.focus_filter);
    let id = Id::unique(FILTER_ID);
    // The accent outline says where typing goes.
    let focused = ui.memory(|m| m.has_focus(id));
    let border = if focused {
        tokens::ACCENT
    } else {
        tokens::BORDER_STRONG
    };
    Frame::new()
        .fill(tokens::BG)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(tokens::RADIUS_MD)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            // The field is as tall as a control (26 px with its margins and border), not as a
            // 28 px row.
            ui.spacing_mut().interact_size.y = tokens::CONTROL_H - 8.0;
            ui.horizontal(|ui| {
                Icon::Search.show(ui, 14.0, tokens::TEXT_3);
                let field = ui
                    .add(
                        TextEdit::singleline(&mut state.nav.filter)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY)
                            .hint_text(hint),
                    )
                    .accessible_name(FILTER_LABEL);
                if focus {
                    field.request_focus();
                }
            });
        });
}

/// The words the filter splits into.
fn filter_words(state: &AppState) -> Vec<String> {
    state.nav.filter.split_whitespace().map(str::to_owned).collect()
}

/// The list of `tree` for the active rail entry: rows, or `empty` when there are none. `extra`
/// words narrow the filter further, e.g. `artifact` for the Artifacts entry.
///
/// Besides listing, this is where every tree reacts to the user: a click selects, Shift-click
/// adds to the selection or takes a row out, a double click zooms the canvas to the node, arrow
/// keys move the selection (Shift+↑/↓ extend it, ⌘+Space toggles the focused row), and the row
/// under the pointer is previewed on the canvas. Every selected node's row is highlighted.
fn tree_body(ui: &mut Ui, state: &mut AppState, tree: NavTree, extra: &[&str], empty: &str) {
    let mut words = filter_words(state);
    words.extend(extra.iter().map(|w| (*w).to_owned()));
    let filtered = !state.nav.filter.trim().is_empty();
    let paths: Vec<Vec<NodeId>> = state
        .selection
        .nodes()
        .iter()
        .map(|selected| state.path_to(selected))
        .collect();
    let primary = state.selection.primary_index();
    let rail = state.nav.rail;
    let Some(model) = state.tree_model(tree) else {
        return;
    };
    model.set_filter(&words);
    if model.is_empty() {
        ui.label(RichText::new(empty).color(tokens::TEXT_3));
        return;
    }
    if model.visible_len() == 0 {
        if filtered {
            no_match_in(ui, state, "rows");
        } else {
            ui.label(RichText::new(empty).color(tokens::TEXT_3));
        }
        return;
    }
    // The row to highlight for each selected node: itself, or the nearest ancestor this tree has
    // a row for.
    let rows: Vec<Option<NodeId>> = paths
        .iter()
        .map(|path| path.iter().rev().find(|id| model.contains(id)).cloned())
        .collect();
    let highlight = primary.and_then(|at| rows.get(at).cloned().flatten());
    model.set_also_selected(
        rows.into_iter()
            .flatten()
            .filter(|row| Some(row) != highlight.as_ref())
            .collect(),
    );
    let output = model.ui(ui, highlight.as_ref(), (rail, tree));
    if let Some(id) = output.hovered {
        state.canvas.preview = Some(id);
    }
    if let Some((id, row)) = output.context {
        context_menu::attach(&row, state, &id);
    }
    if let Some(id) = output.key_selected {
        state.select(id);
    }
    if let Some(id) = output.key_extended {
        state.extend_selection(id);
    }
    if let Some(id) = output.toggled {
        state.toggle_selected(id);
    }
    if let Some(id) = output.clicked {
        select_with_modifiers(ui, state, id);
    }
    if let Some(id) = output.double_clicked {
        state.zoom_to_node(id);
    }
}

/// A plain click selects `id`; Shift-click adds it to the selection, or takes it out.
pub(crate) fn select_with_modifiers(ui: &Ui, state: &mut AppState, id: NodeId) {
    if ui.input(|i| i.modifiers.shift) {
        state.select_additional(id);
    } else {
        state.select(id);
    }
}
