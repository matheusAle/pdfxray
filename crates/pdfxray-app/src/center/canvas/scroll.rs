//! The scroll of pages: lays every page out, eases the zoom and keeps the spot it zooms at in
//! place, scrolls where the state asks (a page, the selection) and draws the pages in view.

use egui::containers::scroll_area::ScrollSource;
use egui::{CursorIcon, Id, Pos2, Rect as UiRect, ScrollArea, Sense, Ui, Vec2, vec2};
use pdfxray_core::PageIx;
use pdfxray_render::RenderError;

use super::cards::above_stepper;
use super::page::{draw_page, overlays};
use super::pointer::apply_wheel_zoom;
use super::{CanvasOutput, PAGE_GAP, anchored_offset, layout_pages, most_visible};
use crate::page_cache::RenderFailure;
use crate::selection::node_page;
use crate::state::AppState;
use crate::state::canvas::Reveal;
use crate::theme::{effects, motion};
use crate::view::{ZoomMode, fit_to_pixels, fit_zoom, snap_to_pixels};

/// Size of a page whose dimensions cannot be read: US Letter, so it still takes its place.
const FALLBACK_SIZE: Vec2 = vec2(612.0, 792.0);

/// Where the horizontal (`"x"`) and the vertical (`"y"`) scroll offset of a jump are tweened.
fn scroll_id(axis: &'static str) -> Id {
    Id::unique(("canvas-scroll", axis))
}

/// How many frames after the canvas scrolls by itself (to a page or to the selection) the
/// scroll position is not taken as the user's choice of current page.
const SETTLE_FRAMES: u8 = 2;

/// How close to the edge of the view the pointer starts scrolling the canvas while a marquee is
/// drawn.
const EDGE: f32 = 24.0;

/// The fastest the canvas scrolls under a marquee, in points per frame.
const EDGE_SPEED: f32 = 24.0;

/// How far the canvas scrolls this frame under a marquee whose pointer is at `pointer`: nothing
/// inside the view, more the further it is into the edge band or past it.
fn edge_scroll(view: UiRect, pointer: Pos2) -> Vec2 {
    let axis = |at: f32, min: f32, max: f32| {
        if at < min + EDGE {
            -((min + EDGE - at) / 2.0).min(EDGE_SPEED)
        } else if at > max - EDGE {
            ((at - (max - EDGE)) / 2.0).min(EDGE_SPEED)
        } else {
            0.0
        }
    };
    vec2(
        axis(pointer.x, view.min.x, view.max.x),
        axis(pointer.y, view.min.y, view.max.y),
    )
}

