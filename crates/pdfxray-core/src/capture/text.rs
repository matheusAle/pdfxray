//! Text showing: the character codes of a text operator, the geometry of the glyphs the engine
//! drew for them, and how far they move the text matrix.
//!
//! Glyph transforms and outline boxes come from the engine as `kurbo` values; they are turned
//! into core boxes and points here.

use hayro_syntax::object::{MaybeRef, Object};
use kurbo::Point as KPoint;

use super::degrade::DegradeReason;
use super::font::FontInfo;
use super::geom::from_kurbo;
use super::ops::RawOp;
use super::recorder::RunGlyph;
use super::types::{GState, TextState};
use crate::{Matrix, Point, Rect};

/// What a text-showing operator holds.
pub(super) struct ShownText {
    /// Character codes of every string, in order, with their byte lengths.
    pub codes: Vec<(u32, usize)>,
    /// Sum of the `TJ` adjustments, thousandths of text space, sign flipped so that positive
    /// moves right.
    pub adjust: f64,
    /// The part of `adjust` that precedes the first string.
    pub lead: f64,
}

impl ShownText {
    /// Reads the strings and adjustments of a `Tj`, `TJ`, `'` or `"` in `font`.
    pub(super) fn read(op: &RawOp<'_>, font: &FontInfo) -> Self {
        let mut codes = Vec::new();
        let mut adjust = 0.0;
        let mut lead = 0.0;
        let mut take = |o: &Object<'_>| match o {
            Object::String(s) => codes.extend(font.read_codes(s.as_bytes())),
            Object::Number(n) => {
                adjust -= n.as_f64();
                if codes.is_empty() {
                    lead -= n.as_f64();
                }
            }
            _ => {}
        };
        match op.operator.as_str() {
            "TJ" => {
                if let Some(Object::Array(a)) = op.operands.first() {
                    for item in a.raw_iter() {
                        if let MaybeRef::NotRef(o) = item {
                            take(&o);
                        }
                    }
                }
            }
            "\"" => {
                if let Some(o) = op.operands.get(2) {
                    take(o);
                }
            }
            _ => {
                if let Some(o) = op.operands.first() {
                    take(o);
                }
            }
        }
        Self { codes, adjust, lead }
    }

    /// How far the text matrix moves in text space: per code, the glyph's advance (the
    /// engine's, or the Type 3 `/Widths`) plus the spacing, and per `TJ` adjustment.
    pub(super) fn advance(&self, text: &TextState, glyphs: Option<&[RunGlyph]>, font: &FontInfo) -> f64 {
        let th = text.horizontal_scaling / 100.0;
        let mut dx = self.adjust * text.font_size / 1000.0 * th;
        for (k, (code, len)) in self.codes.iter().enumerate() {
            let w = glyphs
                .and_then(|g| g.get(k))
                .and_then(|g| g.advance)
                .or_else(|| font.type3_widths.as_ref().and_then(|w| w.width(*code)))
                .unwrap_or(0.0);
            let word = if *code == 32 && *len == 1 {
                text.word_spacing
            } else {
                0.0
            };
            dx += (w / 1000.0 * text.font_size + text.char_spacing + word) * th;
        }
        dx
    }
}

/// A pure translation by `(tx, ty)`.
pub(super) fn translation(tx: f64, ty: f64) -> Matrix {
    Matrix {
        e: tx,
        f: ty,
        ..Matrix::IDENTITY
    }
}

/// Cross-checks the text position the walker tracked against the engine's: the first glyph
/// must sit at the text matrix (moved by a leading `TJ` adjustment `lead` and the rise) under
/// the CTM. A disagreement means the walker lost track of the text state (or the font writes
/// vertically), so geometry cannot be trusted.
pub(super) fn check_first_origin(
    op: u32,
    pre: &GState,
    lead: f64,
    first: &RunGlyph,
) -> Option<DegradeReason> {
    let t = &pre.text;
    let dx = lead * t.font_size / 1000.0 * t.horizontal_scaling / 100.0;
    let tracked = Matrix::from(t.tm)
        .concat(&Matrix::from(pre.ctm))
        .apply(Point::new(dx, t.rise));
    let engine = first.transform * KPoint::ZERO;
    let (ex, ey) = (tracked.x - engine.x, tracked.y - engine.y);
    let tolerance = 0.02 + 1e-5 * tracked.x.abs().max(tracked.y.abs());
    ((ex * ex + ey * ey).sqrt() > tolerance).then(|| DegradeReason::FirstGlyphMisplaced {
        op,
        engine: Point::new(engine.x, engine.y),
        tracked,
    })
}

