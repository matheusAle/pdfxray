//! One page: its bitmap (or a placeholder while it renders), the overlays drawn over it, and what
//! the pointer did on it.

use egui::{
    Color32, CursorIcon, EventFilter, Id, Image, Pos2, Rect as UiRect, Sense, Stroke, StrokeKind, Ui, Vec2,
    load::SizedTexture,
};
use pdfxray_core::{NodeId, PageInfo, PageIx};

use super::geometry::PageGeometry;
use super::measure::Cursor;
use super::overlay::{COMPARED, SELECTION};
use super::{
    Overlays, PointerOutput, RENDERING_TEXT, action_bar, canvas_label, footnotes, marks, overlay,
    paint_order, table_overlay, tag_overlay, validation_badges,
};
use crate::components::context_menu;
use crate::findings::PageBadge;
use crate::page_cache::{Frame as PageFrame, RenderFailure};
use crate::state::{AppState, RailItem, describe};
use crate::theme::{motion, tokens};
use crate::view::render_scale;

/// The failed checks to mark on `page`: only the Validation entry shows them.
fn validation_badges(state: &AppState, page: u32) -> Vec<PageBadge> {
    if state.nav.rail == RailItem::Validation {
        state.page_badges(page)
    } else {
        Vec::new()
    }
}

/// What to draw over `page` this frame.
pub(super) fn overlays(
    state: &mut AppState,
    page: PageIx,
    info: PageInfo,
    preview: Option<&NodeId>,
) -> Overlays {
    let selection = state.selection_rect().filter(|s| s.page == page).map(|s| s.rect);
    let current = page == state.canvas.page;
    Overlays {
        boxes: state.canvas.shown_boxes(),
        selection,
        compared: state.compare_rect().filter(|s| s.page == page).map(|s| s.rect),
        comparison: state.comparison().filter(|_| selection.is_some()),
        line: state.canvas.measure.line().filter(|line| line.page == page),
        measuring: state.canvas.measure.active,
        reveal: state.canvas.reveal && selection.is_some(),
        lengths: state.lengths(),
        // The stepper walks the current page.
        ghost: if current {
            paint_order::collect(state)
        } else {
            paint_order::Ghost::default()
        },
        marks: marks::collect(state, page, preview),
        badges: validation_badges(state, page),
        tags: tag_overlay::collect(state, page),
        table: table_overlay::collect(state, page),
        notes: footnotes::collect(state, page),
        info,
    }
}

