//! The canvas: every page of the document in one vertical scroll, with rulers and overlays,
//! click-to-hit, the selection outline, the measure tool, the tag overlay ([`tag_overlay`]) and the
//! floating cards. The toolbar
//! above it is [`super::toolbar`].
//!
//! Only the pages in view are rendered and drawn; the others only take their space. The page
//! that fills most of the view becomes the current page when the user scrolls, and a click on a
//! page makes that page current (see [`crate::CanvasState`]).
//!
//! This module draws the host (toolbar, rulers, the scroll of pages, the cards) in [`ui`] and
//! applies what the pointer did. The page layout is in `layout`, the scroll of pages and its
//! jumps in `scroll`, one page with its overlays in `page`, and the pointer's effects on the
//! state in `pointer`.

pub mod action_bar;
pub mod cards;
mod connector;
pub mod footnotes;
pub mod geometry;
mod layout;
pub mod marks;
pub mod measure;
pub mod overlay;
mod page;
pub mod paint_order;
mod pointer;
pub mod role_legend;
pub mod rulers;
mod scroll;
pub mod table_overlay;
pub mod tag_overlay;
mod validation_badges;
pub mod validation_card;

use egui::{
    Frame, Label, Modifiers, Panel, Pos2, Rect as UiRect, RichText, Sense, TextWrapMode, Ui, UiBuilder, Vec2,
    vec2,
};
use pdfxray_core::{BoxKind, EngineError, NodeId, PageInfo, PageIx, Point};
use pdfxray_render::{RenderError, page_dimensions};

use self::geometry::PageGeometry;
pub use self::layout::{PAGE_GAP, PageLayout, layout_pages, most_visible};
use self::measure::{Comparison, Cursor, Line};
use self::pointer::apply_pointer;
use self::rulers::Strips;
use self::scroll::pages_view;
use super::toolbar;
use crate::findings::PageBadge;
use crate::state::AppState;
use crate::theme::{effects, fonts, tokens};
use crate::units::Lengths;

/// Shown instead of the page for encrypted documents.
pub const ENCRYPTED_NOT_RENDERED: &str = "Encrypted: not rendered in v1";

/// Shown instead of the page when the engine cannot read the file.
pub const ENGINE_FAILED: &str = "The rendering engine could not read this document.";

/// Shown by the measure tool before anything is measured.
pub const MEASURE_HINT: &str =
    "Measure: drag across the page, or click two points. Hold Shift to snap to 45°.";

/// Shown while a bitmap is being rendered.
pub const RENDERING_TEXT: &str = "Rendering...";

/// Accessible name of the page image; clicking it picks the nodes under the pointer.
pub fn canvas_label(page: u32) -> String {
    format!("Page {} canvas", page + 1)
}

/// What the page view shows besides the bitmap, on one page.
struct Overlays {
    info: PageInfo,
    /// The page boxes whose outline is drawn.
    boxes: Vec<BoxKind>,
    selection: Option<pdfxray_core::Rect>,
    compared: Option<pdfxray_core::Rect>,
    comparison: Option<Comparison>,
    line: Option<Line>,
    measuring: bool,
    /// Scroll the selection into view on this page.
    reveal: bool,
    lengths: Lengths,
    /// The operators the stepper has not reached yet.
    ghost: paint_order::Ghost,
    /// Hover, preview, glyph boxes, untagged content and the tags that name them.
    marks: marks::Marks,
    /// The failed checks of the Validation entry.
    badges: Vec<PageBadge>,
    /// Tag boxes, chips, artifacts and the reading order.
    tags: tag_overlay::TagLayer,
    /// The grid and header connectors of the selected table.
    table: table_overlay::TableLayer,
    /// The connectors from footnote references to their notes.
    notes: footnotes::NoteLayer,
}

/// What the pointer did on one page this frame.
struct PointerOutput {
    clicked: Option<Point>,
    drag_started: Option<Point>,
    dragged: Option<Point>,
    cursor: Option<Cursor>,
    /// The modifier keys held down during the frame, for clicks.
    modifiers: Modifiers,
    /// The finding of the validation badge that was clicked.
    badge_clicked: Option<u32>,
    /// The element whose chip, badge or box (while the tag boxes are on) was clicked.
    tag_clicked: Option<NodeId>,
    /// The other end of the footnote link whose label was clicked: the canvas goes there.
    jump_to: Option<NodeId>,
}

impl PointerOutput {
    /// The page was clicked, dragged on or had a badge clicked: it becomes current.
    const fn acted(&self) -> bool {
        self.clicked.is_some()
            || self.drag_started.is_some()
            || self.dragged.is_some()
            || self.badge_clicked.is_some()
            || self.tag_clicked.is_some()
            || self.jump_to.is_some()
    }
}

