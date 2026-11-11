//! The status bar's display item: pages drawn on an emulated Windows screen's device pixels,
//! rounded as the chosen viewer rounds, and back on this Mac's.
//!
//! Update the PNG with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test display`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{app, builder, digest, has_label, open_fixture, wait_for_render};
use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::PdfxrayApp;
use pdfxray_app::emulation::OFF_TEXT;
use pdfxray_app::modes::Pixels;
use pdfxray_app::shell::display::{
    COMPARING_NOTE, DISPLAY_LABEL, EMULATE_LABEL, ENHANCE_LABEL, LINE_WEIGHTS_LABEL, item_text,
};
use pdfxray_core::Document;
use pdfxray_render::{AcrobatPrefs, Display, Emulation, RenderSpec, Viewer, ViewerZoom, render_page};
use pretty_assertions::assert_eq;

/// The digest of page 0 of `fixture` drawn on `emulation`'s screen, as its viewer rounds.
fn emulated_digest(fixture: &str, emulation: Emulation) -> u64 {
    let doc = Document::open(common::fixture_path(fixture)).unwrap();
    let size = pdfxray_render::page_dimensions(&doc, 0).unwrap();
    let spec = RenderSpec {
        viewer: emulation.viewer,
        acrobat: emulation.acrobat,
        ..RenderSpec::new(emulation.scale_for(size))
    };
    pdfxray_app::pixel_digest(&render_page(&doc, 0, &spec).unwrap().rgba)
}

fn click(harness: &mut Harness<'_, PdfxrayApp>, role: Role, name: &str) {
    harness.get_by_role_and_label(role, name).click();
    harness.run();
}

#[test]
fn the_display_item_draws_the_page_on_an_emulated_windows_screen() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    wait_for_render(&mut harness);
    assert_eq!(item_text(&harness.state().state), OFF_TEXT);
    let mac = digest(&harness);

    click(&mut harness, Role::Button, DISPLAY_LABEL);
    click(&mut harness, Role::Switch, EMULATE_LABEL);
    wait_for_render(&mut harness);

    // Chrome at 100 % on a 1920 × 1080 screen at 96 DPI: the defaults.
    let chrome = Emulation {
        viewer: Some(Viewer::Chrome),
        display: Display::default(),
        zoom: ViewerZoom::Percent(100),
        acrobat: AcrobatPrefs::default(),
    };
    let state = &harness.state().state;
    assert_eq!(item_text(state), "Chrome · 1920×1080 · 96 DPI · 100%");
    assert_eq!(
        state.shown_request().map(|r| r.pixels),
        Some(Pixels::Emulated(chrome))
    );
    assert_eq!(digest(&harness), emulated_digest("minimal-one-page", chrome));
    assert!(
        has_label(&harness, "1.333 device px per pt · page 266 × 133 px"),
        "the popover says what the screen does to the page"
    );

    // ⌘0 is 1×: each device pixel one display pixel, the page as wide as its 266 px bitmap.
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    let ppp = harness.ctx.pixels_per_point();
    let unit = 96.0 / 72.0 / ppp;
    assert!((harness.state().state.canvas.zoom - unit).abs() < 1e-4);
    let shown = |h: &Harness<'_, PdfxrayApp>| {
        h.get_by_role_and_label(Role::Image, "Page 1 canvas")
            .rect()
            .width()
            * ppp
    };
    assert!((shown(&harness) - 266.0).abs() < 0.01, "{}", shown(&harness));
    // Zooming in goes to 2×: every device pixel two display pixels.
    harness.state_mut().state.zoom_in();
    harness.run_steps(30);
    assert!((shown(&harness) - 532.0).abs() < 0.01, "{}", shown(&harness));

    // Acrobat at 150 % scaling.
    click(&mut harness, Role::Button, "Acrobat");
    click(&mut harness, Role::Button, "150%");
    wait_for_render(&mut harness);

    let acrobat = Emulation {
        viewer: Some(Viewer::Acrobat),
        display: Display {
            dpi: 144,
            ..Display::default()
        },
        ..chrome
    };
    assert_eq!(
        item_text(&harness.state().state),
        "Acrobat · 1920×1080 · 144 DPI · 100%"
    );
    assert_eq!(digest(&harness), emulated_digest("minimal-one-page", acrobat));
    assert!(has_label(&harness, "2 device px per pt · page 400 × 200 px"));
    // Acrobat's thin-line preferences show for Acrobat, both on as it starts.
    assert!(common::is_selected(
        &harness.get_by_role_and_label(Role::Switch, LINE_WEIGHTS_LABEL)
    ));
    assert!(common::is_selected(
        &harness.get_by_role_and_label(Role::Switch, ENHANCE_LABEL)
    ));
    click(&mut harness, Role::Switch, ENHANCE_LABEL);
    wait_for_render(&mut harness);
    let acrobat = Emulation {
        acrobat: AcrobatPrefs {
            enhance_thin_lines: false,
            line_weights: true,
        },
        ..acrobat
    };
    assert_eq!(digest(&harness), emulated_digest("minimal-one-page", acrobat));

    // Smoke: off again (and fitting, as at first), the page is back on the Mac's pixels and
    // the settings are kept.
    harness.state_mut().state.fit_page();
    click(&mut harness, Role::Switch, EMULATE_LABEL);
    wait_for_render(&mut harness);
    let state = &harness.state().state;
    assert_eq!(item_text(state), OFF_TEXT);
    assert!(matches!(
        state.shown_request().map(|r| r.pixels),
        Some(Pixels::Canvas(_))
    ));
    assert_eq!(state.screen.settings, acrobat);
    assert_eq!(digest(&harness), mac);
    assert!(!has_label(&harness, COMPARING_NOTE));
}

#[test]
fn snapshot_display_emulation() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "graphics-state");
    {
        let state = &mut harness.state_mut().state;
        state.screen.on = true;
        state.screen.settings.viewer = Some(Viewer::Acrobat);
        state.screen.settings.display.dpi = 120;
        // Magnified, so the emulated screen's pixels show.
        state.set_zoom(4.0);
    }
    harness.run_steps(4);
    wait_for_render(&mut harness);
    click(&mut harness, Role::Button, DISPLAY_LABEL);
    harness.remove_cursor();
    harness.run_steps(4);

    harness.snapshot("app_display_emulation");
}
