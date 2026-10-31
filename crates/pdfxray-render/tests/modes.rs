//! Debug render modes over the generated fixtures: isolate, hide, paint-order stepper, the
//! rewritten document, x-ray / clip / transparency overlays.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::collections::BTreeSet;
use std::sync::Arc;

use pdfxray_core::capture::{CapturedPage, OpRecord, PaintKind};
use pdfxray_core::{Document, NodeId, Rect};
use pdfxray_raw::{Object, RawDocument, Severity, XrefKind};
use pdfxray_render::xray::{
    ARTIFACT, CLIP, HEAT_BLEND, INVISIBLE_TEXT, KIND_GLYPH, KIND_IMAGE, KIND_PATH, KIND_SHADING, TAGGED,
    UNTAGGED_TAG,
};
use pdfxray_render::{
    ColorBy, ContentFilter, Overlay, ROLE_COLORS, RenderError, RenderSpec, RenderedPage, XrayOptions,
    alpha_heatmap_overlays, clip_overlays, filtered_pdf, ops_for, render_page, xray_overlays,
};
use pretty_assertions::assert_eq;

fn open(name: &str) -> Document {
    let path = pdfxray_fixtures::pdf_path(name);
    Document::open(path).unwrap()
}

fn captured(doc: &Document) -> &CapturedPage {
    doc.captured(0).unwrap()
}

fn render(doc: &Document, content: ContentFilter) -> RenderedPage {
    render_page(
        doc,
        0,
        &RenderSpec {
            content,
            ..RenderSpec::new(1.0)
        },
    )
    .unwrap()
}

fn is_white(px: &[u8]) -> bool {
    px[..3] == [255, 255, 255]
}

fn non_white(page: &RenderedPage) -> usize {
    page.rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| !is_white(*px))
        .count()
}

/// The page-space box of all non-white pixels, `None` for a blank page.
fn ink(page: &RenderedPage) -> Option<Rect> {
    let w = page.width as usize;
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for (i, px) in page.rgba.as_chunks::<4>().0.iter().enumerate() {
        if is_white(px) {
            continue;
        }
        let (x, y) = (i % w, i / w);
        bounds = Some(bounds.map_or((x, y, x + 1, y + 1), |(x0, y0, x1, y1)| {
            (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1))
        }));
    }
    let (x0, y0, x1, y1) = bounds?;
    let a = page.pixel_to_page(pdfxray_render::PixelPoint::new(x0 as f64, y0 as f64));
    let b = page.pixel_to_page(pdfxray_render::PixelPoint::new(x1 as f64, y1 as f64));
    Some(Rect::new(a.x, a.y, b.x, b.y))
}

/// The pixels of `page` inside the page-space rectangle `r` (rows top to bottom).
fn crop(page: &RenderedPage, r: Rect) -> Vec<u8> {
    let a = page.page_to_pixel.rect_to_pixel(r);
    let (x0, x1) = (
        a.x0.floor().max(0.0) as usize,
        (a.x1.ceil() as usize).min(page.width as usize),
    );
    let (y0, y1) = (
        a.y0.floor().max(0.0) as usize,
        (a.y1.ceil() as usize).min(page.height as usize),
    );
    let mut out = Vec::new();
    for y in y0..y1 {
        let row = (y * page.width as usize + x0) * 4;
        out.extend_from_slice(&page.rgba[row..row + (x1 - x0) * 4]);
    }
    out
}

fn set(items: &[u32]) -> BTreeSet<u32> {
    items.iter().copied().collect()
}

fn all_ops(doc: &Document) -> BTreeSet<u32> {
    captured(doc).ops.iter().map(|o| o.ix).collect()
}

fn nth_op<'a>(doc: &'a Document, operator: &str, n: usize) -> &'a OpRecord {
    captured(doc)
        .ops
        .iter()
        .filter(|o| o.operator == operator)
        .nth(n)
        .unwrap()
}

/// Union of the item boxes of `ops`.
fn items_box(doc: &Document, ops: &[u32]) -> Rect {
    let boxes: Vec<Rect> = captured(doc)
        .items
        .iter()
        .filter(|i| ops.contains(&i.op))
        .map(|i| i.bbox)
        .collect();
    boxes.iter().fold(boxes[0], |a, b| {
        Rect::new(a.x0.min(b.x0), a.y0.min(b.y0), a.x1.max(b.x1), a.y1.max(b.y1))
    })
}

