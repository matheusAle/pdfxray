//! Text, paint, image and Type 3 facts: re-derived by interpreting the content streams found in the
//! bytes, then pinned with constants whose arithmetic is spelled out.

use pdfxray_fixtures::{ColorFacts, GlyphPaint, ImageKind, all, find};
use pretty_assertions::assert_eq;

use crate::common::{dict_text, latin1, raw_stream_data, stream_data};
use crate::content::{capture_pages, parse_ops};

fn built(name: &str) -> (Vec<u8>, pdfxray_fixtures::Expected) {
    let b = find(name).unwrap().build().unwrap();
    (b.pdf, b.expected)
}

#[test]
fn interpreted_text_paint_images_and_glyphs_equal_the_declared_facts() {
    let mut with_facts = Vec::new();
    for fixture in all() {
        let built = fixture.build().unwrap();
        let declares = !built.expected.text_ops.is_empty()
            || !built.expected.paint_ops.is_empty()
            || !built.expected.images.is_empty()
            || !built.expected.type3_glyphs.is_empty();
        if !declares {
            continue;
        }
        let capture = capture_pages(&built.pdf);

        // Only compare the kinds a fixture declares: other fixtures draw text or paths too.
        if !built.expected.text_ops.is_empty() {
            assert_eq!(capture.text, built.expected.text_ops, "{} text", fixture.name);
        }
        if !built.expected.paint_ops.is_empty() {
            assert_eq!(capture.paints, built.expected.paint_ops, "{} paint", fixture.name);
        }
        if !built.expected.images.is_empty() {
            assert_eq!(capture.images, built.expected.images, "{} images", fixture.name);
        }
        if !built.expected.type3_glyphs.is_empty() {
            assert_eq!(
                capture.glyphs, built.expected.type3_glyphs,
                "{} glyphs",
                fixture.name
            );
        }
        with_facts.push(fixture.name);
    }

    assert_eq!(
        with_facts,
        ["text-state", "graphics-state", "images", "type3-font"]
    );
}

#[test]
fn text_state_content_is_flate_compressed_and_decodes_to_the_operators() {
    let (pdf, expected) = built("text-state");

    let stored = raw_stream_data(&pdf, 4);
    let decoded = latin1(&stream_data(&pdf, 4));

    assert!(latin1(&dict_text(&pdf, 4)).contains("/Filter /FlateDecode"));
    // A zlib stream starts with CMF 0x78 and a check byte making the pair divisible by 31.
    assert_eq!(stored[0], 0x78);
    assert_eq!((u16::from(stored[0]) << 8 | u16::from(stored[1])) % 31, 0);
    assert!(decoded.starts_with("BT\n/F1 12 Tf\n20 270 Td\n(Plain Tj) Tj\nET\n"));
    assert!(decoded.ends_with("[-250 (Shifted)] TJ\n100 Tz\nET"));
    // Smoke: compression shrank the stream and every declared op index is a showing operator.
    assert!(stored.len() < decoded.len());
    let ops = parse_ops(decoded.as_bytes());
    for op in &expected.text_ops {
        assert_eq!(
            ops[op.op_index as usize].name, op.operator,
            "operator {} of text-state",
            op.op_index
        );
    }
    assert_eq!(ops.len(), 67);
}

#[test]
fn text_state_origins_follow_the_text_matrix_rules() {
    let (_, expected) = built("text-state");

    let rows: Vec<(&str, &str, [f64; 2], f64, u8)> = expected
        .text_ops
        .iter()
        .map(|t| {
            (
                t.operator.as_str(),
                t.text.as_str(),
                t.origin,
                t.effective_size,
                t.render_mode,
            )
        })
        .collect();

    assert_eq!(
        rows,
        vec![
            ("Tj", "Plain Tj", [20.0, 270.0], 12.0, 0),
            ("Tj", "Spaced out words", [20.0, 240.0], 18.0, 0),
            ("Tj", "Line one", [20.0, 210.0], 12.0, 0),
            // ' and " move down by TL = 14: 210 - 14 = 196, 196 - 14 = 182.
            ("'", "Line two", [20.0, 196.0], 12.0, 0),
            ("\"", "Line three", [20.0, 182.0], 12.0, 0),
            // Tf size 1 under Tm [16 0 0 16 20 150]: size 1 * 16 = 16.
            ("Tj", "Sized by Tm", [20.0, 150.0], 16.0, 0),
            // Ts 5 raises the baseline: y = 120 + 5.
            ("Tj", "Raised", [20.0, 125.0], 12.0, 0),
            // Td steps of 90: x = 20, 20 + 90, 110 + 90, 200 + 90.
            ("Tj", "Fill", [20.0, 80.0], 24.0, 0),
            ("Tj", "Line", [110.0, 80.0], 24.0, 1),
            ("Tj", "Both", [200.0, 80.0], 24.0, 2),
            ("Tj", "Gone", [290.0, 80.0], 24.0, 3),
            ("TJ", "Kerning", [20.0, 50.0], 12.0, 0),
            // Leading TJ number -250 at size 12, Tz 150: 250 / 1000 * 12 * 1.5 = 4.5 -> 20 + 4.5.
            ("TJ", "Shifted", [24.5, 25.0], 12.0, 0),
        ]
    );
    // Smoke: two fonts are in play, and the double-quote operator set the spacing it carries.
    let fonts: Vec<&str> = expected
        .text_ops
        .iter()
        .map(|t| t.base_font.as_deref().unwrap())
        .collect();
    assert_eq!(fonts.iter().filter(|f| **f == "Times-Roman").count(), 3);
    let quote = &expected.text_ops[4];
    assert_eq!((quote.word_spacing, quote.char_spacing), (3.0, 1.0));
}

