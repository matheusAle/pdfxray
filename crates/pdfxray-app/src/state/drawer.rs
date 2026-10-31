//! The drawer under the canvas.
//!
//! It rests as a [`STRIP_H`] tab strip and opens to its full height when the user clicks the
//! page (the hit stack), when a validation run finishes and when a tab is clicked; clicking the
//! active tab of an open drawer collapses it. In the entries where the drawer is the main
//! content (Content, Objects, Validation, and a file with a damaged index) it starts open, and
//! it opens with the paint-order stepper.

use std::time::Instant;

use pdfxray_core::{NodeId, PageIx, Point};

use super::{RailItem, SpeechState};
use crate::theme::tokens;

/// Height of the tab strip: all the drawer shows while it is collapsed.
pub const STRIP_H: f32 = 34.0;

/// Room kept for the canvas above an open drawer.
const CANVAS_MIN_H: f32 = 160.0;

/// A tab of the drawer; ⌘1 to ⌘5 pick them in the order [`DrawerTab::for_rail`] lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DrawerTab {
    /// Everything under the clicked point.
    #[default]
    HitStack,
    /// Parser and model diagnostics.
    Diagnostics,
    /// The validation findings.
    Validation,
    /// The operators of the page.
    Operators,
    /// The paint-order stepper; it takes the place of Operators while the stepper is on.
    PaintOrder,
    /// What the selected object points to, and what points to it.
    References,
    /// What a screen reader announces, in order.
    Speech,
}

impl DrawerTab {
    /// The tabs of the canvas entries, in ⌘1 to ⌘5 order.
    const CANVAS: [Self; 5] = [
        Self::HitStack,
        Self::Diagnostics,
        Self::Validation,
        Self::Operators,
        Self::Speech,
    ];

    /// The tabs of the canvas entries while the paint-order stepper is on.
    const STEPPING: [Self; 5] = [
        Self::HitStack,
        Self::Diagnostics,
        Self::Validation,
        Self::PaintOrder,
        Self::Speech,
    ];

    /// The tabs of the Objects entry, in ⌘1 to ⌘3 order.
    const OBJECTS: [Self; 3] = [Self::References, Self::Diagnostics, Self::HitStack];

    /// The tabs the drawer shows for `rail`, in ⌘1 order. With the paint-order `stepper` on, the
    /// canvas entries swap Operators for Paint order.
    pub const fn for_rail(rail: RailItem, stepper: bool) -> &'static [Self] {
        match rail {
            RailItem::Objects => &Self::OBJECTS,
            _ if stepper => &Self::STEPPING,
            _ => &Self::CANVAS,
        }
    }

    /// Text on the tab.
    pub const fn title(self) -> &'static str {
        match self {
            Self::HitStack => "Hit stack",
            Self::Diagnostics => "Diagnostics",
            Self::Validation => "Validation",
            Self::Operators => "Operators",
            Self::PaintOrder => "Paint order",
            Self::References => "References",
            Self::Speech => "Speech",
        }
    }

    /// The line at the right end of the tab bar while this tab shows.
    pub const fn hint(self) -> Option<&'static str> {
        match self {
            Self::References => Some("what this object points to, and what points to it"),
            Self::PaintOrder => Some("← → step · ⇧ next visible change"),
            _ => None,
        }
    }
}

/// One node under the clicked point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HitEntry {
    /// The node.
    pub id: NodeId,
    /// Display name, e.g. `CropBox` or `Page 1`.
    pub label: String,
    /// Node category, e.g. `page-box`.
    pub kind: String,
}

/// Everything found under the last click on the canvas, most specific first.
#[derive(Debug, Clone, PartialEq)]
pub struct HitStack {
    /// Page that was clicked.
    pub page: PageIx,
    /// Clicked point in page space.
    pub point: Point,
    /// The nodes, in the order the core returned them.
    pub entries: Vec<HitEntry>,
}

/// Drawer state.
#[derive(Debug)]
pub struct DrawerState {
    /// The tab that is showing.
    pub tab: DrawerTab,
    /// Height in points, set by dragging the top edge.
    pub height: f32,
    /// What the last canvas click found.
    pub hit_stack: Option<HitStack>,
    /// The Speech tab: which reader it models and whether it lists what is skipped.
    pub speech: SpeechState,
    /// The Diagnostics tab lists only what concerns page boxes and rotation (on by default on
    /// the Pages entry).
    pub page_boxes_only: bool,
    /// The Diagnostics tab lists the worst problems first (the damage banner's "View problems").
    pub sorted_by_severity: bool,
    /// The tab that was showing when the paint-order stepper went on; it comes back when the
    /// stepper goes off.
    pub before_stepper: Option<DrawerTab>,
    /// The drawer shows its tab's content; closed, it is only the strip of tabs.
    pub open: bool,
    /// What the drawer saw last frame, to tell what changed since.
    seen: Seen,
}

/// What the drawer looked at last frame.
#[derive(Debug, Default)]
struct Seen {
    stepper: bool,
    validated: Option<Instant>,
}

/// What made the drawer open on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    /// Nothing changed.
    No,
    /// The paint-order stepper went on.
    Yes,
    /// A validation run finished: the drawer should show its findings.
    Validated,
}

impl Default for DrawerState {
    fn default() -> Self {
        Self {
            tab: DrawerTab::default(),
            height: tokens::DRAWER_H,
            hit_stack: None,
            speech: SpeechState::default(),
            page_boxes_only: false,
            sorted_by_severity: false,
            before_stepper: None,
            open: false,
            seen: Seen::default(),
        }
    }
}

