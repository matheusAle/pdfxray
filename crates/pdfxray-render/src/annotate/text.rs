//! The one text measurer of the annotation layer: the embedded faces, their metrics and their
//! outlines.
//!
//! The app draws the labels with its own text engine but puts them where this module says; the
//! exporter draws them with these outlines. Chip rects, badge spots and the nudging between them
//! all come from these metrics, so the app and the exported image place every label alike.

use std::sync::LazyLock;

use kurbo::{BezPath, Point as KPoint};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// Inter Bold 4.1 (SIL OFL 1.1, `assets/fonts`): chips, badges and the legend.
const INTER_BOLD: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Bold.ttf");
/// `JetBrains Mono` Medium 2.304 (SIL OFL 1.1, `assets/fonts`): the notes on the reading-order path.
const MONO_MEDIUM: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Medium.ttf");
/// `JetBrains Mono` Bold 2.304 (SIL OFL 1.1, `assets/fonts`): the `#` of a header chip.
const MONO_BOLD: &[u8] = include_bytes!("../../../../assets/fonts/JetBrainsMono-Bold.ttf");

/// A typeface of the annotation layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// Inter Bold: chip names, badge numbers, the legend.
    Bold,
    /// `JetBrains Mono` Medium: the notes on the reading-order path.
    Mono,
    /// `JetBrains Mono` Bold: the `#` of a header chip.
    MonoBold,
}

/// A face at a size, in output units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    /// The typeface.
    pub face: Face,
    /// Em size.
    pub size: f32,
}

impl Font {
    /// `face` at `size`.
    pub const fn new(face: Face, size: f32) -> Self {
        Self { face, size }
    }
}

/// One parsed face and the metrics every measurement needs.
struct Loaded {
    font: FontRef<'static>,
    units_per_em: f32,
    ascent: f32,
    descent: f32,
    leading: f32,
    /// The advance of a character the face does not carry: its `.notdef`.
    fallback: f32,
    /// The most a screen's shaper widens the gap between two characters, font units: between
    /// two ASCII letters or digits, and between any two characters.
    kern: (f32, f32),
}

impl Loaded {
    fn parse(bytes: &'static [u8], kern: (f32, f32)) -> Self {
        // The bytes are compiled in and checked by the tests below: they always parse.
        let font = FontRef::new(bytes).unwrap_or_else(|_| unreachable!("embedded font"));
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let advances = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        Self {
            units_per_em: f32::from(metrics.units_per_em),
            ascent: metrics.ascent,
            descent: metrics.descent,
            leading: metrics.leading,
            fallback: advances.advance_width(GlyphId::NOTDEF).unwrap_or_default(),
            kern,
            font,
        }
    }

    fn glyph(&self, c: char) -> Option<GlyphId> {
        self.font.charmap().map(c)
    }
}

/// Inter Bold kerns some pairs apart: by up to 45 units of 2048 between letters and digits
/// (`ty`, `tv`, `WW`), by up to 234 next to a raised `*`. Measured with `HarfBuzz` over every
/// pair of printable ASCII characters.
const INTER_BOLD_KERN: (f32, f32) = (45.0, 234.0);
/// `JetBrains Mono` never moves a glyph off its 600-unit cell.
const MONO_KERN: (f32, f32) = (0.0, 0.0);

static BOLD: LazyLock<Loaded> = LazyLock::new(|| Loaded::parse(INTER_BOLD, INTER_BOLD_KERN));
static MONO: LazyLock<Loaded> = LazyLock::new(|| Loaded::parse(MONO_MEDIUM, MONO_KERN));
static MONO_BOLD_FACE: LazyLock<Loaded> = LazyLock::new(|| Loaded::parse(MONO_BOLD, MONO_KERN));

fn loaded(face: Face) -> &'static Loaded {
    match face {
        Face::Bold => &BOLD,
        Face::Mono => &MONO,
        Face::MonoBold => &MONO_BOLD_FACE,
    }
}

/// Rounds `v` to a multiple of `step`; a step that is not positive leaves `v` as it is.
fn snap(v: f32, step: f32) -> f32 {
    if step > 0.0 { (v / step).round() * step } else { v }
}

/// The size of `text` set on one line in `font`: the width it may take by the height of a line
/// (ascent, descent and line gap), the height snapped to whole device pixels of length `pixel`
/// the way a screen sets a text row.
///
/// The width is an over-estimate on purpose, so that text set by a screen's text engine never
/// outgrows the rect measured for it: the sum of the advances, plus for every gap between two
/// characters the most the face's kerning ever widens one, rounded *up* to 1/32 (the rounding
/// grid of egui's text layout). Kerning that tightens is ignored, which only leaves room.
pub fn measure(text: &str, font: Font, pixel: f32) -> (f32, f32) {
    let face = loaded(font.face);
    let scale = font.size / face.units_per_em;
    let advances = face.font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let advance: f32 = text
        .chars()
        .map(|c| {
            face.glyph(c)
                .and_then(|glyph| advances.advance_width(glyph))
                .unwrap_or(face.fallback)
        })
        .sum();
    let alnum = |c: char| c.is_ascii_alphanumeric();
    let kern: f32 = text
        .chars()
        .zip(text.chars().skip(1))
        .map(|(a, b)| {
            if alnum(a) && alnum(b) {
                face.kern.0
            } else {
                face.kern.1
            }
        })
        .sum();
    let width = (advance + kern) * scale;
    (
        (width * 32.0).ceil() / 32.0,
        snap(line_height(face, scale), pixel),
    )
}

