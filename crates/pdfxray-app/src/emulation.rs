//! The emulated screen: the page drawn on another screen's device pixels (its size and DPI, at a
//! viewer's zoom), rounded as a Windows viewer rounds, set from the status bar.
//!
//! The rounding itself is `pdfxray_render::emulate`'s. Here are the settings, kept for the
//! session whatever document is open, and what they say.

use pdfxray_core::{Document, PageIx};
use pdfxray_render::{AcrobatPrefs, Display, Emulation, Viewer, ViewerZoom, page_dimensions};

use crate::state::AppState;

/// The screen sizes offered in one click, in device pixels.
pub const SCREENS: [(u32, u32); 4] = [(1366, 768), (1920, 1080), (2560, 1440), (3840, 2160)];

/// Windows' display scaling steps, in percent; the DPI is 96 times the step.
pub const SCALINGS: [u32; 5] = [100, 125, 150, 175, 200];

/// The zoom percentages a viewer is set to, from 10 % to 6400 %.
pub const ZOOM_RANGE: std::ops::RangeInclusive<u32> = 10..=6400;

/// The DPI a screen can have: from 48, half of 100 %, to 480, 500 %.
pub const DPI_RANGE: std::ops::RangeInclusive<u32> = 48..=480;

/// The pixels a side of the screen can have.
pub const SIDE_RANGE: std::ops::RangeInclusive<u32> = 320..=16_384;

/// What the status bar says while pages are drawn on this Mac's own pixels.
pub const OFF_TEXT: &str = "Mac display";

/// The emulated screen's settings, and whether pages are drawn on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenEmulation {
    /// Whether pages are drawn on the emulated screen.
    pub on: bool,
    /// The screen, the viewer's zoom and the viewer; kept while it is off.
    pub settings: Emulation,
}

impl Default for ScreenEmulation {
    /// Off; when turned on, Chrome at 100 % on a 1920 × 1080 screen at 96 DPI.
    fn default() -> Self {
        Self {
            on: false,
            settings: Emulation {
                viewer: Some(Viewer::Chrome),
                display: Display::default(),
                zoom: ViewerZoom::default(),
                acrobat: AcrobatPrefs::default(),
            },
        }
    }
}

/// The DPI of Windows' `percent` display scaling.
pub const fn dpi_of(percent: u32) -> u32 {
    percent * pdfxray_render::emulate::BASE_DPI / 100
}

/// Who draws: the viewer's name, or pdfxray's when no viewer is emulated.
pub fn viewer_name(viewer: Option<Viewer>) -> &'static str {
    viewer.map_or("pdfxray", Viewer::name)
}

/// The status bar's summary: `Chrome · 1920×1080 · 120 DPI · 100%`.
pub fn summary(emulation: &Emulation) -> String {
    let Display { width, height, dpi } = emulation.display;
    format!(
        "{} · {width}×{height} · {dpi} DPI · {}",
        viewer_name(emulation.viewer),
        emulation.zoom
    )
}

/// How the viewer rounds, in a sentence.
pub fn rounding_text(viewer: Option<Viewer>) -> &'static str {
    match viewer.map(Viewer::engine) {
        Some(pdfxray_render::emulate::Engine::Pdfium) => {
            "PDFium: each glyph on a whole pixel, rectangular clips grown out to whole pixels, \
             upright images on whole pixels, no stroke under 1 px, fills without area a faint \
             1 px line. No snapping, overlap or stroke adjustment."
        }
        Some(pdfxray_render::emulate::Engine::Adobe) => {
            "Adobe AGM: coordinates within 0.0001 px snap to the pixel, fills overlap 0.01 px, \
             stroke edges go on pixel boundaries (a thin line is 1 or 2 px), text advances are \
             rounded, form fields are painted last."
        }
        None => "pdfxray's own rounding, on the screen's device pixels.",
    }
}

/// What the screen does to page `page` of `doc`: `2.08 device px per pt · page 1275 × 1650 px`.
/// `None` when the page cannot be measured.
pub fn scale_text(emulation: &Emulation, doc: &Document, page: PageIx) -> Option<String> {
    let size = page_dimensions(doc, page).ok()?;
    let scale = emulation.scale_for(size);
    let pixels = |points: f32| (points * scale).trunc();
    Some(format!(
        "{} device px per pt · page {} × {} px",
        trim(f64::from(scale)),
        pixels(size.0),
        pixels(size.1)
    ))
}

