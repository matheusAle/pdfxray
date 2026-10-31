//! The paint-order stepper: which operator the page is painted up to, how to move it (one
//! operator, to the next visible change, playing) and the keys that do so.
//!
//! The position is [`crate::ViewMode::step`], the index of the last operator that paints. The
//! count shown to the user is one more (`step` operators have run once operator `step` has), so
//! it agrees with [`pdfxray_core::PaintStep::step`].

use std::time::Duration;

use egui::{Context, Key, Modifiers};
use pdfxray_core::NodeId;

use crate::state::{AppState, DrawerTab};

/// How many operators playing advances per second.
pub const PLAY_RATE: u64 = 8;

/// The time one operator takes while playing.
const PLAY_STEP: Duration = Duration::from_millis(1000 / PLAY_RATE);

/// The longest frame playing counts, so a stall does not make it jump ahead.
const LONGEST_FRAME: Duration = Duration::from_millis(250);

/// Delay between repaints while playing.
const PLAY_REPAINT: Duration = Duration::from_millis(16);

impl AppState {
    /// Highest operator index of `page`, `None` when it has no captured operators.
    pub fn last_op(&self, page: pdfxray_core::PageIx) -> Option<u32> {
        self.document
            .as_ref()?
            .doc
            .captured(page)?
            .ops
            .last()
            .map(|op| op.ix)
    }

    /// Number of operators on the page on the canvas.
    pub fn op_total(&self) -> u32 {
        self.document
            .as_ref()
            .and_then(|loaded| loaded.doc.captured(self.canvas.page))
            .map_or(0, |captured| {
                u32::try_from(captured.ops.len()).unwrap_or(u32::MAX)
            })
    }

    /// Turns the paint-order stepper on or off. Turning it on starts at the selected operator of
    /// this page, or after the last one, selects that operator and shows the Paint order tab of
    /// the drawer; turning it off brings back the tab that was showing.
    pub fn set_stepper(&mut self, on: bool) {
        if !on {
            self.canvas.view.stepper = false;
            self.canvas.view.playing = false;
            if let Some(before) = self.drawer.before_stepper.take()
                && self.drawer.tab == DrawerTab::PaintOrder
            {
                self.drawer.tab = before;
            }
            return;
        }
        let Some(last) = self.last_op(self.canvas.page) else {
            return;
        };
        let start = match &self.selection.primary {
            Some(NodeId::Op { page, ix }) if *page == self.canvas.page => (*ix).min(last),
            _ => last,
        };
        if !self.canvas.view.stepper {
            self.drawer.before_stepper = Some(self.drawer.tab);
        }
        self.canvas.view.stepper = true;
        self.drawer.tab = DrawerTab::PaintOrder;
        self.step_to(start);
    }

    /// Paints operators up to and including `ix` and selects that operator.
    pub fn step_to(&mut self, ix: u32) {
        let Some(last) = self.last_op(self.canvas.page) else {
            return;
        };
        let ix = ix.min(last);
        self.canvas.view.step = ix;
        self.select(NodeId::Op {
            page: self.canvas.page,
            ix,
        });
    }

    /// Moves the stepper by `delta` operators, stopping at the first and last.
    pub fn step_by(&mut self, delta: i64) {
        let Some(last) = self.last_op(self.canvas.page) else {
            return;
        };
        let target = (i64::from(self.canvas.view.step) + delta).clamp(0, i64::from(last));
        self.step_to(u32::try_from(target).unwrap_or(last));
    }

    /// Keeps the stepper inside the operators of the page on the canvas and stops playing.
    pub(crate) fn clamp_step(&mut self) {
        let last = self.last_op(self.canvas.page).unwrap_or(0);
        self.canvas.view.step = self.canvas.view.step.min(last);
        self.canvas.view.playing = false;
    }