/// Ascent minus descent plus line gap at `scale`, each part rounded to 1/32 as a screen does.
fn line_height(face: &Loaded, scale: f32) -> f32 {
    let part = |v: f32| snap(v * scale, 1.0 / 32.0);
    part(face.ascent) - part(face.descent) + part(face.leading)
}

/// The distance from the top of a line of `font` to its baseline.
pub(crate) fn ascent(font: Font) -> f32 {
    let face = loaded(font.face);
    snap(face.ascent * font.size / face.units_per_em, 1.0 / 32.0)
}

/// The outlines of `text` in `font` with the top-left corner of its line at `origin`, y down.
pub(crate) fn outlines(text: &str, font: Font, origin: KPoint) -> BezPath {
    let outline_face = loaded(font.face);
    let scale = font.size / outline_face.units_per_em;
    let advances = outline_face
        .font
        .glyph_metrics(Size::unscaled(), LocationRef::default());
    let glyphs = outline_face.font.outline_glyphs();
    let mut pen = Pen {
        path: BezPath::new(),
        x: origin.x,
        y: origin.y + f64::from(ascent(font)),
    };
    for c in text.chars() {
        let glyph = outline_face.glyph(c);
        if let Some(outline) = glyph.and_then(|g| glyphs.get(g)) {
            let settings = DrawSettings::unhinted(Size::new(font.size), LocationRef::default());
            // A glyph that cannot be drawn is left out; its advance still moves the pen.
            let _ = outline.draw(settings, &mut pen);
        }
        let advance = glyph
            .and_then(|g| advances.advance_width(g))
            .unwrap_or(outline_face.fallback);
        pen.x += f64::from(advance * scale);
    }
    pen.path
}

/// Collects glyph outlines (y up, from the pen position) into a y-down path.
struct Pen {
    path: BezPath,
    x: f64,
    /// The baseline.
    y: f64,
}

impl Pen {
    fn at(&self, x: f32, y: f32) -> KPoint {
        KPoint::new(self.x + f64::from(x), self.y - f64::from(y))
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.path.move_to(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.at(x, y);
        self.path.line_to(p);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.at(cx0, cy0), self.at(x, y));
        self.path.quad_to(c, p);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.at(cx0, cy0), self.at(cx1, cy1), self.at(x, y));
        self.path.curve_to(c0, c1, p);
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use kurbo::Shape as _;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_embedded_faces_parse_and_carry_latin() {
        for face in [Face::Bold, Face::Mono, Face::MonoBold] {
            let loaded = loaded(face);
            assert!(loaded.glyph('H').is_some(), "{face:?}");
            assert!(loaded.units_per_em > 0.0, "{face:?}");
        }
        // Smoke: the mono face has no ↰, which falls back to `.notdef`'s advance.
        assert_eq!(loaded(Face::Mono).glyph('↰'), None);
    }

    #[test]
    fn widths_add_up_and_rows_snap_to_device_pixels() {
        let bold = Font::new(Face::Bold, 11.0);
        let (h1, row) = measure("H1", bold, 1.0);
        let (h, _) = measure("H", bold, 1.0);
        let (one, _) = measure("1", bold, 1.0);

        // The pair takes the two advances and room for the most Inter ever kerns two apart.
        let room = 45.0 * 11.0 / 2048.0;
        assert!((h1 - (h + one + room)).abs() <= 1.0 / 16.0, "{h1} {h} {one}");
        assert_eq!(h1 * 32.0, (h1 * 32.0).round());
        // Inter at 11: 10.65625 - -2.65625 = 13.3125, set in 13 whole pixels, 13.5 on a 2x screen.
        assert_eq!(row, 13.0);
        assert_eq!(measure("H1", bold, 0.5).1, 13.5);
        // Smoke: every mono glyph is 0.6 em wide.
        assert!((measure("abc#", Font::new(Face::Mono, 10.0), 1.0).0 - 24.0).abs() < 1e-4);
        assert_eq!(
            measure("#", Font::new(Face::MonoBold, 10.0), 1.0),
            measure("#", Font::new(Face::Mono, 10.0), 1.0)
        );
    }

    #[test]
    fn outlines_sit_on_the_baseline_below_the_origin() {
        let bold = Font::new(Face::Bold, 11.0);
        let path = outlines("H", bold, KPoint::new(10.0, 20.0));
        let bounds = path.bounding_box();

        // The cap height of H sits on the baseline, `ascent` below the top of the line.
        assert!(
            (bounds.y1 - (20.0 + f64::from(ascent(bold)))).abs() < 0.01,
            "{bounds:?}"
        );
        assert!(bounds.x0 >= 10.0 && bounds.y0 > 20.0, "{bounds:?}");
        // Smoke: a space has no outline.
        assert_eq!(outlines(" ", bold, KPoint::ZERO).elements().len(), 0);
    }
}
