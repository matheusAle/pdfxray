//! Look and feel: design tokens, fonts, the egui style built from them, the shared surface
//! effects and the motion helpers.

pub mod effects;
pub mod fonts;
pub mod motion;
pub mod style;
pub mod tokens;

use egui::{Context, ThemePreference};

/// Installs everything the app needs from `ctx` before the first frame: the dark theme, the
/// fonts, the style and the image loaders that draw the SVG icons.
pub fn install(ctx: &Context) {
    ctx.set_theme(ThemePreference::Dark);
    fonts::install(ctx);
    style::install(ctx);
    // Reads the macOS setting once, here, so no frame pays for it.
    motion::reduce_motion();
    egui_extras::install_image_loaders(ctx);
}
