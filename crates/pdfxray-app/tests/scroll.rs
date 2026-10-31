//! The canvas is one vertical scroll of every page: scrolling decides the current page, a click
//! on another page makes it current, and the page controls scroll instead of swapping the page.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod common;

use common::{Region, app, click_at, nodes_in, open_fixture, the, wait_for_render, wait_until};
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Modifiers, MouseWheelUnit, Rect, Role, TextureId, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_app::center::canvas::PAGE_GAP;
use pretty_assertions::assert_eq;

/// Where the image of page `page` (zero-based) is, if it is drawn.
fn page_rect(harness: &Harness<'_, PdfxrayApp>, page: u32) -> Option<Rect> {
    harness
        .query_by_role_and_label(Role::Image, &format!("Page {} canvas", page + 1))
        .map(|node| node.rect())
}

/// The text of the toolbar's page field.
fn page_field(harness: &Harness<'_, PdfxrayApp>) -> String {
    nodes_in(harness, Region::Center, Role::TextInput)
        .remove(0)
        .value()
        .unwrap_or_default()
}

fn wheel(harness: &mut Harness<'_, PdfxrayApp>, dy: f32) {
    harness.event(Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    });
}

#[test]
fn every_page_is_stacked_in_one_scroll_with_a_gap_between_them() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.set_zoom(0.4);
    harness.run_steps(3);
    wait_for_render(&mut harness);

    // All three pages are drawn, top to bottom, PAGE_GAP apart, centred across.
    let rects: Vec<Rect> = (0..3).map(|page| page_rect(&harness, page).unwrap()).collect();
    for pair in rects.windows(2) {
        assert!(
            (pair[1].top() - pair[0].bottom() - PAGE_GAP).abs() < 1.0,
            "gap between {:?} and {:?}",
            pair[0],
            pair[1]
        );
        assert!((pair[1].center().x - pair[0].center().x).abs() < 1.0);
    }
    // Smoke: drawing the other pages did not change the current one.
    assert_eq!(harness.state().state.canvas.page, 0);
    assert_eq!(page_field(&harness), "1");
}

#[test]
fn scrolling_down_makes_the_page_that_fills_the_view_current() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    wait_for_render(&mut harness);
    let first = page_rect(&harness, 0).unwrap();
    assert_eq!(harness.state().state.canvas.page, 0);

    // One page height and a gap down: page 2 fills the view.
    harness.hover_at(first.center());
    wheel(&mut harness, -(first.height() + PAGE_GAP));
    wait_until(&mut harness, "page 2 to become current", |state| {
        state.canvas.page == 1
    });
    wait_for_render(&mut harness);

    assert_eq!(page_field(&harness), "2");
    assert!(harness.state().state.canvas.scroll_offset.y > first.height() / 2.0);
    // Page 2 is on screen and rendered, and the render the tests compare against is its own.
    let second = page_rect(&harness, 1).unwrap();
    assert!(second.top() < first.bottom(), "{second:?}");
    assert_eq!(harness.state().state.shown_request().map(|r| r.page), Some(1));
    // Smoke: scrolling back up returns to page 1.
    harness.hover_at(second.center());
    wheel(&mut harness, first.height() + PAGE_GAP);
    wait_until(&mut harness, "page 1 to become current again", |state| {
        state.canvas.page == 0
    });
    assert_eq!(page_field(&harness), "1");
}

#[test]
fn clicking_a_page_below_makes_it_current_without_scrolling() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.set_zoom(0.4);
    harness.run_steps(3);
    wait_for_render(&mut harness);
    let offset = harness.state().state.canvas.scroll_offset;

    let second = page_rect(&harness, 1).unwrap();
    click_at(&mut harness, second.center());

    let state = &harness.state().state;
    assert_eq!(state.canvas.page, 1);
    // The hit stack is page 2's, and the click selected something on it.
    assert_eq!(state.drawer.hit_stack.as_ref().map(|stack| stack.page), Some(1));
    assert!(
        state
            .selection
            .primary
            .as_ref()
            .is_some_and(|id| id.to_string().contains(":1")),
        "{:?}",
        state.selection.primary
    );
    // Smoke: the view stayed where it was.
    assert_eq!(state.canvas.scroll_offset, offset);
    assert_eq!(page_field(&harness), "2");
}

/// The shapes painted last frame, flattened, in paint order.
fn painted(harness: &Harness<'_, PdfxrayApp>) -> Vec<Shape> {
    fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
        if let Shape::Vec(shapes) = shape {
            for shape in shapes {
                flatten(shape, out);
            }
        } else {
            out.push(shape.clone());
        }
    }
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut out);
    }
    out
}

