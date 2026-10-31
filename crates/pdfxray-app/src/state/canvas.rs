//! The canvas: the current page, zoom, layer mode and the debug tools.
//!
//! The canvas is one vertical scroll of every page. The *current* page is the one the page
//! field, the Content tree, the Operators drawer, the stepper and isolation work on: the page
//! that fills most of the view, or the one last clicked.

use std::collections::BTreeSet;

use egui::Vec2;
use pdfxray_core::{BoxKind, LayerMode, NodeId, PageIx};

use super::AppState;
use crate::measure::{Cursor, MeasureTool};
use crate::modes::{SelectedOps, ViewMode};
use crate::view::{ZoomMode, clamp_zoom, zoom_in, zoom_out};

/// State of the canvas.
#[derive(Debug)]
pub struct CanvasState {
    /// Zero-based index of the current page.
    pub page: PageIx,
    /// A page the canvas should scroll to the top of on its next frame (set by
    /// [`AppState::go_to_page`]).
    pub(crate) scroll_to: Option<PageIx>,
    /// Text in the page number field of the toolbar.
    pub page_field: String,
    /// Canvas zoom, 1.0 = 100 %. In [`ZoomMode::Fit`] the canvas keeps it up to date.
    pub zoom: f32,
    /// Whether the canvas fits the page or follows [`Self::zoom`].
    pub zoom_mode: ZoomMode,
    /// Which kinds of nodes a click may select.
    pub layer_mode: LayerMode,
    /// Whether the page boxes overlay is on.
    pub show_boxes: bool,
    /// Boxes whose outline the eye toggle of the Pages list hides, on every page.
    pub hidden_boxes: BTreeSet<BoxKind>,
    /// The measure tool.
    pub measure: MeasureTool,
    /// Debug render modes: x-ray, isolate, stepper, clip and heat map.
    pub view: ViewMode,
    /// The pointer over the page, in both coordinate systems; `None` when it is elsewhere.
    pub cursor: Option<Cursor>,
    /// How far the canvas is scrolled, as of the last frame.
    pub scroll_offset: Vec2,
    /// The scroll offset the canvas compared against last frame, to tell a scroll by the user.
    pub(crate) last_offset: Vec2,
    /// Frames left before a scroll the canvas made itself settles.
    pub(crate) settle: u8,
    /// Size of the scroll area's view (inside its scroll bars) as of the last frame.
    pub(crate) scroll_view: Vec2,
    /// The scroll offset that centres the selection, applied on the next frame.
    pub(crate) reveal_to: Option<Vec2>,
    /// The offset a jump (to a page or the selection) is easing towards.
    pub(crate) scroll_anim: Option<Vec2>,
    /// Size of the scrolled content and of the view as of the last frame.
    pub(crate) last_layout: (Vec2, Vec2),
    /// The node a tree or table row under the pointer asks the canvas to preview; the canvas
    /// takes it each frame.
    pub preview: Option<NodeId>,
    /// The preview the canvas drew in the last frame: the node and the page it is on.
    pub drawn_preview: Option<NodeId>,
    /// The topmost node under the pointer on the canvas, outlined thinly.
    pub hover: Option<NodeId>,
    /// Size of the canvas area as of the last frame, for zooming to a node.
    pub viewport: Vec2,
    /// The selection should be scrolled into view the next time it is drawn.
    pub(crate) reveal: bool,
    /// The operators behind the selection, for the isolate and hide buttons.
    pub(crate) selected_ops: Option<SelectedOps>,
    /// The selection whose outline the canvas is growing in, or has grown in.
    pub(crate) outlined: Option<NodeId>,
    /// The node the right-click menu of the canvas acts on: the one under the pointer when the
    /// menu was opened.
    pub(crate) context_node: Option<NodeId>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            page: 0,
            scroll_to: None,
            page_field: String::new(),
            zoom: 1.0,
            zoom_mode: ZoomMode::Fit,
            layer_mode: LayerMode::default(),
            show_boxes: false,
            hidden_boxes: BTreeSet::new(),
            measure: MeasureTool::default(),
            view: ViewMode::default(),
            cursor: None,
            scroll_offset: Vec2::ZERO,
            last_offset: Vec2::ZERO,
            settle: 0,
            last_layout: (Vec2::ZERO, Vec2::ZERO),
            reveal_to: None,
            scroll_anim: None,
            scroll_view: Vec2::ZERO,
            preview: None,
            drawn_preview: None,
            hover: None,
            viewport: Vec2::ZERO,
            reveal: false,
            selected_ops: None,
            outlined: None,
            context_node: None,
        }
    }
}

