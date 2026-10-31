//! `graphics-state`: nesting, matrices, line parameters, colour spaces, transparency, clipping and
//! a shading, with the page-space box and graphics state of every painting operator.

use super::common::{built, page_dict_with};
use crate::expected::{CatalogFacts, ColorFacts, Expected, LineFacts, PageFacts, PaintOpFacts, Sourced};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// One operator per line. Comments in `build` give the operator index of each painting line.
const CONTENT: &str = "\
q
1 0 0 rg
20 20 80 60 re
f
Q
q
0 0 1 RG
4 w
1 J
1 j
[6 3] 2 d
120 20 80 60 re
S
Q
q
0.5 g
0.2 0.8 0.2 RG
2 w
220 20 80 60 re
B
Q
q
0 1 1 0 k
1 0 0 0 K
8 w
2 J
20 100 m
100 100 l
60 160 l
h
B
Q
q
1 0 0 1 150 100 cm
q
2 0 0 2 0 0 cm
0 0.5 0 rg
10 10 20 20 re
f
Q
0 1 -1 0 0 0 cm
0.6 g
0 0 30 20 re
f
Q
q
/GS1 gs
1 0.5 0 rg
220 100 100 100 re
f
0 0 1 RG
6 w
250 130 100 100 re
S
Q
q
200 220 150 100 re
W
n
/Sh1 sh
1 0 0 rg
150 200 80 60 re
f
0 0.6 0 rg
220 240 60 40 re
f
0 0 1 rg
360 330 30 30 re
f
q
240 240 200 60 re
W
n
0 g
230 230 40 40 re
f
Q
Q
20 200 40 40 re
f";

/// A painting operator in the initial graphics state: normal blend, nothing else set.
fn paint(op_index: u32, operator: &str, bbox: [f64; 4]) -> PaintOpFacts {
    PaintOpFacts {
        page: 0,
        op_index,
        operator: operator.to_owned(),
        bbox,
        visible_bbox: Some(bbox),
        fill: None,
        stroke: None,
        fill_alpha: None,
        stroke_alpha: None,
        blend_mode: "Normal".to_owned(),
        clip: None,
        line: None,
        shading: None,
    }
}

