//! `type3-font`: a Type 3 font with a filled square and a stroked triangle, shown at two sizes.

use super::common::{built, page_dict_with, text_op};
use crate::expected::{CatalogFacts, Expected, GlyphPaint, PageFacts, Sourced, TextOpFacts, Type3GlyphFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Colours are set before the text because `d1` glyphs inherit them.
const CONTENT: &str = "\
0 0.4 0 rg
0.6 0 0 RG
BT
/T3 20 Tf
30 100 Td
(ab) Tj
ET
BT
/T3 1 Tf
30 0 0 30 120 100 Tm
(a) Tj
ET";

/// Glyph `a`: a filled 800 x 800 square, advance 1000.
const SQUARE: &str = "1000 0 0 0 800 800 d1\n0 0 800 800 re f";

/// Glyph `b`: a stroked triangle, 40 units wide line, advance 800, declared cell 600 x 700.
const TRIANGLE: &str = "800 0 0 0 600 700 d1\n40 w\n50 50 m\n550 50 l\n300 650 l\nh\nS";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        page_dict_with("/MediaBox [0 0 200 150]", "/Font << /T3 5 0 R >>"),
    );
    w.stream(4, "", CONTENT.as_bytes());
    w.object(
        5,
        "<< /Type /Font /Subtype /Type3 /Name /T3 /FontBBox [0 0 1000 1000] \
         /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /sq 6 0 R /tri 7 0 R >> \
         /Encoding << /Type /Encoding /Differences [97 /sq /tri] >> \
         /FirstChar 97 /LastChar 98 /Widths [1000 800] /Resources << >> >>",
    );
    w.stream(6, "", SQUARE.as_bytes());
    w.stream(7, "", TRIANGLE.as_bytes());
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let glyph = |op_index: u32, glyph_index: u32, code: u8, name: &str, proc_object: u32| Type3GlyphFacts {
        page: 0,
        op_index,
        glyph_index,
        code,
        glyph_name: name.to_owned(),
        proc_object,
        origin: [0.0, 0.0],
        declared_bbox: [0.0; 4],
        path_bbox: [0.0; 4],
        paint: GlyphPaint::Fill,
    };
    let t3_text =
        |op_index: u32, text: &str, font_size: f64, effective_size: f64, origin: [f64; 2]| TextOpFacts {
            font: "T3".to_owned(),
            base_font: None,
            font_size,
            effective_size,
            origin,
            ..text_op(op_index, "Tj", text)
        };
    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 150.0], 3)))
        .with_text_op(t3_text(5, "ab", 20.0, 20.0, [30.0, 100.0]))
        .with_text_op(t3_text(10, "a", 1.0, 30.0, [120.0, 100.0]))
        // Size 20 and FontMatrix 0.001 scale glyph space by 0.02. Glyph a sits at the Td point
        // (30,100); its square 0..800 becomes 0..16.
        .with_type3_glyph(Type3GlyphFacts {
            origin: [30.0, 100.0],
            declared_bbox: [30.0, 100.0, 46.0, 116.0],
            path_bbox: [30.0, 100.0, 46.0, 116.0],
            ..glyph(5, 0, 97, "sq", 6)
        })
        // Glyph b starts one advance later: 30 + 1000 * 0.02 = 50. Its d1 cell 0..600 x 0..700
        // becomes 12 x 14; the triangle (50,50) (550,50) (300,650) becomes x 1..11, y 1..13.
        .with_type3_glyph(Type3GlyphFacts {
            origin: [50.0, 100.0],
            declared_bbox: [50.0, 100.0, 62.0, 114.0],
            path_bbox: [51.0, 101.0, 61.0, 113.0],
            paint: GlyphPaint::Stroke,
            ..glyph(5, 1, 98, "tri", 7)
        })
        // Size 1 under a Tm scale of 30: glyph scale 0.001 * 30 = 0.03, so 800 becomes 24.
        .with_type3_glyph(Type3GlyphFacts {
            origin: [120.0, 100.0],
            declared_bbox: [120.0, 100.0, 144.0, 124.0],
            path_bbox: [120.0, 100.0, 144.0, 124.0],
            ..glyph(10, 0, 97, "sq", 6)
        });
    Ok(built(out, expected))
}
