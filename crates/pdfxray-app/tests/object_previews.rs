//! Object previews in the Objects entry: the cards of design board "2.9 Objects · Preview cards".
//! The picture of an image, a font, a form, a colour space, a shading or a page in the
//! inspector's Preview section and under the dictionary, rendered off the UI thread; and the
//! plain words shown when there is nothing of the file's to draw.
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test object_previews`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

mod common;

use std::path::PathBuf;

use common::{
    Region, app, builder, click_rail, named, nodes_in, open_fixture, open_path, texts_in, the,
    wait_for_previews,
};
use egui::{Rect, Role};
use egui_kittest::Harness;
use pdfxray_app::components::object_preview::{
    FALLBACK_LABEL, GLYPHS_LABEL, INSPECTOR_CANVAS_H, NO_PREVIEW, NOT_EMBEDDED, NOT_EMBEDDED_WHY,
    PREVIEW_TITLE, RENDERING, caption, image_label,
};
use pdfxray_app::components::prop_row::KEY_W;
use pdfxray_app::inspector::object::PREVIEW_SECTION;
use pdfxray_app::state::PreviewScale;
use pdfxray_app::theme::tokens;
use pdfxray_app::{PdfxrayApp, preview_digest};
use pdfxray_core::{NodeId, ObjRef, SPECIMEN};
use pdfxray_fixtures::gallery::{
    GALLERY_BROKEN_FONT, GALLERY_EMBEDDED_FONT, GALLERY_INDEXED, GALLERY_MISSING_FONT, GALLERY_SHADING,
    object_gallery,
};
use pdfxray_render::render_preview;
use pretty_assertions::assert_eq;

fn obj(n: u32) -> ObjRef {
    ObjRef::new(n, 0)
}

/// The gallery of previewable objects, written once to a file the app can open.
fn gallery_path() -> PathBuf {
    let dir = std::env::temp_dir().join("pdfxray-object-previews");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("object-gallery.pdf");
    let bytes = object_gallery().unwrap();
    if std::fs::read(&path).ok().as_deref() != Some(&bytes[..]) {
        std::fs::write(&path, bytes).unwrap();
    }
    path
}

/// Clicks object `n` in the navigator; one scrolled out of the list is selected as a link to it
/// would.
fn select(harness: &mut Harness<'_, PdfxrayApp>, n: u32) {
    let label = format!("{n} 0 obj");
    if named(harness, Region::Navigator, Role::Button, &label).is_empty() {
        harness.state_mut().state.select(NodeId::Object(obj(n)));
    } else {
        the(harness, Region::Navigator, Role::Button, &label).click();
    }
    harness.run_steps(3);
}

/// Opens `fixture` (a generated one, or a path), shows the Objects entry and selects object `n`.
fn objects_at(harness: &mut Harness<'_, PdfxrayApp>, file: Source, n: u32) {
    match file {
        Source::Fixture(name) => open_fixture(harness, name),
        Source::Gallery => open_path(harness, gallery_path()),
    }
    click_rail(harness, "Objects");
    select(harness, n);
    wait_for_previews(harness);
}

#[derive(Clone, Copy)]
enum Source {
    Fixture(&'static str),
    Gallery,
}

/// The regions that show the preview bitmap of `r`.
fn bitmaps_of(harness: &Harness<'_, PdfxrayApp>, r: ObjRef) -> Vec<Region> {
    [Region::Center, Region::Inspector]
        .into_iter()
        .filter(|region| !named(harness, *region, Role::Image, &image_label(r)).is_empty())
        .collect()
}

/// What `render_preview` makes of object `r` of `doc`, as a digest.
fn expected_digest(harness: &Harness<'_, PdfxrayApp>, r: ObjRef) -> u64 {
    let doc = &harness.state().state.document.as_ref().unwrap().doc;
    preview_digest(&render_preview(doc, &doc.visual(r).unwrap()).unwrap())
}

/// The rectangle of the one node with `role` named `name` in `region`.
fn rect_of(harness: &Harness<'_, PdfxrayApp>, region: Region, role: Role, name: &str) -> Rect {
    the(harness, region, role, name).rect()
}

/// The caption under the image in `region`, and the factor it states.
fn image_caption(harness: &Harness<'_, PdfxrayApp>, region: Region) -> (String, f32) {
    let caption = texts_in(harness, region, Role::Label)
        .into_iter()
        .find(|t| t.contains(" px · shown ×"))
        .unwrap_or_else(|| panic!("no image caption in {region:?}"));
    let factor = caption
        .split('×')
        .nth(2)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|f| f.parse().ok())
        .unwrap_or_else(|| panic!("no factor in `{caption}`"));
    (caption, factor)
}

#[test]
fn an_image_shows_its_pixels_under_the_dictionary_and_in_the_inspector() {
    let mut harness = app();

    objects_at(&mut harness, Source::Fixture("images"), 5);

    assert_eq!(bitmaps_of(&harness, obj(5)), [Region::Center, Region::Inspector]);
    assert_eq!(
        harness.state().state.preview_digest(obj(5)),
        Some(expected_digest(&harness, obj(5)))
    );
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    for fact in ["2 × 2", "8 bits per component", "DeviceRGB"] {
        assert!(inspector.contains(&fact.to_owned()), "{fact} in {inspector:?}");
    }
    let center = texts_in(&harness, Region::Center, Role::Label);
    for text in [PREVIEW_TITLE, "2 × 2 px · 8 bpc · DeviceRGB"] {
        assert!(center.contains(&text.to_owned()), "{text} in {center:?}");
    }
    let chips: Vec<String> = labels_after(&center, "2 × 2 px · 8 bpc · DeviceRGB");
    assert!(
        chips.ends_with(&["DeviceRGB".to_owned(), "8 bpc".to_owned(), "no SMask".to_owned()]),
        "{chips:?}"
    );
    // Smoke: the rendering line is gone once the bitmap is there.
    assert!(!center.contains(&RENDERING.to_owned()));
}

/// The labels after `text` in `labels`.
fn labels_after(labels: &[String], text: &str) -> Vec<String> {
    let at = labels
        .iter()
        .position(|t| t == text)
        .unwrap_or_else(|| panic!("no `{text}` in {labels:?}"));
    labels[at + 1..].to_vec()
}

#[test]
fn an_image_is_shown_by_a_whole_factor_that_its_caption_states() {
    let mut harness = app();

    objects_at(&mut harness, Source::Fixture("images"), 5);

    let image = rect_of(&harness, Region::Center, Role::Image, &image_label(obj(5)));
    let (text, factor) = image_caption(&harness, Region::Center);
    // Two pixels across, each `factor` screen pixels (one point per pixel in tests).
    assert_eq!(text, caption(2, 2, factor));
    assert_eq!(image.width(), 2.0 * factor);
    assert_eq!(image.height(), image.width());
    assert_eq!(
        factor.fract(),
        0.0,
        "a whole factor, so every pixel is as wide: ×{factor}"
    );
    // The inspector's canvas is smaller: a smaller whole factor, also in its one chip.
    let (_, small) = image_caption(&harness, Region::Inspector);
    assert!(
        small >= 1.0 && small < factor,
        "inspector ×{small}, centre ×{factor}"
    );
    assert_eq!(
        named(
            &harness,
            Region::Inspector,
            Role::Label,
            &format!("×{small} · nearest")
        )
        .len(),
        1
    );
    // Smoke: the caption sits under the image.
    let label = rect_of(&harness, Region::Center, Role::Label, &text);
    assert!(label.top() >= image.bottom(), "{label:?} under {image:?}");
}

#[test]
fn one_to_one_shows_a_preview_pixel_per_screen_pixel_and_fit_brings_the_fit_back() {
    let mut harness = app();
    objects_at(&mut harness, Source::Fixture("images"), 5);
    let (_, fitted) = image_caption(&harness, Region::Center);

    the(&harness, Region::Center, Role::Button, "1:1").click();
    harness.run_steps(3);

    let image = rect_of(&harness, Region::Center, Role::Image, &image_label(obj(5)));
    assert_eq!((image.width(), image.height()), (2.0, 2.0));
    assert_eq!(
        image_caption(&harness, Region::Center),
        ("2 × 2 px · shown ×1 · nearest".to_owned(), 1.0)
    );
    assert_eq!(harness.state().state.objects.preview_scale, PreviewScale::Actual);
    // The inspector's card is always fitted.
    let (_, inspector) = image_caption(&harness, Region::Inspector);
    assert!(inspector > 1.0, "×{inspector}");

    the(&harness, Region::Center, Role::Button, "Fit").click();
    harness.run_steps(3);

    assert_eq!(image_caption(&harness, Region::Center).1, fitted);
    // Smoke: the control is only on the centre card.
    assert_eq!(named(&harness, Region::Inspector, Role::Button, "1:1").len(), 0);
}

#[test]
fn the_inspector_card_is_a_canvas_one_chip_and_label_value_rows() {
    let mut harness = app();

    objects_at(&mut harness, Source::Fixture("images"), 5);

    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    // No header: no title, no facts line, no Fit · 1:1.
    assert!(!inspector.contains(&PREVIEW_TITLE.to_owned()), "{inspector:?}");
    assert!(!inspector.contains(&"2 × 2 px · 8 bpc · DeviceRGB".to_owned()));
    assert_eq!(named(&harness, Region::Inspector, Role::Button, "Fit").len(), 0);
    // The image, its one chip, then the rows: label in a 116-point column, value after it.
    let image = rect_of(&harness, Region::Inspector, Role::Image, &image_label(obj(5)));
    let (_, factor) = image_caption(&harness, Region::Inspector);
    let chip = rect_of(
        &harness,
        Region::Inspector,
        Role::Label,
        &format!("×{factor} · nearest"),
    );
    let rows: Vec<(Rect, Rect)> = [
        ("Pixels", "2 × 2"),
        ("Depth", "8 bits per component"),
        ("Colour space", "DeviceRGB"),
        ("Alpha", "none (no SMask)"),
    ]
    .iter()
    .map(|(key, value)| {
        (
            rect_of(&harness, Region::Inspector, Role::Label, key),
            rect_of(&harness, Region::Inspector, Role::Label, value),
        )
    })
    .collect();
    for (key, value) in &rows {
        assert_eq!(value.left() - key.left(), KEY_W, "{key:?} {value:?}");
        assert!((value.center().y - key.center().y).abs() < 2.0);
    }
    let tops: Vec<f32> = rows.iter().map(|(key, _)| key.top()).collect();
    let step = tops[1] - tops[0];
    assert!(step >= tokens::ROW_H, "{tops:?}");
    assert!(tops.windows(2).all(|w| w[1] - w[0] == step), "{tops:?}");
    assert!(chip.top() > image.bottom() && rows[0].0.top() > chip.bottom());
    // The canvas is 160 points: the chip row starts right under it.
    let canvas_top = chip.top() - INSPECTOR_CANVAS_H;
    assert!(
        image.top() > canvas_top && image.top() - canvas_top < 40.0,
        "{image:?} {chip:?}"
    );
}

#[test]
fn a_font_without_its_program_shows_the_amber_frame_and_a_labelled_fallback() {
    let mut harness = app();

    objects_at(&mut harness, Source::Fixture("text-state"), 5);

    assert_eq!(bitmaps_of(&harness, obj(5)), []);
    let count = |region, text: &str| named(&harness, region, Role::Label, text).len();
    // The centre card: title, why, the fallback specimen and its label.
    for text in [NOT_EMBEDDED, NOT_EMBEDDED_WHY, SPECIMEN, FALLBACK_LABEL] {
        assert_eq!(count(Region::Center, text), 1, "{text}");
    }
    assert_eq!(
        NOT_EMBEDDED_WHY,
        "Readers substitute their own font, so text can reflow or lose glyphs."
    );
    assert_eq!(FALLBACK_LABEL, "fallback specimen · not the file's font");
    // The inspector card has no room for why, but keeps the label of the fallback.
    for (text, n) in [
        (NOT_EMBEDDED, 1),
        (SPECIMEN, 1),
        (FALLBACK_LABEL, 1),
        (NOT_EMBEDDED_WHY, 0),
    ] {
        assert_eq!(count(Region::Inspector, text), n, "{text}");
    }
    // The chips and the rows.
    let center = texts_in(&harness, Region::Center, Role::Label);
    for chip in ["Type1 · Helvetica", "not embedded", "standard 14 font"] {
        assert!(center.contains(&chip.to_owned()), "{chip} in {center:?}");
    }
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    for row in ["not embedded", "Helvetica (standard 14)"] {
        assert!(inspector.contains(&row.to_owned()), "{row} in {inspector:?}");
    }
    // The fallback is under the title, its label under it.
    let title = rect_of(&harness, Region::Center, Role::Label, NOT_EMBEDDED);
    let fallback = rect_of(&harness, Region::Center, Role::Label, SPECIMEN);
    let label = rect_of(&harness, Region::Center, Role::Label, FALLBACK_LABEL);
    assert!(title.bottom() < fallback.top() && fallback.bottom() <= label.top());
    // Smoke: nothing was sent to the renderer.
    assert_eq!(harness.state().state.preview_digest(obj(5)), None);
}

#[test]
fn an_embedded_font_shows_a_specimen_drawn_with_its_program() {
    let mut harness = app();

    objects_at(&mut harness, Source::Gallery, GALLERY_EMBEDDED_FONT);

    let r = obj(GALLERY_EMBEDDED_FONT);
    assert_eq!(bitmaps_of(&harness, r), [Region::Center, Region::Inspector]);
    assert_eq!(
        harness.state().state.preview_digest(r),
        Some(expected_digest(&harness, r))
    );
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    for fact in ["embedded (TrueType)", "2 distinct codes", "WinAnsiEncoding"] {
        assert!(inspector.contains(&fact.to_owned()), "{fact} in {inspector:?}");
    }
}

#[test]
fn an_embedded_font_lists_its_glyphs_in_cells_with_their_hex_codes() {
    let mut harness = app();

    objects_at(&mut harness, Source::Gallery, GALLERY_EMBEDDED_FONT);

    for region in [Region::Center, Region::Inspector] {
        assert_eq!(
            named(&harness, region, Role::Label, GLYPHS_LABEL).len(),
            1,
            "{region:?}"
        );
        // `Hi`: a 44 x 56 cell each, side by side, its code under the glyph.
        let cells: Vec<Rect> = ["Glyph 48", "Glyph 69"]
            .iter()
            .map(|name| rect_of(&harness, region, Role::Image, name))
            .collect();
        for cell in &cells {
            assert_eq!((cell.width(), cell.height()), (44.0, 56.0), "{region:?}");
        }
        assert_eq!(cells[1].left() - cells[0].right(), 8.0);
        assert_eq!(cells[0].top(), cells[1].top());
        for (cell, code) in cells.iter().zip(["48", "69"]) {
            let label = rect_of(&harness, region, Role::Label, code);
            assert!(cell.contains_rect(label), "{code} inside {cell:?}: {label:?}");
            assert!((label.center().x - cell.center().x).abs() < 1.0);
        }
    }
    // Smoke: the cells are under the specimen.
    let specimen = rect_of(
        &harness,
        Region::Center,
        Role::Image,
        &image_label(obj(GALLERY_EMBEDDED_FONT)),
    );
    let caption = rect_of(&harness, Region::Center, Role::Label, GLYPHS_LABEL);
    assert!(specimen.bottom() < caption.top());
}

#[test]
fn a_font_program_that_cannot_be_read_is_reported_not_substituted() {
    let mut harness = app();

    objects_at(&mut harness, Source::Gallery, GALLERY_BROKEN_FONT);

    assert_eq!(bitmaps_of(&harness, obj(GALLERY_BROKEN_FONT)), []);
    let reason = "The embedded font program cannot be read; a viewer would substitute another font.";
    for region in [Region::Center, Region::Inspector] {
        assert_eq!(
            named(&harness, region, Role::Label, NO_PREVIEW).len(),
            1,
            "{region:?}"
        );
        assert_eq!(
            named(&harness, region, Role::Label, reason).len(),
            1,
            "{region:?}"
        );
    }
    // Smoke: it was asked for and answered.
    assert!(!harness.state().state.previews_pending());
}

#[test]
fn an_indexed_colour_space_shows_its_palette_row() {
    let mut harness = app();

    objects_at(&mut harness, Source::Gallery, GALLERY_INDEXED);

    let r = obj(GALLERY_INDEXED);
    assert_eq!(bitmaps_of(&harness, r), [Region::Center, Region::Inspector]);
    assert_eq!(
        named(
            &harness,
            Region::Center,
            Role::Label,
            "INDEXED PALETTE · 2 COLOURS"
        )
        .len(),
        1
    );
    // Red and blue, as the engine paints them: 40-point swatches in the centre, 28 in the
    // inspector, each with its index under it in the centre.
    for (region, side) in [(Region::Center, 40.0), (Region::Inspector, 28.0)] {
        let swatches: Vec<Rect> = ["Swatch 0 · #FF0000", "Swatch 1 · #0000FF"]
            .iter()
            .map(|name| rect_of(&harness, region, Role::Image, name))
            .collect();
        for swatch in &swatches {
            assert_eq!((swatch.width(), swatch.height()), (side, side), "{region:?}");
        }
        assert_eq!(swatches[1].left() - swatches[0].right(), 8.0);
        // The index right under each swatch, on the centre card only.
        let indices: Vec<String> = swatches
            .iter()
            .map(|swatch| {
                nodes_in(&harness, region, Role::Label)
                    .into_iter()
                    .filter(|label| {
                        let at = label.rect();
                        at.top() >= swatch.bottom()
                            && at.top() - swatch.bottom() < 8.0
                            && (at.left() - swatch.left()).abs() < 4.0
                    })
                    .filter_map(|label| label.value())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        let expected = if region == Region::Center {
            ["0", "1"]
        } else {
            ["", ""]
        };
        assert_eq!(indices, expected, "{region:?}");
    }
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    for fact in ["Indexed (Base=DeviceRGB, HiVal=1)", "Indexed · 2 colours"] {
        assert!(inspector.contains(&fact.to_owned()), "{fact} in {inspector:?}");
    }
    // Smoke: the colours are what the renderer made.
    assert_eq!(
        harness.state().state.preview_digest(r),
        Some(expected_digest(&harness, r))
    );
}

#[test]
fn an_axial_shading_shows_its_bar_with_the_values_of_its_stops() {
    let mut harness = app();

    objects_at(&mut harness, Source::Gallery, GALLERY_SHADING);

    let r = obj(GALLERY_SHADING);
    let bar = rect_of(&harness, Region::Center, Role::Image, &image_label(r));
    assert!(bar.width() > 8.0 * bar.height(), "{bar:?}");
    assert_eq!(bar.height(), 56.0);
    // Red at t = 0 under the bar's left end, blue at t = 1 under its right end.
    let start = rect_of(&harness, Region::Center, Role::Label, "0 · 1 0 0");
    let end = rect_of(&harness, Region::Center, Role::Label, "1 · 0 0 1");
    assert!(start.top() > bar.bottom() && (start.left() - bar.left()).abs() < 8.0);
    assert!((end.right() - bar.right()).abs() < 8.0, "{end:?} {bar:?}");
    let center = texts_in(&harness, Region::Center, Role::Label);
    for text in [
        "axial · 2 stops · DeviceRGB",
        "Type 2 · axial",
        "Extend true true",
        "Function 2 · exponential",
    ] {
        assert!(center.contains(&text.to_owned()), "{text} in {center:?}");
    }
    // The inspector: the bar without values, one chip.
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(inspector.contains(&"axial · 2 stops".to_owned()), "{inspector:?}");
    assert!(!inspector.contains(&"0 · 1 0 0".to_owned()));
    // Smoke: the bar is what the renderer made.
    assert_eq!(
        harness.state().state.preview_digest(r),
        Some(expected_digest(&harness, r))
    );
}

#[test]
fn a_form_keeps_its_instructions_in_the_centre_and_its_picture_in_the_inspector() {
    let mut harness = app();

    objects_at(&mut harness, Source::Fixture("images"), 6);

    assert_eq!(bitmaps_of(&harness, obj(6)), [Region::Inspector]);
    let center = texts_in(&harness, Region::Center, Role::Label);
    assert!(
        center.contains(&"Drawing instructions (decoded)".to_owned()),
        "{center:?}"
    );
    assert!(
        texts_in(&harness, Region::Inspector, Role::Label).contains(&"100 × 100 pt at 200, 20".to_owned())
    );
}

#[test]
fn a_page_object_shows_the_page() {
    let mut harness = app();

    objects_at(&mut harness, Source::Fixture("images"), 3);

    assert_eq!(bitmaps_of(&harness, obj(3)), [Region::Center, Region::Inspector]);
    assert_eq!(
        harness.state().state.preview_digest(obj(3)),
        Some(expected_digest(&harness, obj(3)))
    );
}

#[test]
fn objects_without_a_picture_have_no_preview_section() {
    let mut harness = app();

    // The catalog.
    objects_at(&mut harness, Source::Fixture("images"), 1);

    let sections = texts_in(&harness, Region::Inspector, Role::Button);
    assert!(!sections.contains(&PREVIEW_SECTION.to_owned()), "{sections:?}");
    assert!(sections.contains(&"Object".to_owned()), "{sections:?}");
    assert_eq!(bitmaps_of(&harness, obj(1)), []);
}

fn snapshot_of(file: Source, n: u32, name: &str) {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    objects_at(&mut harness, file, n);
    harness.remove_cursor();
    harness.run_steps(2);

    harness.snapshot(name);
}

#[test]
fn snapshot_image_preview() {
    snapshot_of(Source::Fixture("images"), 5, "app_objects_image_preview");
}

#[test]
fn snapshot_font_preview() {
    snapshot_of(Source::Gallery, GALLERY_EMBEDDED_FONT, "app_objects_font_preview");
}

#[test]
fn snapshot_missing_font_preview() {
    snapshot_of(
        Source::Gallery,
        GALLERY_MISSING_FONT,
        "app_objects_missing_font_preview",
    );
}

#[test]
fn snapshot_shading_preview() {
    snapshot_of(Source::Gallery, GALLERY_SHADING, "app_objects_shading_preview");
}

#[test]
fn snapshot_palette_preview() {
    snapshot_of(Source::Gallery, GALLERY_INDEXED, "app_objects_palette_preview");
}
