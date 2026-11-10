//! The selection action bar: a small frosted bar under the selection's outline on the canvas with
//! the actions people reach for first, as icon buttons. The same actions are in the right-click
//! menu ([`crate::components::context_menu`]); both list [`Action::for_node`] and run
//! [`actions::run`].
//!
//! With two or more nodes selected the bar goes under all of them and acts on the whole
//! selection: `4 elements` · Isolate · Hide · Show in tree · Copy ids · ×
//! ([`SelectionAction`]).

use egui::{Align2, Area, Context, Id, Margin, Order, Rect, Ui, pos2};

use super::cards;
use crate::actions::{self, Action, SelectionAction, tree_of};
use crate::components::{ToolButton, tooltip};
use crate::modes::{ViewTool, elements_text};
use crate::state::{AppState, RailItem};
use crate::theme::{effects, fonts, tokens};

/// The actions of the bar, in order; the rest of [`Action::ALL`] is in the menu only.
pub const BAR_ACTIONS: [Action; 6] = [
    Action::Isolate,
    Action::Hide,
    Action::PaintOrder,
    Action::ShowInTree,
    Action::Object,
    Action::CopyId,
];

/// Space between the outline and the bar.
const GAP: f32 = 8.0;

/// What the action does, for its tooltip.
const fn detail(action: Action) -> &'static str {
    match action {
        Action::Isolate => "Paint only the selected node’s content",
        Action::Hide => "Paint everything except the selected node’s content",
        Action::PaintOrder => "Step to its first painting operator",
        Action::ShowInTree => "Select it in the tree that lists it",
        Action::Object => "Open the object behind it",
        Action::RawBytes => "Its bytes as they are in the file",
        Action::StreamBytes => "Its stream data, marked in the raw bytes",
        Action::SaveImage => "Write the image to a PNG file at its own size",
        Action::CopyId => "Copy the node’s id",
        Action::CopyJson => "Copy everything the inspector shows, as JSON",
    }
}

/// The bar's actions for the selection, or none when nothing is selected.
pub fn actions_for(state: &AppState) -> Vec<Action> {
    state.selection.primary().map_or_else(Vec::new, |id| {
        Action::for_node(state, id)
            .into_iter()
            .filter(|action| BAR_ACTIONS.contains(action))
            .collect()
    })
}

/// Draws the bar under `outline` (screen), kept inside `clip` and off the paint-order stepper, and
/// runs the action clicked. `label` is the tag drawn under the outline (the baseline's), which
/// the bar goes under instead of covering.
pub(super) fn show(ctx: &Context, state: &mut AppState, outline: Rect, label: Option<Rect>, clip: Rect) {
    // In Validation the failure badges sit around the selection and are what the user acts on;
    // the bar would cover them (right-click still has the same actions).
    if state.nav.rail == RailItem::Validation {
        return;
    }
    if state.selection.len() >= 2 {
        multi_bar(ctx, state, outline, clip);
        return;
    }
    let Some(node) = state.selection.primary().cloned() else {
        return;
    };
    let offered = actions_for(state);
    if offered.is_empty() {
        return;
    }
    let mut picked = None;
    let below = label.map_or(outline.max.y, |label| outline.max.y.max(label.max.y));
    Area::new(Id::unique("canvas-action-bar"))
        .order(Order::Middle)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(pos2(outline.center().x, below + GAP))
        .constrain_to(cards::above_stepper(ctx, clip))
        .show(ctx, |ui| {
            effects::on_page_bar()
                .inner_margin(Margin::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for action in &offered {
                            if button(ui, state, *action) {
                                picked = Some(*action);
                            }
                        }
                    });
                });
        });
    if let Some(action) = picked {
        actions::run(state, ctx, &node, action);
    }
}

/// One button; true when it was clicked. Isolate and Paint order keep their name beside the icon,
/// the others show only the icon and say their name in the tooltip.
fn button(ui: &mut Ui, state: &AppState, action: Action) -> bool {
    let tool = match action {
        Action::Isolate => Some(ViewTool::Isolate),
        Action::Hide => Some(ViewTool::Hide),
        _ => None,
    };
    let active = tool.is_some_and(|tool| tool.active(state));
    let enabled = tool.is_none_or(|tool| tool.available(state));
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            ToolButton::new(action.short_label())
                .name(action.label())
                .icon(action.icon())
                .icon_only(!matches!(action, Action::Isolate | Action::PaintOrder))
                .active(active)
                .compact(true)
                .show(ui)
        })
        .inner;
    let clicked = response.clicked();
    tooltip::named(response, action.label(), detail(action), action.shortcut());
    clicked
}

/// The actions of the bar for a selection of several nodes: Show in tree only when the primary
/// is listed in a tree.
pub fn selection_actions(state: &AppState) -> Vec<SelectionAction> {
    let listed = state.selection.primary().and_then(tree_of).is_some();
    SelectionAction::ALL
        .into_iter()
        .filter(|action| *action != SelectionAction::ShowInTree || listed)
        .collect()
}

/// The bar of a multi-selection, under `anchor` (the bounds of the selected nodes on screen).
fn multi_bar(ctx: &Context, state: &mut AppState, anchor: Rect, clip: Rect) {
    let offered = selection_actions(state);
    let count = elements_text(state.selection.len());
    let mut picked = None;
    Area::new(Id::unique("canvas-action-bar"))
        .order(Order::Middle)
        .pivot(Align2::CENTER_TOP)
        .fixed_pos(pos2(anchor.center().x, anchor.max.y + GAP))
        .constrain_to(cards::above_stepper(ctx, clip))
        .show(ctx, |ui| {
            effects::on_page_bar()
                .inner_margin(Margin::same(4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&count)
                                    .font(fonts::semibold(tokens::text::BODY))
                                    .color(tokens::TEXT),
                            )
                            .selectable(false),
                        );
                        ui.add_space(4.0);
                        ui.separator();
                        for action in &offered {
                            if selection_button(ui, state, *action) {
                                picked = Some(*action);
                            }
                        }
                    });
                });
        });
    if let Some(action) = picked {
        actions::run_on_selection(state, ctx, action);
    }
}

/// One button of the multi-selection bar; true when it was clicked. Every action keeps its name
/// beside its icon but the × that clears.
fn selection_button(ui: &mut Ui, state: &AppState, action: SelectionAction) -> bool {
    let tool = match action {
        SelectionAction::Isolate => Some(ViewTool::Isolate),
        SelectionAction::Hide => Some(ViewTool::Hide),
        _ => None,
    };
    let active = tool.is_some_and(|tool| tool.active(state));
    let enabled = tool.is_none_or(|tool| tool.available(state));
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            ToolButton::new(action.label())
                .icon(action.icon())
                .icon_only(action == SelectionAction::Clear)
                .active(active)
                .compact(true)
                .show(ui)
        })
        .inner;
    let clicked = response.clicked();
    tooltip::named(response, action.label(), action.detail(), action.shortcut());
    clicked
}
