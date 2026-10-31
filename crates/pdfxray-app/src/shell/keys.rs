//! Global shortcuts (the keyboard table of the design).
//!
//! ⌘N new window · ⌘O open · ⌘W close window (the macOS menu bar has these, so there the window
//! leaves them to it; ⌘Q is egui's own quit shortcut, or the menu's) · ⌘F navigator filter · ⌘1 to ⌘5 drawer tabs · ⌘[ and ⌘] object history (previous and
//! next finding on the Validation entry) · 1 to 6 the toolbar's Pick menu (layer mode) · X I H P C
//! A the View menu's modes · ⇧T L R its tag overlay options · M B tools · Esc: popover, then the
//! measured line, then the second selection, then the selection.

use egui::{Context, Key, Modifiers, Popup, Ui};

use crate::center::pick;
use crate::commands::MenuCommand;
use crate::modes::ViewTool;
use crate::state::{AppState, DrawerTab, RailItem};
use crate::states::Screen;

/// ⌘1 to ⌘5 pick the first to fifth tab of the drawer.
const DRAWER_KEYS: [Key; 5] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];

/// Applies this frame's shortcuts to `state`.
pub(super) fn handle(ui: &Ui, state: &mut AppState) {
    let ctx = ui.ctx();
    window_keys(ctx, state);
    // The rest belongs to the workspace: the drop zone, the opening card and the encrypted file's
    // explainer have no canvas to act on.
    if state.screen() != Screen::Workspace {
        return;
    }
    crate::stepper::tick(ctx, state);
    command_keys(ctx, state);
    if ctx.text_edit_focused() {
        return;
    }
    plain_keys(ctx, state);
}

/// The keys of the File menu. With a native menu bar the system gives them to its items, which
/// reach the same commands; handling them here as well would run each command twice.
fn window_keys(ctx: &Context, state: &mut AppState) {
    if state.native_menu {
        return;
    }
    for (key, command) in [
        (Key::N, MenuCommand::NewWindow),
        (Key::O, MenuCommand::Open),
        (Key::W, MenuCommand::CloseWindow),
    ] {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, key)) {
            command.apply(state, ctx);
        }
    }
}

fn command_keys(ctx: &Context, state: &mut AppState) {
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
        state.nav.focus_filter = true;
    }
    for (index, key) in DRAWER_KEYS.into_iter().enumerate() {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, key))
            && let Some(&tab) = DrawerTab::for_rail(state.nav.rail, state.canvas.view.stepper).get(index)
        {
            state.drawer.show_tab(tab);
        }
    }
    // The same two keys walk the object history, or the findings on the Validation entry.
    let findings = state.nav.rail == RailItem::Validation;
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::OpenBracket)) {
        if findings {
            state.step_finding(-1);
        } else {
            state.history_back();
        }
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::CloseBracket)) {
        if findings {
            state.step_finding(1);
        } else {
            state.history_forward();
        }
    }
}

fn plain_keys(ctx: &Context, state: &mut AppState) {
    let pressed = |key: Key| ctx.input(|i| i.key_pressed(key) && i.modifiers.is_none());
    for (mode, key, _) in pick::MODES {
        if pressed(key) && pick::available(mode) {
            state.canvas.layer_mode = mode;
        }
    }
    for tool in ViewTool::ALL {
        if pressed(tool.key()) {
            tool.toggle(state);
        }
    }
    // The tag overlay of the X-ray: ⇧T all tag boxes, L labels, R reading order.
    if ctx.input(|i| i.key_pressed(Key::T) && i.modifiers.shift_only()) {
        state.canvas.view.show.all_boxes = !state.canvas.view.show.all_boxes;
    }
    if pressed(Key::L) {
        state.canvas.view.show.labels = !state.canvas.view.show.labels;
    }
    if pressed(Key::R) {
        state.canvas.view.show.reading_order = !state.canvas.view.show.reading_order;
    }
    if pressed(Key::M) {
        state.canvas.measure.toggle();
    }
    if pressed(Key::B) {
        state.canvas.show_boxes = !state.canvas.show_boxes;
    }
    crate::stepper::keys(ctx, state);
    if pressed(Key::Escape) {
        escape(ctx, state);
    }
}

/// Esc closes the innermost thing: an open popover (egui closes it itself), the measured line,
/// the second selection, the selection.
fn escape(ctx: &Context, state: &mut AppState) {
    if Popup::is_any_open(ctx) {
        return;
    }
    if state.canvas.measure.line().is_some() {
        state.canvas.measure.clear();
    } else if state.selection.secondary.is_some() {
        state.selection.secondary = None;
    } else {
        state.clear_selection();
    }
}
