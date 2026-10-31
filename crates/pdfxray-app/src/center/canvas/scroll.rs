//! The scroll of pages: lays every page out, eases the zoom, scrolls where the state asks (a page,
//! the selection) and draws the pages in view.

use egui::{Id, ScrollArea, Sense, Ui, Vec2, vec2};
use pdfxray_core::PageIx;
use pdfxray_render::RenderError;

use super::page::{draw_page, overlays};
use super::pointer::apply_wheel_zoom;
use super::{CanvasOutput, PAGE_GAP, layout_pages, most_visible};
use crate::page_cache::RenderFailure;
use crate::state::AppState;
use crate::theme::{effects, motion};
use crate::view::{ZoomMode, fit_zoom};

/// Size of a page whose dimensions cannot be read: US Letter, so it still takes its place.
const FALLBACK_SIZE: Vec2 = vec2(612.0, 792.0);

/// Where the horizontal (`"x"`) and the vertical (`"y"`) scroll offset of a jump are tweened.
fn scroll_id(axis: &'static str) -> Id {
    Id::unique(("canvas-scroll", axis))
}

/// How many frames after the canvas scrolls by itself (to a page or to the selection) the
/// scroll position is not taken as the user's choice of current page.
const SETTLE_FRAMES: u8 = 2;