/// Draws one page at `rect` (screen), `size` points big: its bitmap, or a placeholder while it
/// renders, and its overlays. `None` when there is no bitmap to draw over yet.
pub(super) fn draw_page(
    ui: &mut Ui,
    state: &mut AppState,
    page: PageIx,
    rect: UiRect,
    size: Vec2,
    failed: Option<RenderFailure>,
    overlays: &Overlays,
) -> Option<(PageGeometry, PointerOutput)> {
    let ctx = ui.ctx().clone();
    let frame = if let Some(failure) = failed {
        Some(PageFrame::Failed(failure))
    } else {
        let max_side = ctx.input(|i| i.max_texture_side).try_into().unwrap_or(u32::MAX);
        let key = render_scale(state.canvas.zoom, ctx.pixels_per_point(), size, max_side);
        state
            .render_request(page, key)
            .and_then(|request| state.cached_page(&ctx, request))
    };
    let cached = match frame {
        Some(PageFrame::Ready(cached) | PageFrame::Stale(cached)) => cached,
        Some(PageFrame::Rendering) | None => {
            placeholder(ui, rect, RENDERING_TEXT, tokens::TEXT_3);
            return None;
        }
        Some(PageFrame::Failed(failure)) => {
            placeholder(
                ui,
                rect,
                &format!("Cannot render page {}: {}", page + 1, describe(&failure)),
                tokens::ERR,
            );
            return None;
        }
    };
    let geo = PageGeometry {
        rect,
        bitmap: cached.size,
        to_pixel: cached.page_to_pixel,
    };

    let sense = if overlays.measuring {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let image = Image::from_texture(SizedTexture::new(cached.texture.id(), rect.size()))
        .sense(sense)
        .alt_text(canvas_label(page));
    let mut response = ui.put(rect, image);
    if overlays.measuring {
        response = response.on_hover_cursor(CursorIcon::Crosshair);
    }

    let painter = ui.painter_at(rect.expand(2.0));
    overlay::draw_boxes(&painter, &geo, &overlays.info, &overlays.boxes);
    paint_order::draw(&painter, &geo, &overlays.ghost);
    if let Some(compared) = overlays.compared {
        overlay::draw_selection(&painter, geo.rect_to_screen(compared), COMPARED, 1.0);
    }
    let outline = overlays.selection.map(|selected| {
        let outline = geo.rect_to_screen(selected);
        let grown = outline_progress(&ctx, state);
        overlay::draw_selection(&painter, outline, SELECTION, grown);
        outline
    });
    // The table's grid goes under the tags, so that its lines do not cut through their text.
    let table = table_overlay::draw(ui, &geo, page, &overlays.table);
    let baseline_tag = marks::draw(&painter, &geo, &overlays.marks);
    if let Some(outline) = outline
        && !overlays.measuring
    {
        action_bar::show(&ctx, state, outline, baseline_tag, ui.clip_rect());
    }
    let badge_clicked = validation_badges::draw(ui, &geo, &overlays.badges);
    let tags = tag_overlay::draw(ui, &geo, page, &overlays.tags, response.hover_pos());
    let notes = footnotes::draw(ui, &geo, page, &overlays.notes);
    if let Some(comparison) = &overlays.comparison {
        overlay::draw_gaps(ui, &geo, comparison, &overlays.lengths);
    }
    if let Some(line) = &overlays.line {
        overlay::draw_measure(ui, &geo, line, response.hover_pos(), &overlays.lengths);
    }
    if overlays.measuring
        && let Some(pos) = response.hover_pos()
    {
        overlay::draw_crosshair(ui, pos, &overlays.info, &geo, &overlays.lengths);
    }

    right_click_menu(state, &response, &geo);
    if response.has_focus() {
        // Keep Left/Right for the stepper instead of moving the focus.
        let filter = EventFilter {
            horizontal_arrows: true,
            ..EventFilter::default()
        };
        ui.memory_mut(|m| m.set_focus_lock_filter(response.id, filter));
    }
    let at = |pos: Pos2| geo.screen_to_page(pos);
    let clicked = response
        .clicked()
        .then(|| response.interact_pointer_pos())
        .flatten()
        .map(at);
    let modifiers = ui.input(|i| i.modifiers);
    // A plain click inside a tag box (while they are drawn) picks the element; with a modifier,
    // or while measuring, the click does what it always did.
    let box_hit = clicked
        .filter(|_| !overlays.measuring && modifiers.is_none())
        .and_then(|point| overlays.tags.box_at(point))
        .map(|tag| tag.id.clone());
    let activity = PointerOutput {
        clicked,
        tag_clicked: tags.clicked.or(table.clicked).or(box_hit),
        jump_to: notes.jump,
        drag_started: response
            .drag_started()
            .then(|| ui.input(|i| i.pointer.press_origin()))
            .flatten()
            .map(at),
        dragged: response
            .dragged()
            .then(|| response.interact_pointer_pos())
            .flatten()
            .map(at),
        cursor: response.hover_pos().map(|pos| Cursor {
            page: at(pos),
            display: geo.screen_to_display(pos),
        }),
        modifiers,
        badge_clicked,
    };
    Some((geo, activity))
}

/// How far the selection's outline has grown in (0..=1): it starts over when the selection
/// changes.
fn outline_progress(ctx: &egui::Context, state: &mut AppState) -> f32 {
    let id = Id::unique("canvas-selection-outline");
    if state.canvas.outlined != state.selection.primary {
        motion::reset(ctx, id);
        state.canvas.outlined.clone_from(&state.selection.primary);
    }
    motion::appear(ctx, id, motion::SELECTION)
}

/// Right-click on a page: selects the topmost node under the pointer and offers its actions in a
/// menu, the same ones as the tree rows and the selection bar.
fn right_click_menu(state: &mut AppState, response: &egui::Response, geo: &PageGeometry) {
    if response.secondary_clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        state.click_page_point(geo.screen_to_page(pos));
        state.canvas.context_node = state
            .drawer
            .hit_stack
            .as_ref()
            .and_then(|stack| stack.entries.first())
            .map(|entry| entry.id.clone());
    }
    if let Some(node) = state.canvas.context_node.clone() {
        context_menu::attach(response, state, &node);
    }
}

/// A page that has no bitmap yet: a blank sheet with a note.
fn placeholder(ui: &Ui, rect: UiRect, text: &str, ink: Color32) {
    let painter = ui.painter_at(rect.expand(2.0));
    painter.rect_filled(rect, 0.0, tokens::SURFACE_2);
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, tokens::BORDER), StrokeKind::Outside);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        crate::theme::fonts::ui(tokens::text::BODY),
        ink,
    );
}