// ---------------------------------------------------------------------------------------------
// Isolate / hide / stepper
// ---------------------------------------------------------------------------------------------

#[test]
fn isolating_the_only_text_op_keeps_the_page_and_hiding_it_blanks_it() {
    let doc = open("minimal-one-page");
    let tj = nth_op(&doc, "Tj", 0).ix;
    let full = render(&doc, ContentFilter::All);

    let only = render(&doc, ContentFilter::Only(set(&[tj])));
    let hidden = render(&doc, ContentFilter::Hide(set(&[tj])));
    let nothing = render(&doc, ContentFilter::Only(BTreeSet::new()));

    assert_eq!(tj, 3);
    assert!(non_white(&full) > 0);
    assert_eq!(only.rgba, full.rgba);
    assert_eq!(non_white(&hidden), 0);
    assert_eq!(non_white(&nothing), 0);
    // Smoke: the filtered render has the size of the original page.
    assert_eq!((hidden.width, hidden.height), (full.width, full.height));
    assert_eq!(hidden.rgba, vec![255; hidden.rgba.len()]);
}

#[test]
fn hiding_state_operators_changes_nothing_because_only_painting_is_suppressed() {
    let doc = open("minimal-one-page");
    let full = render(&doc, ContentFilter::All);

    // BT, Tf, Td, ET set up the text but paint nothing.
    let out = render(&doc, ContentFilter::Hide(set(&[0, 1, 2, 4])));

    assert_eq!(out.rgba, full.rgba);
    assert!(non_white(&out) > 0);
}

#[test]
fn stepping_through_graphics_state_only_ever_adds_ink() {
    let doc = open("graphics-state");
    let last = captured(&doc).ops.last().unwrap().ix;
    let full = render(&doc, ContentFilter::All);

    let counts: Vec<usize> = (0..=last)
        .map(|n| non_white(&render(&doc, ContentFilter::UpTo(n))))
        .collect();

    assert!(counts.windows(2).all(|w| w[0] <= w[1]), "{counts:?}");
    // Nothing paints before the first fill (op 3); the fill adds ink.
    assert_eq!(counts[..3], [0, 0, 0]);
    assert!(counts[3] > 0, "{counts:?}");
    assert_eq!(counts[last as usize], non_white(&full));
    assert_eq!(render(&doc, ContentFilter::UpTo(last)).rgba, full.rgba);
    // Smoke: stepping really changes the picture along the way.
    assert!(counts[3] < counts[last as usize]);
    assert_eq!(
        render(&doc, ContentFilter::UpTo(0)).rgba,
        vec![255; full.rgba.len()]
    );
}

#[test]
fn isolating_the_inner_form_paints_only_inside_its_bounding_box() {
    let doc = open("form-xobject-nesting");
    let cap = captured(&doc);
    let inner = &cap.xobjects[1];
    let inner_box = inner.page_bbox.unwrap();
    let inner_text = nth_op(&doc, "Tj", 2).ix;
    let full = render(&doc, ContentFilter::All);

    let only_text = render(&doc, ContentFilter::Only(set(&[inner_text])));
    let only_form = render(
        &doc,
        ContentFilter::Only(ops_for(
            &doc,
            &NodeId::Op {
                page: 0,
                ix: inner.op,
            },
        )),
    );

    assert_eq!((inner.name.as_str(), inner_text), ("FmB", 24));
    for out in [&only_text, &only_form] {
        let bounds = ink(out).unwrap();
        assert!(
            inner_box.contains_rect(&bounds),
            "ink {bounds:?} leaks out of the inner form {inner_box:?}"
        );
        assert!(non_white(out) < non_white(&full));
    }
    // The text alone is a subset of the whole inner form (rectangle and text).
    assert!(non_white(&only_text) < non_white(&only_form));
    // Smoke: the full page paints the outer text far outside the inner box.
    assert!(!inner_box.contains_rect(&ink(&full).unwrap()));
    // Hiding the inner form's operators leaves the outer text and the outer form's text.
    let hidden = render(
        &doc,
        ContentFilter::Hide(ops_for(
            &doc,
            &NodeId::Op {
                page: 0,
                ix: inner.op,
            },
        )),
    );
    assert!(non_white(&hidden) > non_white(&only_text));
    assert!(non_white(&hidden) < non_white(&full));
}

