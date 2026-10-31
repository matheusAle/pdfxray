//! What the pointer did on a page, applied to the state: clicks, drags, the measure tool, the
//! hit stack, and wheel or pinch zoom.

use egui::Ui;
use pdfxray_core::PageIx;

use super::PointerOutput;
use crate::state::AppState;

/// Turns what the pointer did on `page` into measure tool and hit stack updates. A page the user
/// acts on becomes the current page.
pub(super) fn apply_pointer(state: &mut AppState, page: PageIx, output: &PointerOutput) {
    if output.acted() {
        state.set_current_page(page);
    }
    if let Some(finding) = output.badge_clicked {
        state.select_finding(finding);
    }
    if let Some(id) = &output.jump_to
        && !state.canvas.measure.active
    {
        state.select(id.clone());
    } else if let Some(id) = &output.tag_clicked
        && !state.canvas.measure.active
    {
        state.select_on_canvas(id.clone());
    } else if state.canvas.measure.active {
        if let Some(point) = output.drag_started {
            state.canvas.measure.drag_start(page, point);
        }
        if let Some(point) = output.dragged {
            state.canvas.measure.drag_to(point, output.modifiers.shift);
        }
        if let Some(point) = output.clicked {
            state.canvas.measure.click(page, point, output.modifiers.shift);
        }
    } else if let Some(point) = output.clicked {
        if output.modifiers.alt {
            state.cycle_page_point(point);
        } else if output.modifiers.shift {
            state.shift_click_page_point(point);
        } else {
            state.click_page_point(point);
        }
    }
    state.update_hover(output.cursor.map(|cursor| (page, cursor.page)));
}

/// Ctrl/Cmd + scroll and pinch zoom while the pointer is over the canvas.
pub(super) fn apply_wheel_zoom(ui: &Ui, state: &mut AppState) {
    if !ui.rect_contains_pointer(ui.max_rect()) {
        return;
    }
    let delta = ui.input(egui::InputState::zoom_delta);
    if (delta - 1.0).abs() > f32::EPSILON {
        state.set_zoom(state.canvas.zoom * delta);
    }
}
