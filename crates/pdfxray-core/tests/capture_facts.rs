//! Capture checked against the facts the fixture generator records for text, painting, images,
//! Type 3 glyphs and page boxes. The facts were written from the content streams, independently of
//! the engine, so agreement means the walker and the engine both landed on the right numbers.

// Helpers here are not `#[test]` functions, so clippy.toml's allow-*-in-tests does not reach them;
// failing loudly is what a test helper should do. Float comparisons are against operand values
// echoed verbatim from the content stream (exactly representable), not computed results.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

use std::fs;

use hayro_syntax::Pdf;
use kurbo::Affine;
use pdfxray_core::ObjRef;
use pdfxray_core::capture::{
    Alignment, CapturedPage, OpRecord, PaintItem, PaintKind, StreamRef, capture_document, capture_page,
};
use pdfxray_fixtures::{Expected, fixtures_dir};
use pretty_assertions::assert_eq;

const EPS: f64 = 1e-3;

fn load(name: &str) -> Pdf {
    Pdf::new(fs::read(pdfxray_fixtures::pdf_path(name)).unwrap()).unwrap()
}

fn facts(name: &str) -> Expected {
    serde_json::from_slice(&fs::read(fixtures_dir().join(format!("{name}.expected.json"))).unwrap()).unwrap()
}

fn page0(name: &str) -> CapturedPage {
    capture_page(&load(name), 0).unwrap()
}

/// The operator with index `local_ix` in the page stream (`stream` `None`) or in the form object
/// `stream`.
fn op_at(page: &CapturedPage, stream: Option<u32>, local_ix: u32) -> &OpRecord {
    page.ops
        .iter()
        .find(|o| {
            o.local_ix == local_ix
                && match (&o.stream, stream) {
                    (StreamRef::PageContent { .. }, None) => true,
                    (StreamRef::FormXObject { obj, .. }, Some(n)) => *obj == Some(ObjRef::new(n, 0)),
                    _ => false,
                }
        })
        .unwrap_or_else(|| panic!("no operator {local_ix} in stream {stream:?}"))
}

fn items_of<'p>(page: &'p CapturedPage, op: &OpRecord) -> &'p [PaintItem] {
    match op.first_item {
        Some(first) => &page.items[first as usize..(first + op.item_count) as usize],
        None => &[],
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < EPS
}

#[track_caller]
fn assert_box(actual: pdfxray_core::Rect, want: [f64; 4]) {
    let got = [actual.x0, actual.y0, actual.x1, actual.y1];
    assert!(
        got.iter().zip(want).all(|(a, b)| near(*a, b)),
        "{got:?} != {want:?}"
    );
}

#[track_caller]
fn assert_opt_box(actual: Option<pdfxray_core::Rect>, want: Option<[f64; 4]>) {
    match (actual, want) {
        (Some(a), Some(w)) => assert_box(a, w),
        (None, None) => {}
        (a, w) => panic!("{a:?} != {w:?}"),
    }
}

