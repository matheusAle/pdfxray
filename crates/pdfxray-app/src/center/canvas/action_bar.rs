//! The selection action bar: a small frosted bar under the selection's outline on the canvas with
//! the actions people reach for first, as icon buttons. The same actions are in the right-click
//! menu ([`crate::components::context_menu`]); both list [`Action::for_node`] and run
//! [`actions::run`].

use egui::{Align2, Area, Context, Id, Margin, Order, Rect, Ui, pos2};

use super::cards;
use crate::actions::{self, Action};
use crate::components::{ToolButton, tooltip};
use crate::modes::ViewTool;
use crate::state::{AppState, RailItem};
use crate::theme::effects;

/// The actions of the bar, in order; the rest of [`Action::ALL`] is in the menu only.
pub const BAR_ACTIONS: [Action; 5] = [
    Action::Isolate,
    Action::Hide,
    Action::ShowInTree,
    Action::Object,
    Action::CopyId,
];

/// Space between the outline and the bar.
const GAP: f32 = 8.0;

/// What the action does, for its tooltip.
const fn detail(action: Action) -> &'static str {
    match action {
        Action::Isolate => "Paint only the selected node's content",
        Action::Hide => "Paint everything except the selected node's content",
        Action::ShowInTree => "Select it in the tree that lists it",
        Action::Object => "Open the object behind it",
        Action::RawBytes => "Its bytes as they are in the file",
        Action::StreamBytes => "Its stream data, marked in the raw bytes",
        Action::CopyId => "Copy the node's id",
        Action::CopyJson => "Copy everything the inspector shows, as JSON",
    }
}

/// The bar's actions for the selection, or none when nothing is selected.
pub fn actions_for(state: &AppState) -> Vec<Action> {
    state.selection.primary.as_ref().map_or_else(Vec::new, |id| {
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
    let Some(node) = state.selection.primary.clone() else {
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

/// One button; true when it was clicked. Isolate keeps its name beside the icon, the others show
/// only the icon and say their name in the tooltip.
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
            ToolButton::new(action.label())
                .icon(action.icon())
                .icon_only(action != Action::Isolate)
                .active(active)
                .compact(true)
                .show(ui)
        })
        .inner;
    let clicked = response.clicked();
    tooltip::named(response, action.label(), detail(action), action.shortcut());
    clicked
}
