//! The paint-order stepper: which operator the page is painted up to, how to move it (one
//! operator, to the next visible change, to the start or end of a page, playing) and the keys
//! that do so.
//!
//! In a comparison the stepper walks one side, the comparison's active side
//! ([`AppState::stepped_side`]): that side's page is cut and the other drawn whole.
//!
//! The position is [`crate::ViewMode::step`], the last operator that paints, anywhere in the
//! document: the stepper walks the paint order of the whole document, page after page, and the
//! page it is on is cut at that operator while every other page is painted whole. The count
//! shown to the user is the operator's place in that order plus one ([`AppState::step_index`]
//! operators have run before it).

use std::time::Duration;

use egui::{Context, Key, Modifiers};
use pdfxray_core::diff::Side;
use pdfxray_core::{Document, NodeId, OpPosition, PageIx};
use pdfxray_render::ops_for;

use crate::selection::node_page;
use crate::state::canvas::Reveal;
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
    /// The side the stepper walks: the comparison's active side, A outside a comparison.
    pub fn stepped_side(&self) -> Side {
        self.diff.as_ref().map_or(Side::A, |diff| diff.active)
    }

    /// The document the stepper walks: [`Self::stepped_side`]'s.
    pub fn stepped_doc(&self) -> Option<&Document> {
        self.doc_of(self.stepped_side()).map(AsRef::as_ref)
    }

    /// Makes `side` the comparison's active side; a stepper that is on starts over on that
    /// side, at its page on the canvas. Nothing happens outside a comparison.
    pub fn set_stepped_side(&mut self, side: Side) {
        let Some(diff) = self.diff.as_mut() else {
            return;
        };
        if diff.active == side {
            return;
        }
        diff.active = side;
        if self.canvas.view.stepper
            && let Some(start) = self.stepper_start()
        {
            self.step_to(start);
        }
    }

    /// Highest operator index of `page`, `None` when it has no captured operators.
    pub fn last_op(&self, page: PageIx) -> Option<u32> {
        self.stepped_doc()?.captured(page)?.ops.last().map(|op| op.ix)
    }

    /// Number of operators the stepper walks: every page's, in paint order.
    pub fn op_total(&self) -> u32 {
        self.stepped_doc().map_or(0, Document::op_total)
    }

    /// The stepper's position in the paint order of the whole document (0 is the first
    /// operator), `None` without a document.
    pub fn step_index(&self) -> Option<u32> {
        self.stepped_doc()?.paint_order_index(self.canvas.view.step)
    }

    /// The last paint-order position, `None` when no page has operators.
    pub fn last_step_index(&self) -> Option<u32> {
        self.op_total().checked_sub(1)
    }

    /// Turns the paint-order stepper on or off. Turning it on starts at the selected operator,
    /// or after the last operator of the page on the canvas (of the document when that page has
    /// none), selects that operator and shows the Paint order tab of the drawer; turning it off
    /// brings back the tab that was showing.
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
        let Some(start) = self.stepper_start() else {
            return;
        };
        if !self.canvas.view.stepper {
            self.drawer.before_stepper = Some(self.drawer.tab);
        }
        self.canvas.view.stepper = true;
        self.drawer.tab = DrawerTab::PaintOrder;
        self.step_to(start);
    }

    /// Where turning the stepper on starts: see [`Self::set_stepper`]. In a comparison, the
    /// selected operator counts when it is of the stepped side, and the page is that side's
    /// page on the canvas.
    fn stepper_start(&self) -> Option<OpPosition> {
        let doc = self.stepped_doc()?;
        let side = self.stepped_side();
        if let Some(NodeId::Op { page, ix }) = self.selection.primary()
            && self.selection.side() == side
        {
            let selected = OpPosition::new(*page, *ix);
            if doc.paint_order_index(selected).is_some() {
                return Some(selected);
            }
        }
        let page = match self.diff.as_ref().and_then(crate::state::DiffSession::pages) {
            Some((a, b)) => match side {
                Side::A => a,
                Side::B => b,
            }
            .unwrap_or(0),
            None => self.canvas.page,
        };
        self.last_op(page)
            .map(|ix| OpPosition::new(page, ix))
            .or_else(|| doc.op_at(self.last_step_index()?))
    }

    /// Paints operators up to and including `at`, on its page, and selects that operator; the
    /// page becomes the current one. With "Follow step" the canvas scrolls what it paints into
    /// view (to its page first when that is out of view), keeping the zoom; otherwise the view
    /// stays. Nothing happens for an operator that does not exist.
    pub fn step_to(&mut self, at: OpPosition) {
        let exists = self
            .stepped_doc()
            .is_some_and(|doc| doc.paint_order_index(at).is_some());
        if !exists {
            return;
        }
        // A comparison shows the page pair that holds the step, and selects the operator on its
        // side.
        if let Some(diff) = self.diff.as_mut() {
            let side = diff.active;
            diff.show_page_of(side, at.page);
            self.canvas.view.step = at;
            self.select_in(side, at.into());
            return;
        }
        // The stepper keeps its own page: the canvas may scroll onto another one meanwhile.
        self.set_current_page(at.page);
        self.canvas.view.step = at;
        self.select_on_canvas(at.into());
        self.canvas.reveal = self.canvas.view.follow_step.then_some(Reveal::IntoView);
    }

    /// Moves the stepper to paint-order position `index` of the whole document, clamped to the
    /// last operator: what the track and the first and last buttons do.
    pub fn step_to_index(&mut self, index: u32) {
        let Some(last) = self.last_step_index() else {
            return;
        };
        let at = self.stepped_doc().and_then(|doc| doc.op_at(index.min(last)));
        if let Some(at) = at {
            self.step_to(at);
        }
    }

    /// Where *Go to paint order* takes the stepper for `id`, a node of A: the operator itself,
    /// or the first operator of the node's content that paints. `None` for a node that paints
    /// nothing, and while a comparison's stepper walks B.
    pub fn paint_order_start(&self, id: &NodeId) -> Option<OpPosition> {
        if self.stepped_side() != Side::A {
            return None;
        }
        let doc = &self.document.as_ref()?.doc;
        let target = self.target_of(id)?;
        if let NodeId::Op { page, ix } = target {
            let at = OpPosition::new(page, ix);
            return doc.paint_order_index(at).map(|_| at);
        }
        let page = node_page(&target).or_else(|| doc.bbox(&target).map(|(page, _)| page))?;
        // The selection's operators are known already: the action bar asks every frame.
        let cached = self
            .canvas
            .selected_ops
            .as_ref()
            .filter(|s| s.nodes.as_slice() == std::slice::from_ref(&target));
        let first = match cached {
            Some(selected) => selected
                .ops
                .iter()
                .copied()
                .find(|ix| !doc.is_state_only(page, *ix)),
            None => ops_for(doc, &target)
                .into_iter()
                .find(|ix| !doc.is_state_only(page, *ix)),
        };
        first.map(|ix| OpPosition::new(page, ix))
    }

    /// Turns the stepper on, if it is off, at the first painting operator of `id`
    /// ([`Self::paint_order_start`]). False when `id` paints nothing.
    pub fn go_to_paint_order(&mut self, id: &NodeId) -> bool {
        let Some(at) = self.paint_order_start(id) else {
            return false;
        };
        if !self.canvas.view.stepper {
            self.set_stepper(true);
        }
        self.step_to(at);
        true
    }

    /// Where *Page start* goes: the first operator of the stepper's page, or, already there,
    /// the first of the page before. `None` at the document's first operator.
    pub fn page_start_target(&self) -> Option<OpPosition> {
        let doc = self.stepped_doc()?;
        let at = self.canvas.view.step;
        let index = doc.paint_order_index(at)?;
        let start = doc.paint_order_index(OpPosition::new(at.page, 0))?;
        if index > start {
            return doc.op_at(start);
        }
        let before = doc.op_at(index.checked_sub(1)?)?;
        Some(OpPosition::new(before.page, 0))
    }

    /// Where *Page end* goes: the last operator of the stepper's page, or, already there, the
    /// last of the page after. `None` at the document's last operator.
    pub fn page_end_target(&self) -> Option<OpPosition> {
        let doc = self.stepped_doc()?;
        let at = self.canvas.view.step;
        let last = self.last_op(at.page)?;
        if at.ix < last {
            return Some(OpPosition::new(at.page, last));
        }
        let after = doc.op_at(doc.paint_order_index(at)?.checked_add(1)?)?;
        Some(OpPosition::new(after.page, self.last_op(after.page)?))
    }

    /// True when *Page start* (`forward` false) or *Page end* would leave the stepper's page.
    pub fn page_edge_crosses(&self, forward: bool) -> bool {
        let target = if forward {
            self.page_end_target()
        } else {
            self.page_start_target()
        };
        target.is_some_and(|at| at.page != self.canvas.view.step.page)
    }

    /// Moves to the start (`forward` false) or the end of the page, as the buttons and Home and
    /// End do. False when there is nowhere to go.
    pub fn step_to_page_edge(&mut self, forward: bool) -> bool {
        let target = if forward {
            self.page_end_target()
        } else {
            self.page_start_target()
        };
        target.map(|at| self.step_to(at)).is_some()
    }

    /// Moves the stepper by `delta` operators of the paint order, across pages, stopping at the
    /// first and last.
    pub fn step_by(&mut self, delta: i64) {
        let (Some(index), Some(last)) = (self.step_index(), self.last_step_index()) else {
            return;
        };
        let target = (i64::from(index) + delta).clamp(0, i64::from(last));
        self.step_to_index(u32::try_from(target).unwrap_or(last));
    }

    /// The operator after (`forward`) or before the current one that an arrow key goes to: the
    /// next one in the paint order (past the end of the page to the next page), or with "Skip
    /// state-only ops" the next one that paints something. `None` at the end.
    pub fn next_target(&self, forward: bool) -> Option<OpPosition> {
        if self.canvas.view.skip_state_only {
            return self.next_visible_change(forward);
        }
        let index = self.step_index()?;
        let target = if forward {
            index.checked_add(1)?
        } else {
            index.checked_sub(1)?
        };
        self.stepped_doc()?.op_at(target)
    }

    /// The next operator after (`forward`) or before the current one that paints something,
    /// on this page or the next (previous) ones, whatever "Skip state-only ops" says: where
    /// shift and an arrow key goes.
    pub fn next_visible_change(&self, forward: bool) -> Option<OpPosition> {
        self.stepped_doc()?
            .next_visible_change_in_document(self.canvas.view.step, forward)
    }

    /// Moves one operator forward or back as an arrow key does. False when there is nowhere to go.
    pub fn step_next(&mut self, forward: bool) -> bool {
        self.next_target(forward).map(|at| self.step_to(at)).is_some()
    }

    /// Jumps to the next (or previous) operator that paints something. False when there is none.
    pub fn jump_to_visible_change(&mut self, forward: bool) -> bool {
        self.next_visible_change(forward)
            .map(|at| self.step_to(at))
            .is_some()
    }

    /// Starts or pauses playing. Starting at the end plays from the document's first operator.
    pub fn toggle_play(&mut self) {
        if !self.canvas.view.stepper {
            return;
        }
        if self.canvas.view.playing {
            self.canvas.view.playing = false;
            return;
        }
        if self.next_target(true).is_none() {
            self.step_to_index(0);
        }
        self.canvas.view.play_carry = Duration::ZERO;
        self.canvas.view.playing = true;
    }

    /// Lets `elapsed` time pass for playing: advances [`PLAY_RATE`] operators per second, from
    /// page to page, and pauses at the end of the document.
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

