//! `Document::visual`: which objects can be pictured, and the facts a preview of each needs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pdfxray_core::{
    ColorSpaceVisual, Document, FontProgram, FontVisual, FormVisual, ImageVisual, ObjRef, PatternVisual,
    Point, Rect, SPECIMEN, ShadingStop, ShadingVisual, Visual, VisualKind,
};
use pdfxray_fixtures::gallery::{
    GALLERY_BROKEN_FONT, GALLERY_CONTENT, GALLERY_EMBEDDED_FONT, GALLERY_FORM, GALLERY_ICC, GALLERY_IMAGE,
    GALLERY_INDEXED, GALLERY_MASK, GALLERY_MISSING_FONT, GALLERY_PAGE, GALLERY_PATTERN, GALLERY_SEPARATION,
    GALLERY_SHADING, object_gallery,
};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
use pretty_assertions::assert_eq;

fn fixture(name: &str) -> Document {
    Document::open(pdfxray_fixtures::pdf_path(name)).unwrap()
}

fn gallery() -> Document {
    Document::from_bytes("gallery", object_gallery().unwrap()).unwrap()
}

fn kind(doc: &Document, n: u32) -> VisualKind {
    doc.visual(ObjRef::new(n, 0))
        .unwrap_or_else(|| panic!("{n} 0 obj has no visual"))
        .kind
}

fn font(doc: &Document, n: u32) -> FontVisual {
    match kind(doc, n) {
        VisualKind::Font(font) => *font,
        other => panic!("{n} 0 obj is not a font: {other:?}"),
    }
}

fn hex(codes: &[pdfxray_core::CharCode]) -> Vec<String> {
    codes.iter().map(|c| c.hex.clone()).collect()
}

#[test]
fn an_image_tells_its_pixels_depth_and_colour_space() {
    let doc = fixture("images");

    let visual = doc.visual(ObjRef::new(5, 0));

    assert_eq!(
        visual,
        Some(Visual {
            obj: ObjRef::new(5, 0),
            kind: VisualKind::Image(ImageVisual {
                width: 2,
                height: 2,
                bits_per_component: Some(8),
                color_space: Some("DeviceRGB".to_owned()),
                image_mask: false,
                soft_mask: false,
            }),
        })
    );
}

#[test]
fn an_image_mask_is_one_bit_without_a_colour_space() {
    let doc = gallery();

    assert_eq!(
        kind(&doc, GALLERY_MASK),
        VisualKind::Image(ImageVisual {
            width: 8,
            height: 1,
            bits_per_component: Some(1),
            color_space: None,
            image_mask: true,
            soft_mask: false,
        })
    );
}

#[test]
fn a_form_covers_its_bbox_after_its_matrix() {
    let images = fixture("images");
    let gallery = gallery();

    // `/BBox [0 0 100 100] /Matrix [1 0 0 1 200 20]`, own resources.
    assert_eq!(
        kind(&images, 6),
        VisualKind::Form(FormVisual {
            area: Rect::new(200.0, 20.0, 300.0, 120.0),
            borrows_from: None,
        })
    );
    // `/BBox [0 0 50 50] /Matrix [2 0 0 2 10 10]`, no resources: it borrows those of page 1.
    assert_eq!(
        kind(&gallery, GALLERY_FORM),
        VisualKind::Form(FormVisual {
            area: Rect::new(10.0, 10.0, 110.0, 110.0),
            borrows_from: Some(0),
        })
    );
}

#[test]
fn an_embedded_font_gets_the_specimen_and_the_codes_the_page_shows() {
    let doc = gallery();

    let font = font(&doc, GALLERY_EMBEDDED_FONT);

    assert_eq!(font.program, FontProgram::Embedded);
    let specimen: Vec<String> = SPECIMEN.bytes().map(|b| format!("{b:02X}")).collect();
    assert_eq!(hex(&font.sample), specimen);
    // `(Hi) Tj`.
    assert_eq!(hex(&font.glyphs), ["48", "69"]);
    assert_eq!(font.used, 2);
    // Smoke: the facts are the inspector's.
    assert_eq!(font.facts.base_font.as_deref(), Some("Inter-Regular"));
}