/// What happened in the scroll of pages this frame.
struct CanvasOutput {
    /// Where the current page is on screen, when it is in view and rendered.
    current: Option<PageGeometry>,
    /// The page under the pointer (or being dragged on) and what the pointer did there.
    pointer: Option<(PageIx, PointerOutput)>,
    /// The pages in view, top to bottom.
    visible: Vec<PageIx>,
    /// The page that fills most of the view.
    most_visible: Option<PageIx>,
    /// Where the pages in view are on screen, whether or not their bitmap is there yet.
    pages: Vec<UiRect>,
    offset: Vec2,
}

/// Draws the canvas host: toolbar, then the pages. Without pages it explains why.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let count = state.page_count();
    Panel::top("canvas_toolbar")
        .exact_size(toolbar::HEIGHT)
        .frame(Frame::new().fill(tokens::TOOLBAR_BG))
        .show(ui, |ui| toolbar::ui(ui, state, count));
    crate::states::broken::banner(ui, state);
    if count == 0 {
        no_pages(ui, state);
        return;
    }
    match state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.engine_error())
    {
        Some(EngineError::Encrypted) => {
            message(ui, ENCRYPTED_NOT_RENDERED);
            return;
        }
        Some(EngineError::Invalid) => {
            message(ui, ENGINE_FAILED);
            return;
        }
        None => {}
    }
    let Some(loaded) = state.document.as_ref() else {
        return;
    };
    let Some(info) = loaded.doc.page(state.canvas.page).cloned() else {
        return;
    };
    let dims: Vec<Result<Vec2, RenderError>> = (0..count)
        .map(|page| {
            let page = PageIx::try_from(page).unwrap_or(PageIx::MAX);
            page_dimensions(&loaded.doc, page).map(|(w, h)| vec2(w, h))
        })
        .collect();

    // Rulers take a strip along the top and left; the pages get the rest.
    let avail = ui.available_rect_before_wrap();
    let thickness = rulers::THICKNESS;
    let strips = Strips {
        top: UiRect::from_min_max(
            avail.min + vec2(thickness, 0.0),
            Pos2::new(avail.max.x, avail.min.y + thickness),
        ),
        left: UiRect::from_min_max(
            avail.min + vec2(0.0, thickness),
            Pos2::new(avail.min.x + thickness, avail.max.y),
        ),
    };
    let corner = UiRect::from_min_size(avail.min, vec2(thickness, thickness));
    let content = UiRect::from_min_max(avail.min + vec2(thickness, thickness), avail.max);
    if !content.is_positive() {
        return;
    }
    // The canvas tone with the faint grid, which pans with the pages.
    ui.painter().rect_filled(avail, 0.0, tokens::CANVAS);
    effects::grid(ui.painter(), content, -state.canvas.scroll_offset);
    if state.selection.primary.is_none() {
        // A selection made again grows in again.
        state.canvas.outlined = None;
    }

    state.canvas.viewport = content.size();
    let lengths = state.lengths();
    let output = ui
        .scope_builder(UiBuilder::new().max_rect(content), |ui| {
            pages_view(ui, state, &dims)
        })
        .inner;

    state.canvas.scroll_offset = output.offset;
    state.keep_shown_pages(&output.visible);
    if state.canvas.settle > 0 {
        state.canvas.settle -= 1;
    } else if let Some(page) = output.most_visible
        && output.offset != state.canvas.last_offset
    {
        // The user scrolled: the page that fills the view is the one they are looking at.
        state.set_current_page(page);
    }
    state.canvas.last_offset = output.offset;
    if let Some((page, pointer)) = output.pointer {
        state.canvas.cursor = pointer.cursor;
        apply_pointer(state, page, &pointer);
    } else {
        state.canvas.cursor = None;
        state.update_hover(None);
    }
    if let Some(geo) = output.current {
        let corner_label = Label::new(
            RichText::new(rulers::corner_text(lengths.unit))
                .font(fonts::mono(tokens::mono::TINY))
                .color(tokens::TEXT_3),
        )
        .sense(Sense::hover())
        .wrap_mode(TextWrapMode::Extend);
        ui.put(corner, corner_label)
            .on_hover_text(rulers::origin_help(info.user_unit));
        rulers::paint(ui, strips, &geo, &lengths);
    }
    let info = state
        .document
        .as_ref()
        .and_then(|loaded| loaded.doc.page(state.canvas.page).cloned())
        .unwrap_or(info);
    cards::show(ui, state, content, &info, &output.pages);
}

/// A centred line of text in the canvas area.
fn message(ui: &mut Ui, text: &str) {
    ui.centered_and_justified(|ui| {
        ui.label(
            RichText::new(text)
                .font(fonts::ui(tokens::text::LARGE))
                .color(tokens::TEXT_3),
        );
    });
}

fn no_pages(ui: &mut Ui, state: &AppState) {
    let reason = match &state.document {
        Some(loaded) => match loaded.doc.engine_error() {
            Some(EngineError::Encrypted) => ENCRYPTED_NOT_RENDERED,
            Some(EngineError::Invalid) => ENGINE_FAILED,
            None => "This document has no pages.",
        },
        None => "Open a PDF to see its pages.",
    };
    message(ui, reason);
}
