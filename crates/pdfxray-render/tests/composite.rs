//! The visual diff over the fixture pairs: the pixel grid, the three modes, registration, the
//! geometry refusal, and the header scope the export reads from core.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use std::collections::HashMap;

use pdfxray_core::{Document, HeaderScope, NodeId, ScopeSource};
use pdfxray_render::annotate::{self, CONFLICT_RING, Grid, Palette, TagShow, TagView};
use pdfxray_render::composite::{self, Comparator, CompositeError, Fit, Mode, Registration, Request};
use pdfxray_render::{Overlay, PixelGrid, RenderError, RenderSpec, render_page};
use pretty_assertions::assert_eq;

fn open(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn pair(name: &str) -> (Document, Document) {
    let (a, b) = pdfxray_fixtures::pairs::pair(name);
    (Document::open(a).unwrap(), Document::open(b).unwrap())
}

fn request(mode: Mode) -> Request {
    Request {
        mode,
        scale: 1.0,
        fit: Fit::Refuse,
        registration: None,
        comparator: Comparator::same_engine(),
    }
}

#[test]
fn a_page_drawn_into_a_larger_grid_is_moved_by_its_offset_and_keeps_its_pixels() {
    let doc = open("minimal-one-page");
    let plain = render_page(&doc, 0, &RenderSpec::new(1.0)).unwrap();
    let grid = PixelGrid {
        width: plain.width + 5,
        height: plain.height + 3,
        offset: (5.0, 3.0),
    };

    let moved = render_page(
        &doc,
        0,
        &RenderSpec {
            grid: Some(grid),
            ..RenderSpec::new(1.0)
        },
    )
    .unwrap();

    assert_eq!((moved.width, moved.height), (plain.width + 5, plain.height + 3));
    let px = |page: &pdfxray_render::RenderedPage, x: u32, y: u32| {
        let at = ((y * page.width + x) * 4) as usize;
        page.rgba[at..at + 4].to_vec()
    };
    let differing = (0..plain.height)
        .flat_map(|y| (0..plain.width).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&plain, x, y) != px(&moved, x + 5, y + 3))
        .count();
    assert_eq!(differing, 0);
    assert_eq!(
        moved.page_to_pixel.to_pixel(pdfxray_core::Point::new(0.0, 0.0)).x,
        plain.page_to_pixel.to_pixel(pdfxray_core::Point::new(0.0, 0.0)).x + 5.0
    );
    // Smoke: the page is not blank, so the comparison compared ink.
    assert!(plain.rgba.chunks(4).any(|p| p != [255, 255, 255, 255]));
}

#[test]
fn renumbered_objects_overlay_onto_one_grid_without_a_differing_pixel() {
    let (a, b) = pair("renumber-objects");

    let overlay = composite::composite((&a, 0), (&b, 0), &request(Mode::Overlay)).unwrap();
    let side = composite::composite((&a, 0), (&b, 0), &request(Mode::SideBySide)).unwrap();

    let visual = *overlay.delta.unwrap().visual().unwrap();
    assert_eq!((visual.differing_pixels, visual.anti_aliased), (0, 0));
    assert_eq!((overlay.a.bounds, overlay.registration), (overlay.b.bounds, None));
    assert!(side.width > 2 * overlay.width, "two pages and a gutter");
    // Side by side, pages of one size are on one grid anyway, so they are scored too.
    assert_eq!(side.delta.map(|d| d.differing_pixels()), Some(0));
    // Smoke: shared ink is darker than either tint alone, so there is ink at all.
    assert!(overlay.rgba.chunks(4).any(|p| p[0] < 100 && p[2] < 200));
}

#[test]
fn registration_is_opt_in_and_removes_the_offset_the_pairing_reports() {
    let (a, b) = pair("global-shift-2pt");
    let offset = pdfxray_core::diff::pair(&a, &b).offset.unwrap();
    let registered = Request {
        registration: Some(Registration {
            dx: offset.dx,
            dy: offset.dy,
        }),
        ..request(Mode::Mask)
    };

    let raw = composite::composite((&a, 0), (&b, 0), &request(Mode::Mask)).unwrap();
    let fixed = composite::composite((&a, 0), (&b, 0), &registered).unwrap();

    assert!(raw.delta.unwrap().differing_pixels() > 1000, "{:?}", raw.delta);
    assert_eq!(fixed.delta.unwrap().differing_pixels(), 0);
    assert_eq!(fixed.registration, registered.registration);
    assert_eq!((offset.dx, offset.dy), (2.0, -2.0));
    // Smoke: unregistered, nothing says it was.
    assert_eq!(raw.registration, None);
}

