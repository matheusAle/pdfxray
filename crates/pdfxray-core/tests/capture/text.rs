//! Synthetic pages: text.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn text_state_operators_move_glyph_origins() {
    // Helvetica: A=667 B=667 C=722 H=722 x=500 space=278.
    let page = synth(
        b"BT /F1 10 Tf 12 TL 20 150 Td (AB) Tj T* (C) Tj 1 Tc 2 Tw (A B) Tj 0 Tc 0 Tw 50 Tz (H) Tj \
          100 Tz 5 Ts (x) Tj 0 Ts [(A) -500 (B) 250 (C)] TJ ET",
        "",
        &[],
    );

    let all = glyphs(&page);

    assert_eq!(text(&all), "ABCA BHxABC");
    assert_points(
        &origins(&all),
        &[
            (20.0, 150.0),
            (26.67, 150.0),
            (20.0, 138.0),
            (27.22, 138.0),
            (34.89, 138.0),
            (40.67, 138.0),
            (48.34, 138.0),
            (51.95, 143.0),
            (56.95, 138.0),
            (68.62, 138.0),
            (72.79, 138.0),
        ],
    );
    // 50 % horizontal scaling halves the advance of "H".
    assert!(close(all[6].advance, 3.61));
    assert_eq!(all[6].item.gstate.text.horizontal_scaling, 50.0);
    // Tc/Tw show up in the state of the items they affect.
    assert_eq!(
        (
            all[3].item.gstate.text.char_spacing,
            all[3].item.gstate.text.word_spacing
        ),
        (1.0, 2.0)
    );
    // The text matrix ends after the last glyph.
    let tj = page.ops.iter().rfind(|o| o.operator == "TJ").unwrap();
    assert_matrix(tj.gstate.text.tm, [1.0, 0.0, 0.0, 1.0, 80.01, 138.0]);
    assert_eq!(tj.operands, vec!["[(A) -500 (B) 250 (C)]".to_owned()]);
    assert_eq!(tj.item_count, 3);
    assert_eq!(page.alignment, Alignment::Exact);
    // Smoke: glyph_index restarts per operator.
    assert_eq!(
        all.iter().map(|g| g.index).collect::<Vec<_>>(),
        [0, 1, 0, 0, 1, 2, 0, 0, 0, 1, 2]
    );
}