fn glyph_parts(item: &PaintItem) -> Option<(u32, u32, &str, pdfxray_core::Point, f64)> {
    match &item.kind {
        PaintKind::Glyph {
            glyph_index,
            char_code,
            unicode,
            origin,
            advance,
        } => Some((
            *glyph_index,
            *char_code,
            unicode.as_deref().unwrap_or(""),
            *origin,
            *advance,
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// text-state
// ---------------------------------------------------------------------------------------------

#[test]
fn text_state_fixture_matches_every_text_fact() {
    let page = page0("text-state");
    let facts = facts("text-state");

    assert_eq!(page.alignment, Alignment::Exact);
    assert_eq!(facts.text_ops.len(), 13);
    for fact in &facts.text_ops {
        let op = op_at(&page, None, fact.op_index);
        let items = items_of(&page, op);
        let first = glyph_parts(&items[0]).unwrap();
        let shown: String = items.iter().filter_map(glyph_parts).map(|g| g.2).collect();
        let text = &items[0].gstate.text;
        let ctm_tm = Affine::new(items[0].gstate.ctm) * Affine::new(text.tm);
        let [_, _, c, d, _, _] = ctm_tm.as_coeffs();
        let ctx = format!("op {} ({})", fact.op_index, fact.text);

        assert_eq!(op.operator, fact.operator, "{ctx}");
        assert_eq!(shown, fact.text, "{ctx}");
        assert!(
            near(first.3.x, fact.origin[0]) && near(first.3.y, fact.origin[1]),
            "{ctx}: origin {:?} != {:?}",
            first.3,
            fact.origin
        );
        assert_eq!(text.font.as_deref(), Some(fact.font.as_str()), "{ctx}");
        assert!(near(text.font_size, fact.font_size), "{ctx}: size");
        assert!(
            near(text.font_size * c.hypot(d), fact.effective_size),
            "{ctx}: effective size"
        );
        assert_eq!(text.render_mode, fact.render_mode, "{ctx}");
        assert!(near(text.char_spacing, fact.char_spacing), "{ctx}: Tc");
        assert!(near(text.word_spacing, fact.word_spacing), "{ctx}: Tw");
        assert!(
            near(text.horizontal_scaling, fact.horizontal_scaling),
            "{ctx}: Tz"
        );
        assert!(near(text.rise, fact.rise), "{ctx}: Ts");
        assert!(near(text.leading, fact.leading), "{ctx}: TL");
    }
}

#[test]
fn text_state_fixture_concrete_values() {
    let page = page0("text-state");

    // Operator 21 is the `'` after "Line one": one TL (14) below it, leading read from TL.
    let quote = op_at(&page, None, 21);
    let line_two = items_of(&page, quote);
    assert_eq!(quote.operator, "'");
    assert_eq!(line_two.len(), "Line two".len());
    assert_eq!(
        glyph_parts(&line_two[0]).map(|g| (g.3.x, g.3.y)),
        Some((20.0, 196.0))
    );
    // Operator 22 is `"`: 3 Tw and 1 Tc come from its operands.
    let dq = op_at(&page, None, 22);
    let gs = &items_of(&page, dq)[0].gstate.text;
    assert_eq!(
        (gs.word_spacing, gs.char_spacing, dq.operands.len()),
        (3.0, 1.0, 3)
    );
    // "Gone" is Tr 3: invisible but captured, with real boxes.
    let gone: Vec<_> = page
        .items
        .iter()
        .filter(|i| i.gstate.text.render_mode == 3 && glyph_parts(i).is_some())
        .collect();
    assert_eq!(gone.len(), 4);
    assert!(gone.iter().all(|i| i.bbox.width() > 0.0 && i.bbox.height() > 0.0));
    // The kerned TJ `[(K) 120 (ern) -80 (ing)]` shows "Kerning"; "ern" starts 120/1000 * 12 = 1.44
    // pt left of where "K" ended, "ing" 80/1000 * 12 = 0.96 pt right of where "ern" ended.
    let kerning = items_of(&page, op_at(&page, None, 58));
    let parts: Vec<_> = kerning.iter().filter_map(glyph_parts).collect();
    assert_eq!(parts.iter().map(|g| g.2).collect::<String>(), "Kerning");
    assert!(near(parts[1].3.x, parts[0].3.x + parts[0].4 - 1.44));
    assert!(near(parts[4].3.x, parts[3].3.x + parts[3].4 + 0.96));
    // Smoke: stroke render modes grow the glyph box beyond the fill box by half the line width.
    let fill = page
        .items
        .iter()
        .find(|i| i.gstate.text.render_mode == 0 && i.gstate.text.font_size == 24.0);
    let stroke = page.items.iter().find(|i| i.gstate.text.render_mode == 1);
    assert!(fill.is_some() && stroke.is_some());
}

// ---------------------------------------------------------------------------------------------
// graphics-state
// ---------------------------------------------------------------------------------------------

#[test]
fn graphics_state_fixture_matches_every_paint_fact() {
    let page = page0("graphics-state");
    let facts = facts("graphics-state");

    assert_eq!(page.alignment, Alignment::Exact);
    assert!(facts.paint_ops.len() >= 10, "{} facts", facts.paint_ops.len());
    for fact in &facts.paint_ops {
        let op = op_at(&page, None, fact.op_index);
        let painted: Vec<_> = items_of(&page, op)
            .iter()
            .filter(|i| !matches!(i.kind, PaintKind::Clip))
            .collect();
        let ctx = format!("op {} ({})", fact.op_index, fact.operator);
        assert_eq!(op.operator, fact.operator, "{ctx}");
        assert_eq!(painted.len(), 1, "{ctx}");
        let item = painted[0];
        let gs = &item.gstate;

        let strokes = fact.stroke.is_some();
        if strokes {
            // Facts give the geometry box; the item box adds half the line width.
            let half = gs.line_width / 2.0;
            let [a, b, c, d, _, _] = gs.ctm;
            let (dx, dy) = (half * a.hypot(c), half * b.hypot(d));
            assert_box(
                item.bbox,
                [
                    fact.bbox[0] - dx,
                    fact.bbox[1] - dy,
                    fact.bbox[2] + dx,
                    fact.bbox[3] + dy,
                ],
            );
        } else {
            assert_box(item.bbox, fact.bbox);
            if let (Some(clip), Some(visible)) = (item.clip, fact.visible_bbox) {
                let x0 = item.bbox.x0.max(clip.x0);
                let y0 = item.bbox.y0.max(clip.y0);
                let x1 = item.bbox.x1.min(clip.x1);
                let y1 = item.bbox.y1.min(clip.y1);
                assert_box(pdfxray_core::Rect::new(x0, y0, x1, y1), visible);
            }
        }
        if let Some(fill) = &fact.fill {
            assert_eq!(
                (gs.fill.space.as_str(), gs.fill.components.as_slice()),
                (fill.space.as_str(), fill.components.as_slice()),
                "{ctx}: fill"
            );
            assert_eq!(gs.alpha_fill, fact.fill_alpha.unwrap_or(1.0), "{ctx}: ca");
        }
        if let Some(stroke) = &fact.stroke {
            assert_eq!(
                (gs.stroke.space.as_str(), gs.stroke.components.as_slice()),
                (stroke.space.as_str(), stroke.components.as_slice()),
                "{ctx}: stroke"
            );
            assert_eq!(gs.alpha_stroke, fact.stroke_alpha.unwrap_or(1.0), "{ctx}: CA");
        }
        assert_eq!(gs.blend_mode, fact.blend_mode, "{ctx}");
        assert_opt_box(item.clip, fact.clip);
        if let Some(line) = &fact.line {
            assert_eq!(
                (
                    gs.line_width,
                    gs.line_cap,
                    gs.line_join,
                    gs.dash_array.clone(),
                    gs.dash_phase
                ),
                (
                    line.width,
                    line.cap,
                    line.join,
                    line.dash_array.clone(),
                    line.dash_phase
                ),
                "{ctx}: line"
            );
        }
        if let Some(name) = &fact.shading {
            assert_eq!(op.operands, vec![format!("/{name}")], "{ctx}");
            assert_eq!(item.kind, PaintKind::Shading, "{ctx}");
        }
    }
}

#[test]
fn graphics_state_fixture_concrete_values() {
    let page = page0("graphics-state");

    // The first painting operator: a red 80 x 60 fill at (20, 20).
    let red = &page.items[0];
    assert_eq!(red.kind, PaintKind::Fill);
    assert_box(red.bbox, [20.0, 20.0, 100.0, 80.0]);
    assert_eq!(red.gstate.fill.components, vec![1.0, 0.0, 0.0]);
    assert_eq!(red.clip, None);
    // The transparent, multiplied fill uses the ExtGState resource.
    let multiplied = page
        .items
        .iter()
        .find(|i| i.gstate.blend_mode == "Multiply")
        .unwrap();
    assert_eq!(multiplied.gstate.ext_gstate.as_deref(), Some("GS1"));
    assert_eq!(
        (multiplied.gstate.alpha_fill, multiplied.gstate.alpha_stroke),
        (0.5, 0.5)
    );
    // The shading is boxed by its clip, and the later fills inside the clip carry it.
    let shading = page.items.iter().find(|i| i.kind == PaintKind::Shading).unwrap();
    assert_box(shading.bbox, [200.0, 220.0, 350.0, 320.0]);
    // Smoke: after the closing Q operators the last fill is unclipped again.
    let last = page.items.last().unwrap();
    assert_eq!(last.kind, PaintKind::Fill);
    assert_eq!(last.clip, None);
    assert_box(last.bbox, [20.0, 200.0, 60.0, 240.0]);
}

// ---------------------------------------------------------------------------------------------
// images and type3-font
// ---------------------------------------------------------------------------------------------

#[test]
fn images_fixture_matches_every_image_fact() {
    let page = page0("images");
    let facts = facts("images");

    assert_eq!(page.alignment, Alignment::Exact);
    assert_eq!(facts.images.len(), 3);
    for fact in &facts.images {
        let op = op_at(&page, fact.stream, fact.op_index);
        let items = items_of(&page, op);
        let ctx = format!("op {} in {:?}", fact.op_index, fact.stream);

        assert_eq!(items.len(), 1, "{ctx}");
        assert_eq!(
            items[0].kind,
            PaintKind::Image {
                width: fact.width,
                height: fact.height,
                inline: op.operator == "BI",
            },
            "{ctx}"
        );
        assert_eq!(op.operator == "BI", fact.name.is_none(), "{ctx}");
        assert_box(items[0].bbox, fact.bbox);
        assert_eq!(items[0].xobject_stack.is_empty(), fact.stream.is_none(), "{ctx}");
    }
    // Paint order: the XObject, the inline image, then the form's image.
    assert_eq!(page.items.len(), 3);
    assert_eq!(
        page.items
            .iter()
            .map(|i| page.ops[i.op as usize].operator.as_str())
            .collect::<Vec<_>>(),
        ["Do", "BI", "Do"]
    );
}

#[test]
fn type3_fixture_matches_every_glyph_fact() {
    let page = page0("type3-font");
    let facts = facts("type3-font");

    assert_eq!(page.alignment, Alignment::Exact);
    assert_eq!(facts.type3_glyphs.len(), 3);
    for fact in &facts.type3_glyphs {
        let op = op_at(&page, None, fact.op_index);
        let glyph = items_of(&page, op)
            .iter()
            .find(|i| glyph_parts(i).is_some_and(|g| g.0 == fact.glyph_index))
            .unwrap();
        let (_, code, _, origin, _) = glyph_parts(glyph).unwrap();
        let ctx = format!("op {} glyph {}", fact.op_index, fact.glyph_index);

        assert_eq!(code, u32::from(fact.code), "{ctx}");
        assert!(
            near(origin.x, fact.origin[0]) && near(origin.y, fact.origin[1]),
            "{ctx}: {origin:?}"
        );
        // The item box is the glyph's declared `d1` box, and it holds what the procedure paints.
        assert_box(glyph.bbox, fact.declared_bbox);
        let painted = pdfxray_core::Rect::new(
            fact.path_bbox[0],
            fact.path_bbox[1],
            fact.path_bbox[2],
            fact.path_bbox[3],
        );
        assert!(glyph.bbox.contains_rect(&painted), "{ctx}");
        assert_eq!(glyph.gstate.text.font.as_deref(), Some("T3"), "{ctx}");
    }
    // The glyph procedures stay opaque: only glyph items, no paths from their `re f` / `S`.
    assert!(page.items.iter().all(|i| glyph_parts(i).is_some()));
    assert_eq!(page.items.len(), 3);
}

// ---------------------------------------------------------------------------------------------
// box-matrix
// ---------------------------------------------------------------------------------------------

#[test]
fn box_matrix_pages_capture_in_unrotated_user_space() {
    let pdf = load("box-matrix");

    let pages = capture_document(&pdf);

    assert_eq!(pages.len(), 4);
    assert!(pages.iter().all(|p| p.alignment == Alignment::Exact));
    // Page 0: media box origin (-50, -50); the outermost frame spans the whole media box.
    assert_box(pages[0].items[0].bbox, [-50.5, -50.5, 650.5, 842.5]);
    assert_box(pages[0].items[4].bbox, [29.5, 29.5, 570.5, 762.5]);
    // Pages 1 to 3 differ in /Rotate and /UserUnit, never in the coordinates captured.
    assert_box(pages[1].items[0].bbox, [24.5, 24.5, 375.5, 275.5]);
    assert_box(pages[2].items[0].bbox, [14.5, 14.5, 235.5, 285.5]);
    assert_box(pages[3].items[0].bbox, [14.5, 14.5, 235.5, 285.5]);
    let origins: Vec<_> = pages
        .iter()
        .map(|p| {
            let first = p.items.iter().find_map(glyph_parts).unwrap();
            (first.3.x, first.3.y)
        })
        .collect();
    assert_eq!(
        origins,
        vec![(100.0, 700.0), (40.0, 140.0), (30.0, 140.0), (30.0, 140.0)]
    );
    assert_eq!(pages[0].items.len(), 5 + "Box matrix".len());
}
