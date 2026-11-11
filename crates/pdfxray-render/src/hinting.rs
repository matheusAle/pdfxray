//! Glyph hinting for viewer emulation: a glyph's outline fitted to the device pixel grid at the
//! size it is drawn, as a Windows viewer's font engine does, so its stems land on whole pixels
//! and small text stays sharp.
//!
//! The hinters are skrifa's: the TrueType instruction interpreter, the CFF hinter (a port of
//! `FreeType`'s, which grew from code Adobe contributed) and an autohinter for fonts without
//! hints. Only an embedded TrueType or OpenType font can be hinted (skrifa reads neither a bare
//! CFF program nor a Type 1 font), and only upright text drawn at one size across and down: any
//! other glyph keeps its outline.

use std::cell::RefCell;
use std::collections::HashMap;

use hayro::hayro_interpret::font::OutlineGlyph;
use kurbo::{Affine, BezPath};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::pen::OutlinePen;
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, Target};
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// Units per em of the glyph space hayro draws outlines in ([`OutlineGlyph::outline`]).
const GLYPH_UNITS: f64 = 1000.0;

/// Sizes outside this range, in device pixels per em, are drawn unhinted: hinting a large glyph
/// changes nothing that shows, and a tiny one is a smudge either way.
const HINTED_PPEM: std::ops::RangeInclusive<f64> = 4.0..=200.0;

/// Which hinter a viewer's font engine is closest to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hinting {
    /// The font's own hints (TrueType instructions, CFF hints), the autohinter for a font
    /// without them: Acrobat's `CoolType`.
    FontHints,
    /// The autohinter for every font: `PDFium`'s `FreeType` autohint.
    Auto,
}

/// Hinted outlines of the glyphs of one render, cached per font, glyph and size.
pub(crate) struct Hinter {
    hinting: Hinting,
    /// The hinting instance of each font at each size (`None`: the font cannot be hinted).
    instances: RefCell<HashMap<(u128, u32), Option<HintingInstance>>>,
    /// Hinted outlines in glyph space, per font, glyph and size.
    outlines: RefCell<HashMap<(u128, u32, u32), Option<BezPath>>>,
}

impl Hinter {
    pub(crate) fn new(hinting: Hinting) -> Self {
        Self {
            hinting,
            instances: RefCell::new(HashMap::new()),
            outlines: RefCell::new(HashMap::new()),
        }
    }

    /// `glyph`'s outline hinted for `device`, in glyph space; `None` for a glyph that is not
    /// hinted (see the module's documentation).
    pub(crate) fn outline(&self, glyph: &OutlineGlyph, device: Affine) -> Option<BezPath> {
        let ppem = upright_ppem(device)?;
        let data = glyph.font_data()?;
        // Sizes in 64ths of a pixel, as font engines count them.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "4..=200 ppem"
        )]
        let size = (ppem * 64.0).round() as u32;
        let gid = glyph.glyph_id().to_u32();
        let key = (data.cache_key, gid, size);
        if let Some(cached) = self.outlines.borrow().get(&key) {
            return cached.clone();
        }
        let bytes: &[u8] = (*data.data).as_ref();
        let hinted = FontRef::new(bytes)
            .ok()
            .and_then(|font| self.draw(&font, data.cache_key, gid, size));
        self.outlines.borrow_mut().insert(key, hinted.clone());
        hinted
    }

    /// Glyph `gid` of `font` hinted at `size` 64ths of a pixel per em, in glyph space.
    fn draw(&self, font: &FontRef<'_>, font_key: u128, gid: u32, size: u32) -> Option<BezPath> {
        let outlines = font.outline_glyphs();
        #[expect(clippy::cast_precision_loss, reason = "a small size")]
        let ppem = size as f32 / 64.0;
        let mut instances = self.instances.borrow_mut();
        let instance = instances
            .entry((font_key, size))
            .or_insert_with(|| {
                let engine = match self.hinting {
                    Hinting::FontHints => Engine::AutoFallback,
                    Hinting::Auto => Engine::Auto(None),
                };
                let options = HintingOptions {
                    engine,
                    target: Target::default(),
                };
                HintingInstance::new(&outlines, Size::new(ppem), LocationRef::default(), options).ok()
            })
            .as_ref()?;
        let outline = outlines.get(GlyphId::new(gid))?;
        let mut pen = Pen::default();
        outline
            .draw(DrawSettings::hinted(instance, false), &mut pen)
            .ok()?;
        // Pixels at `ppem` back to glyph space.
        let to_glyph = GLYPH_UNITS / f64::from(ppem);
        Some(Affine::scale(to_glyph) * pen.path)
    }
}

/// The device pixels per em of a glyph drawn with `device` (glyph space, 1000 units an em, to
/// device pixels), when it is drawn upright, flipped or not, at the same size across and down
/// and in the hinted range: `None` otherwise.
fn upright_ppem(device: Affine) -> Option<f64> {
    let [a, b, c, d, _, _] = device.as_coeffs();
    let down = d.abs();
    let upright = b.abs() <= 1e-6 * down && c.abs() <= 1e-6 * down;
    let square = (a.abs() - down).abs() <= 0.01 * down;
    let ppem = down * GLYPH_UNITS;
    (upright && square && HINTED_PPEM.contains(&ppem)).then_some(ppem)
}

/// A pen that collects an outline into a path, as skrifa draws it (y up).
#[derive(Default)]
struct Pen {
    path: BezPath,
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to((f64::from(x), f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path
            .quad_to((f64::from(cx0), f64::from(cy0)), (f64::from(x), f64::from(y)));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_upright_text_of_one_size_across_and_down_is_hinted() {
        // 12 px a em, flipped as a page is: hinted at 12.
        let ppem = upright_ppem(Affine::new([0.012, 0.0, 0.0, -0.012, 30.0, 40.0]));
        assert!((ppem.unwrap() - 12.0).abs() < 1e-9);
        // Turned, squeezed, or too large: not hinted.
        assert_eq!(upright_ppem(Affine::rotate(0.4).then_scale(0.012)), None);
        assert_eq!(
            upright_ppem(Affine::new([0.006, 0.0, 0.0, -0.012, 0.0, 0.0])),
            None
        );
        assert_eq!(upright_ppem(Affine::scale(0.5)), None);
    }
}
