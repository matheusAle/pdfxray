//! `text-state`: every text state parameter, the four showing operators, two fonts and two ways to
//! reach a font size. The page content is Flate-compressed to exercise stream decoding.

use super::common::{built, text_op, type1_font, zlib};
use crate::expected::{CatalogFacts, Expected, PageFacts, Sourced, TextOpFacts};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// One operator per line; the number in the comment is the operator index of the line's last
/// operator (an operator index counts operators only, so `/F1 12 Tf` is one).
const CONTENT: &str = "\
BT
/F1 12 Tf
20 270 Td
(Plain Tj) Tj
ET
BT
/F2 18 Tf
2 Tc
5 Tw
150 Tz
20 240 Td
(Spaced out words) Tj
ET
BT
/F1 12 Tf
0 Tc
0 Tw
100 Tz
14 TL
20 210 Td
(Line one) Tj
(Line two) '
3 1 (Line three) \"
ET
BT
/F2 1 Tf
0 Tc
0 Tw
0 TL
16 0 0 16 20 150 Tm
(Sized by Tm) Tj
ET
BT
/F1 12 Tf
5 Ts
20 120 Td
(Raised) Tj
0 Ts
ET
BT
/F1 24 Tf
20 80 Td
0 Tr
(Fill) Tj
90 0 Td
1 Tr
(Line) Tj
90 0 Td
2 Tr
(Both) Tj
90 0 Td
3 Tr
(Gone) Tj
0 Tr
ET
BT
/F1 12 Tf
20 50 Td
[(K) 120 (ern) -80 (ing)] TJ
ET
BT
/F2 12 Tf
150 Tz
20 25 Td
[-250 (Shifted)] TJ
100 Tz
ET";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] \
         /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "/Filter /FlateDecode", &zlib(CONTENT.as_bytes()));
    w.object(5, type1_font("Helvetica"));
    w.object(6, type1_font("Times-Roman"));
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let times = |op: TextOpFacts| TextOpFacts {
        font: "F2".to_owned(),
        base_font: Some("Times-Roman".to_owned()),
        ..op
    };
    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 400.0, 300.0], 3)))
        // Operator 3: first BT block, plain Tj at the Td position.
        .with_text_op(TextOpFacts {
            origin: [20.0, 270.0],
            ..text_op(3, "Tj", "Plain Tj")
        })
        // Operator 11: Times 18 with Tc 2, Tw 5, Tz 150.
        .with_text_op(times(TextOpFacts {
            font_size: 18.0,
            effective_size: 18.0,
            char_spacing: 2.0,
            word_spacing: 5.0,
            horizontal_scaling: 150.0,
            origin: [20.0, 240.0],
            ..text_op(11, "Tj", "Spaced out words")
        }))
        // Operator 20: Tj at (20, 210) with TL 14.
        .with_text_op(TextOpFacts {
            leading: 14.0,
            origin: [20.0, 210.0],
            ..text_op(20, "Tj", "Line one")
        })
        // Operator 21: ' moves down by TL: 210 - 14 = 196.
        .with_text_op(TextOpFacts {
            leading: 14.0,
            origin: [20.0, 196.0],
            ..text_op(21, "'", "Line two")
        })
        // Operator 22: " sets Tw 3 and Tc 1, then moves down once more: 196 - 14 = 182.
        .with_text_op(TextOpFacts {
            leading: 14.0,
            word_spacing: 3.0,
            char_spacing: 1.0,
            origin: [20.0, 182.0],
            ..text_op(22, "\"", "Line three")
        })
        // Operator 30: Tf size 1 scaled by Tm 16 -> effective size 16, origin from Tm translation.
        .with_text_op(times(TextOpFacts {
            font_size: 1.0,
            effective_size: 16.0,
            origin: [20.0, 150.0],
            ..text_op(30, "Tj", "Sized by Tm")
        }))
        // Operator 36: Ts 5 lifts the origin: y = 120 + 5.
        .with_text_op(TextOpFacts {
            rise: 5.0,
            origin: [20.0, 125.0],
            ..text_op(36, "Tj", "Raised")
        })
        // Operators 43, 46, 49, 52: render modes 0..3 at x = 20, 110, 200, 290 (Td steps of 90).
        .with_text_op(TextOpFacts {
            font_size: 24.0,
            effective_size: 24.0,
            origin: [20.0, 80.0],
            ..text_op(43, "Tj", "Fill")
        })
        .with_text_op(TextOpFacts {
            font_size: 24.0,
            effective_size: 24.0,
            render_mode: 1,
            origin: [110.0, 80.0],
            ..text_op(46, "Tj", "Line")
        })
        .with_text_op(TextOpFacts {
            font_size: 24.0,
            effective_size: 24.0,
            render_mode: 2,
            origin: [200.0, 80.0],
            ..text_op(49, "Tj", "Both")
        })
        .with_text_op(TextOpFacts {
            font_size: 24.0,
            effective_size: 24.0,
            render_mode: 3,
            origin: [290.0, 80.0],
            ..text_op(52, "Tj", "Gone")
        })
        // Operator 58: TJ without a leading number starts at the Td position.
        .with_text_op(TextOpFacts {
            origin: [20.0, 50.0],
            ..text_op(58, "TJ", "Kerning")
        })
        // Operator 64: leading adjustment -250 moves right by 250/1000 * 12 * 150/100 = 4.5.
        .with_text_op(times(TextOpFacts {
            horizontal_scaling: 150.0,
            origin: [24.5, 25.0],
            ..text_op(64, "TJ", "Shifted")
        }));
    Ok(built(out, expected))
}
