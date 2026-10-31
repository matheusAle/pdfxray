//! Synthetic pages: the graphics state.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn graphics_state_is_saved_restored_and_snapshotted() {
    let page = synth(
        b"q 2 0 0 2 10 10 cm 5 w 1 J 2 j 4 M 0 0 1 RG [3 2] 1 d 10 10 30 30 re B Q 0 0 5 5 re f",
        "",
        &[],
    );

    let boxed = &page.items[0];
    assert!(matches!(boxed.kind, PaintKind::FillStroke));
    assert_rect(boxed.bbox, [25.0, 25.0, 95.0, 95.0]); // 30..90 plus half of 5 w scaled by 2
    let gs = &boxed.gstate;
    assert_matrix(gs.ctm, [2.0, 0.0, 0.0, 2.0, 10.0, 10.0]);
    assert_eq!(
        (gs.line_width, gs.line_cap, gs.line_join, gs.miter_limit),
        (5.0, 1, 2, 4.0)
    );
    assert_eq!((gs.dash_array.clone(), gs.dash_phase), (vec![3.0, 2.0], 1.0));
    assert_eq!(gs.stroke.space, "DeviceRGB");
    assert_eq!(gs.stroke.components, vec![0.0, 0.0, 1.0]);
    assert_eq!(gs.fill, gray(0.0));
    // After Q everything is back to the defaults and the later fill is a plain 5 x 5 box.
    let q = page.ops.iter().find(|o| o.operator == "Q").unwrap();
    assert_matrix(q.gstate.ctm, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!((q.gstate.line_width, q.gstate.dash_array.clone()), (1.0, vec![]));
    assert_rect(page.items[1].bbox, [0.0, 0.0, 5.0, 5.0]);
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn ext_gstate_values_reach_the_state() {
    let page = synth(
        b"/GS1 gs 10 10 50 50 re f /GS2 gs 70 70 20 20 re f",
        "/ExtGState << /GS1 << /LW 3 /CA 0.25 /ca 0.5 /BM /Multiply /D [[4 1] 2] /SMask << /Type /Mask /S /Alpha /G 6 0 R >> >> \
         /GS2 << /SMask /None /BM [/Screen /Normal] >> >>",
        &[(
            6,
            Obj::Stream(
                "/Type /XObject /Subtype /Form /BBox [0 0 200 200] /Group << /S /Transparency >>",
                b"1 g 0 0 200 200 re f",
            ),
        )],
    );

    assert_eq!(page.items.len(), 2);
    let (first, second) = (&page.items[0].gstate, &page.items[1].gstate);
    assert_eq!(first.ext_gstate.as_deref(), Some("GS1"));
    assert_eq!(
        (
            first.line_width,
            first.alpha_stroke,
            first.alpha_fill,
            first.blend_mode.as_str(),
            first.soft_mask
        ),
        (3.0, 0.25, 0.5, "Multiply", true)
    );
    assert_eq!(
        (first.dash_array.clone(), first.dash_phase),
        (vec![4.0, 1.0], 2.0)
    );
    assert_eq!(second.ext_gstate.as_deref(), Some("GS2"));
    assert_eq!((second.blend_mode.as_str(), second.soft_mask), ("Screen", false));
    // Values GS2 does not mention carry over.
    assert_eq!(
        (second.alpha_fill, second.alpha_stroke, second.line_width),
        (0.5, 0.25, 3.0)
    );
    assert_rect(page.items[1].bbox, [70.0, 70.0, 90.0, 90.0]);
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn color_spaces_and_patterns_are_recorded() {
    let page = synth(
        b"0 1 0 0 k 0 0 10 10 re f /Sp cs 0.5 scn 20 0 10 10 re f /Pattern cs /P1 scn 40 0 10 10 re f",
        "/ColorSpace << /Sp [/Separation /Spot /DeviceGray << /FunctionType 2 /Domain [0 1] /N 1 /C0 [1] /C1 [0] >>] >> \
         /Pattern << /P1 6 0 R >>",
        &[(
            6,
            Obj::Stream(
                "/Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << >>",
                b"1 0 0 rg 0 0 5 5 re f",
            ),
        )],
    );

    let colors: Vec<_> = page
        .items
        .iter()
        .map(|i| {
            let f = &i.gstate.fill;
            (
                f.space.as_str(),
                f.resource.as_deref(),
                f.components.clone(),
                f.pattern.as_deref(),
            )
        })
        .collect();

    assert_eq!(
        colors,
        vec![
            ("DeviceCMYK", None, vec![0.0, 1.0, 0.0, 0.0], None),
            ("Separation", Some("Sp"), vec![0.5], None),
            ("Pattern", None, vec![], Some("P1")),
        ]
    );
    // A pattern fill is one item with the filled path's box, not the pattern cells.
    assert_rect(page.items[2].bbox, [40.0, 0.0, 50.0, 10.0]);
    assert_eq!(page.items.len(), 3);
}

#[test]
fn clips_apply_until_restored_and_intersect() {
    let page = synth(
        b"q 10 10 m 50 10 l 50 50 l h W n 0 0 200 200 re f Q 0 0 200 200 re f \
          q 0 0 100 100 re W n 50 50 100 100 re W n 0 0 200 200 re f Q",
        "",
        &[],
    );

    let rows: Vec<_> = page
        .items
        .iter()
        .map(|i| {
            (
                std::mem::discriminant(&i.kind) == std::mem::discriminant(&PaintKind::Clip),
                i.clip,
            )
        })
        .collect();

    // clip, fill(clipped), fill(no clip), clip, clip(already clipped), fill(clipped twice)
    assert_eq!(rows.len(), 6);
    assert_eq!(
        rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        [true, false, false, true, true, false]
    );
    assert_rect(page.items[0].bbox, [10.0, 10.0, 50.0, 50.0]);
    assert_eq!(page.items[0].clip, None);
    assert_rect(page.items[1].clip.unwrap(), [10.0, 10.0, 50.0, 50.0]);
    assert_eq!(page.items[2].clip, None);
    assert_rect(page.items[4].clip.unwrap(), [0.0, 0.0, 100.0, 100.0]);
    assert_rect(page.items[5].clip.unwrap(), [50.0, 50.0, 100.0, 100.0]);
    assert_eq!(page.items[5].gstate.clip, page.items[5].clip);
    // The `n` that applied the clip owns the Clip item.
    let n_ops: Vec<_> = page
        .ops
        .iter()
        .filter(|o| o.operator == "n")
        .map(|o| o.item_count)
        .collect();
    assert_eq!(n_ops, vec![1, 1, 1]);
}

#[test]
fn images_are_boxed_by_the_ctm_for_xobjects_and_inline_data() {
    let content = b"q 50 0 0 30 20 40 cm /Im Do Q q 0 20 -20 0 150 50 cm /Im Do Q \
                    q 10 0 0 10 100 100 cm BI /W 1 /H 1 /BPC 8 /CS /G ID \x80 EI Q";
    let page = synth(
        content,
        "/XObject << /Im 6 0 R >>",
        &[(
            6,
            Obj::Stream(
                "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8",
                b"\x00\x40\x80\xff",
            ),
        )],
    );

    let images: Vec<_> = page.items.iter().map(|i| (i.kind.clone(), i.bbox)).collect();

    assert_eq!(images.len(), 3);
    assert_eq!(
        images[0].0,
        PaintKind::Image {
            width: 2,
            height: 2,
            inline: false
        }
    );
    assert_rect(images[0].1, [20.0, 40.0, 70.0, 70.0]);
    // Rotated 90 degrees: the box is the axis-aligned hull of the rotated unit square.
    assert_eq!(
        images[1].0,
        PaintKind::Image {
            width: 2,
            height: 2,
            inline: false
        }
    );
    assert_rect(images[1].1, [130.0, 50.0, 150.0, 70.0]);
    assert_eq!(
        images[2].0,
        PaintKind::Image {
            width: 1,
            height: 1,
            inline: true
        }
    );
    assert_rect(images[2].1, [100.0, 100.0, 110.0, 110.0]);
    // The inline image's span covers its data and `EI`.
    let bi = page.ops.iter().find(|o| o.operator == "BI").unwrap();
    let (start, end) = bi.span.unwrap();
    assert!(content[start..end].starts_with(b"BI /W 1") && content[start..end].ends_with(b"EI"));
    assert_eq!(page.ops.last().map(|o| o.operator.as_str()), Some("Q"));
    assert_eq!(page.alignment, Alignment::Exact);
}

#[test]
fn shadings_cover_the_clip_or_the_page() {
    let page = synth(
        b"q 20 20 100 100 re W n /Sh sh Q /Sh sh",
        "/Shading << /Sh << /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 200 0] \
         /Function << /FunctionType 2 /Domain [0 1] /N 1 /C0 [0] /C1 [1] >> >> >>",
        &[],
    );

    let shadings = kinds_of(&page, |k| matches!(k, PaintKind::Shading));

    assert_eq!(shadings.len(), 2);
    assert_rect(shadings[0].bbox, [20.0, 20.0, 120.0, 120.0]);
    assert_rect(shadings[1].bbox, [0.0, 0.0, 200.0, 200.0]);
    assert_eq!(shadings[0].clip.map(|c| c.area()), Some(10_000.0));
    assert_eq!(page.items.len(), 3); // the clip, then the two shadings
}
