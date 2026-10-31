//! What `hayro-syntax` and `hayro` make of each fixture.

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::page::Rotation;
use hayro::hayro_syntax::{LoadPdfError, Pdf, PdfVersion};
use hayro::kurbo::Shape;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use pdfxray_fixtures::{Built, Defect, Fixture, Sourced, all, find};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Built {
    find(name).unwrap().build().unwrap()
}

fn version_of(header: &str) -> PdfVersion {
    match header {
        "1.4" => PdfVersion::Pdf14,
        "1.5" => PdfVersion::Pdf15,
        "1.7" => PdfVersion::Pdf17,
        "2.0" => PdfVersion::Pdf20,
        other => panic!("add {other} to version_of"),
    }
}

fn rect(r: hayro::hayro_syntax::object::Rect) -> [f64; 4] {
    [r.x0, r.y0, r.x1, r.y1]
}

/// Expected render size: the crop box (or media box), swapped for quarter turns.
fn expected_size(page: &pdfxray_fixtures::PageFacts) -> (f32, f32) {
    let b = page.crop_box.as_ref().unwrap_or(&page.media_box).value;
    let (w, h) = ((b[2] - b[0]) as f32, (b[3] - b[1]) as f32);
    match page.rotate {
        Some(Sourced { value, .. }) if value % 180 != 0 => (h, w),
        _ => (w, h),
    }
}

/// Parses, checks the page tree against `Expected`, renders page 1 and checks the pixels.
fn assert_parses_and_renders(fixture: &Fixture) {
    let built = fixture.build().unwrap();
    let name = fixture.name;
    let pdf = Pdf::new(built.pdf.clone()).unwrap_or_else(|e| panic!("{name}: {e:?}"));

    assert_eq!(
        pdf.version(),
        version_of(&built.expected.header_version),
        "{name} version"
    );
    assert_eq!(
        pdf.pages().len(),
        built.expected.page_count as usize,
        "{name} page count"
    );
    for (page, facts) in pdf.pages().iter().zip(&built.expected.pages) {
        assert_eq!(
            rect(page.media_box()),
            facts.media_box.value,
            "{name} page {}",
            facts.index
        );
    }

    let page = &pdf.pages()[0];
    let pixmap = render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            bg_color: WHITE,
            ..PixmapSettings::default()
        },
    );
    let (w, h) = expected_size(&built.expected.pages[0]);
    assert_eq!(
        (f32::from(pixmap.width()), f32::from(pixmap.height())),
        (w, h),
        "{name} size"
    );

    let pixels: Vec<&[u8]> = pixmap.data_as_u8_slice().chunks(4).collect();
    let painted = pixels.iter().filter(|p| **p != [255, 255, 255, 255]).count();
    let background = pixels.len() - painted;
    // Smoke: something was drawn, and the page is not completely covered.
    assert!(painted > 0, "{name}: rendered page is blank");
    assert!(background > 0, "{name}: rendered page has no background left");
}

#[test]
fn non_defective_fixtures_parse_and_render() {
    let names: Vec<&str> = all()
        .iter()
        .filter(|f| !f.build().unwrap().expected.is_defective())
        .map(|f| f.name)
        .collect();

    assert_eq!(
        names,
        [
            "minimal-one-page",
            "tagged-simple",
            "artifacts",
            "incremental-update",
            "xref-stream-objstm",
            "form-xobject-nesting",
            "text-state",
            "graphics-state",
            "images",
            "type3-font",
            "struct-rich",
            "pdf2-namespaces",
            "annotations",
            "box-matrix",
            "resources-inheritance",
            "hybrid-xref",
            "long-doc",
            "reading-order",
            "reading-order-columns",
            "reading-order-footnote",
            "speech-alt-hides",
            "speech-artifacts",
            "speech-footnote",
            "table-simple-pdf17",
            "table-scope-pdf2",
            "table-headers-ids",
            "speech-lang",
            "speech-link-objr",
            "rolemap-two-hops",
        ]
    );
    for name in names {
        assert_parses_and_renders(find(name).unwrap());
    }
}