#[test]
fn a_size_mismatch_is_refused_with_the_reason_unless_scaling_b_is_asked_for() {
    let (a, b) = (open("minimal-one-page"), open("tagged-simple"));

    let refused = composite::composite((&a, 0), (&b, 0), &request(Mode::Overlay)).unwrap_err();
    let scaled = composite::composite(
        (&a, 0),
        (&b, 0),
        &Request {
            fit: Fit::ScaleBToA,
            ..request(Mode::Overlay)
        },
    )
    .unwrap();
    let side = composite::composite((&a, 0), (&b, 0), &request(Mode::SideBySide));

    let CompositeError::Geometry(why) = refused else {
        panic!("{refused:?}");
    };
    assert_eq!(
        why,
        "the pages differ in size: A is 200.0 x 100.0 pt, B is 200.0 x 200.0 pt"
    );
    assert_eq!(scaled.b_scale, Some(0.5));
    assert_eq!((scaled.width, scaled.height), (200, 100));
    // Smoke: side by side needs no common grid, so it is not refused.
    assert!(side.is_ok());
}

#[test]
fn an_encrypted_side_gets_no_visual_diff_and_says_why() {
    let (a, b) = (open("minimal-one-page"), open("encrypted-stub"));

    let error = composite::composite((&a, 0), (&b, 0), &request(Mode::SideBySide)).unwrap_err();

    assert_eq!(
        error,
        CompositeError::Render {
            side: 'B',
            error: RenderError::Engine(pdfxray_core::EngineError::Encrypted),
        }
    );
    // Smoke: the message names the side and the reason.
    assert!(error.to_string().contains("side B cannot be drawn"), "{error}");
}

#[test]
fn the_legend_gets_a_band_of_its_own_below_the_pages() {
    let (a, b) = pair("move-paragraph-3pt");
    let mut picture = composite::composite((&a, 1), (&b, 1), &request(Mode::Overlay)).unwrap();
    let height = picture.height;
    let legend = annotate::change_legend(Mode::Overlay, annotate::ChangeShow::default(), None, &[], [0; 4]);

    let shapes = picture.annotate(1.0, &Palette::DESIGN, &[], &legend);

    assert!(picture.height > height);
    assert_eq!(picture.rgba.len(), (picture.width * picture.height * 4) as usize);
    let block = shapes
        .iter()
        .find_map(|s| match s.shape {
            Overlay::Frame { rect, .. } => Some(rect),
            _ => None,
        })
        .unwrap();
    assert!(block.y0 >= f64::from(height), "{block:?} overlaps the pages");
    // Smoke: every legend row is drawn.
    let texts = shapes
        .iter()
        .filter(|s| matches!(s.shape, Overlay::Text { .. }))
        .count();
    assert_eq!(texts, legend.rows.len());
}

#[test]
fn header_scopes_come_from_core_so_an_export_draws_the_conflict_ring() {
    let doc = open("table-scope-conflict");
    let tags = doc.tag_boxes(0);
    let scopes: HashMap<NodeId, HeaderScope> = doc.header_scopes(0).into_iter().collect();
    let scope_of = |id: &NodeId| scopes.get(id);
    let view = TagView {
        page: 0,
        tags: &tags,
        artifacts: &[],
        order: None,
        show: TagShow {
            labels: true,
            ..TagShow::default()
        },
        scope_of: &scope_of,
        in_focus: &|_, _| true,
    };

    let shapes = annotate::layout(
        &Grid::page(&doc, 0, 1.0, 1.0).unwrap(),
        &Palette::DESIGN,
        &[annotate::tag_scene(&view)],
        None,
    );

    assert!(
        scopes.values().any(|s| s.source == ScopeSource::Conflict),
        "{scopes:?}"
    );
    assert!(scopes.keys().all(|id| tags.iter().any(|t| &t.id == id)));
    let rings = shapes
        .iter()
        .filter(|s| {
            matches!(&s.shape, Overlay::Frame { stroke: Some(line), fill: None, .. }
                if line.width == CONFLICT_RING && line.ink.rgba == Palette::DESIGN.warn)
        })
        .count();
    let conflicts = scopes
        .values()
        .filter(|s| s.source == ScopeSource::Conflict)
        .count();
    assert_eq!(rings, conflicts);
    // Smoke: a page without tables has no header scopes.
    assert_eq!(open("minimal-one-page").header_scopes(0), Vec::new());
}