/// A fill-only operator with opaque `fill`.
fn filled(op_index: u32, operator: &str, bbox: [f64; 4], fill: ColorFacts) -> PaintOpFacts {
    PaintOpFacts {
        fill: Some(fill),
        fill_alpha: Some(1.0),
        ..paint(op_index, operator, bbox)
    }
}

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        page_dict_with(
            "/MediaBox [0 0 400 400]",
            "/ExtGState << /GS1 << /Type /ExtGState /CA 0.5 /ca 0.5 /BM /Multiply >> >> \
             /Shading << /Sh1 5 0 R >>",
        ),
    );
    w.stream(4, "", CONTENT.as_bytes());
    w.object(
        5,
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [200 270 350 270] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0.8 0] /C1 [0 0.6 1] /N 1 >> \
         /Extend [true true] >>",
    );
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let solid = |width: f64, cap: u8, join: u8| LineFacts {
        width,
        cap,
        join,
        ..LineFacts::initial_solid()
    };
    let clip = [200.0, 220.0, 350.0, 320.0];
    let in_clip = |op: PaintOpFacts, visible: Option<[f64; 4]>| PaintOpFacts {
        clip: Some(clip),
        visible_bbox: visible,
        ..op
    };

    let expected = Expected::new(PdfVersion::V1_7, &out.revisions, CatalogFacts::new(1))
        .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 400.0, 400.0], 3)))
        // Operator 3: red fill of (20,20)-(100,80).
        .with_paint_op(filled(
            3,
            "f",
            [20.0, 20.0, 100.0, 80.0],
            ColorFacts::rgb(1.0, 0.0, 0.0),
        ))
        // Operator 12: blue stroke, width 4, round cap and join, dash [6 3] phase 2.
        .with_paint_op(PaintOpFacts {
            stroke: Some(ColorFacts::rgb(0.0, 0.0, 1.0)),
            stroke_alpha: Some(1.0),
            line: Some(LineFacts {
                dash_array: vec![6.0, 3.0],
                dash_phase: 2.0,
                ..solid(4.0, 1, 1)
            }),
            ..paint(12, "S", [120.0, 20.0, 200.0, 80.0])
        })
        // Operator 19: B paints both; the Q at 13 restored width, cap, join and dash.
        .with_paint_op(PaintOpFacts {
            stroke: Some(ColorFacts::rgb(0.2, 0.8, 0.2)),
            stroke_alpha: Some(1.0),
            line: Some(solid(2.0, 0, 0)),
            ..filled(19, "B", [220.0, 20.0, 300.0, 80.0], ColorFacts::gray(0.5))
        })
        // Operator 30: CMYK triangle, box of its three corners (20,100) (100,100) (60,160).
        .with_paint_op(PaintOpFacts {
            stroke: Some(ColorFacts::cmyk(1.0, 0.0, 0.0, 0.0)),
            stroke_alpha: Some(1.0),
            line: Some(solid(8.0, 2, 0)),
            ..filled(
                30,
                "B",
                [20.0, 100.0, 100.0, 160.0],
                ColorFacts::cmyk(0.0, 1.0, 1.0, 0.0),
            )
        })
        // Operator 38: CTM = scale 2 then translate (150,100): x' = 2x + 150, y' = 2y + 100, so
        // (10,10)-(30,30) maps to (170,120)-(210,160).
        .with_paint_op(filled(
            38,
            "f",
            [170.0, 120.0, 210.0, 160.0],
            ColorFacts::rgb(0.0, 0.5, 0.0),
        ))
        // Operator 43: cm [0 1 -1 0 0 0] on top of the translation maps (x,y) to (150 - y, 100 + x);
        // (0,0)-(30,20) becomes x in 150-20..150, y in 100..100+30.
        .with_paint_op(filled(
            43,
            "f",
            [130.0, 100.0, 150.0, 130.0],
            ColorFacts::gray(0.6),
        ))
        // Operator 49: orange fill at alpha 0.5 with Multiply.
        .with_paint_op(PaintOpFacts {
            fill_alpha: Some(0.5),
            blend_mode: "Multiply".to_owned(),
            ..filled(
                49,
                "f",
                [220.0, 100.0, 320.0, 200.0],
                ColorFacts::rgb(1.0, 0.5, 0.0),
            )
        })
        // Operator 53: blue stroke at alpha 0.5 with Multiply, width 6.
        .with_paint_op(PaintOpFacts {
            stroke: Some(ColorFacts::rgb(0.0, 0.0, 1.0)),
            stroke_alpha: Some(0.5),
            blend_mode: "Multiply".to_owned(),
            line: Some(solid(6.0, 0, 0)),
            ..paint(53, "S", [250.0, 130.0, 350.0, 230.0])
        })
        // Operator 59: the shading fills the whole clip (200,220)-(350,320).
        .with_paint_op(in_clip(
            PaintOpFacts {
                fill_alpha: Some(1.0),
                shading: Some("Sh1".to_owned()),
                ..paint(59, "sh", clip)
            },
            Some(clip),
        ))
        // Operator 62: straddles the clip edge, only (200,220)-(230,260) is visible.
        .with_paint_op(in_clip(
            filled(
                62,
                "f",
                [150.0, 200.0, 230.0, 260.0],
                ColorFacts::rgb(1.0, 0.0, 0.0),
            ),
            Some([200.0, 220.0, 230.0, 260.0]),
        ))
        // Operator 65: completely inside the clip.
        .with_paint_op(in_clip(
            filled(
                65,
                "f",
                [220.0, 240.0, 280.0, 280.0],
                ColorFacts::rgb(0.0, 0.6, 0.0),
            ),
            Some([220.0, 240.0, 280.0, 280.0]),
        ))
        // Operator 68: completely outside the clip, nothing visible.
        .with_paint_op(in_clip(
            filled(
                68,
                "f",
                [360.0, 330.0, 390.0, 360.0],
                ColorFacts::rgb(0.0, 0.0, 1.0),
            ),
            None,
        ))
        // Operator 75: nested clip (240,240)-(440,300) intersected with (200,220)-(350,320) is
        // (240,240)-(350,300); the square (230,230)-(270,270) keeps (240,240)-(270,270).
        .with_paint_op(PaintOpFacts {
            clip: Some([240.0, 240.0, 350.0, 300.0]),
            visible_bbox: Some([240.0, 240.0, 270.0, 270.0]),
            ..filled(75, "f", [230.0, 230.0, 270.0, 270.0], ColorFacts::gray(0.0))
        })
        // Operator 79: after both Q operators nothing clips and the colour is the initial black.
        .with_paint_op(filled(79, "f", [20.0, 200.0, 60.0, 240.0], ColorFacts::gray(0.0)));
    Ok(built(out, expected))
}
