//! The design tokens.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn tokens_are_the_ones_of_the_design() {
    let hex = |c: egui::Color32| format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());

    assert_eq!(
        [
            hex(tokens::BG),
            hex(tokens::SURFACE),
            hex(tokens::SURFACE_2),
            hex(tokens::SURFACE_3),
            hex(tokens::BORDER),
            hex(tokens::BORDER_STRONG),
            hex(tokens::TEXT),
            hex(tokens::TEXT_2),
            hex(tokens::TEXT_3),
            hex(tokens::ACCENT),
        ],
        [
            "0B0D10", "121519", "181C21", "20252C", "252A31", "323842", "E7EAEE", "A2AAB6", "6B7380",
            "3DD6F5"
        ]
    );
    assert_eq!(
        [
            hex(tokens::KIND_TAG),
            hex(tokens::KIND_CONTENT),
            hex(tokens::KIND_ARTIFACT),
            hex(tokens::KIND_ANNOT),
            hex(tokens::OK),
            hex(tokens::WARN),
            hex(tokens::ERR),
        ],
        [
            "B48CFF", "5AA9FF", "8A93A3", "FFB547", "4ADE80", "FBBF24", "F87171"
        ]
    );
    // accent-dim is the accent at 0x1F.
    assert_eq!(
        tokens::ACCENT_DIM,
        egui::Color32::from_rgba_unmultiplied(0x3D, 0xD6, 0xF5, 0x1F)
    );
    // The layout sizes.
    assert_eq!(
        [
            tokens::RAIL_W,
            tokens::NAVIGATOR_W,
            tokens::INSPECTOR_W,
            tokens::DRAWER_H,
            tokens::TITLE_H,
            tokens::STATUS_H
        ],
        [52.0, 300.0, 340.0, 196.0, 44.0, 28.0]
    );
}

#[test]
fn the_fonts_are_installed_and_have_the_weights_the_ui_uses() {
    let harness = builder(vec2(200.0, 60.0)).build_ui(|ui| {
        ui.label(egui::RichText::new("Inter Regular").font(fonts::ui(12.0)));
        ui.label(egui::RichText::new("Inter Medium").font(fonts::medium(12.0)));
        ui.label(egui::RichText::new("Inter SemiBold").font(fonts::semibold(12.0)));
        ui.label(egui::RichText::new("0123 [0 0 612 792]").font(fonts::mono(11.0)));
        ui.label(egui::RichText::new("Tj 12 0 R").font(fonts::mono_medium(11.0)));
    });

    // Missing glyphs panic in the harness, so reaching here means every family laid out.
    let widths: Vec<f32> = ["Inter Regular", "Inter Medium", "Inter SemiBold"]
        .iter()
        .map(|text| harness.get_by_label(text).rect().width())
        .collect();
    // Heavier weights of the same text are wider: the weights really are different faces.
    assert!(widths[0] > 0.0);
    let regular = harness.ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            "Hamburgefonstiv".to_owned(),
            fonts::ui(12.0),
            egui::Color32::WHITE,
        )
        .size()
        .x
    });
    let semibold = harness.ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            "Hamburgefonstiv".to_owned(),
            fonts::semibold(12.0),
            egui::Color32::WHITE,
        )
        .size()
        .x
    });
    assert!(semibold > regular, "{semibold} vs {regular}");
}
