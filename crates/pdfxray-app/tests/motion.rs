//! Motion on the shared components: popovers and tooltips fade and scale in over 140 ms, and
//! "Reduce motion" makes them appear at once. This file is its own test binary because the
//! reduce-motion flag is process-wide.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

use std::sync::{Mutex, MutexGuard, PoisonError};

use egui::{RectAlign, Role, vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, HarnessBuilder};
use pdfxray_app::components::{Icon, ToolButton, popover, tooltip};
use pdfxray_app::theme::{motion, tokens};

static FLAG: Mutex<()> = Mutex::new(());

/// Held by every test: they flip the process-wide flag, so they run one at a time. Sets it to
/// `reduce` for the test.
fn motion_is(reduce: bool) -> MutexGuard<'static, ()> {
    let guard = FLAG.lock().unwrap_or_else(PoisonError::into_inner);
    motion::set_reduce_motion(reduce);
    guard
}

/// 40 ms per step: 140 ms is three and a half steps.
fn builder() -> HarnessBuilder<()> {
    Harness::builder()
        .with_size(vec2(420.0, 260.0))
        .with_step_dt(0.04)
        .with_context_setup(pdfxray_app::theme::install)
}

/// Every shape of the last frame with the scale its layer gets after tessellation (the growth of
/// an opening popover), groups flattened.
fn shapes_of(harness: &Harness<'_, ()>) -> Vec<(egui::Shape, f32)> {
    fn flatten(shape: &egui::Shape, scale: f32, out: &mut Vec<(egui::Shape, f32)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| flatten(shape, scale, out)),
            other => out.push((other.clone(), scale)),
        }
    }
    let mut out = Vec::new();
    for clipped in &harness.output().shapes {
        flatten(
            &clipped.shape,
            clipped.transform_after_tessellation.scaling,
            &mut out,
        );
    }
    out
}

/// The alpha of the popover's fill and the scale its layer is drawn at, after the last step.
fn popover_state(harness: &Harness<'_, ()>) -> Option<(u8, f32)> {
    let content = harness.query_by_label("Revisions")?.rect();
    let shapes = shapes_of(harness);
    let fill = shapes.iter().find_map(|(shape, _)| match shape {
        egui::Shape::Rect(r) if r.rect.contains_rect(content) && r.blur_width == 0.0 && r.fill.a() > 0 => {
            Some(r.fill.a())
        }
        _ => None,
    })?;
    let scale = shapes.iter().find_map(|(shape, scale)| match shape {
        egui::Shape::Text(t) if t.galley.text() == "Revisions" => Some(*scale),
        _ => None,
    })?;
    Some((fill, scale))
}

fn popover_app() -> Harness<'static, ()> {
    let mut harness = builder().build_ui(|ui| {
        let chip = ui.button("rev 3 of 3");
        popover::show(&chip, RectAlign::BOTTOM_START, 180.0, |ui| {
            ui.label("Revisions");
        });
    });
    harness.run();
    harness
}

#[test]
fn a_popover_fades_in_and_grows_from_97_percent_over_140_ms() {
    let _flag = motion_is(false);
    let mut harness = popover_app();
    harness.get_by_role_and_label(Role::Button, "rev 3 of 3").click();

    let mut seen = Vec::new();
    let mut text_opacity = Vec::new();
    for _ in 0..10 {
        harness.step();
        text_opacity.extend(shapes_of(&harness).iter().find_map(|(shape, _)| match shape {
            egui::Shape::Text(t) if t.galley.text() == "Revisions" => Some(t.fallback_color.a()),
            _ => None,
        }));
        if let Some(state) = popover_state(&harness) {
            seen.push(state);
        }
    }

    let full = tokens::POPOVER_FILL.a();
    let (_, first_scale) = *seen.first().expect("the popover was drawn");
    let (last_fill, last_scale) = *seen.last().unwrap();
    // It starts translucent and slightly smaller...
    assert!(
        seen.iter().any(|(fill, _)| *fill < full),
        "never translucent: {seen:?}"
    );
    assert!(
        (motion::POPOVER_FROM_SCALE - 1e-3..1.0).contains(&first_scale),
        "{first_scale}"
    );
    // ...never gets more transparent or smaller while it opens...
    assert!(
        seen.windows(2).all(|w| w[1].0 >= w[0].0 && w[1].1 >= w[0].1),
        "{seen:?}"
    );
    // ...and ends fully opaque at its own size.
    assert_eq!((last_fill, last_scale), (full, 1.0));
    // The text fades with the panel.
    // (Its full alpha is whatever the glyphs have at rest, so it is compared with itself.)
    let rest = *text_opacity.last().expect("the text was drawn");
    assert!(
        text_opacity.first().is_some_and(|first| *first < rest),
        "{text_opacity:?}"
    );
    assert!(text_opacity.windows(2).all(|w| w[1] >= w[0]), "{text_opacity:?}");
    assert!(text_opacity.iter().all(|a| *a <= rest), "{text_opacity:?}");
    // Smoke: the content is the popover's.
    let _label = harness.get_by_label("Revisions");
}

#[test]
fn closing_and_opening_again_plays_the_animation_again() {
    let _flag = motion_is(false);
    let mut harness = popover_app();
    for round in 0..2 {
        harness.get_by_role_and_label(Role::Button, "rev 3 of 3").click();
        harness.run_steps(2);
        let (fill, _) = popover_state(&harness).unwrap_or((0, 0.0));
        assert!(
            fill < tokens::POPOVER_FILL.a(),
            "round {round}: it must start again, not appear at once"
        );
        harness.run_steps(8);
        assert_eq!(popover_state(&harness).unwrap().0, tokens::POPOVER_FILL.a());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(harness.query_by_label("Revisions").is_none());
    }
}

#[test]
fn with_reduce_motion_the_popover_is_there_at_once_and_at_full_size() {
    let _flag = motion_is(true);
    let mut harness = popover_app();
    harness.get_by_role_and_label(Role::Button, "rev 3 of 3").click();

    let mut first = None;
    for _ in 0..4 {
        harness.step();
        if first.is_none() {
            first = popover_state(&harness);
        }
    }
    assert_eq!(first, Some((tokens::POPOVER_FILL.a(), 1.0)));
    // Smoke: the switch is the one the tests set.
    assert!(motion::reduce_motion());
}

#[test]
fn a_tooltip_grows_in_too_and_reduce_motion_skips_it() {
    for (reduce, expect_instant) in [(false, false), (true, true)] {
        let _flag = motion_is(reduce);
        let mut harness = builder().build_ui(|ui| {
            let response = ToolButton::new("Measure")
                .icon(Icon::Measure)
                .icon_only(true)
                .show(ui);
            tooltip::with_shortcut(response, "Measure distances", "M");
        });
        harness.run();
        let button = harness
            .get_by_role_and_label(Role::Button, "Measure")
            .rect()
            .center();
        harness.hover_at(button);

        let mut scales = Vec::new();
        for _ in 0..24 {
            harness.step();
            let text = shapes_of(&harness).iter().find_map(|(shape, scale)| match shape {
                egui::Shape::Text(t) if t.galley.text() == "Measure distances" => Some(*scale),
                _ => None,
            });
            scales.extend(text);
        }
        let first = *scales.first().expect("the tooltip showed");
        // It starts at 0.97 and settles at 1, or is at 1 from the first frame.
        assert_eq!(first == 1.0, expect_instant, "reduce={reduce}: {scales:?}");
        assert!(first >= motion::POPOVER_FROM_SCALE - 1e-3);
        assert_eq!(*scales.last().unwrap(), 1.0, "it ends at full size: {scales:?}");
    }
}
