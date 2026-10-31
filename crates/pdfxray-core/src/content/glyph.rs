//! The glyph inspector's data: character code, Unicode and where it came from, `ActualText`,
//! width and advance, and the font behind it.

use serde::Serialize;

use super::PageData;
use crate::capture::{Alignment, PaintKind};
use crate::{Document, FontFacts, Matrix, NodeId, Point, Rect};

/// A character code as written in the string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CharCode {
    /// The code as a number.
    pub value: u32,
    /// The code as hexadecimal digits, two per byte (`41`; the UI shows `<41>`).
    pub hex: String,
    /// Number of bytes the code takes in the string. For a Type 0 font whose `CMap` is not an
    /// Identity one this is the fewest bytes that hold the value.
    pub bytes: usize,
    /// False when capture could not read the code (the page's alignment is degraded and the
    /// engine's glyph could not be matched to a code); `value` is then 0.
    pub known: bool,
}

/// Where a glyph's Unicode text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub enum UnicodeSource {
    /// The font's `/ToUnicode` map.
    ToUnicode,
    /// The font's encoding (a standard or custom encoding, glyph names).
    Encoding,
    /// Nothing maps the glyph to text.
    None,
}

/// An `/ActualText` that applies to a glyph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActualText {
    /// The replacement text.
    pub text: String,
    /// False when the innermost marked-content sequence around the glyph carries it; true when
    /// it comes from an enclosing sequence.
    pub inherited: bool,
}

/// Everything the glyph panel shows. See [`Document::glyph_info`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GlyphInfo {
    /// The glyph node.
    pub glyph: NodeId,
    /// The text-showing operator that painted it.
    pub op: NodeId,
    /// The character code.
    pub char_code: CharCode,
    /// The Unicode text the font maps the glyph to.
    pub unicode: Option<String>,
    /// The code points of [`GlyphInfo::unicode`].
    pub code_points: Vec<u32>,
    /// Where the Unicode text came from.
    pub unicode_source: UnicodeSource,
    /// The `/ActualText` that applies, if any.
    pub actual_text: Option<ActualText>,
    /// Advance width in thousandths of an em (`722` for a Helvetica `H`).
    pub width_units: f64,
    /// True when `width_units` was read from the font's width table, false when it was derived
    /// from the advance the engine measured (fonts without a table, such as the standard 14).
    pub width_from_font: bool,
    /// Advance width in points: `width_units / 1000 × font size × horizontal scaling`, before
    /// the text and current transformation matrices.
    pub width_pt: f64,
    /// The advance vector `[x, y]` in page space, points.
    pub advance: [f64; 2],
    /// Where the glyph sits on the baseline, page space.
    pub origin: Point,
    /// The glyph's box in page space.
    pub bbox: Rect,
    /// Font size set by `Tf`.
    pub font_size: f64,
    /// The font. Its ascent, descent and bounding box are in [`FontFacts`]; `None` when the
    /// content selects a font its resources do not define.
    pub font: Option<FontFacts>,
}

impl Document {
    /// The glyph panel's data for a [`NodeId::Glyph`]. `None` for any other node, for a glyph
    /// that does not exist, and for a document whose content could not be captured.
    pub fn glyph_info(&self, id: &NodeId) -> Option<GlyphInfo> {
        let NodeId::Glyph { page, op, glyph } = *id else {
            return None;
        };
        let data = self.content.data(page)?;
        let record = data.op(op)?;
        let item = data.glyph_item(op, glyph)?;
        let PaintKind::Glyph {
            char_code,
            unicode,
            origin,
            advance,
            ..
        } = &item.kind
        else {
            return None;
        };
        let ts = &item.gstate.text;
        let owner = self.resources_owner(page, &data.cap, record);
        let font = self.font_facts(ts.font.as_deref(), owner, ts.font_ref);
        let known = *char_code != 0 || matches!(data.cap.alignment, Alignment::Exact);

        let to_page = Matrix::from(ts.tm).concat(&Matrix::from(item.gstate.ctm));
        let scale = to_page.a.hypot(to_page.b);
        let horizontal = ts.horizontal_scaling / 100.0;
        let from_font = font
            .as_ref()
            .and_then(|f| f.width_units(*char_code))
            .filter(|_| known);
        let width_units = from_font.unwrap_or_else(|| {
            let denominator = ts.font_size * horizontal * scale;
            if denominator.abs() < f64::EPSILON {
                0.0
            } else {
                round3(advance / denominator * 1000.0)
            }
        });
        let (dx, dy) = if scale > 0.0 {
            (to_page.a / scale, to_page.b / scale)
        } else {
            (1.0, 0.0)
        };
        let text = unicode.clone().filter(|u| !u.is_empty());
        Some(GlyphInfo {
            glyph: id.clone(),
            op: NodeId::Op { page, ix: op },
            char_code: char_code_of(*char_code, known, font.as_ref()),
            code_points: text.iter().flat_map(|t| t.chars().map(u32::from)).collect(),
            unicode_source: match (&text, &font) {
                (None, _) => UnicodeSource::None,
                (Some(_), Some(f)) if f.has_to_unicode => UnicodeSource::ToUnicode,
                (Some(_), _) => UnicodeSource::Encoding,
            },
            unicode: text,
            actual_text: actual_text(data, &item.mc_stack),
            width_units,
            width_from_font: from_font.is_some(),
            width_pt: width_units / 1000.0 * ts.font_size * horizontal,
            advance: [advance * dx, advance * dy],
            origin: *origin,
            bbox: item.bbox,
            font_size: ts.font_size,
            font,
        })
    }
}

/// `value` as the bytes a string of `font` writes it with: the font's code width, or the fewest
/// bytes that hold it when the font does not tell.
pub(crate) fn char_code_of(value: u32, known: bool, font: Option<&FontFacts>) -> CharCode {
    let bytes = font.and_then(FontFacts::code_bytes).unwrap_or(match value {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x1_0000..=0xFF_FFFF => 3,
        _ => 4,
    });
    CharCode {
        value,
        hex: format!("{value:0width$X}", width = bytes * 2),
        bytes,
        known,
    }
}

/// The `/ActualText` of the innermost sequence around a glyph that has one.
fn actual_text(data: &PageData, mc_stack: &[u32]) -> Option<ActualText> {
    mc_stack.iter().rev().enumerate().find_map(|(depth, &s)| {
        let text = data.seq(s)?.actual_text.clone()?;
        Some(ActualText {
            text,
            inherited: depth > 0,
        })
    })
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