/// Page-space box, origin and advance length of one glyph whose advance is `width` (1/1000
/// em); `declared` is a Type 3 glyph's own `d1` box.
pub(super) fn glyph_geometry(
    font: &FontInfo,
    glyph: &RunGlyph,
    width: f64,
    declared: Option<kurbo::Rect>,
) -> (Rect, Point, f64) {
    let metrics = kurbo::Rect::new(
        0.0,
        font.descent.min(font.ascent),
        width,
        font.ascent.max(font.descent),
    );
    // A Type 3 glyph's own `d1` box is the best statement of its ink; empty ones fall back.
    let mut local = declared
        .filter(|d| d.width() > 0.0 && d.height() > 0.0)
        .unwrap_or(metrics);
    if let Some(ink) = glyph.outline_bbox {
        local = local.union(ink);
    }
    let t = glyph.transform;
    let origin = t * KPoint::ZERO;
    let tip = t * KPoint::new(width, 0.0);
    let bbox = from_kurbo(t.transform_rect_bbox(local)).unwrap_or(Rect {
        x0: origin.x,
        y0: origin.y,
        x1: origin.x,
        y1: origin.y,
    });
    (bbox, Point::new(origin.x, origin.y), (tip - origin).hypot())
}

/// Grows a glyph box by half the stroke width for stroked text.
pub(super) fn inflate_stroke(bbox: Rect, gs: &GState) -> Rect {
    let [a, b, c, d, _, _] = gs.ctm;
    let half = gs.line_width.max(0.0) / 2.0;
    let (dx, dy) = (half * a.hypot(c), half * b.hypot(d));
    Rect {
        x0: bbox.x0 - dx,
        y0: bbox.y0 - dy,
        x1: bbox.x1 + dx,
        y1: bbox.y1 + dy,
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "the operands are exactly representable and the arithmetic exact"
)]
mod tests {
    use kurbo::Affine;
    use pretty_assertions::assert_eq;

    use super::*;

    fn glyph_at(x: f64, y: f64) -> RunGlyph {
        RunGlyph {
            transform: Affine::new([0.012, 0.0, 0.0, 0.012, x, y]),
            unicode: None,
            advance: Some(500.0),
            outline_bbox: None,
        }
    }

    #[test]
    fn the_first_glyph_check_accepts_the_tracked_origin_and_names_a_stray_one() {
        let gs = GState {
            ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 100.0],
            text: TextState {
                tm: [1.0, 0.0, 0.0, 1.0, 20.0, 40.0],
                font_size: 12.0,
                ..TextState::default()
            },
            ..GState::default()
        };

        let on_track = check_first_origin(3, &gs, 0.0, &glyph_at(20.0, 140.0));
        let shifted = check_first_origin(3, &gs, 0.0, &glyph_at(25.0, 140.0));

        assert_eq!(on_track, None);
        assert_eq!(
            shifted,
            Some(DegradeReason::FirstGlyphMisplaced {
                op: 3,
                engine: Point::new(25.0, 140.0),
                tracked: Point::new(20.0, 140.0),
            })
        );
        // smoke: a leading `TJ` adjustment moves the tracked origin left by thousandths of em
        let lead = check_first_origin(3, &gs, -1000.0, &glyph_at(8.0, 140.0));
        assert_eq!(lead, None);
    }

    #[test]
    fn the_advance_adds_widths_spacing_and_adjustments() {
        let text = TextState {
            font_size: 10.0,
            char_spacing: 1.0,
            word_spacing: 3.0,
            ..TextState::default()
        };
        let shown = ShownText {
            codes: vec![(65, 1), (32, 1)],
            adjust: -100.0,
            lead: 0.0,
        };
        let glyphs = [glyph_at(0.0, 0.0), glyph_at(6.0, 0.0)];

        let dx = shown.advance(&text, Some(&glyphs), &FontInfo::fallback());

        // -1 for the adjustment, then 5 + 1 for `A` and 5 + 1 + 3 for the space.
        assert_eq!(dx, 14.0);
        // smoke: without engine glyphs a non-Type 3 font advances by spacing only
        assert_eq!(shown.advance(&text, None, &FontInfo::fallback()), 4.0);
    }

    #[test]
    fn stroked_glyph_boxes_grow_by_half_the_scaled_line_width() {
        let gs = GState {
            ctm: [2.0, 0.0, 0.0, 3.0, 0.0, 0.0],
            line_width: 1.0,
            ..GState::default()
        };

        let grown = inflate_stroke(Rect::new(10.0, 10.0, 20.0, 20.0), &gs);

        assert_eq!(grown, Rect::new(9.0, 8.5, 21.0, 21.5));
        // smoke: the translation is the matrix's e and f
        assert_eq!(translation(3.0, 4.0).to_array(), [1.0, 0.0, 0.0, 1.0, 3.0, 4.0]);
    }
}