#[test]
fn defective_fixtures_hayro_tolerates_still_parse_and_render() {
    let names: Vec<&str> = all()
        .iter()
        .filter(|f| {
            let expected = f.build().unwrap().expected;
            expected.is_defective()
                && !expected
                    .defects
                    .iter()
                    .any(|d| matches!(d, Defect::Encrypted { .. }))
        })
        .map(|f| f.name)
        .collect();

    assert_eq!(
        names,
        [
            "page-boxes",
            "rolemap-custom",
            "broken-xref-offsets",
            "orphan-mcid",
            "untagged-content",
            "startxref-broken"
        ]
    );
    for name in names {
        assert_parses_and_renders(find(name).unwrap());
    }
}

#[test]
fn encrypted_stub_is_rejected_as_password_protected() {
    let built = fixture("encrypted-stub");

    let result = Pdf::new(built.pdf);

    assert!(matches!(
        result,
        Err(LoadPdfError::Decryption(
            hayro::hayro_syntax::DecryptionError::PasswordProtected
        ))
    ));
}

#[test]
fn hayro_repairs_wrong_xref_offsets_silently() {
    let built = fixture("broken-xref-offsets");

    let pdf = Pdf::new(built.pdf).unwrap();

    // Despite two lying xref entries, the page and its content are found.
    let page = &pdf.pages()[0];
    assert_eq!(
        page.page_stream().map(String::from_utf8_lossy).as_deref(),
        Some("BT /F1 18 Tf 20 40 Td (Broken xref) Tj ET")
    );
    assert_eq!(pdf.len(), 5);
}

#[test]
fn hayro_sees_the_latest_revision_of_an_updated_stream() {
    let built = fixture("incremental-update");

    let pdf = Pdf::new(built.pdf).unwrap();

    assert_eq!(
        pdf.pages()[0]
            .page_stream()
            .map(String::from_utf8_lossy)
            .as_deref(),
        Some("BT /F1 18 Tf 20 40 Td (Hello pdfxray rev 2) Tj ET")
    );
    assert_eq!(pdf.len(), 6);
}

#[test]
fn hayro_reads_objects_out_of_an_object_stream() {
    let built = fixture("xref-stream-objstm");

    let pdf = Pdf::new(built.pdf).unwrap();

    assert_eq!(pdf.version(), PdfVersion::Pdf15);
    assert_eq!(rect(pdf.pages()[0].media_box()), [0.0, 0.0, 240.0, 100.0]);
    assert_eq!(
        pdf.pages()[0]
            .page_stream()
            .map(String::from_utf8_lossy)
            .as_deref(),
        Some("BT /F1 18 Tf 20 40 Td (Hello object streams) Tj ET")
    );
    // Objects 1-7: five ordinary or compressed objects, the object stream and the xref stream.
    assert_eq!(pdf.len(), 7);
}