#[test]
fn a_font_without_a_program_says_so_and_a_subset_gets_no_specimen() {
    let doc = gallery();

    let missing = font(&doc, GALLERY_MISSING_FONT);
    let subset = font(&doc, GALLERY_BROKEN_FONT);

    assert_eq!(missing.program, FontProgram::Missing);
    assert_eq!(missing.sample, []);
    assert_eq!(hex(&missing.glyphs), ["48", "6F"]);
    // The program is there (whether it reads is the renderer's business), but a subset may lack
    // the specimen's characters: only the code the page shows is listed.
    assert_eq!(subset.program, FontProgram::Embedded);
    assert_eq!(subset.sample, []);
    assert_eq!(hex(&subset.glyphs), ["78"]);
}

#[test]
fn a_type3_font_draws_its_procedures_for_the_codes_shown() {
    let doc = fixture("type3-font");

    let font = font(&doc, 5);

    assert_eq!(font.program, FontProgram::Procedures);
    assert_eq!(font.sample, []);
    assert_eq!(hex(&font.glyphs), ["61", "62"]);
    assert_eq!(font.used, 2);
}

#[test]
fn colour_spaces_get_swatches_in_their_own_components() {
    let doc = gallery();

    let indexed = kind(&doc, GALLERY_INDEXED);
    let separation = kind(&doc, GALLERY_SEPARATION);

    assert_eq!(
        indexed,
        VisualKind::ColorSpace(ColorSpaceVisual {
            family: "Indexed".to_owned(),
            summary: "Indexed (Base=DeviceRGB, HiVal=1)".to_owned(),
            components: 1,
            icc_profile: false,
            swatches: vec![vec![0.0], vec![1.0]],
            columns: 2,
        })
    );
    let VisualKind::ColorSpace(separation) = separation else {
        panic!("not a colour space: {separation:?}");
    };
    assert_eq!(separation.family, "Separation");
    // A tint ramp from paper to full ink.
    assert_eq!(separation.swatches.len(), 9);
    assert_eq!(separation.swatches.first(), Some(&vec![0.0]));
    assert_eq!(separation.swatches.last(), Some(&vec![1.0]));
}

#[test]
fn an_icc_profile_stream_previews_as_an_iccbased_space() {
    let doc = gallery();

    let VisualKind::ColorSpace(icc) = kind(&doc, GALLERY_ICC) else {
        panic!("not a colour space");
    };

    assert_eq!(
        (
            icc.family.as_str(),
            icc.summary.as_str(),
            icc.components,
            icc.icc_profile
        ),
        ("ICCBased", "ICCBased (N=3, Alternate=DeviceRGB)", 3, true)
    );
    // Primaries, secondaries, white and black.
    assert_eq!(icc.swatches.len(), 8);
    assert_eq!(icc.swatches[0], [1.0, 0.0, 0.0]);
}

#[test]
fn a_shading_and_a_pattern_tell_the_area_worth_showing() {
    let doc = gallery();

    assert_eq!(
        kind(&doc, GALLERY_SHADING),
        VisualKind::Shading(ShadingVisual {
            shading_type: Some(2),
            summary: "axial (type 2), DeviceRGB".to_owned(),
            color_space: Some("DeviceRGB".to_owned()),
            // The axis (0,0)-(100,0), half its length deep.
            area: Some(Rect::new(0.0, -25.0, 100.0, 25.0)),
            axis: Some((Point::new(0.0, 0.0), Point::new(100.0, 0.0))),
            extend: [true, true],
            function_type: Some(2),
            // Red at the start, blue at the end.
            stops: vec![
                ShadingStop {
                    t: 0.0,
                    position: 0.0,
                    color: vec![1.0, 0.0, 0.0],
                },
                ShadingStop {
                    t: 1.0,
                    position: 1.0,
                    color: vec![0.0, 0.0, 1.0],
                },
            ],
        })
    );
    assert_eq!(
        kind(&doc, GALLERY_PATTERN),
        VisualKind::Pattern(PatternVisual {
            pattern_type: Some(1),
            summary: "Tiling, coloured".to_owned(),
            uncoloured: false,
            // Three 20-point steps across and down from the tile's corner.
            area: Some(Rect::new(0.0, 0.0, 60.0, 60.0)),
        })
    );
}