#[test]
fn ops_for_expands_forms_marked_content_and_glyphs() {
    let doc = open("form-xobject-nesting");

    assert_eq!(ops_for(&doc, &NodeId::Op { page: 0, ix: 4 }), set(&[4]));
    assert_eq!(
        ops_for(
            &doc,
            &NodeId::Glyph {
                page: 0,
                op: 4,
                glyph: 2
            }
        ),
        set(&[4])
    );
    // The Do of the inner form and everything it executes.
    assert_eq!(
        ops_for(&doc, &NodeId::Op { page: 0, ix: 15 }),
        (15..=26).collect::<BTreeSet<u32>>()
    );
    // MCID 0 is used by three sequences: the page's, form A's and form B's.
    assert_eq!(
        ops_for(&doc, &NodeId::MarkedContent { page: 0, mcid: 0 }),
        (0..=6).chain(8..=14).chain(16..=26).collect::<BTreeSet<u32>>()
    );
    // Not on the page, or not content at all.
    assert_eq!(ops_for(&doc, &NodeId::Op { page: 0, ix: 999 }), BTreeSet::new());
    assert_eq!(
        ops_for(&doc, &NodeId::MarkedContent { page: 0, mcid: 42 }),
        BTreeSet::new()
    );
    assert_eq!(ops_for(&doc, &NodeId::Page(0)), BTreeSet::new());
}

#[test]
fn ops_for_numbers_artifacts_in_paint_order() {
    let doc = open("artifacts");
    let cap = captured(&doc);
    let artifacts: Vec<_> = cap.marked.iter().filter(|m| m.artifact.is_some()).collect();
    assert!(artifacts.len() >= 2, "the fixture has several artifacts");

    for (seq, m) in artifacts.iter().enumerate() {
        let ops = ops_for(
            &doc,
            &NodeId::Artifact {
                page: 0,
                seq: seq as u32,
            },
        );

        assert_eq!(ops.first(), Some(&m.begin_op));
        assert_eq!(ops.last(), m.end_op.as_ref());
    }
    assert_eq!(
        ops_for(
            &doc,
            &NodeId::Artifact {
                page: 0,
                seq: artifacts.len() as u32
            }
        ),
        BTreeSet::new()
    );
}

#[test]
fn hiding_a_text_run_keeps_the_text_that_follows_in_place() {
    let doc = open("text-state");
    let cap = captured(&doc);
    // Hide each text-showing operator in turn: the next one must not move.
    let showing: Vec<u32> = cap
        .ops
        .iter()
        .filter(|o| matches!(o.operator.as_str(), "Tj" | "TJ" | "'" | "\""))
        .map(|o| o.ix)
        .collect();
    let operators: Vec<&str> = showing
        .iter()
        .map(|ix| cap.ops[*ix as usize].operator.as_str())
        .collect();
    assert!(operators.contains(&"TJ"), "{operators:?}");
    assert!(
        operators.contains(&"'") && operators.contains(&"\""),
        "{operators:?}"
    );
    let full = render(&doc, ContentFilter::All);

    for pair in showing.windows(2) {
        let (hidden_op, next_op) = (pair[0], pair[1]);
        let out = render(&doc, ContentFilter::Hide(set(&[hidden_op])));

        let margin = 1.0;
        let next_box = items_box(&doc, &[next_op]);
        let around = Rect::new(
            next_box.x0 - margin,
            next_box.y0 - margin,
            next_box.x1 + margin,
            next_box.y1 + margin,
        );
        let hidden_box = items_box(&doc, &[hidden_op]);
        assert_eq!(
            crop(&out, around),
            crop(&full, around),
            "hiding {} ({}) moved {} ({})",
            hidden_op,
            cap.ops[hidden_op as usize].operator,
            next_op,
            cap.ops[next_op as usize].operator
        );
        // The hidden run is gone from its own area, unless it was invisible text to begin with.
        let was_visible = cap.ops[hidden_op as usize].gstate.text.render_mode != 3;
        assert_eq!(
            crop(&out, hidden_box) != crop(&full, hidden_box),
            was_visible,
            "hiding op {hidden_op} {hidden_box:?}"
        );
    }
    // Smoke: the fixture shows seven runs with the four operators and one invisible run.
    assert_eq!(showing.len(), 13);
}