#[test]
fn hayro_applies_inheritance_rotation_and_crop_box_but_ignores_user_unit() {
    let built = fixture("page-boxes");

    let pdf = Pdf::new(built.pdf).unwrap();

    let summary: Vec<([f64; 4], [f64; 4], bool, (f32, f32))> = pdf
        .pages()
        .iter()
        .map(|p| {
            (
                rect(p.media_box()),
                rect(p.crop_box()),
                matches!(p.rotation(), Rotation::Horizontal),
                p.render_dimensions(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                [0.0, 0.0, 400.0, 300.0],
                [10.0, 10.0, 390.0, 290.0],
                true,
                (280.0, 380.0)
            ),
            (
                [0.0, 0.0, 300.0, 400.0],
                [0.0, 0.0, 300.0, 400.0],
                false,
                (300.0, 400.0)
            ),
            // UserUnit 2.0 does not scale the render size.
            (
                [0.0, 0.0, 200.0, 200.0],
                [0.0, 0.0, 200.0, 200.0],
                false,
                (200.0, 200.0)
            ),
        ]
    );
}

mod probe {
    use hayro::hayro_interpret::font::GlyphRun;
    use hayro::hayro_interpret::{
        BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
        InterpreterSettings, SoftMask, interpret_page,
    };
    use hayro::hayro_syntax::Pdf;
    use hayro::kurbo::{Affine, BezPath, Rect, Shape};

    /// What hayro's interpreter reports to a device, in page space (the initial transform is the identity).
    #[derive(Default, Debug)]
    pub(super) struct Probe {
        /// First glyph origin of every glyph run and whether the run was invisible.
        pub(super) runs: Vec<([f64; 2], bool)>,
        /// Page-space boxes of drawn paths (fills, strokes and glyph-less shapes).
        pub(super) paths: Vec<[f64; 4]>,
        /// Transforms of drawn images.
        pub(super) images: Vec<Affine>,
        /// Alpha/blend of every path, in order.
        pub(super) blends: Vec<BlendMode>,
        pub(super) clips: usize,
        pub(super) marked: Vec<(String, Option<i32>)>,
    }

    fn bounds(rect: Rect) -> [f64; 4] {
        [rect.x0, rect.y0, rect.x1, rect.y1]
    }

    impl<'a> Device<'a> for Probe {
        fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, _: &DrawMode) {
            self.paths
                .push(bounds((props.transform * path.clone()).bounding_box()));
            self.blends.push(props.blend_mode);
        }
        fn push_clip_path(&mut self, _: &ClipPath) {
            self.clips += 1;
        }
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
        fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
            if let Some(first) = run.glyphs().first() {
                let origin = props.transform * first.transform() * hayro::kurbo::Point::new(0.0, 0.0);
                self.runs
                    .push(([origin.x, origin.y], matches!(mode, DrawMode::Invisible)));
            }
        }
        fn draw_image(&mut self, _: Image<'a, '_>, props: ImageDrawProps<'a>) {
            self.images.push(props.transform);
        }
        fn pop_clip(&mut self) {}
        fn pop_transparency_group(&mut self) {}
        fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
            self.marked
                .push((String::from_utf8_lossy(tag).into_owned(), mcid));
        }
    }

    pub(super) fn probe_page(pdf: &Pdf, index: usize) -> Probe {
        let page = &pdf.pages()[index];
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            Affine::IDENTITY,
            Rect::new(-10_000.0, -10_000.0, 10_000.0, 10_000.0),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut probe = Probe::default();
        interpret_page(page, &mut context, &mut probe);
        probe
    }
}

/// Dedupes consecutive equal entries: hayro reports a fill and a stroke separately for `B`,
/// `Tr 2` and friends where the content stream has one operator.
fn collapse<T: PartialEq + Clone>(items: &[T]) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for item in items {
        if out.last() != Some(item) {
            out.push(item.clone());
        }
    }
    out
}

fn hayro_pdf(name: &str) -> (Pdf, pdfxray_fixtures::Expected) {
    let built = fixture(name);
    (Pdf::new(built.pdf).unwrap(), built.expected)
}

#[test]
fn hayro_places_every_glyph_run_where_the_text_facts_say() {
    for name in ["text-state", "type3-font"] {
        let (pdf, expected) = hayro_pdf(name);

        let probe = probe::probe_page(&pdf, 0);

        let origins = collapse(&probe.runs.iter().map(|(o, _)| *o).collect::<Vec<_>>());
        // A run starts at its first glyph, also for the two-glyph Type 3 string.
        let wanted: Vec<[f64; 2]> = expected.text_ops.iter().map(|t| t.origin).collect();
        assert_eq!(origins, wanted, "{name}");
    }
    let (pdf, _) = hayro_pdf("text-state");
    // Only render mode 3 is reported as invisible; mode 2 arrives as a fill run and a stroke run.
    let runs = probe::probe_page(&pdf, 0).runs;
    let invisible: Vec<[f64; 2]> = runs
        .iter()
        .filter(|(_, hidden)| *hidden)
        .map(|(o, _)| *o)
        .collect();
    assert_eq!(invisible, [[290.0, 80.0]]);
    assert_eq!(runs.len(), 14);
}