/// The scroll of pages: lays every page out, scrolls where the state asks, and draws the pages in
/// view with their overlays.
pub(super) fn pages_view(
    ui: &mut Ui,
    state: &mut AppState,
    dims: &[Result<Vec2, RenderError>],
) -> CanvasOutput {
    let wheel_anchor = apply_wheel_zoom(ui, state);
    let current = state.canvas.page;
    let current_ix = usize::try_from(current).unwrap_or(0);
    let emulation = state.emulation();
    // On an emulated screen a page is as big as its bitmap, a whole number of device pixels:
    // shown at a whole-pixel step, each device pixel then covers the same display pixels.
    let size_of = |page: usize| {
        let size = dims
            .get(page)
            .and_then(|d| d.as_ref().ok().copied())
            .unwrap_or(FALLBACK_SIZE);
        emulation.map_or(size, |emulation| {
            let scale = emulation.scale_for((size.x, size.y));
            Vec2::new((size.x * scale).trunc() / scale, (size.y * scale).trunc() / scale)
        })
    };
    // On an emulated screen the zoom keeps to whole pixels of it (see `AppState::set_zoom`).
    let unit = emulation.map(|emulation| {
        let size = dims
            .get(current_ix)
            .and_then(|d| d.as_ref().ok().copied())
            .unwrap_or(FALLBACK_SIZE);
        emulation.scale_for((size.x, size.y)) / ui.ctx().pixels_per_point()
    });
    if unit != state.canvas.pixel_unit {
        state.canvas.pixel_unit = unit;
        state.canvas.zoom_settle = None;
        if let Some(unit) = unit
            && state.canvas.zoom_mode == ZoomMode::Manual
        {
            state.canvas.zoom = snap_to_pixels(state.canvas.zoom, unit);
        }
    }
    if state.canvas.zoom_mode == ZoomMode::Fit {
        // Fit the largest page in the view, with the gap around it: one zoom for the whole
        // scroll, so it does not jump between pages of different sizes.
        let largest = (0..dims.len()).map(size_of).fold(Vec2::ZERO, Vec2::max);
        let fitted = fit_zoom(largest, ui.available_size() - Vec2::splat(PAGE_GAP * 2.0));
        let fitted = unit.map_or(fitted, |unit| fit_to_pixels(fitted, unit));
        if (fitted - state.canvas.zoom).abs() > f32::EPSILON {
            state.canvas.zoom = fitted;
            ui.ctx().request_repaint();
        }
    }
    // The zoom on screen eases towards `state.canvas.zoom`; the bitmaps are rendered for the
    // target, so a zoom does not ask for a render per frame. The wheel and a pinch zoom at once:
    // egui already spreads the wheel over frames, and easing each small step again would lag
    // behind the hand and stutter.
    let zoom_id = Id::unique("canvas-zoom");
    if wheel_anchor.is_some() {
        motion::reset(ui.ctx(), zoom_id);
    }
    let zoom = motion::value(ui.ctx(), zoom_id, state.canvas.zoom, motion::ZOOM);
    let sizes_at = |zoom: f32| -> Vec<Vec2> { (0..dims.len()).map(|page| size_of(page) * zoom).collect() };
    let view = ui.available_size();
    let layout = layout_pages(&sizes_at(zoom), view);
    let last_view = state.canvas.last_layout.1;
    let shown_zoom = std::mem::replace(&mut state.canvas.shown_zoom, zoom);
    // A new zoom or view size moves the scroll by itself: that is not the user scrolling.
    let relaid = (layout.size, view) != state.canvas.last_layout;
    if relaid {
        state.canvas.last_layout = (layout.size, view);
        state.canvas.settle = SETTLE_FRAMES;
    }

    let mut area = ScrollArea::both().id_salt("canvas").auto_shrink(false);
    // Space held: a drag anywhere pans the pages (egui only drags to scroll on touch screens).
    if super::multi::panning(ui) {
        area = area
            .scroll_source(ScrollSource::ALL)
            .on_hover_cursor(CursorIcon::Grab)
            .on_drag_cursor(CursorIcon::Grabbing);
    }
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
    // A zoom keeps the spot under the pointer (the wheel, a pinch), or at the centre of the view
    // (the buttons, the keys, fitting), where it is. The offset is worked out in this frame's
    // layout and set in this frame, so the pages never show at the new size and the old offset.
    let anchored = if !jumping && shown_zoom > 0.0 && (zoom - shown_zoom).abs() > f32::EPSILON {
        let before = layout_pages(&sizes_at(shown_zoom), last_view);
        anchored_offset(
            &before,
            &layout,
            state.canvas.scroll_offset,
            wheel_anchor.unwrap_or(inner / 2.0),
        )
        .map(|offset| offset.clamp(Vec2::ZERO, max_offset))
    } else {
        None
    };
    if let Some(offset) = anchored {
        area = area.scroll_offset(offset);
    }
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

    // A marquee dragged near or past the edge of the view scrolls the canvas under it.
    if state.canvas.marquee.is_some()
        && !jumping
        && anchored.is_none()
        && let Some(pointer) = ui.input(|i| i.pointer.latest_pos())
    {
        let step = edge_scroll(ui.max_rect(), pointer);
        if step != Vec2::ZERO {
            let offset = (state.canvas.scroll_offset + step).clamp(Vec2::ZERO, max_offset);
            area = area.scroll_offset(offset);
            ui.ctx().request_repaint();
        }
    }

    let preview = state.canvas.preview.take();
    state.canvas.drawn_preview = None;
    // egui fades the edges of a scroll area into its background: over a white page that is a
    // grey band under the ruler. The pages scroll without it.
    let fade = std::mem::replace(&mut ui.spacing_mut().scroll.fade.strength, 0.0);
    let kinetic = ui.spacing().scroll.kinetic;
    if anchored.is_some() {
        // The scroll area checks the offset against last frame's content: zooming in, it would
        // take the new offset for overscroll and spring it back. It would also coast on with the
        // speed of a scroll just before. Neither happens in a frame the zoom places the pages.
        let physics = &mut ui.spacing_mut().scroll.kinetic;
        physics.rubber_band = false;
        physics.decay_time = 0.0;
    }
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
            if let Some(reveal) = overlays.reveal
                && let Some(selected) = overlays.selection
            {
                let outline = geo.rect_to_screen(selected);
                // What the stepper paints only moves the view when it is out of it, or under the
                // floating stepper.
                let seen = above_stepper(ui.ctx(), ui.clip_rect()).contains_rect(outline);
                if reveal == Reveal::Centre || !seen {
                    // Centre the selection next frame (in content coordinates).
                    let centre = outline.center() - origin;
                    state.canvas.reveal_to = Some(centre.to_vec2() - view / 2.0);
                    ui.ctx().request_repaint();
                }
                state.canvas.reveal = None;
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
    ui.spacing_mut().scroll.kinetic = kinetic;
    // The selection is on a page out of view: go there first, reveal it once it is drawn.
    if state.canvas.reveal.is_some()
        && let Some(page) = state.selection_rect().map(|s| s.page)
        && !output.inner.visible.contains(&page)
    {
        state.canvas.scroll_to = Some(page);
        ui.ctx().request_repaint();
    } else if state.canvas.reveal.is_some() && state.selection_rect().is_none() {
        // Nothing to outline (an operator that paints nothing, say): its page is what can be
        // shown, so a page out of view is scrolled to; the zoom stays as it is.
        if let Some(page) = state.selection.primary().and_then(node_page)
            && !output.inner.visible.contains(&page)
        {
            state.canvas.scroll_to = Some(page);
            ui.ctx().request_repaint();
        }
        state.canvas.reveal = None;
    }
    CanvasOutput {
        offset: {
            state.canvas.scroll_view = output.inner_rect.size();
            output.state.clamped_offset()
        },
        ..output.inner
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_marquee_scrolls_the_canvas_only_at_or_past_the_edge_of_the_view() {
        let view = UiRect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));

        assert_eq!(edge_scroll(view, pos2(200.0, 150.0)), Vec2::ZERO);
        // 14 into the bottom band: half of that, downwards.
        assert_eq!(edge_scroll(view, pos2(200.0, 290.0)), vec2(0.0, 7.0));
        // Past the left edge, capped.
        assert_eq!(edge_scroll(view, pos2(-200.0, 150.0)), vec2(-EDGE_SPEED, 0.0));
        // Smoke: a corner scrolls both ways.
        let corner = edge_scroll(view, pos2(410.0, -10.0));
        assert!(corner.x > 0.0 && corner.y < 0.0, "{corner:?}");
    }
}