#[test]
fn quote_operators_move_to_the_next_line_and_set_spacing() {
    let page = synth(
        b"BT /F1 10 Tf 14 TL 20 100 Td (A) ' 3 1 (B) \" 0 -20 TD (C) Tj T* (H) Tj ET",
        "",
        &[],
    );

    let all = glyphs(&page);

    assert_eq!(text(&all), "ABCH");
    assert_points(
        &origins(&all),
        &[(20.0, 86.0), (20.0, 72.0), (20.0, 52.0), (20.0, 32.0)],
    );
    // `"` set word and character spacing before showing; the item already sees them.
    assert_eq!(
        (
            all[1].item.gstate.text.word_spacing,
            all[1].item.gstate.text.char_spacing
        ),
        (3.0, 1.0)
    );
    // `TD` set the leading to 20, which `T*` then used.
    assert_eq!(all[3].item.gstate.text.leading, 20.0);
    assert_eq!(
        page.ops.iter().map(|o| o.operator.as_str()).collect::<Vec<_>>()[4..7],
        ["'", "\"", "TD"]
    );
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn rotated_text_matrix_rotates_glyph_boxes() {
    let page = synth(b"BT /F1 10 Tf 0 1 -1 0 100 100 Tm (H) Tj ET", "", &[]);

    let all = glyphs(&page);

    assert_eq!(all.len(), 1);
    assert_points(&origins(&all), &[(100.0, 100.0)]);
    // Text space x runs up the page: the H box (0..7.22 x -2.07..7.18) turns 90 degrees.
    assert_rect(all[0].item.bbox, [92.82, 100.0, 102.07, 107.22]);
    assert!(close(all[0].advance, 7.22));
    assert_matrix(all[0].item.gstate.text.tm, [0.0, 1.0, -1.0, 0.0, 100.0, 100.0]);
}

#[test]
fn invisible_text_is_captured_with_its_boxes() {
    let page = synth(b"BT /F1 10 Tf 20 100 Td 3 Tr (Hid) Tj ET", "", &[]);

    let all = glyphs(&page);

    assert_eq!(text(&all), "Hid");
    assert!(all.iter().all(|g| g.item.gstate.text.render_mode == 3));
    assert_points(&origins(&all), &[(20.0, 100.0), (27.22, 100.0), (29.44, 100.0)]);
    assert_rect(all[0].item.bbox, [20.0, 97.93, 27.22, 107.18]);
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn stroked_text_boxes_include_half_the_line_width() {
    let page = synth(b"BT /F1 10 Tf 2 w 1 Tr 20 100 Td (H) Tj ET", "", &[]);

    let all = glyphs(&page);

    assert_eq!(all.len(), 1);
    assert_rect(all[0].item.bbox, [19.0, 96.93, 28.22, 108.18]);
    assert_eq!(all[0].item.gstate.text.render_mode, 1);
}

#[test]
fn clip_only_text_becomes_a_clip_at_et() {
    let page = synth(
        b"BT /F1 10 Tf 20 150 Td 7 Tr (Hi) Tj ET 0 0 200 200 re f",
        "",
        &[],
    );

    let all = glyphs(&page);
    let clips = kinds_of(&page, |k| matches!(k, PaintKind::Clip));
    let filled = fills(&page);

    assert_eq!(text(&all), "Hi");
    assert!(all.iter().all(|g| g.item.gstate.text.render_mode == 7));
    assert_eq!(clips.len(), 1);
    assert_rect(clips[0].bbox, [20.0, 147.93, 29.44, 157.18]);
    assert_eq!(page.ops[clips[0].op as usize].operator, "ET");
    assert_eq!(filled.len(), 1);
    assert_rect(filled[0].bbox, [0.0, 0.0, 200.0, 200.0]);
    assert_eq!(filled[0].clip, Some(clips[0].bbox));
}

#[test]
fn type3_glyphs_get_declared_boxes_and_widths_from_the_font() {
    let page = synth(
        b"BT /T3 10 Tf 20 100 Td (ab) Tj ET",
        "/Font << /T3 6 0 R /F1 5 0 R >>",
        &[
            (
                6,
                Obj::Plain(
                    "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 10 10] /FontMatrix [0.1 0 0 0.1 0 0] \
                     /CharProcs << /a 7 0 R /b 7 0 R >> \
                     /Encoding << /Type /Encoding /Differences [97 /a /b] >> /FirstChar 97 /LastChar 98 /Widths [10 12] >>",
                ),
            ),
            (7, Obj::Stream("", b"10 0 0 0 10 10 d1 0 0 10 10 re f")),
        ],
    );

    let all = glyphs(&page);

    assert_eq!(all.iter().map(|g| g.code).collect::<Vec<_>>(), vec![97, 98]);
    assert_points(&origins(&all), &[(20.0, 100.0), (30.0, 100.0)]);
    assert_rect(all[0].item.bbox, [20.0, 100.0, 30.0, 110.0]);
    // The box is the glyph's own `d1` box (0..10 in glyph space, 0.1 per unit), not its advance.
    assert_rect(all[1].item.bbox, [30.0, 100.0, 40.0, 110.0]);
    assert!(close(all[1].advance, 12.0));
    assert_eq!(all[0].item.gstate.text.font.as_deref(), Some("T3"));
    // The glyph procedure itself stays opaque: no extra paint items from its `re f`.
    assert_eq!(page.items.len(), 2);
}

#[test]
fn font_state_survives_q_and_q_restores_it() {
    // The Tf inside q/Q must not leak: after Q the string is read with the outer font again.
    let page = synth(
        b"BT /F1 10 Tf 20 100 Td q /F2 20 Tf Q (A) Tj ET",
        "/Font << /F1 5 0 R /F2 6 0 R >>",
        &[(
            6,
            Obj::Plain("<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>"),
        )],
    );

    let all = glyphs(&page);

    assert_eq!(all.len(), 1);
    assert_eq!(all[0].item.gstate.text.font.as_deref(), Some("F1"));
    assert!(close(all[0].advance, 6.67));
    // Helvetica metrics (descent 207), not the Courier ones (157), prove the font was restored.
    assert_rect(all[0].item.bbox, [20.0, 97.93, 26.67, 107.18]);
}