/// `value` with at most three decimals, trailing zeros dropped.
fn trim(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// What a length on the page is measured against on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct OnScreen {
    /// Pixels per point.
    pub pixels_per_point: f64,
    /// Where those pixels are: `136%`, or `Chrome 100%, 120 DPI` on an emulated screen.
    pub at: String,
}

impl AppState {
    /// The emulated screen pages are drawn on, while it is on.
    pub fn emulation(&self) -> Option<Emulation> {
        self.screen.on.then_some(self.screen.settings)
    }

    /// The pixels a length on the current page covers: the emulated screen's device pixels while
    /// it is on, the canvas's points at its zoom otherwise.
    pub fn on_screen(&self) -> OnScreen {
        let canvas = OnScreen {
            pixels_per_point: f64::from(self.canvas.zoom),
            at: format!("{:.0}%", self.canvas.zoom * 100.0),
        };
        let (Some(emulation), Some(loaded)) = (self.emulation(), self.document.as_ref()) else {
            return canvas;
        };
        let Ok(size) = page_dimensions(&loaded.doc, self.canvas.page) else {
            return canvas;
        };
        OnScreen {
            pixels_per_point: f64::from(emulation.scale_for(size)),
            at: format!(
                "{} {:.0}%, {} DPI",
                viewer_name(emulation.viewer),
                emulation.zoom_for(size) * 100.0,
                emulation.display.dpi
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::install_fixture;

    #[test]
    fn the_summary_names_the_viewer_the_screen_and_the_zoom() {
        let mut emulation = ScreenEmulation::default().settings;
        assert_eq!(summary(&emulation), "Chrome · 1920×1080 · 96 DPI · 100%");
        emulation.viewer = None;
        emulation.display.dpi = dpi_of(150);
        emulation.zoom = ViewerZoom::FitWidth;
        assert_eq!(summary(&emulation), "pdfxray · 1920×1080 · 144 DPI · fit width");
        // Smoke: every scaling step is a whole DPI.
        assert_eq!(SCALINGS.map(dpi_of), [96, 120, 144, 168, 192]);
    }

    #[test]
    fn a_wheel_zoom_follows_the_hand_then_settles_on_a_whole_pixel_step() {
        let unit = 96.0 / 72.0;
        let mut state = AppState::default();
        state.canvas.pixel_unit = Some(unit);
        state.canvas.zoom = unit;
        let pointer = egui::vec2(10.0, 20.0);

        // The wheel zooms as far as the hand goes, off the steps.
        state.wheel_zoom(1.5, 1.0, pointer);
        state.wheel_zoom(1.25, 1.05, pointer);
        let followed = state.canvas.zoom;
        assert!((followed - unit * 1.875).abs() < 1e-4, "{followed}");
        // Too soon after it moved: nothing settles yet.
        assert_eq!(state.settle_zoom(1.1), None);
        assert!((state.canvas.zoom - followed).abs() < f32::EPSILON);

        // Once the hand has stopped, the zoom settles on 2×, anchored at the pointer.
        assert_eq!(state.settle_zoom(1.25), Some(pointer));
        assert!((state.canvas.zoom - 2.0 * unit).abs() < 1e-4);
        assert_eq!(state.settle_zoom(2.0), None, "settled once");
        // Smoke: on this display's own pixels the wheel has nothing to settle.
        state.canvas.pixel_unit = None;
        state.wheel_zoom(1.1, 3.0, pointer);
        assert_eq!(state.settle_zoom(4.0), None);
    }

    #[test]
    fn on_an_emulated_screen_a_length_is_measured_in_its_device_pixels() {
        // minimal-one-page is 200 × 100 pt.
        let mut state = AppState::default();
        install_fixture(&mut state, "minimal-one-page");
        state.canvas.zoom = 1.36;

        let canvas = state.on_screen();
        state.screen.on = true;
        state.screen.settings.display.dpi = 120;
        let emulated = state.on_screen();

        assert_eq!(canvas.at, "136%");
        assert!((canvas.pixels_per_point - 1.36).abs() < 1e-6);
        assert_eq!(emulated.at, "Chrome 100%, 120 DPI");
        assert!((emulated.pixels_per_point - 120.0 / 72.0).abs() < 1e-6);
        let doc = &state.document.as_ref().unwrap().doc;
        assert_eq!(
            scale_text(&state.screen.settings, doc, 0).as_deref(),
            Some("1.667 device px per pt · page 333 × 166 px")
        );
        // Smoke: turned off, the settings stay for next time.
        state.screen.on = false;
        assert_eq!(state.emulation(), None);
        assert_eq!(state.screen.settings.display.dpi, 120);
    }
}