#[test]
fn hiding_everything_blanks_every_renderable_fixture() {
    let mut blank = Vec::new();
    for fixture in pdfxray_fixtures::all() {
        if matches!(fixture.name, "encrypted-stub") {
            continue;
        }
        let doc = open(fixture.name);
        for page in 0..doc.page_count() {
            let page = u32::try_from(page).unwrap();
            let Some(cap) = doc.captured(page) else {
                continue;
            };
            let ops: BTreeSet<u32> = cap.ops.iter().map(|o| o.ix).collect();
            let spec = RenderSpec {
                content: ContentFilter::Hide(ops),
                ..RenderSpec::new(1.0)
            };

            let out = render_page(&doc, page, &spec).unwrap();

            // Annotation appearances are drawn by the engine but are not page content, so no
            // operator filter reaches them; every other page must come out empty.
            if non_white(&out) == 0 {
                blank.push(format!("{}#{page}", fixture.name));
            } else {
                assert!(
                    fixture.name == "annotations",
                    "{} page {page} still has {} inked pixels",
                    fixture.name,
                    non_white(&out)
                );
            }
        }
    }
    assert!(blank.len() >= 20, "{blank:?}");
    assert!(blank.contains(&"minimal-one-page#0".to_owned()));
    assert!(blank.contains(&"images#0".to_owned()));
    // Type 3 glyph procedures are silenced by the rendering mode too.
    assert!(blank.contains(&"type3-font#0".to_owned()));
}