impl DrawerState {
    /// Height of the drawer panel in a window with `room` points for the canvas and the drawer:
    /// the strip while collapsed, else the dragged height, which is kept inside what the window
    /// allows.
    pub fn panel_height(&mut self, room: f32) -> f32 {
        if !self.open {
            return STRIP_H;
        }
        self.height = self.height.clamp(
            tokens::DRAWER_MIN_H,
            (room - CANVAS_MIN_H).max(tokens::DRAWER_MIN_H),
        );
        self.height
    }

    /// Swaps in `tab` because the one showing does not belong to the entry: not a request to
    /// look at it, so the drawer stays as it is.
    pub fn settle_tab(&mut self, tab: DrawerTab) {
        self.tab = tab;
    }

    /// Shows `tab`, opening the drawer.
    pub fn show_tab(&mut self, tab: DrawerTab) {
        self.tab = tab;
        self.open = true;
    }

    /// A click on the page found `stack`: shows it, opening the drawer on the Hit stack tab,
    /// also when it is the same point again.
    pub fn show_hits(&mut self, stack: HitStack) {
        self.hit_stack = Some(stack);
        self.show_tab(DrawerTab::HitStack);
    }

    /// What a click on `tab` does: the active tab of an open drawer collapses it, any other tab
    /// is shown and opens it.
    pub fn click_tab(&mut self, tab: DrawerTab) {
        if self.open && self.tab == tab {
            self.open = false;
        } else {
            self.show_tab(tab);
        }
    }

    /// Starts over for a rail entry or a document: the drawer is `open` as the entry wants it
    /// and takes what is shown now as seen, so only later changes open it.
    pub(crate) fn reset(&mut self, open: bool, stepper: bool, validated: Option<Instant>) {
        self.open = open;
        self.seen = Seen { stepper, validated };
    }

    /// Notices what happened elsewhere since the last frame and opens the drawer for it: the
    /// paint-order stepper going on, a validation run finishing. (Clicks and tabs open it
    /// themselves, through [`Self::show_hits`] and [`Self::show_tab`].)
    pub fn follow(&mut self, stepper: bool, validated: Option<Instant>) -> Opened {
        let stepper_changed = stepper != self.seen.stepper;
        // Going on opens the drawer for the stepper; going off brings the old tab back and says
        // nothing about whether the user wants the drawer.
        let opens = stepper && stepper_changed;
        let validated_now = validated.is_some() && validated != self.seen.validated;
        self.seen = Seen { stepper, validated };
        if validated_now {
            self.open = true;
            Opened::Validated
        } else if opens {
            self.open = true;
            Opened::Yes
        } else {
            Opened::No
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use pretty_assertions::assert_eq;

    use super::*;

    fn stack() -> HitStack {
        HitStack {
            page: 0,
            point: Point::new(1.0, 2.0),
            entries: Vec::new(),
        }
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn a_collapsed_drawer_is_the_strip_and_an_open_one_keeps_room_for_the_canvas() {
        let mut drawer = DrawerState::default();
        assert_eq!(drawer.panel_height(900.0), STRIP_H);
        drawer.open = true;
        assert_eq!(drawer.panel_height(900.0), tokens::DRAWER_H);
        // A short window squeezes it, down to its minimum.
        assert_eq!(drawer.panel_height(300.0), 140.0);
        assert_eq!(drawer.panel_height(100.0), tokens::DRAWER_MIN_H);
    }

    #[test]
    fn the_active_tab_of_an_open_drawer_collapses_it_and_any_other_tab_opens_it() {
        let mut drawer = DrawerState::default();
        drawer.click_tab(DrawerTab::HitStack);
        assert!(drawer.open);
        drawer.click_tab(DrawerTab::HitStack);
        assert!(!drawer.open);
        drawer.click_tab(DrawerTab::Diagnostics);
        assert_eq!((drawer.open, drawer.tab), (true, DrawerTab::Diagnostics));
        // Smoke: a collapsed drawer remembers its tab.
        drawer.click_tab(DrawerTab::Diagnostics);
        assert_eq!(drawer.tab, DrawerTab::Diagnostics);
    }

    #[test]
    fn it_opens_for_the_stepper_going_on_and_a_finished_run() {
        let mut drawer = DrawerState::default();
        drawer.reset(false, false, None);
        assert_eq!(drawer.follow(false, None), Opened::No);

        // The stepper going on opens it; going off does not.
        assert_eq!(drawer.follow(true, None), Opened::Yes);
        assert!(drawer.open);
        drawer.open = false;
        assert_eq!(drawer.follow(false, None), Opened::No);
        assert!(!drawer.open);

        assert_eq!(drawer.follow(false, Some(Instant::now())), Opened::Validated);
        assert!(drawer.open);
    }

    #[test]
    fn a_click_on_the_page_opens_the_hit_stack_every_time_even_on_the_same_point() {
        let mut drawer = DrawerState::default();
        drawer.reset(false, false, None);
        drawer.show_tab(DrawerTab::Diagnostics);
        drawer.open = false;

        drawer.show_hits(stack());
        assert_eq!((drawer.open, drawer.tab), (true, DrawerTab::HitStack));
        assert_eq!(drawer.hit_stack, Some(stack()));

        // Closed by the user, the same point clicked again opens it again.
        drawer.open = false;
        drawer.show_hits(stack());
        assert!(drawer.open);
        // Smoke: nothing but the click opened it.
        drawer.open = false;
        assert_eq!(drawer.follow(false, None), Opened::No);
        assert!(!drawer.open);
    }

    #[test]
    fn settling_a_tab_is_not_a_request_to_look_at_it() {
        let mut drawer = DrawerState::default();
        drawer.reset(false, false, None);
        drawer.settle_tab(DrawerTab::Operators);
        assert_eq!(drawer.follow(false, None), Opened::No);
        assert!(!drawer.open);
    }
}