/// The scroll of pages: lays every page out, scrolls where the state asks, and draws the pages in
/// view with their overlays.
pub(super) fn pages_view(
    ui: &mut Ui,
    state: &mut AppState,
    dims: &[Result<Vec2, RenderError>],
) -> CanvasOutput {
    apply_wheel_zoom(ui, state);
    let current = state.canvas.page;
    let current_ix = usize::try_from(current).unwrap_or(0);
    let size_of = |page: usize| {
        dims.get(page)
            .and_then(|d| d.as_ref().ok().copied())
            .unwrap_or(FALLBACK_SIZE)
    };
    if state.canvas.zoom_mode == ZoomMode::Fit {
        // Fit the largest page in the view, with the gap around it: one zoom for the whole
        // scroll, so it does not jump between pages of different sizes.
        let largest = (0..dims.len()).map(size_of).fold(Vec2::ZERO, Vec2::max);
        let fitted = fit_zoom(largest, ui.available_size() - Vec2::splat(PAGE_GAP * 2.0));
        if (fitted - state.canvas.zoom).abs() > f32::EPSILON {
            state.canvas.zoom = fitted;
            ui.ctx().request_repaint();
        }
    }
    // The zoom on screen eases towards `state.canvas.zoom`; the bitmaps are rendered for the
    // target, so a zoom does not ask for a render per frame.
    let zoom = motion::value(
        ui.ctx(),
        Id::unique("canvas-zoom"),
        state.canvas.zoom,
        motion::ZOOM,
    );
    let sizes: Vec<Vec2> = (0..dims.len()).map(|page| size_of(page) * zoom).collect();
    let view = ui.available_size();
    let layout = layout_pages(&sizes, view);
    // A new zoom or view size moves the scroll by itself: that is not the user scrolling.
    let relaid = (layout.size, view) != state.canvas.last_layout;
    if relaid {
        state.canvas.last_layout = (layout.size, view);
        state.canvas.settle = SETTLE_FRAMES;
    }

    let mut area = ScrollArea::both().id_salt("canvas").auto_shrink(false);
    // Past either end the scroll area would spring back: every target stays inside.
    // The scroll bars take room from the view: use last frame's inner size once there is one.
    let inner = if state.canvas.scroll_view == Vec2::ZERO {
        view
    } else {
        state.canvas.scroll_view
    };
    let max_offset = (layout.size - inner).max(Vec2::ZERO);
    // The scroll area checks an offset against last frame's content: right after a new zoom it
    // would take a jump for overscroll and spring back, so jumps wait one frame.
    let jumping = state.canvas.scroll_to.is_some()
        || state.canvas.reveal_to.is_some()
        || state.canvas.scroll_anim.is_some();
    if relaid && jumping {
        ui.ctx().request_repaint();
    } else {
        let mut target = None;
        if let Some(page) = state.canvas.scroll_to.take()
            && let Some(rect) = usize::try_from(page).ok().and_then(|p| layout.pages.get(p))
        {
            target = Some(vec2(
                state.canvas.scroll_offset.x,
                (rect.min.y - PAGE_GAP).clamp(0.0, max_offset.y),
            ));
        }
        if let Some(offset) = state.canvas.reveal_to.take() {
            target = Some(offset.clamp(Vec2::ZERO, max_offset));
        }
        if let Some(target) = target {
            // A jump starts from where the view is now; one in flight is taken over.
            motion::reset(ui.ctx(), scroll_id("x"));
            motion::reset(ui.ctx(), scroll_id("y"));
            motion::value(
                ui.ctx(),
                scroll_id("x"),
                state.canvas.scroll_offset.x,
                motion::ZOOM,
            );
            motion::value(
                ui.ctx(),
                scroll_id("y"),
                state.canvas.scroll_offset.y,
                motion::ZOOM,
            );
            state.canvas.scroll_anim = Some(target);
        }
        if let Some(target) = state.canvas.scroll_anim {
            let wheel = ui.input(|i| i.smooth_scroll_delta != Vec2::ZERO);
            let at = vec2(
                motion::value(ui.ctx(), scroll_id("x"), target.x, motion::ZOOM),
                motion::value(ui.ctx(), scroll_id("y"), target.y, motion::ZOOM),
            );
            let arrived = (at - target).abs().max_elem() < 0.5;
            if wheel {
                // The user takes the scroll back.
                state.canvas.scroll_anim = None;
            } else {
                area = area.scroll_offset(if arrived { target } else { at });
                state.canvas.settle = SETTLE_FRAMES;
                if arrived {
                    state.canvas.scroll_anim = None;
                }
            }
        }
    }

    let preview = state.canvas.preview.take();
    state.canvas.drawn_preview = None;
    // egui fades the edges of a scroll area into its background: over a white page that is a
    // grey band under the ruler. The pages scroll without it.
    let fade = std::mem::replace(&mut ui.spacing_mut().scroll.fade.strength, 0.0);
    let output = area.show_viewport(ui, |ui, viewport| {
        let (content, _) = ui.allocate_exact_size(layout.size, Sense::hover());
        let origin = content.min.to_vec2();
        let mut out = CanvasOutput {
            current: None,
            pointer: None,
            visible: Vec::new(),
            most_visible: most_visible(&layout, viewport),
            pages: Vec::new(),
            offset: Vec2::ZERO,
        };
        // Every shadow under every page: a page's shadow falls past its bottom edge, so drawn
        // with its page it darkens the page below when that one is drawn first.
        let clip = ui.clip_rect();
        for rect in &layout.pages {
            let rect = rect.translate(origin);
            if effects::page_shadow_reach(rect).intersects(clip) {
                effects::page_shadow(ui.painter(), rect);
            }
        }
        // The current page asks the render thread first.
        let mut order: Vec<usize> = (0..layout.pages.len())
            .filter(|&page| layout.pages[page].intersects(viewport))
            .collect();
        order.sort_by_key(|&page| page != current_ix);
        for page_ix in order {
            let Ok(page) = PageIx::try_from(page_ix) else {
                continue;
            };
            out.visible.push(page);
            let rect = layout.pages[page_ix].translate(origin);
            let Some(info) = state.document.as_ref().and_then(|l| l.doc.page(page).cloned()) else {
                continue;
            };
            let overlays = overlays(state, page, info, preview.as_ref());
            let failed = dims
                .get(page_ix)
                .and_then(|d| d.as_ref().err().cloned())
                .map(RenderFailure::Render);
            out.pages.push(rect);
            let Some((geo, pointer)) = draw_page(ui, state, page, rect, size_of(page_ix), failed, &overlays)
            else {
                continue;
            };
            if overlays.reveal
                && let Some(selected) = overlays.selection
            {
                // Centre the selection next frame (in content coordinates).
                let centre = geo.rect_to_screen(selected).center() - origin;
                state.canvas.reveal_to = Some(centre.to_vec2() - view / 2.0);
                state.canvas.reveal = false;
                ui.ctx().request_repaint();
            }
            if page == current {
                out.current = Some(geo);
            }
            let keep = pointer.cursor.is_some() || pointer.acted();
            if keep && out.pointer.as_ref().is_none_or(|(_, p)| !p.acted()) {
                out.pointer = Some((page, pointer));
            }
        }
        out.visible.sort_unstable();
        out
    });
    ui.spacing_mut().scroll.fade.strength = fade;
    // The selection is on a page out of view: go there first, reveal it once it is drawn.
    if state.canvas.reveal
        && let Some(page) = state.selection_rect().map(|s| s.page)
        && !output.inner.visible.contains(&page)
    {
        state.canvas.scroll_to = Some(page);
        ui.ctx().request_repaint();
    } else if state.canvas.reveal && state.selection_rect().is_none() {
        state.canvas.reveal = false;
    }
    CanvasOutput {
        offset: {
            state.canvas.scroll_view = output.inner_rect.size();
            output.state.clamped_offset()
        },
        ..output.inner
    }
}