#[test]
fn graphics_state_boxes_clips_and_alpha_are_pinned() {
    let (_, expected) = built("graphics-state");

    let rows: Vec<(u32, &str, [f64; 4], Option<[f64; 4]>)> = expected
        .paint_ops
        .iter()
        .map(|p| (p.op_index, p.operator.as_str(), p.bbox, p.visible_bbox))
        .collect();

    assert_eq!(
        rows,
        vec![
            (3, "f", [20.0, 20.0, 100.0, 80.0], Some([20.0, 20.0, 100.0, 80.0])),
            (
                12,
                "S",
                [120.0, 20.0, 200.0, 80.0],
                Some([120.0, 20.0, 200.0, 80.0])
            ),
            (
                19,
                "B",
                [220.0, 20.0, 300.0, 80.0],
                Some([220.0, 20.0, 300.0, 80.0])
            ),
            (
                30,
                "B",
                [20.0, 100.0, 100.0, 160.0],
                Some([20.0, 100.0, 100.0, 160.0])
            ),
            // CTM [2 0 0 2 150 100]: (10,10)-(30,30) -> (2*10+150, 2*10+100)-(2*30+150, 2*30+100).
            (
                38,
                "f",
                [170.0, 120.0, 210.0, 160.0],
                Some([170.0, 120.0, 210.0, 160.0])
            ),
            // Quarter turn then translation: x' = 150 - y in 130..150, y' = 100 + x in 100..130.
            (
                43,
                "f",
                [130.0, 100.0, 150.0, 130.0],
                Some([130.0, 100.0, 150.0, 130.0])
            ),
            (
                49,
                "f",
                [220.0, 100.0, 320.0, 200.0],
                Some([220.0, 100.0, 320.0, 200.0])
            ),
            (
                53,
                "S",
                [250.0, 130.0, 350.0, 230.0],
                Some([250.0, 130.0, 350.0, 230.0])
            ),
            (
                59,
                "sh",
                [200.0, 220.0, 350.0, 320.0],
                Some([200.0, 220.0, 350.0, 320.0])
            ),
            // Clip x 200..350: the red box 150..230 loses its left 50 units; visible x is 200..230.
            (
                62,
                "f",
                [150.0, 200.0, 230.0, 260.0],
                Some([200.0, 220.0, 230.0, 260.0])
            ),
            (
                65,
                "f",
                [220.0, 240.0, 280.0, 280.0],
                Some([220.0, 240.0, 280.0, 280.0])
            ),
            // x 360..390 lies right of the clip edge 350: nothing is visible.
            (68, "f", [360.0, 330.0, 390.0, 360.0], None),
            // Second clip intersects to x 240..350, y 240..300; the box 230..270 keeps 240..270.
            (
                75,
                "f",
                [230.0, 230.0, 270.0, 270.0],
                Some([240.0, 240.0, 270.0, 270.0])
            ),
            (
                79,
                "f",
                [20.0, 200.0, 60.0, 240.0],
                Some([20.0, 200.0, 60.0, 240.0])
            ),
        ]
    );
    // Smoke: the three colour spaces and both transparency settings really occur.
    let spaces: Vec<&str> = expected
        .paint_ops
        .iter()
        .flat_map(|p| p.fill.iter().chain(p.stroke.iter()))
        .map(|c| c.space.as_str())
        .collect();
    for space in ["DeviceGray", "DeviceRGB", "DeviceCMYK"] {
        assert!(spaces.contains(&space), "{space}");
    }
    let multiply: Vec<u32> = expected
        .paint_ops
        .iter()
        .filter(|p| p.blend_mode == "Multiply")
        .map(|p| p.op_index)
        .collect();
    assert_eq!(multiply, [49, 53]);
}