/// ← and → step, shift with either jumps to the next visible change, Home and End go to the
/// start and end of the page, space plays and pauses, while the stepper is on. The navigator
/// tree keeps the arrows while the pointer is over it.
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
    if pressed(Modifiers::NONE, Key::Home) {
        state.step_to_page_edge(false);
    } else if pressed(Modifiers::NONE, Key::End) {
        state.step_to_page_edge(true);
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

        assert_eq!(state.canvas.view.step, OpPosition::new(0, 79));
        assert_eq!(state.op_total(), 80);
        assert_eq!(state.drawer.tab, DrawerTab::PaintOrder);
        state.set_stepper(false);
        assert_eq!(state.drawer.tab, DrawerTab::Operators);
        assert!(!state.canvas.view.stepper);
    }

    #[test]
    fn skipping_state_only_operators_lands_on_the_ones_that_paint() {
        let mut state = stepping("graphics-state");
        state.step_to(OpPosition::new(0, 0));
        assert_eq!(state.next_target(true), Some(OpPosition::new(0, 1)));

        state.canvas.view.skip_state_only = true;
        let mut landed = Vec::new();
        while state.step_next(true) {
            landed.push(state.canvas.view.step.ix);
        }

        assert_eq!(landed[..3], [3, 12, 19]);
        let doc = &state.document.as_ref().unwrap().doc;
        assert!(landed.iter().all(|ix| !doc.is_state_only(0, *ix)), "{landed:?}");
        // Nothing paints before op 3, so there is nowhere to go back to.
        state.step_to(OpPosition::new(0, 3));
        assert!(!state.step_next(false));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 3));
    }

    #[test]
    fn a_jump_goes_to_the_next_visible_change_whatever_the_switch_says() {
        let mut state = stepping("graphics-state");
        state.step_to(OpPosition::new(0, 49));

        assert!(state.jump_to_visible_change(true));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 53));
        state.step_to(OpPosition::new(0, 49));
        assert!(state.jump_to_visible_change(false));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 43));
        // The ordinary step still moves one operator.
        state.step_to(OpPosition::new(0, 49));
        assert!(state.step_next(true));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 50));
    }

    #[test]
    fn playing_advances_eight_operators_a_second_and_stops_at_the_end() {
        let mut state = stepping("graphics-state");
        state.step_to(OpPosition::new(0, 70));
        state.toggle_play();
        assert!(state.canvas.view.playing);

        state.advance_play(Duration::from_millis(125));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 71));
        // Short frames add up.
        state.advance_play(Duration::from_millis(60));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 71));
        state.advance_play(Duration::from_millis(70));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 72));
        // A stalled frame does not make it jump ahead.
        state.advance_play(Duration::from_secs(5));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 74));
        for expected in [76, 78, 79] {
            state.advance_play(Duration::from_secs(1));
            assert_eq!(state.canvas.view.step.ix, expected);
        }
        assert!(!state.canvas.view.playing);

        // Playing again from the end starts over.
        state.toggle_play();
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 0));
        assert!(state.canvas.view.playing);
        state.toggle_play();
        assert!(!state.canvas.view.playing);
        state.advance_play(Duration::from_secs(1));
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 0));
    }

    #[test]
    fn playing_with_the_skip_switch_visits_only_painting_operators() {
        let mut state = stepping("graphics-state");
        state.canvas.view.skip_state_only = true;
        state.step_to(OpPosition::new(0, 3));
        state.toggle_play();

        state.advance_play(Duration::from_millis(125));

        assert_eq!(state.canvas.view.step, OpPosition::new(0, 12));
    }

    #[test]
    fn the_stepper_walks_the_whole_document_page_after_page() {
        // long-doc: 40 pages of 28 operators, the four `Tj`s of a page at 4, 11, 18 and 25.
        let mut state = stepping("long-doc");
        state.step_to(OpPosition::new(0, 27));

        let crossed = state.step_next(true);
        let after = (state.canvas.view.step, state.canvas.page, state.step_index());
        state.step_next(false);
        let back = state.canvas.view.step;
        state.step_to_index(28 * 39 + 27);
        let at_end = (state.canvas.view.step, state.next_target(true));
        state.step_by(-28);
        let one_page_back = state.canvas.view.step;

        assert!(crossed);
        assert_eq!(
            after,
            (OpPosition::new(1, 0), 1, Some(28)),
            "past page 1's last operator comes page 2's first, and page 2 becomes current"
        );
        assert_eq!(back, OpPosition::new(0, 27));
        assert_eq!(state.op_total(), 40 * 28);
        assert_eq!(
            at_end,
            (OpPosition::new(39, 27), None),
            "the track ends at the last page"
        );
        assert_eq!(one_page_back, OpPosition::new(38, 27));
        // smoke: the stepper selects the operator it stopped at, on its page
        assert_eq!(state.selection.primary(), Some(&NodeId::Op { page: 38, ix: 27 }));
    }

    #[test]
    fn skipping_state_only_operators_and_jumps_cross_pages_too() {
        let mut state = stepping("long-doc");
        state.canvas.view.skip_state_only = true;
        state.step_to(OpPosition::new(0, 25));

        assert!(state.step_next(true));
        let skipped = state.canvas.view.step;
        assert!(state.jump_to_visible_change(false));
        let jumped_back = state.canvas.view.step;

        assert_eq!(
            skipped,
            OpPosition::new(1, 4),
            "page 2's first Tj, past page 1's ET and EMC"
        );
        assert_eq!(jumped_back, OpPosition::new(0, 25));
        // smoke: both stops paint
        let doc = &state.document.as_ref().unwrap().doc;
        assert!(!doc.is_state_only(skipped.page, skipped.ix));
    }

    #[test]
    fn playing_runs_on_into_the_next_page_and_scrolling_elsewhere_does_not_stop_it() {
        let mut state = stepping("long-doc");
        state.step_to(OpPosition::new(0, 26));
        state.toggle_play();

        state.advance_play(Duration::from_millis(250));
        let after_two = state.canvas.view.step;
        // The canvas scrolls onto another page, as the user (or the view) does.
        state.set_current_page(5);
        state.advance_play(Duration::from_millis(125));

        assert_eq!(after_two, OpPosition::new(1, 0));
        assert!(state.canvas.view.playing, "a page change no longer stops playing");
        assert_eq!(
            state.canvas.view.step,
            OpPosition::new(1, 1),
            "the stepper keeps its own page"
        );
        // smoke: the step's page is current again, so the canvas follows what is painted
        assert_eq!(state.canvas.page, 1);
    }

    #[test]
    fn go_to_paint_order_steps_to_the_first_operator_of_an_element_that_paints() {
        // reading-order: the Figure (struct 11) is MCID 2, a grey square filled by op 17 after
        // the `g` at 15 and the `re` at 16, which paint nothing.
        let mut state = AppState::default();
        install_fixture(&mut state, "reading-order");
        let figure = NodeId::StructElem(pdfxray_core::ObjRef::new(11, 0));

        assert!(state.go_to_paint_order(&figure));

        assert!(state.canvas.view.stepper);
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 17));
        assert_eq!(state.selection.primary(), Some(&NodeId::Op { page: 0, ix: 17 }));
        assert_eq!(state.drawer.tab, DrawerTab::PaintOrder);
        // An operator goes to itself, even one that paints nothing.
        assert_eq!(
            state.paint_order_start(&NodeId::Op { page: 0, ix: 15 }),
            Some(OpPosition::new(0, 15))
        );
        // Smoke: the document, which has no content of its own, paints nothing.
        assert_eq!(state.paint_order_start(&NodeId::Document), None);
        assert!(!state.go_to_paint_order(&NodeId::Document));
    }

    #[test]
    fn page_start_and_end_go_to_the_edges_of_the_page_then_to_the_next_page() {
        // long-doc: 40 pages of 28 operators.
        let mut state = stepping("long-doc");
        state.step_to(OpPosition::new(5, 11));

        let mut visited = Vec::new();
        for forward in [false, false, true, true, true] {
            let crosses = state.page_edge_crosses(forward);
            assert!(state.step_to_page_edge(forward));
            visited.push((state.canvas.view.step, crosses));
        }

        assert_eq!(
            visited,
            [
                (OpPosition::new(5, 0), false),
                (OpPosition::new(4, 0), true),
                (OpPosition::new(4, 27), false),
                (OpPosition::new(5, 27), true),
                (OpPosition::new(6, 27), true),
            ]
        );
        // The ends of the document have nowhere further to go.
        state.step_to(OpPosition::new(0, 0));
        assert_eq!(state.page_start_target(), None);
        assert!(!state.step_to_page_edge(false));
        state.step_to(OpPosition::new(39, 27));
        assert_eq!(state.page_end_target(), None);
        // Smoke: from the middle of the last page, Page end still goes to its end.
        state.step_to(OpPosition::new(39, 3));
        assert_eq!(state.page_end_target(), Some(OpPosition::new(39, 27)));
    }

    #[test]
    fn turning_the_stepper_on_starts_at_the_selected_operator_of_any_page() {
        let mut state = AppState::default();
        install_fixture(&mut state, "long-doc");
        state.select(NodeId::Op { page: 7, ix: 11 });

        state.set_stepper(true);

        assert_eq!(state.canvas.view.step, OpPosition::new(7, 11));
        assert_eq!(state.step_index(), Some(7 * 28 + 11));
        assert_eq!(state.canvas.page, 7);
    }

    #[test]
    fn playing_from_the_end_of_the_document_starts_over_on_its_first_page() {
        let mut state = stepping("long-doc");
        state.step_to_index(state.last_step_index().unwrap());
        let end = state.canvas.view.step;

        state.toggle_play();
        let restarted = (
            state.canvas.view.step,
            state.canvas.page,
            state.canvas.view.playing,
        );
        state.advance_play(Duration::from_millis(125));

        assert_eq!(end, OpPosition::new(39, 27));
        assert_eq!(
            restarted,
            (OpPosition::new(0, 0), 0, true),
            "from page 40 back to page 1"
        );
        assert_eq!(state.canvas.view.step, OpPosition::new(0, 1));
        // smoke: playing to the very end stops there, on the last page
        state.step_to(OpPosition::new(39, 26));
        state.advance_play(Duration::from_secs(1));
        assert_eq!(state.canvas.view.step, OpPosition::new(39, 27));
        assert!(!state.canvas.view.playing);
    }
}