#[test]
fn hayro_draws_paths_with_the_boxes_and_blend_modes_of_the_paint_facts() {
    let (pdf, expected) = hayro_pdf("graphics-state");

    let probe = probe::probe_page(&pdf, 0);

    // `B` is drawn as a fill and a stroke, so collapse; the shading arrives as one path.
    let drawn = collapse(
        &probe
            .paths
            .iter()
            .zip(&probe.blends)
            .map(|(bbox, blend)| (*bbox, format!("{blend:?}")))
            .collect::<Vec<_>>(),
    );
    let wanted: Vec<[f64; 4]> = expected.paint_ops.iter().map(|p| p.bbox).collect();
    assert_eq!(drawn.iter().map(|(b, _)| *b).collect::<Vec<_>>(), wanted);
    let multiply: Vec<usize> = drawn
        .iter()
        .enumerate()
        .filter(|(_, (_, blend))| blend == "Multiply")
        .map(|(i, _)| i)
        .collect();
    // The 7th and 8th painting operators (49 and 53) carry the Multiply ExtGState.
    assert_eq!(multiply, [6, 7]);
}

#[test]
fn hayro_image_transforms_map_the_pixel_grid_onto_the_image_boxes() {
    let (pdf, expected) = hayro_pdf("images");

    let probe = probe::probe_page(&pdf, 0);

    let boxes: Vec<[f64; 4]> = probe
        .images
        .iter()
        .zip(&expected.images)
        .map(|(transform, image)| {
            let grid = hayro::kurbo::Rect::new(0.0, 0.0, f64::from(image.width), f64::from(image.height));
            let r = (*transform * grid.to_path(0.1)).bounding_box();
            [r.x0, r.y0, r.x1, r.y1]
        })
        .collect();
    assert_eq!(boxes, expected.images.iter().map(|i| i.bbox).collect::<Vec<_>>());
    assert_eq!(probe.images.len(), 3);
}

#[test]
fn hayro_reports_marked_content_in_stream_order() {
    let (pdf, _) = hayro_pdf("untagged-content");

    let probe = probe::probe_page(&pdf, 0);

    let marked: Vec<(&str, Option<i32>)> = probe.marked.iter().map(|(t, m)| (t.as_str(), *m)).collect();
    assert_eq!(
        marked,
        [
            ("Artifact", None),
            ("P", Some(0)),
            ("P", Some(1)),
            ("P", Some(1)),
            ("Artifact", None),
            ("Span", Some(2)),
        ]
    );
}

#[test]
fn hayro_page_geometry_matches_the_box_matrix_facts_and_ignores_user_unit() {
    let (pdf, expected) = hayro_pdf("box-matrix");

    let seen: Vec<([f64; 4], (f32, f32))> = pdf
        .pages()
        .iter()
        .map(|p| (rect(p.intersected_crop_box()), p.render_dimensions()))
        .collect();

    let wanted: Vec<([f64; 4], (f32, f32))> = expected
        .page_geometry
        .iter()
        .map(|g| (g.visible_box, (g.render_size[0] as f32, g.render_size[1] as f32)))
        .collect();
    assert_eq!(seen, wanted);
    // Smoke: page 3 has UserUnit 3, which hayro does not apply, and a non-zero media box origin
    // is kept as is.
    assert_eq!(seen[3].1, (230.0, 280.0));
    assert_eq!(rect(pdf.pages()[0].media_box()), [-50.0, -50.0, 650.0, 842.0]);
}

#[test]
fn hayro_follows_xrefstm_to_the_hidden_font_and_survives_a_wrong_startxref() {
    let (hybrid, _) = hayro_pdf("hybrid-xref");
    let font = hybrid.pages()[0]
        .resources()
        .get_font(&hayro::hayro_syntax::object::Name::new(b"F1").unwrap());
    // The font is object 6, free in the classic table and only reachable through /XRefStm.
    assert!(font.is_some());
    assert_eq!(hybrid.version(), PdfVersion::Pdf15);

    let (broken, _) = hayro_pdf("startxref-broken");
    assert_eq!(
        broken.pages()[0]
            .page_stream()
            .map(String::from_utf8_lossy)
            .as_deref(),
        Some("BT /F1 18 Tf 20 40 Td (Wrong startxref) Tj ET")
    );
    assert_eq!(broken.len(), 5);
}