#[test]
fn no_page_shadow_falls_on_a_page_whichever_page_is_current() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.set_zoom(0.4);
    harness.run_steps(3);
    wait_for_render(&mut harness);
    // The last page is current, so it is drawn first, before the pages above it.
    let third = page_rect(&harness, 2).unwrap();
    click_at(&mut harness, third.center());
    harness.run_steps(2);
    assert_eq!(harness.state().state.canvas.page, 2);

    let shapes = painted(&harness);
    let page_rects: Vec<Rect> = (0..3).map(|page| page_rect(&harness, page).unwrap()).collect();
    // A page bitmap: a textured shape where one of the pages is.
    let bitmap_at = |shape: &Shape| match shape {
        Shape::Mesh(mesh) if mesh.texture_id != TextureId::default() => Some(mesh.calc_bounds()),
        Shape::Rect(rect) if rect.brush.is_some() => Some(rect.rect),
        _ => None,
    };
    let is_page = |shape: &Shape| {
        bitmap_at(shape).is_some_and(|at| page_rects.iter().any(|rect| (at.min - rect.min).length() < 1.0))
    };
    let is_page_shadow = |shape: &Shape| matches!(shape, Shape::Rect(rect) if rect.blur_width == 70.0);
    let shadows: Vec<usize> = (0..shapes.len())
        .filter(|&i| is_page_shadow(&shapes[i]))
        .collect();
    let pages: Vec<usize> = (0..shapes.len()).filter(|&i| is_page(&shapes[i])).collect();

    // Every page's far shadow is painted, and all of them before the first page bitmap.
    assert_eq!(shadows.len(), 3, "{shadows:?}");
    assert_eq!(pages.len(), 3, "{pages:?}");
    assert!(
        shadows.iter().max() < pages.iter().min(),
        "shadows {shadows:?} pages {pages:?}"
    );
    // Smoke: the current page is still the one drawn first.
    let first = bitmap_at(&shapes[pages[0]]).unwrap();
    assert!((first.min - page_rects[2].min).length() < 1.0, "{first:?}");
}

#[test]
fn the_pages_scroll_under_the_ruler_without_a_fade_over_them() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    wait_for_render(&mut harness);
    let before = page_rect(&harness, 0).unwrap();
    wheel(&mut harness, -300.0);
    harness.run_steps(4);
    wait_for_render(&mut harness);
    assert!(harness.state().state.canvas.scroll_offset.y > 0.0);

    // A fade is an untextured gradient: its corners are not all one colour.
    let page = page_rect(&harness, 0).unwrap();
    let fades: Vec<Rect> = painted(&harness)
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh)
                if mesh.texture_id == TextureId::default()
                    && mesh.vertices.iter().any(|v| v.color != mesh.vertices[0].color) =>
            {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .filter(|bounds| bounds.intersects(page))
        .collect();
    assert_eq!(fades, Vec::<Rect>::new());
    // Smoke: the page under the ruler is drawn, scrolled up from where it was.
    assert!(page.top() < before.top() - 100.0, "{page:?} vs {before:?}");
}

#[test]
fn the_page_buttons_scroll_the_next_page_to_the_top() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    wait_for_render(&mut harness);
    let top = page_rect(&harness, 0).unwrap().top();

    the(&harness, Region::Center, Role::Button, "Next").click();
    harness.run_steps(3);
    wait_for_render(&mut harness);

    // Page 2's top edge is where page 1's was: one page and a gap further down the scroll.
    let second = page_rect(&harness, 1).unwrap();
    assert!((second.top() - top).abs() < 1.0, "{second:?} vs {top}");
    assert_eq!(harness.state().state.canvas.page, 1);
    // Smoke: the page field follows.
    assert_eq!(page_field(&harness), "2");
}

/// The width of the toolbar's page field with `fixture` open.
fn page_field_width(fixture: &str) -> f32 {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    harness.run_steps(3);
    nodes_in(&harness, Region::Center, Role::TextInput)
        .remove(0)
        .rect()
        .width()
}

#[test]
fn the_page_field_has_room_for_every_digit_of_the_last_page() {
    // `18` of `40` was cut to `1` and half of an `8`: the field grows with the digits of the
    // page count, and a single digit keeps the compact field.
    let one = page_field_width("minimal-one-page");
    let forty = page_field_width("long-doc");

    assert_eq!(one, 26.0);
    assert!(forty > one + 2.0, "{forty} next to {one}");
    // Smoke: it is still a compact field, not a stretched one.
    assert!(forty < 48.0, "{forty}");
}