#[test]
fn graphics_state_is_restored_by_q_and_alpha_is_scoped() {
    let (_, expected) = built("graphics-state");
    let at = |op_index: u32| {
        expected
            .paint_ops
            .iter()
            .find(|p| p.op_index == op_index)
            .unwrap()
    };

    // Operator 12 strokes with width 4 / round caps / dashes, the Q at 13 undoes all of it.
    let dashed = at(12).line.as_ref().unwrap();
    assert_eq!((dashed.width, dashed.cap, dashed.join), (4.0, 1, 1));
    assert_eq!(
        (dashed.dash_array.clone(), dashed.dash_phase),
        (vec![6.0, 3.0], 2.0)
    );
    let plain = at(19).line.as_ref().unwrap();
    assert_eq!(
        (plain.width, plain.cap, plain.join, plain.dash_array.len()),
        (2.0, 0, 0, 0)
    );
    // The ExtGState lives inside q ... Q: operators 49 and 53 are half transparent, 62 is opaque.
    assert_eq!((at(49).fill_alpha, at(53).stroke_alpha), (Some(0.5), Some(0.5)));
    assert_eq!(
        (at(62).fill_alpha, at(62).blend_mode.as_str()),
        (Some(1.0), "Normal")
    );
    // The default colour after all restores is black DeviceGray.
    assert_eq!(at(79).fill, Some(ColorFacts::gray(0.0)));
    assert_eq!(at(79).clip, None);
    assert_eq!(at(65).clip, Some([200.0, 220.0, 350.0, 320.0]));
    assert_eq!(at(75).clip, Some([240.0, 240.0, 350.0, 300.0]));
}

#[test]
fn image_boxes_follow_the_unit_square_through_the_matrices() {
    let (pdf, expected) = built("images");

    let rows: Vec<(Option<u32>, u32, ImageKind, [f64; 4])> = expected
        .images
        .iter()
        .map(|i| (i.stream, i.op_index, i.kind, i.bbox))
        .collect();

    assert_eq!(
        rows,
        vec![
            // cm [100 0 0 100 50 150]: the unit square becomes x 50..150, y 150..250.
            (None, 2, ImageKind::Xobject, [50.0, 150.0, 150.0, 250.0]),
            // cm [0 40 -40 0 120 20]: x' = 120 - 40y in 80..120, y' = 20 + 40x in 20..60.
            (None, 6, ImageKind::Inline, [80.0, 20.0, 120.0, 60.0]),
            // Scale 60 in the form, form matrix +(200,20), page cm +(10,0): x 210..270, y 20..80.
            (Some(6), 2, ImageKind::Xobject, [210.0, 20.0, 270.0, 80.0]),
        ]
    );
    // The image samples are 2 x 2 x 3 bytes: red, green / blue, yellow.
    assert_eq!(
        raw_stream_data(&pdf, 5),
        [0xFF, 0, 0, 0, 0xFF, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0]
    );
    assert!(
        expected
            .images
            .iter()
            .all(|i| (i.width, i.height, i.bits_per_component) == (2, 2, 8))
    );
    // The inline image is one operator even though it spans three lines of the stream.
    let ops = parse_ops(&stream_data(&pdf, 4));
    assert_eq!(ops.len(), 12);
    assert_eq!(ops[6].name, "BI");
}

#[test]
fn type3_glyph_boxes_use_font_matrix_size_and_text_matrix() {
    let (pdf, expected) = built("type3-font");

    let rows: Vec<(u32, u32, u8, &str, [f64; 2], [f64; 4], [f64; 4], GlyphPaint)> = expected
        .type3_glyphs
        .iter()
        .map(|g| {
            (
                g.op_index,
                g.glyph_index,
                g.code,
                g.glyph_name.as_str(),
                g.origin,
                g.declared_bbox,
                g.path_bbox,
                g.paint,
            )
        })
        .collect();

    assert_eq!(
        rows,
        vec![
            // Scale 0.001 * 20 = 0.02: the square 0..800 is 16 wide, at the Td point (30,100).
            (
                5,
                0,
                97,
                "sq",
                [30.0, 100.0],
                [30.0, 100.0, 46.0, 116.0],
                [30.0, 100.0, 46.0, 116.0],
                GlyphPaint::Fill
            ),
            // Advance of `a` is 1000 * 0.02 = 20: origin x = 50. d1 cell 600 x 700 -> 12 x 14.
            // Triangle x 50..550 -> 50 + 1..11, y 50..650 -> 100 + 1..13.
            (
                5,
                1,
                98,
                "tri",
                [50.0, 100.0],
                [50.0, 100.0, 62.0, 114.0],
                [51.0, 101.0, 61.0, 113.0],
                GlyphPaint::Stroke
            ),
            // Tf 1 under Tm scale 30: 0.001 * 30 = 0.03, so 800 becomes 24 at (120,100).
            (
                10,
                0,
                97,
                "sq",
                [120.0, 100.0],
                [120.0, 100.0, 144.0, 124.0],
                [120.0, 100.0, 144.0, 124.0],
                GlyphPaint::Fill
            ),
        ]
    );
    // The glyph procedures are the objects the facts name, and both start with a d1 operator.
    for glyph in &expected.type3_glyphs {
        let ops = parse_ops(&stream_data(&pdf, glyph.proc_object));
        assert_eq!(ops[0].name, "d1", "proc {}", glyph.proc_object);
    }
    assert_eq!(expected.text_ops.len(), 2);
    assert_eq!(expected.text_ops[1].effective_size, 30.0);
}
