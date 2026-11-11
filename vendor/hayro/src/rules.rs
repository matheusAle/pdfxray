//! Device-space adjustments: how a viewer places what lands between device pixels.
//!
//! Not part of upstream hayro: see `PATCHES.md`. Every method receives geometry in device space
//! (pixels, y down) just before it is rasterised and may move it. The defaults change nothing,
//! so [`crate::render_into`] draws exactly as upstream does.

use hayro_interpret::font::OutlineGlyph;
use kurbo::{Affine, BezPath, Point};

/// Rules a renderer applies to device-space geometry before rasterising it.
///
/// They apply to the page's content and its soft masks. They do not apply inside a tiling
/// pattern's cell, nor to glyph outlines and Type 3 glyph content (only to where a glyph is put,
/// [`PixelRules::glyph_origins`]).
pub trait PixelRules {
    /// Whether the rules change anything; `false` keeps hayro's own fast paths.
    fn active(&self) -> bool {
        true
    }

    /// A path about to be filled.
    fn fill(&self, _path: &mut BezPath) {}

    /// How far every fill reaches past its edge on each side, in device pixels.
    fn fill_overlap(&self) -> f64 {
        0.0
    }

    /// The opacity a fill that encloses no area (a line drawn there and back, a rectangle of
    /// zero width) is drawn with, as a 1 pixel line; `None` draws nothing, as upstream.
    fn thin_fill_alpha(&self) -> Option<f32> {
        None
    }

    /// A path about to be stroked, and the stroke, both in device space. The stroke's width
    /// arrives as the PDF asks, before upstream's minimum of one pixel: the rules set their own.
    fn stroke(&self, _path: &mut BezPath, _stroke: &mut DeviceStroke) {}

    /// A clip path about to be pushed.
    fn clip(&self, _path: &mut BezPath) {}

    /// The origins of one run's glyphs, in the order they are drawn.
    fn glyph_origins(&self, _origins: &mut [Point]) {}

    /// `glyph`'s outline as the viewer draws it at `device` (glyph space to device pixels, the
    /// glyph's origin already placed): hinted to the pixel grid, in the glyph space of
    /// [`OutlineGlyph::outline`]. `None` keeps the outline as it is.
    fn glyph_outline(&self, _glyph: &OutlineGlyph, _device: Affine) -> Option<BezPath> {
        None
    }

    /// The map from an image's `width` x `height` sample grid to device space.
    fn image(&self, transform: Affine, _width: u32, _height: u32) -> Affine {
        transform
    }
}

/// A stroke in device space, as [`PixelRules::stroke`] sees and changes it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceStroke {
    /// Width in device pixels.
    pub width: f64,
    /// Width in user space, as the `w` operator set it.
    pub user_width: f64,
    /// Dash lengths in device pixels; empty for a solid line.
    pub dashes: Vec<f64>,
    /// Dash offset in device pixels.
    pub dash_offset: f64,
    /// Set by the rules: the stroke is drawn in opaque black, whatever its colour.
    pub black: bool,
}

/// The rules of upstream hayro: nothing moves.
pub(crate) struct NoRules;

impl PixelRules for NoRules {
    fn active(&self) -> bool {
        false
    }
}

/// `path` drawn with `transform`, adjusted in device space by `adjust`, mapped back. `None` when
/// `transform` cannot be inverted: the path is then drawn as it is.
pub(crate) fn in_device(
    path: &BezPath,
    transform: Affine,
    adjust: impl FnOnce(&mut BezPath),
) -> Option<BezPath> {
    if transform.determinant().abs() < 1e-12 {
        return None;
    }
    let mut device = transform * path.clone();
    adjust(&mut device);
    Some(transform.inverse() * device)
}