#[test]
fn filters_report_unknown_pages_and_encrypted_files() {
    let doc = open("minimal-one-page");
    let spec = RenderSpec {
        content: ContentFilter::UpTo(1),
        ..RenderSpec::new(1.0)
    };
    let encrypted = open("encrypted-stub");

    assert_eq!(
        render_page(&doc, 7, &spec).map(|_| ()),
        Err(RenderError::NoSuchPage { page: 7, count: 1 })
    );
    assert!(matches!(
        render_page(&encrypted, 0, &spec),
        Err(RenderError::Engine(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// The rewritten document
// ---------------------------------------------------------------------------------------------

fn error_codes(raw: &RawDocument) -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = raw
        .diagnostics()
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .map(pdfxray_raw::RawDiagnostic::code)
        .collect();
    codes.sort_unstable();
    codes
}

fn parse(bytes: &[u8]) -> RawDocument {
    RawDocument::parse(Arc::new(bytes.to_vec())).unwrap()
}

#[test]
fn the_rewritten_document_is_a_valid_incremental_update() {
    let fixtures = [
        "minimal-one-page",
        "graphics-state",
        "form-xobject-nesting",
        "text-state",
        "images",
        "incremental-update",
        "xref-stream-objstm",
        "hybrid-xref",
        "resources-inheritance",
    ];
    for name in fixtures {
        let doc = open(name);
        let before = parse(doc.bytes());

        let out = filtered_pdf(&doc, 0, &ContentFilter::Hide(all_ops(&doc))).unwrap();
        let after = parse(&out);

        assert!(
            out.starts_with(doc.bytes()),
            "{name}: the original is not a prefix"
        );
        assert!(out.len() > doc.bytes().len(), "{name}");
        assert_eq!(after.revisions().len(), before.revisions().len() + 1, "{name}");
        assert_eq!(error_codes(&after), error_codes(&before), "{name}: new errors");
        // The new section is of the same kind as the one before it and chains to it.
        let kind = |raw: &RawDocument, i: usize| matches!(raw.revisions()[i].xref.kind, XrefKind::Stream(_));
        assert_eq!(
            kind(&after, after.revisions().len() - 1),
            kind(&before, before.revisions().len() - 1),
            "{name}"
        );
        // The rewritten file loads in the engine and keeps its pages.
        let reopened = Document::from_bytes(name, out).unwrap();
        assert_eq!(reopened.page_count(), doc.page_count(), "{name}");
        assert_eq!(reopened.engine_error(), None, "{name}");
    }
}

#[test]
fn the_original_bytes_and_document_are_never_modified() {
    let doc = open("graphics-state");
    let bytes = doc.bytes().to_vec();

    let _ = render(&doc, ContentFilter::Only(set(&[3])));

    assert_eq!(doc.bytes(), bytes.as_slice());
    assert_eq!(parse(doc.bytes()).revisions().len(), 1);
}

/// The decoded `/Contents` of page 0 of a rewritten file.
fn page_content(bytes: &[u8]) -> String {
    let raw = parse(bytes);
    let doc = Document::from_bytes("x", bytes.to_vec()).unwrap();
    let page = doc.page(0).unwrap().obj.unwrap();
    let page = raw.object(page).unwrap();
    let contents = page.value.value.as_dict().unwrap().get(b"Contents").unwrap();
    let Object::Ref(r) = contents.value else {
        panic!("Contents is {:?}", contents.value);
    };
    let stream = raw.object(r).unwrap();
    let data = raw
        .decode_stream(stream.value.value.as_stream().unwrap())
        .unwrap();
    String::from_utf8(data.bytes).unwrap()
}

#[test]
fn hidden_text_is_wrapped_in_a_rendering_mode_switch_and_nothing_else_changes() {
    let doc = open("minimal-one-page");

    let hidden = page_content(&filtered_pdf(&doc, 0, &ContentFilter::Hide(set(&[3]))).unwrap());
    let kept = page_content(&filtered_pdf(&doc, 0, &ContentFilter::Only(set(&[3]))).unwrap());

    assert_eq!(hidden, "BT /F1 18 Tf 20 40 Td 3 Tr (Hello pdfxray) Tj 0 Tr ET");
    assert_eq!(kept, "BT /F1 18 Tf 20 40 Td (Hello pdfxray) Tj ET");
}

#[test]
fn painting_operators_are_replaced_and_state_operators_are_kept() {
    let doc = open("graphics-state");
    let original = captured(&doc);

    let out = filtered_pdf(&doc, 0, &ContentFilter::Hide(all_ops(&doc))).unwrap();
    let rewritten = Document::from_bytes("x", out).unwrap();
    let after = captured(&rewritten);

    let painting = ["f", "F", "f*", "B", "B*", "b", "b*", "S", "s"];
    let expected: Vec<String> = original
        .ops
        .iter()
        .filter(|o| o.operator != "sh")
        .map(|o| {
            if painting.contains(&o.operator.as_str()) {
                "n".to_owned()
            } else {
                o.operator.clone()
            }
        })
        .collect();
    let got: Vec<String> = after.ops.iter().map(|o| o.operator.clone()).collect();
    assert_eq!(got, expected);
    // The shading is gone, the clips are still there.
    assert_eq!(original.ops.iter().filter(|o| o.operator == "sh").count(), 1);
    assert_eq!(after.ops.iter().filter(|o| o.operator == "sh").count(), 0);
    assert_eq!(
        after
            .items
            .iter()
            .filter(|i| matches!(i.kind, PaintKind::Clip))
            .count(),
        original
            .items
            .iter()
            .filter(|i| matches!(i.kind, PaintKind::Clip))
            .count()
    );
    // Nothing paints any more.
    assert!(after.items.iter().all(|i| matches!(i.kind, PaintKind::Clip)));
}

#[test]
fn hidden_clipping_text_keeps_clipping_and_the_form_copies_are_new_objects() {
    let doc = open("form-xobject-nesting");
    let inner = &captured(&doc).xobjects[1];
    let hide_inner = ops_for(
        &doc,
        &NodeId::Op {
            page: 0,
            ix: inner.op,
        },
    );

    let out = filtered_pdf(&doc, 0, &ContentFilter::Hide(hide_inner)).unwrap();
    let after = parse(&out);
    let before = parse(doc.bytes());

    // Page content, page object, copy of form A, copy of form B: four new definitions.
    let new_objects = after.object_refs().len() - before.object_refs().len();
    assert_eq!(new_objects, 3);
    let content = page_content(&out);
    assert!(content.contains("/PdfxrayForm0 Do"), "{content}");
    assert!(!content.contains("/FmA Do"), "{content}");
    // The untouched original forms are still there for other pages.
    assert!(before.object(pdfxray_core::ObjRef::new(6, 0)).is_some());
    assert_eq!(
        after.object(pdfxray_core::ObjRef::new(6, 0)),
        before.object(pdfxray_core::ObjRef::new(6, 0))
    );
}

// ---------------------------------------------------------------------------------------------
// Overlays
// ---------------------------------------------------------------------------------------------

fn rects(overlays: &[Overlay]) -> Vec<(Rect, [u8; 4], Option<[u8; 4]>)> {
    overlays
        .iter()
        .map(|o| match o {
            Overlay::Rect {
                rect, stroke, fill, ..
            } => (*rect, *stroke, *fill),
        })
        .collect()
}

#[test]
fn xray_outlines_every_glyph_of_the_single_text_run() {
    let doc = open("minimal-one-page");
    let items = &captured(&doc).items;

    let overlays = xray_overlays(&doc, 0, XrayOptions::default());

    assert_eq!(overlays.len(), 13);
    let got = rects(&overlays);
    assert_eq!(
        got.iter().map(|(r, _, _)| *r).collect::<Vec<_>>(),
        items.iter().map(|i| i.bbox).collect::<Vec<_>>()
    );
    assert!(
        got.iter()
            .all(|(_, stroke, fill)| *stroke == KIND_GLYPH && fill.is_none())
    );
    // Smoke: nothing is invisible or outside the page, so including those adds nothing.
    let with_hidden = xray_overlays(
        &doc,
        0,
        XrayOptions {
            include_invisible: true,
            ..XrayOptions::default()
        },
    );
    assert_eq!(with_hidden, overlays);
}

/// Stroke colours of `overlays`, with how often each occurs.
fn palette_counts(overlays: &[Overlay]) -> Vec<([u8; 4], usize)> {
    let mut counts: std::collections::BTreeMap<[u8; 4], usize> = std::collections::BTreeMap::new();
    for (_, stroke, _) in rects(overlays) {
        *counts.entry(stroke).or_default() += 1;
    }
    counts.into_iter().collect()
}

#[test]
fn xray_colours_images_and_outlines_each_one() {
    let doc = open("images");

    let overlays = xray_overlays(&doc, 0, XrayOptions::default());

    // The page image, the inline image and the image inside the form.
    let boxes: Vec<Rect> = rects(&overlays).iter().map(|r| r.0).collect();
    assert_eq!(overlays.len(), 3);
    assert_eq!(palette_counts(&overlays), vec![(KIND_IMAGE, 3)]);
    for (got, expected) in boxes.iter().zip([
        Rect::new(50.0, 150.0, 150.0, 250.0),
        Rect::new(80.0, 20.0, 120.0, 60.0),
        Rect::new(210.0, 20.0, 270.0, 80.0),
    ]) {
        assert!(
            (got.x0 - expected.x0).abs() < 1e-6
                && (got.y0 - expected.y0).abs() < 1e-6
                && (got.x1 - expected.x1).abs() < 1e-6
                && (got.y1 - expected.y1).abs() < 1e-6,
            "{got:?} != {expected:?}"
        );
    }
}

#[test]
fn xray_distinguishes_paths_and_shadings_and_skips_clips() {
    let doc = open("graphics-state");
    let cap = captured(&doc);

    let overlays = xray_overlays(&doc, 0, XrayOptions::default());

    // 16 items, two of them clips.
    assert_eq!(cap.items.len(), 16);
    assert_eq!(overlays.len(), 14);
    assert_eq!(
        palette_counts(&overlays),
        vec![(KIND_PATH, 13), (KIND_SHADING, 1)]
    );
    let got = rects(&overlays);
    assert_eq!(got[0].0, Rect::new(20.0, 20.0, 100.0, 80.0));
    // Items 0..=7 come first; the shading (item 9) follows the first clip, so it is overlay 8.
    assert_eq!(got[8].1, KIND_SHADING);
    assert_eq!(got[8].0, Rect::new(200.0, 220.0, 350.0, 320.0));
}

#[test]
fn xray_marks_invisible_text_only_when_asked() {
    let doc = open("text-state");
    let cap = captured(&doc);
    let gone = cap.ops.iter().find(|o| o.ix == 52).unwrap();
    assert_eq!(gone.operator, "Tj");
    let first = gone.first_item.unwrap() as usize;
    let count = gone.item_count as usize;

    let plain = xray_overlays(&doc, 0, XrayOptions::default());
    let everything = xray_overlays(
        &doc,
        0,
        XrayOptions {
            include_invisible: true,
            ..XrayOptions::default()
        },
    );

    assert_eq!((cap.items.len(), count), (97, 4));
    assert_eq!(plain.len(), 93);
    assert_eq!(everything.len(), 97);
    let red: Vec<usize> = rects(&everything)
        .iter()
        .enumerate()
        .filter(|(_, r)| r.1 == INVISIBLE_TEXT)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(red, (first..first + count).collect::<Vec<_>>());
    // The highlight carries a translucent fill, the ordinary outlines do not.
    assert!(rects(&everything)[first].2.is_some());
    assert!(rects(&plain).iter().all(|r| r.2.is_none()));
    // Smoke: everything else is a glyph outline.
    assert_eq!(
        palette_counts(&everything),
        vec![(KIND_GLYPH, 93), (INVISIBLE_TEXT, 4)]
    );
}

#[test]
fn xray_colours_by_artifact_and_by_tag() {
    let artifacts = open("artifacts");
    let untagged = open("untagged-content");
    let tagged = open("tagged-simple");
    let by = |color_by| XrayOptions {
        color_by,
        ..XrayOptions::default()
    };

    // 24 items sit in artifact sequences, 9 in the tagged paragraph.
    assert_eq!(
        palette_counts(&xray_overlays(&artifacts, 0, by(ColorBy::Artifact))),
        vec![(TAGGED, 9), (ARTIFACT, 24)]
    );
    // The span nested in an artifact counts as artifact content; 14 items are outside any sequence.
    let outcome = palette_counts(&xray_overlays(&untagged, 0, by(ColorBy::Artifact)));
    assert_eq!(
        outcome,
        vec![(TAGGED, 32), ([40, 90, 220, 255], 14), (ARTIFACT, 12)]
    );

    // By tag: one colour per innermost tag, grey outside any sequence.
    let heading_and_paragraph = palette_counts(&xray_overlays(&tagged, 0, by(ColorBy::Tag)));
    assert_eq!(
        heading_and_paragraph,
        vec![([44, 160, 44, 255], 7), ([188, 189, 34, 255], 14)]
    );
    let mixed = palette_counts(&xray_overlays(&untagged, 0, by(ColorBy::Tag)));
    assert_eq!(
        mixed,
        vec![
            (UNTAGGED_TAG, 14),
            ([148, 103, 189, 255], 6),
            ([188, 189, 34, 255], 32),
            ([255, 127, 14, 255], 6),
        ]
    );
    // The same tag gets the same colour in different documents ("P" above).
    assert_eq!(
        palette_counts(&xray_overlays(&artifacts, 0, by(ColorBy::Tag))),
        vec![([148, 103, 189, 255], 24), ([188, 189, 34, 255], 9)]
    );
}

#[test]
fn xray_colours_by_the_role_class_that_owns_the_content() {
    let doc = open("reading-order");
    let by_role = XrayOptions {
        color_by: ColorBy::TagRole,
        ..XrayOptions::default()
    };
    let slot = |class| pdfxray_render::role_slot(class);
    let heading = ROLE_COLORS[slot(pdfxray_core::RoleClass::Heading)];
    let paragraph = ROLE_COLORS[slot(pdfxray_core::RoleClass::Paragraph)];
    let figure = ROLE_COLORS[slot(pdfxray_core::RoleClass::Figure)];
    let caption = ROLE_COLORS[slot(pdfxray_core::RoleClass::Caption)];

    let overlays = xray_overlays(&doc, 0, by_role);

    // Heading, two paragraphs, the figure and its caption: each in its class colour.
    let colours: BTreeSet<[u8; 4]> = palette_counts(&overlays).into_iter().map(|(c, _)| c).collect();
    assert_eq!(colours, BTreeSet::from([heading, paragraph, figure, caption]));
    // Smoke: untagged content is grey, not a role colour.
    let untagged = palette_counts(&xray_overlays(&open("untagged-content"), 0, by_role));
    assert!(untagged.iter().any(|(c, _)| *c == UNTAGGED_TAG));
}

#[test]
fn clip_overlay_shows_the_clip_in_effect_for_an_operator() {
    let doc = open("graphics-state");

    let before_any_clip = clip_overlays(&doc, 0, 3);
    let inside_first = clip_overlays(&doc, 0, 59);
    let nested = clip_overlays(&doc, 0, 75);
    let after_restore = clip_overlays(&doc, 0, 79);

    assert_eq!(before_any_clip, Vec::new());
    assert_eq!(
        rects(&inside_first),
        vec![(
            Rect::new(200.0, 220.0, 350.0, 320.0),
            CLIP,
            Some([CLIP[0], CLIP[1], CLIP[2], 32])
        )]
    );
    // The nested clip is the intersection of both.
    assert_eq!(rects(&nested)[0].0, Rect::new(240.0, 240.0, 350.0, 300.0));
    // Q Q restored the unclipped state.
    assert_eq!(after_restore, Vec::new());
    assert_eq!(clip_overlays(&doc, 0, 9999), Vec::new());
    assert_eq!(clip_overlays(&doc, 7, 59), Vec::new());
}

#[test]
fn alpha_heat_map_tints_the_blended_items_only() {
    let doc = open("graphics-state");
    let cap = captured(&doc);

    let overlays = alpha_heatmap_overlays(&doc, 0);

    // Items 6 and 7 are drawn with ca/CA 0.5 and the Multiply blend mode.
    assert_eq!(
        rects(&overlays),
        vec![
            (
                cap.items[6].bbox,
                [HEAT_BLEND[0], HEAT_BLEND[1], HEAT_BLEND[2], 255],
                Some([HEAT_BLEND[0], HEAT_BLEND[1], HEAT_BLEND[2], 90])
            ),
            (
                cap.items[7].bbox,
                [HEAT_BLEND[0], HEAT_BLEND[1], HEAT_BLEND[2], 255],
                Some([HEAT_BLEND[0], HEAT_BLEND[1], HEAT_BLEND[2], 90])
            ),
        ]
    );
    // Smoke: a plain opaque page has nothing to tint.
    assert_eq!(alpha_heatmap_overlays(&open("minimal-one-page"), 0), Vec::new());
}

#[test]
fn overlays_compose_with_a_filtered_render() {
    let doc = open("graphics-state");
    let plain = render(&doc, ContentFilter::UpTo(12));

    let marked = render_page(
        &doc,
        0,
        &RenderSpec {
            overlays: xray_overlays(&doc, 0, XrayOptions::default()),
            content: ContentFilter::UpTo(12),
            ..RenderSpec::new(1.0)
        },
    )
    .unwrap();

    assert_eq!((marked.width, marked.height), (plain.width, plain.height));
    assert!(non_white(&marked) > non_white(&plain));
    // The outline of the first rectangle (green) is drawn over its red fill.
    assert_ne!(marked.rgba, plain.rgba);
}

#[test]
fn ops_for_struct_elem_unions_its_marked_content_on_its_first_page() {
    let doc = pdfxray_core::Document::open(pdfxray_fixtures::pdf_path("tagged-simple")).unwrap();
    let document = doc.tree(pdfxray_core::TreeKind::Tags)[0].children[0].id.clone();
    let h1 = pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 0 };
    let p = pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 1 };

    let expected: std::collections::BTreeSet<u32> = pdfxray_render::ops_for(&doc, &h1)
        .union(&pdfxray_render::ops_for(&doc, &p))
        .copied()
        .collect();
    assert_eq!(pdfxray_render::ops_for(&doc, &document), expected);
    assert!(!expected.is_empty());
}