    /// The operator after (`forward`) or before the current one that an arrow key goes to: the
    /// next one, or with "Skip state-only ops" the next one that paints something. `None` at
    /// the end.
    pub fn next_target(&self, forward: bool) -> Option<u32> {
        let view = &self.canvas.view;
        if view.skip_state_only {
            return self.next_visible_change(forward);
        }
        if forward {
            let last = self.last_op(self.canvas.page)?;
            (view.step < last).then(|| view.step + 1)
        } else {
            view.step.checked_sub(1)
        }
    }

    /// The next operator after (`forward`) or before the current one that paints something,
    /// whatever "Skip state-only ops" says: where shift and an arrow key goes.
    pub fn next_visible_change(&self, forward: bool) -> Option<u32> {
        self.document
            .as_ref()?
            .doc
            .next_visible_change(self.canvas.page, self.canvas.view.step, forward)
    }

    /// Moves one operator forward or back as an arrow key does. False when there is nowhere to go.
    pub fn step_next(&mut self, forward: bool) -> bool {
        self.next_target(forward).map(|ix| self.step_to(ix)).is_some()
    }

    /// Jumps to the next (or previous) operator that paints something. False when there is none.
    pub fn jump_to_visible_change(&mut self, forward: bool) -> bool {
        self.next_visible_change(forward)
            .map(|ix| self.step_to(ix))
            .is_some()
    }

    /// Starts or pauses playing. Starting at the end plays from the first operator.
    pub fn toggle_play(&mut self) {
        if !self.canvas.view.stepper {
            return;
        }
        if self.canvas.view.playing {
            self.canvas.view.playing = false;
            return;
        }
        if self.next_target(true).is_none() {
            self.step_to(0);
        }
        self.canvas.view.play_carry = Duration::ZERO;
        self.canvas.view.playing = true;
    }

    /// Lets `elapsed` time pass for playing: advances [`PLAY_RATE`] operators per second and
    /// pauses at the end.
    pub fn advance_play(&mut self, elapsed: Duration) {
        if !self.canvas.view.playing {
            return;
        }
        self.canvas.view.play_carry += elapsed.min(LONGEST_FRAME);
        while self.canvas.view.play_carry >= PLAY_STEP {
            self.canvas.view.play_carry -= PLAY_STEP;
            if !self.step_next(true) {
                break;
            }
        }
        if self.next_target(true).is_none() {
            self.canvas.view.playing = false;
        }
    }
}

/// Moves playing on by the time of this frame and asks for the next one while it runs.
pub(crate) fn tick(ctx: &Context, state: &mut AppState) {
    if !state.canvas.view.playing {
        return;
    }
    let dt = ctx.input(|i| i.unstable_dt).max(0.0);
    state.advance_play(Duration::from_secs_f32(dt));
    if state.canvas.view.playing {
        ctx.request_repaint_after(PLAY_REPAINT);
    }
}

/// ← and → step, shift with either jumps to the next visible change, space plays and pauses,
/// while the stepper is on. The navigator tree keeps the arrows while the pointer is over it.
pub(crate) fn keys(ctx: &Context, state: &mut AppState) {
    if !state.canvas.view.stepper || over_navigator(ctx) {
        return;
    }
    let pressed = |modifiers, key| ctx.input_mut(|i| i.consume_key(modifiers, key));
    // Shift first: a key press matches a pattern without Shift too.
    if pressed(Modifiers::SHIFT, Key::ArrowLeft) {
        state.jump_to_visible_change(false);
    } else if pressed(Modifiers::SHIFT, Key::ArrowRight) {
        state.jump_to_visible_change(true);
    } else if pressed(Modifiers::NONE, Key::ArrowLeft) {
        state.step_next(false);
    } else if pressed(Modifiers::NONE, Key::ArrowRight) {
        state.step_next(true);
    }
    if pressed(Modifiers::NONE, Key::Space) {
        state.toggle_play();
    }
}

