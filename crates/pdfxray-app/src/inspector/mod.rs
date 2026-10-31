//! The inspector: breadcrumb, title, a strip of facts, the selection's problem, then the sections
//! of the selected node.
//!
//! There is no button footer: what the old buttons did (raw object, stream bytes, copy as JSON)
//! is in the right-click menu of the title and of every value, and on the canvas action bar. The
//! Validation entry keeps its Previous, Rule docs and Next finding bar.

pub mod defaults;
pub mod diagnostic_card;
pub mod font;
pub mod geometry;
pub mod glyph;
pub mod graphics;
pub mod header;
pub mod measure;
pub mod object;
pub mod paint_order;
pub mod print;
pub mod problem;
pub mod reading_order;
pub mod relevance;
pub mod structure;
pub mod summary;
pub mod table;
pub mod text_state;
pub mod validation;

use egui::{Frame, Margin, Panel, RichText, ScrollArea, Stroke, Ui};
use pdfxray_core::{NodeId, Props};

use crate::components::{context_menu, prop_row};
use crate::state::AppState;
use crate::theme::tokens;

/// Shown when nothing is selected.
pub const NOTHING_SELECTED: &str = "Nothing selected.";

/// Space between the left edge of the panel and its plain text (a note, an empty-state line):
/// the inset the property rows keep.
pub(super) const INSET: i8 = 8;

/// A line of plain text in the panel body, inset like the property rows around it.
pub(super) fn note(ui: &mut Ui, text: RichText) {
    Frame::new()
        .inner_margin(Margin::symmetric(INSET, 4))
        .show(ui, |ui| {
            ui.label(text);
        });
}

/// What the user asked for in the inspector this frame.
#[derive(Debug, Default)]
pub(crate) struct Action {
    /// Select this node (a crumb, a link).
    pub(crate) navigate: Option<NodeId>,
    /// Move the paint-order stepper to this operator.
    pub(crate) step_to: Option<u32>,
    /// Open this validation finding (the Details button of a problem card).
    pub(crate) details: Option<u32>,
}

/// Draws the inspector host for the current selection.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(selection) = state
        .selection
        .pinned
        .clone()
        .or_else(|| state.selection.primary.clone())
    else {
        Frame::new().inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.label(RichText::new(NOTHING_SELECTED).color(tokens::TEXT_3));
        });
        return;
    };
    // A right-click on a value of another panel is not ours.
    prop_row::take_context(ui.ctx());
    let mut action = Action::default();
    // The Validation entry shows the failed check instead of the node's properties.
    let finding = validation::finding_of(state, &selection);
    let props = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.node(&selection));

    if let Some(index) = finding {
        Panel::bottom("inspector_footer")
            .frame(Frame::new().inner_margin(Margin::symmetric(10, 8)))
            .show(ui, |ui| validation::footer(ui, state, index));
    }

    // The reading order of a selected page; read here so the panels below only borrow the state.
    let extras = Extras {
        order: reading_order::Data::of(state, &selection),
        table: table::Data::of(state, &selection),
    };
    let comparing = state.selection.pinned.is_none() && state.selection.secondary.is_some();
    if comparing {
        measure::ui(ui, state, &mut action);
    } else if let Some(index) = finding {
        validation::ui(ui, state, index, &selection, &mut action);
    } else if let Some(facts) = state
        .object_facts()
        .filter(|facts| facts.object.is_some())
        .cloned()
    {
        object::ui(ui, state, &facts, &mut action);
    } else {
        header::crumbs(ui, state, &selection, &mut action);
        let heading = extras
            .order
            .as_ref()
            .filter(|_| state.canvas.view.show.reading_order)
            .zip(match selection {
                NodeId::Page(page) => Some(page),
                _ => None,
            })
            .map(|(data, page)| reading_order::heading(page, data));
        let title = header::title(ui, state, &selection, props.as_ref(), heading);
        if title.pin_pressed {
            state.selection.pinned = state.selection.pinned.is_none().then(|| selection.clone());
        }
        context_menu::attach(&title.area, state, &selection);
        summary::show(ui, &summary::of(state, &selection, props.as_ref()));
        let problems = problem::of(state, &selection);
        body(
            ui,
            state,
            &selection,
            props.as_ref(),
            &problems,
            &extras,
            &mut action,
        );
    }
    // The menu of a value that was right-clicked lists the actions of the selection.
    if let Some(value) = prop_row::take_context(ui.ctx()) {
        context_menu::attach(&value, state, &selection);
    }

    if let Some(id) = action.navigate {
        state.select(id);
    }
    if let Some(ix) = action.step_to {
        state.step_to(ix);
    }
    if let Some(finding) = action.details {
        problem::open(state, finding);
    }
}

/// What the sections that read more of the document than the node itself need, read before the
/// panels are drawn so they only borrow the state.
struct Extras {
    /// The reading order of a selected page.
    order: Option<reading_order::Data>,
    /// The table that holds the selection.
    table: Option<table::Data>,
}

fn body(
    ui: &mut Ui,
    state: &AppState,
    selection: &NodeId,
    props: Option<&Props>,
    problems: &[problem::Problem],
    extras: &Extras,
    action: &mut Action,
) {
    let lengths = state.lengths();
    // A new node starts at the top, not where the last one was scrolled to.
    let last = ui.scope_id().with("inspector-last");
    let moved = ui.data_mut(|d| d.get_temp::<NodeId>(last).as_ref() != Some(selection));
    if moved {
        ui.data_mut(|d| d.insert_temp(last, selection.clone()));
    }
    let mut area = ScrollArea::vertical().id_salt("inspector").auto_shrink(false);
    if moved {
        area = area.vertical_scroll_offset(0.0);
    }
    let output = area.show(ui, |ui| {
        // Rows are told apart per node, so a value flashes when it changes under the same
        // node and not each time the selection moves.
        ui.push_id(selection, |ui| {
            action.details = problem::show(ui, problems);
            let Some(props) = props else {
                note(
                    ui,
                    RichText::new(format!("No properties are available for {selection} yet."))
                        .color(tokens::TEXT_3),
                );
                return;
            };
            if let Some(order) = &extras.order {
                reading_order::show(ui, state, order, action);
            }
            if let Some(table) = &extras.table {
                table::show(ui, table, action);
            }
            if structure::handles(selection) {
                structure::sections(ui, state, selection, props, action);
            } else {
                structure::props(ui, props, &lengths, state.nav.rail, action);
            }
            if props.groups.is_empty() && problems.is_empty() {
                note(
                    ui,
                    RichText::new("This node has no properties.").color(tokens::TEXT_3),
                );
            }
        });
    });
    // Once the body is scrolled, a hairline tells where it slides under the header stack.
    if output.state.clamped_offset().y > 0.5 {
        let top = output.inner_rect.top();
        ui.painter()
            .hline(output.inner_rect.x_range(), top, Stroke::new(1.0, tokens::BORDER));
    }
}