impl CanvasState {
    /// Whether the overlay draws the outline of `kind`: the overlay is on and the box is not hidden.
    pub fn box_shown(&self, kind: BoxKind) -> bool {
        self.show_boxes && !self.hidden_boxes.contains(&kind)
    }

    /// The boxes the overlay draws an outline for, outermost first.
    pub fn shown_boxes(&self) -> Vec<BoxKind> {
        BoxKind::ALL
            .into_iter()
            .filter(|kind| self.box_shown(*kind))
            .collect()
    }

    /// Hides the outline of `kind` on every page, or shows it again.
    pub fn toggle_box(&mut self, kind: BoxKind) {
        if !self.hidden_boxes.remove(&kind) {
            self.hidden_boxes.insert(kind);
        }
    }

    /// True when a selection is waiting to be scrolled into view.
    pub const fn reveal_pending(&self) -> bool {
        self.reveal
    }

    /// Back to the start for a new document: first page, fitted, no isolation or stepper.
    pub(crate) fn reset_for_document(&mut self) {
        self.measure.clear();
        self.cursor = None;
        self.preview = None;
        self.hover = None;
        self.view.isolation = None;
        self.view.stepper = false;
        self.view.step = 0;
        self.selected_ops = None;
        self.zoom_mode = ZoomMode::Fit;
        self.page = 0;
        self.scroll_to = Some(0);
        "1".clone_into(&mut self.page_field);
    }
}

impl AppState {
    /// Makes `page` current and scrolls the canvas to it, if it exists.
    pub fn go_to_page(&mut self, page: PageIx) {
        if self.set_current_page(page) {
            self.canvas.scroll_to = Some(page);
        }
    }

    /// Makes `page` current without scrolling: the canvas does this when the view scrolls onto
    /// another page or a page is clicked. Leaving a page drops its hit stack and isolation and
    /// keeps the stepper inside the new page. False when there is no such page.
    pub(crate) fn set_current_page(&mut self, page: PageIx) -> bool {
        let in_range = usize::try_from(page).is_ok_and(|p| p < self.page_count());
        if in_range && page != self.canvas.page {
            self.canvas.page = page;
            self.drawer.hit_stack = None;
            self.canvas.cursor = None;
            // Filters are per page.
            self.canvas.view.isolation = None;
            self.clamp_step();
        }
        self.canvas.page_field = (self.canvas.page + 1).to_string();
        in_range
    }

    /// Shows the next page, if any.
    pub fn next_page(&mut self) {
        self.go_to_page(self.canvas.page.saturating_add(1));
    }

    /// Shows the previous page, if any.
    pub fn prev_page(&mut self) {
        self.go_to_page(self.canvas.page.saturating_sub(1));
    }

    /// Applies the text of the page number field (one-based); anything else resets the field.
    pub fn commit_page_field(&mut self) {
        match self.canvas.page_field.trim().parse::<u32>() {
            Ok(n) if n >= 1 => self.go_to_page(n - 1),
            _ => self.go_to_page(self.canvas.page),
        }
    }

    /// Zooms in one step and stops fitting the page.
    pub fn zoom_in(&mut self) {
        self.set_zoom(zoom_in(self.canvas.zoom));
    }

    /// Zooms out one step and stops fitting the page.
    pub fn zoom_out(&mut self) {
        self.set_zoom(zoom_out(self.canvas.zoom));
    }

    /// Sets an explicit zoom, limited to the allowed range.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.canvas.zoom = clamp_zoom(zoom);
        self.canvas.zoom_mode = ZoomMode::Manual;
    }

    /// Fits the whole page into the canvas from now on.
    pub fn fit_page(&mut self) {
        self.canvas.zoom_mode = ZoomMode::Fit;
    }
}