fn over_navigator(ctx: &Context) -> bool {
    ctx.pointer_hover_pos()
        .zip(crate::shell::layout::navigator_rect(ctx))
        .is_some_and(|(pos, navigator)| navigator.x_range().contains(pos.x))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::install_fixture;

    fn stepping(fixture: &str) -> AppState {
        let mut state = AppState::default();
        install_fixture(&mut state, fixture);
        state.set_stepper(true);
        state
    }

    #[test]
    fn the_stepper_starts_after_the_last_operator_and_takes_the_drawer_tab_while_on() {
        let mut state = AppState::default();
        install_fixture(&mut state, "graphics-state");
        state.drawer.tab = DrawerTab::Operators;

        state.set_stepper(true);

        assert_eq!(state.canvas.view.step, 79);
        assert_eq!(state.op_total(), 80);
        assert_eq!(state.drawer.tab, DrawerTab::PaintOrder);
        state.set_stepper(false);
        assert_eq!(state.drawer.tab, DrawerTab::Operators);
        assert!(!state.canvas.view.stepper);
    }

    #[test]
    fn skipping_state_only_operators_lands_on_the_ones_that_paint() {
        let mut state = stepping("graphics-state");
        state.step_to(0);
        assert_eq!(state.next_target(true), Some(1));

        state.canvas.view.skip_state_only = true;
        let mut landed = Vec::new();
        while state.step_next(true) {
            landed.push(state.canvas.view.step);
        }

        assert_eq!(landed[..3], [3, 12, 19]);
        let doc = &state.document.as_ref().unwrap().doc;
        assert!(landed.iter().all(|ix| !doc.is_state_only(0, *ix)), "{landed:?}");
        // Nothing paints before op 3, so there is nowhere to go back to.
        state.step_to(3);
        assert!(!state.step_next(false));
        assert_eq!(state.canvas.view.step, 3);
    }

    #[test]
    fn a_jump_goes_to_the_next_visible_change_whatever_the_switch_says() {
        let mut state = stepping("graphics-state");
        state.step_to(49);

        assert!(state.jump_to_visible_change(true));
        assert_eq!(state.canvas.view.step, 53);
        state.step_to(49);
        assert!(state.jump_to_visible_change(false));
        assert_eq!(state.canvas.view.step, 43);
        // The ordinary step still moves one operator.
        state.step_to(49);
        assert!(state.step_next(true));
        assert_eq!(state.canvas.view.step, 50);
    }

    #[test]
    fn playing_advances_eight_operators_a_second_and_stops_at_the_end() {
        let mut state = stepping("graphics-state");
        state.step_to(70);
        state.toggle_play();
        assert!(state.canvas.view.playing);

        state.advance_play(Duration::from_millis(125));
        assert_eq!(state.canvas.view.step, 71);
        // Short frames add up.
        state.advance_play(Duration::from_millis(60));
        assert_eq!(state.canvas.view.step, 71);
        state.advance_play(Duration::from_millis(70));
        assert_eq!(state.canvas.view.step, 72);
        // A stalled frame does not make it jump ahead.
        state.advance_play(Duration::from_secs(5));
        assert_eq!(state.canvas.view.step, 74);
        for expected in [76, 78, 79] {
            state.advance_play(Duration::from_secs(1));
            assert_eq!(state.canvas.view.step, expected);
        }
        assert!(!state.canvas.view.playing);

        // Playing again from the end starts over.
        state.toggle_play();
        assert_eq!(state.canvas.view.step, 0);
        assert!(state.canvas.view.playing);
        state.toggle_play();
        assert!(!state.canvas.view.playing);
        state.advance_play(Duration::from_secs(1));
        assert_eq!(state.canvas.view.step, 0);
    }

    #[test]
    fn playing_with_the_skip_switch_visits_only_painting_operators() {
        let mut state = stepping("graphics-state");
        state.canvas.view.skip_state_only = true;
        state.step_to(3);
        state.toggle_play();

        state.advance_play(Duration::from_millis(125));

        assert_eq!(state.canvas.view.step, 12);
    }

    #[test]
    fn changing_page_stops_playing() {
        let mut state = stepping("long-doc");
        state.step_to(0);
        state.toggle_play();
        assert!(state.canvas.view.playing);

        state.go_to_page(1);

        assert!(!state.canvas.view.playing);
    }
}