#[test]
fn a_page_is_its_page_and_other_objects_have_no_picture() {
    let doc = gallery();

    assert_eq!(kind(&doc, GALLERY_PAGE), VisualKind::Page(0));
    // The catalog, the page tree and the content stream: nothing to picture.
    for n in [1, 2, GALLERY_CONTENT] {
        assert_eq!(doc.visual(ObjRef::new(n, 0)), None, "{n} 0 obj");
    }
    // Smoke: an image of the same file does have one.
    assert!(doc.visual(ObjRef::new(GALLERY_IMAGE, 0)).is_some());
}

/// A one-page file whose object 4 is `shading`.
fn with_shading(shading: &str) -> Document {
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Shading << /Sh1 4 0 R >> >> >>",
    );
    w.object(4, shading);
    w.finish_table(&Trailer::new(1)).unwrap();
    Document::from_bytes("shading", w.into_output().unwrap().bytes).unwrap()
}

fn shading(doc: &Document) -> ShadingVisual {
    match kind(doc, 4) {
        VisualKind::Shading(shading) => shading,
        other => panic!("not a shading: {other:?}"),
    }
}

#[test]
fn a_stitching_function_has_a_stop_at_each_bound() {
    // Over t in [2 4]: white to grey until 3, then grey (reversed by /Encode) to black.
    let doc = with_shading(
        "<< /ShadingType 3 /ColorSpace /DeviceGray /Coords [0 0 0 0 0 50] /Domain [2 4] \
         /Function << /FunctionType 3 /Domain [2 4] /Bounds [3] /Encode [0 1 1 0] /Functions [\
         << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0.5] /N 1 >> \
         << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [0.5] /N 1 >>] >> >>",
    );

    let radial = shading(&doc);

    let stops: Vec<(f64, f64, Vec<f64>)> = radial
        .stops
        .iter()
        .map(|s| (s.t, s.position, s.color.clone()))
        .collect();
    assert_eq!(
        stops,
        [
            (2.0, 0.0, vec![1.0]),
            (3.0, 0.5, vec![0.5]),
            (4.0, 1.0, vec![0.0])
        ]
    );
    assert_eq!(radial.function_type, Some(3));
    // Smoke: a radial shading has no axis to run a bar along, and extends nowhere by default.
    assert_eq!((radial.axis, radial.extend), (None, [false, false]));
}

#[test]
fn an_array_of_functions_gives_one_component_each_and_a_sampled_one_none() {
    let split = with_shading(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 10 0] /Extend [false true] /Function [\
         << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 2 >> \
         << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [1] /N 1 >> \
         << /FunctionType 2 /Domain [0 1] /C0 [0.25] /C1 [0.75] /N 1 >>] >>",
    );
    let sampled = with_shading(
        "<< /ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 10 0] /Function << /FunctionType 4 \
         /Domain [0 1] /Range [0 1] >> >>",
    );

    let split = shading(&split);
    let sampled = shading(&sampled);

    let colors: Vec<Vec<f64>> = split.stops.iter().map(|s| s.color.clone()).collect();
    assert_eq!(colors, [vec![0.0, 1.0, 0.25], vec![1.0, 1.0, 0.75]]);
    assert_eq!(split.extend, [false, true]);
    assert_eq!(split.function_type, Some(2));
    // The ends of a PostScript function are known, its colours are not.
    let ends: Vec<(f64, usize)> = sampled.stops.iter().map(|s| (s.t, s.color.len())).collect();
    assert_eq!(ends, [(0.0, 0), (1.0, 0)]);
    // Smoke: the axis runs from the first point to the second.
    assert_eq!(split.axis, Some((Point::new(0.0, 0.0), Point::new(10.0, 0.0))));
}
