//! The logo and what the app says about itself: the window icon, the in-app logo, the version, the
//! licence and where the project lives.
//!
//! The artwork is the official logo exported from the design file into `assets/icon/` (see
//! `assets/icon/SOURCES.md`). It is shown from those PNGs, never redrawn in code: the detailed
//! master above [`SMALL_MAX`], the coarse-stripe small version at and below it.

use std::sync::Arc;

use egui::{
    ColorImage, Context, IconData, Id, Image, Response, Sense, TextureFilter, TextureHandle, TextureOptions,
    Ui, vec2,
};

/// The window icon: the 256 px entry of the app icon.
pub const WINDOW_ICON_PNG: &[u8] = include_bytes!("../../../assets/icon/window-icon-256.png");

/// The detailed logo (288 px), for display sizes above [`SMALL_MAX`].
pub const LOGO_MASTER_PNG: &[u8] = include_bytes!("../../../assets/icon/logo-master.png");

/// The coarse-stripe logo (256 px), for display sizes up to [`SMALL_MAX`].
pub const LOGO_SMALL_PNG: &[u8] = include_bytes!("../../../assets/icon/logo-small.png");

/// The largest display size, in points, that uses the small logo.
pub const SMALL_MAX: f32 = 48.0;

/// Accessible name of every logo in the window.
pub const LOGO_LABEL: &str = "pdfxray logo";

/// The app's name.
pub const NAME: &str = "pdfxray";

/// The version of the app.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The licence of the app.
pub const LICENSE: &str = "MIT License";

/// The copyright line of the About panel.
pub const COPYRIGHT: &str = "Copyright \u{a9} 2026 pdfxray contributors";

/// Where the project lives.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The two drawings of the logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoArt {
    /// Grid, guides, crosshairs and the fine hatch: legible above [`SMALL_MAX`].
    Master,
    /// Coarse stripes only: legible at small sizes.
    Small,
}

impl LogoArt {
    /// The drawing for a logo shown `size` points wide.
    pub fn for_size(size: f32) -> Self {
        if size <= SMALL_MAX {
            Self::Small
        } else {
            Self::Master
        }
    }

    /// The PNG of the drawing.
    pub const fn png(self) -> &'static [u8] {
        match self {
            Self::Master => LOGO_MASTER_PNG,
            Self::Small => LOGO_SMALL_PNG,
        }
    }

    const fn texture_name(self) -> &'static str {
        match self {
            Self::Master => "pdfxray-logo-master",
            Self::Small => "pdfxray-logo-small",
        }
    }
}

/// The window icon, ready for [`egui::ViewportBuilder::with_icon`]. Every platform shows it: the
/// title bar or taskbar on Windows and Linux, the Dock while the app runs on macOS.
pub fn window_icon() -> Arc<IconData> {
    // The PNG is ours and checked by a test; an empty icon leaves the platform's default.
    Arc::new(eframe::icon_data::from_png_bytes(WINDOW_ICON_PNG).unwrap_or_default())
}

/// `art` as straight-alpha RGBA bytes with its width and height, or `None` if it cannot be
/// decoded.
pub fn logo_rgba(art: LogoArt) -> Option<(Vec<u8>, u32, u32)> {
    let logo = image::load_from_memory(art.png()).ok()?.to_rgba8();
    let (width, height) = logo.dimensions();
    Some((logo.into_raw(), width, height))
}

/// `art` as a texture, decoded once per context. Mipmapped, so the large PNG stays smooth when
/// it is drawn small.
pub fn logo_texture(ctx: &Context, art: LogoArt) -> Option<TextureHandle> {
    let id = Id::unique(art.texture_name());
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let (rgba, width, height) = logo_rgba(art)?;
    let size = [usize::try_from(width).ok()?, usize::try_from(height).ok()?];
    let image = ColorImage::from_rgba_unmultiplied(size, &rgba);
    let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
    let texture = ctx.load_texture(art.texture_name(), image, options);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    Some(texture)
}

/// The logo, `size` points square, in the drawing that suits that size. Decorative: it reads as
/// an image named [`LOGO_LABEL`] and only senses hover (for a tooltip).
pub fn logo(ui: &mut Ui, size: f32) -> Response {
    let Some(texture) = logo_texture(ui.ctx(), LogoArt::for_size(size)) else {
        return ui.allocate_response(vec2(size, size), Sense::hover());
    };
    ui.add(
        Image::new((texture.id(), vec2(size, size)))
            .alt_text(LOGO_LABEL)
            .sense(Sense::hover()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_icon_is_the_256_px_artwork() {
        let icon = window_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // the corners are transparent (the tile sits on Apple's grid with room for its shadow)
        assert_eq!(icon.rgba[3], 0);
        // smoke: it is not the empty fallback
        assert_ne!(*icon, IconData::default());
    }

    #[test]
    fn small_sizes_use_the_coarse_drawing_and_large_ones_the_master() {
        assert_eq!(LogoArt::for_size(28.0), LogoArt::Small);
        assert_eq!(LogoArt::for_size(48.0), LogoArt::Small);
        assert_eq!(LogoArt::for_size(48.5), LogoArt::Master);
        assert_eq!(LogoArt::for_size(72.0), LogoArt::Master);
        // both drawings decode, at the sizes the design exports
        let size = |art| logo_rgba(art).map(|(_, width, height)| (width, height));
        assert_eq!(size(LogoArt::Master), Some((288, 288)));
        assert_eq!(size(LogoArt::Small), Some((256, 256)));
        // smoke: the master is not blank
        let (rgba, _, _) = logo_rgba(LogoArt::Master).expect("master decodes");
        assert!(rgba.as_chunks::<4>().0.iter().any(|px| px[3] == 255));
    }
}
