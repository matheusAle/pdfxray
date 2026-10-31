//! The `minimal-one-page` fixture.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn minimal_operator_list_is_exact() {
    let page = fixture_page("minimal-one-page", 0);

    let rows: Vec<_> = page
        .ops
        .iter()
        .map(|o| {
            (
                o.ix,
                o.operator.as_str(),
                o.operands.clone(),
                o.local_ix,
                o.span,
                o.item_count,
            )
        })
        .collect();

    assert_eq!(
        rows,
        vec![
            (0, "BT", vec![], 0, Some((0, 2)), 0),
            (
                1,
                "Tf",
                vec!["/F1".to_owned(), "18".to_owned()],
                1,
                Some((3, 12)),
                0
            ),
            (
                2,
                "Td",
                vec!["20".to_owned(), "40".to_owned()],
                2,
                Some((13, 21)),
                0
            ),
            (3, "Tj", vec!["(Hello pdfxray)".to_owned()], 3, Some((22, 40)), 13),
            (4, "ET", vec![], 4, Some((41, 43)), 0),
        ]
    );
    assert_eq!(page.alignment, Alignment::Exact);
    assert_eq!(page.notes, Vec::new());
    assert!(page.marked.is_empty() && page.xobjects.is_empty());
    assert!(
        page.ops
            .iter()
            .all(|o| o.stream == StreamRef::PageContent { index: 0 })
    );
    assert_consistent(&page);
}

#[test]
fn minimal_has_one_glyph_per_character_with_advancing_origins() {
    let page = fixture_page("minimal-one-page", 0);
    let glyphs = glyphs(&page);

    assert_eq!(text(&glyphs), "Hello pdfxray");
    assert_eq!(
        glyphs.iter().map(|g| g.code).collect::<Vec<_>>(),
        b"Hello pdfxray".iter().map(|b| u32::from(*b)).collect::<Vec<_>>()
    );
    assert_eq!(
        glyphs.iter().map(|g| g.index).collect::<Vec<_>>(),
        (0..13).collect::<Vec<_>>()
    );
    // Helvetica widths (1/1000 em) times the font size 18.
    let widths = [
        722.0, 556.0, 222.0, 222.0, 556.0, 278.0, 556.0, 556.0, 278.0, 500.0, 333.0, 556.0, 500.0,
    ];
    let mut x = 20.0;
    let want_origins: Vec<(f64, f64)> = widths
        .iter()
        .map(|w| {
            let here = (x, 40.0);
            x += w * 18.0 / 1000.0;
            here
        })
        .collect();
    assert_points(&origins(&glyphs), &want_origins);
    for (g, w) in glyphs.iter().zip(widths) {
        assert!(close(g.advance, w * 18.0 / 1000.0), "advance of {}", g.unicode);
    }
    // The text matrix after the operator is the end of the last glyph.
    assert_matrix(page.ops[3].gstate.text.tm, [1.0, 0.0, 0.0, 1.0, x, 40.0]);
    // Smoke: origins strictly increase and each glyph starts where the previous one ended.
    assert!(
        glyphs
            .windows(2)
            .all(|w| close(w[1].origin.x, w[0].origin.x + w[0].advance))
    );
}

#[test]
fn minimal_glyph_boxes_use_font_metrics_and_stay_on_the_page() {
    let page = fixture_page("minimal-one-page", 0);
    let glyphs = glyphs(&page);

    // "H": advance 722, Helvetica descent -207 and ascent 718 at 18 pt.
    assert_rect(glyphs[0].item.bbox, [20.0, 36.274, 32.996, 52.924]);
    // The space still has a box (it can be selected).
    assert_rect(glyphs[5].item.bbox, [61.004, 36.274, 66.008, 52.924]);
    let page_box = Rect::new(0.0, 0.0, 200.0, 100.0);
    assert!(glyphs.iter().all(|g| page_box.contains_rect(&g.item.bbox)));
    assert!(
        glyphs
            .iter()
            .all(|g| g.item.bbox.width() > 0.0 && g.item.bbox.height() > 0.0)
    );
}

#[test]
fn minimal_glyph_gstate_carries_font_and_size() {
    let page = fixture_page("minimal-one-page", 0);
    let first = &glyphs(&page)[0];

    let text = &first.item.gstate.text;

    assert_eq!(text.font.as_deref(), Some("F1"));
    assert_eq!(text.font_ref, Some(ObjRef::new(5, 0)));
    assert!(close(text.font_size, 18.0));
    assert_eq!(text.render_mode, 0);
    assert_matrix(text.tm, [1.0, 0.0, 0.0, 1.0, 20.0, 40.0]);
    assert_eq!(first.item.gstate.fill, gray(0.0));
    assert_eq!(first.item.clip, None);
    assert!(first.item.mc_stack.is_empty() && first.item.xobject_stack.is_empty());
}

#[test]
fn page_index_returns_the_glyph_under_the_point() {
    let page = fixture_page("minimal-one-page", 0);
    let index = PageIndex::new(&page);

    // Inside "H" (box 20..32.996 x 36.274..52.924).
    assert_eq!(index.hit(Point::new(26.0, 45.0)), vec![0]);
    // Inside the "p" of "pdfxray" (glyph 6).
    assert_eq!(index.hit(Point::new(70.0, 42.0)), vec![6]);
    assert_eq!(index.hit(Point::new(5.0, 5.0)), Vec::<u32>::new());
    // Smoke: the centre of every glyph box hits that glyph first.
    for g in glyphs(&page) {
        let c = Point::new(
            f64::midpoint(g.item.bbox.x0, g.item.bbox.x1),
            f64::midpoint(g.item.bbox.y0, g.item.bbox.y1),
        );
        assert_eq!(index.hit(c).first(), Some(&g.item.ix), "glyph {}", g.unicode);
    }
}

#[test]
fn page_index_orders_by_area_then_latest_paint_and_respects_clips() {
    let content = b"0 0 100 100 re f 10 10 20 20 re f \
                    50 50 30 30 re f 60 60 30 30 re f \
                    q 150 150 30 30 re W n 140 140 60 60 re f Q";
    let page = synth(content, "", &[]);
    let index = PageIndex::new(&page);

    // item ix: 0 big, 1 small, 2 first 30x30, 3 second 30x30, 4 clip (not indexed), 5 clipped fill
    assert_eq!(index.hit(Point::new(15.0, 15.0)), vec![1, 0]);
    assert_eq!(index.hit(Point::new(70.0, 70.0)), vec![3, 2, 0]);
    assert_eq!(index.hit(Point::new(160.0, 160.0)), vec![5]);
    assert_eq!(index.hit(Point::new(145.0, 145.0)), Vec::<u32>::new());
    assert_eq!(page.items.len(), 6);
}
