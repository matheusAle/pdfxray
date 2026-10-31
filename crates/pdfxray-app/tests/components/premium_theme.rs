//! The premium theme (V1).

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn the_v2_tokens_are_the_ones_of_the_design() {
    let hex = |c: egui::Color32| format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
    let rgba = |c: egui::Color32| {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        format!("{r:02X}{g:02X}{b:02X}{a:02X}")
    };

    assert_eq!(
        [
            hex(tokens::CANVAS),
            hex(tokens::PANEL),
            hex(tokens::TOOLBAR_BG),
            hex(tokens::DRAWER_BG),
            hex(tokens::RAIL_BG),
            hex(tokens::STATUS_BG),
        ],
        ["0A0C0F", "101317", "0C0F12", "0E1115", "0B0D10", "0B0D10"]
    );
    assert_eq!(
        [
            rgba(tokens::TITLE_BG),
            rgba(tokens::NEUTRAL_ACTIVE),
            rgba(tokens::NEUTRAL_HOVER),
            rgba(tokens::HAIRLINE),
            rgba(tokens::POPOVER_HAIRLINE),
            rgba(tokens::POPOVER_FILL),
            rgba(tokens::CARD_FILL),
        ],
        [
            "0F1216D9", "FFFFFF14", "FFFFFF0A", "FFFFFF0A", "FFFFFF17", "1A1E24F7", "12161BE6"
        ]
    );
    assert_eq!(
        [
            hex(tokens::ROLE_HEADING),
            hex(tokens::ROLE_P),
            hex(tokens::ROLE_FIGURE),
            hex(tokens::ROLE_CAPTION),
            hex(tokens::ROLE_TABLE),
            hex(tokens::ROLE_LIST),
            hex(tokens::ROLE_LINK),
            hex(tokens::ROLE_INLINE),
            hex(tokens::ROLE_GROUP),
            hex(tokens::KIND_ARTIFACT),
        ],
        [
            "B48CFF", "5AA9FF", "FF8A4C", "F472B6", "2DD4BF", "A3E635", "FACC15", "94A3B8", "CBD5E1",
            "8A93A3"
        ]
    );
    assert_eq!(
        [hex(tokens::PRIMARY_TOP), hex(tokens::PRIMARY_BOTTOM)],
        ["6FE3F9", "2CC6E8"]
    );
    // Rows are 28 px; the inspector title is 17 px; section headers 10 px tracked +0.9.
    assert_eq!(
        [
            tokens::ROW_H,
            tokens::text::INSPECTOR_TITLE,
            tokens::text::SECTION,
            tokens::tracking::SECTION
        ],
        [28.0, 17.0, 10.0, 0.9]
    );
    assert!((-0.3..=-0.2).contains(&tokens::tracking::TITLE));
}

#[test]
fn egui_paints_the_panels_and_popups_with_the_v2_tokens_not_a_lighter_step() {
    let ctx = egui::Context::default();
    pdfxray_app::theme::install(&ctx);
    let v = ctx.global_style().visuals.clone();

    assert_eq!(v.panel_fill, tokens::PANEL);
    assert_eq!(v.window_fill, tokens::POPOVER_FILL);
    assert_eq!(v.window_stroke.color, tokens::POPOVER_HAIRLINE);
    assert_eq!(v.extreme_bg_color, tokens::CANVAS);
    assert_eq!(v.widgets.noninteractive.bg_stroke.color, tokens::HAIRLINE);
    // The accent is the selection: no widget state outlines itself with it.
    assert_eq!(v.selection.bg_fill, tokens::ACCENT_DIM);
    for state in [
        v.widgets.inactive,
        v.widgets.hovered,
        v.widgets.active,
        v.widgets.open,
    ] {
        assert_ne!(state.bg_stroke.color, tokens::ACCENT);
        assert_ne!(
            state.bg_fill,
            tokens::SURFACE_2,
            "no step lighter than the tokens"
        );
    }
    // Smoke: the shadow of popups is the deep one of the design.
    assert_eq!(v.popup_shadow.offset, [0, 18]);
}

#[test]
fn the_new_font_weights_lay_out_and_get_heavier() {
    let harness = builder(vec2(300.0, 120.0)).build_ui(|ui| {
        ui.label(egui::RichText::new("Hamburgefonstiv").font(fonts::bold(14.0)));
        ui.label(egui::RichText::new("23 0 obj").font(fonts::mono_semibold(11.0)));
        ui.label(egui::RichText::new("stream-length-mismatch").font(fonts::mono_bold(11.0)));
    });
    let _bold = harness.get_by_role_and_label(Role::Label, "Hamburgefonstiv");
    let _semibold = harness.get_by_role_and_label(Role::Label, "23 0 obj");
    let _mono_bold = harness.get_by_role_and_label(Role::Label, "stream-length-mismatch");
    let width = |font: egui::FontId| {
        harness.ctx.fonts_mut(|f| {
            f.layout_no_wrap("Hamburgefonstiv".to_owned(), font, egui::Color32::WHITE)
                .size()
                .x
        })
    };
    // Bold is wider than semibold, which is wider than regular; mono weights share one advance.
    assert!(width(fonts::bold(14.0)) > width(fonts::semibold(14.0)));
    assert!(width(fonts::semibold(14.0)) > width(fonts::ui(14.0)));
    assert_eq!(width(fonts::mono_bold(11.0)), width(fonts::mono_semibold(11.0)));
    assert_eq!(width(fonts::mono_bold(11.0)), width(fonts::mono(11.0)));
}

#[test]
fn tracked_text_opens_or_tightens_the_letters_and_the_inspector_title_wraps() {
    let mut harness = builder(vec2(220.0, 200.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.label(fonts::section_header("Graphics state"));
            ui.label(fonts::inspector_title(
                "Image XObject /Im1 with a rather long name",
            ));
            ui.label(fonts::tracked("Plain", fonts::ui(12.0), tokens::TEXT, 0.0));
        });
    });
    harness.run();
    let shapes = shapes_of(&harness);
    let job_of = |text: &str| {
        shapes.iter().find_map(|shape| match shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.galley.job.clone()),
            _ => None,
        })
    };

    // The section header: upper case, 10 px, +0.9, text-3.
    let header = job_of("GRAPHICS STATE").expect("the section header is drawn");
    let format = &header.sections[0].format;
    assert_eq!(format.extra_letter_spacing, 0.9);
    assert_eq!(format.font_id.size, 10.0);
    assert_eq!(format.color, tokens::TEXT_3);
    // The title: 17 px, tightened by 0.2 to 0.3.
    let title = job_of("Image XObject /Im1 with a rather long name").expect("the title is drawn");
    let format = &title.sections[0].format;
    assert_eq!(format.font_id.size, 17.0);
    assert!(
        (-0.3..=-0.2).contains(&format.extra_letter_spacing),
        "{}",
        format.extra_letter_spacing
    );
    // It wraps instead of running out of the panel: two lines or more, all inside the width.
    let rect = harness
        .get_by_role_and_label(Role::Label, "Image XObject /Im1 with a rather long name")
        .rect();
    assert!(rect.height() > 17.0 * 1.8, "{rect:?}");
    assert!(rect.right() <= 220.0, "{rect:?}");
    // Smoke: tracking is real: the same letters are narrower with −0.25 than with +0.9.
    let width = |spacing: f32| {
        harness.ctx.fonts_mut(|f| {
            f.layout_job(fonts::tracked(
                "Annual Report",
                fonts::semibold(12.0),
                tokens::TEXT,
                spacing,
            ))
            .size()
            .x
        })
    };
    assert!(width(-0.25) < width(0.0) && width(0.0) < width(0.9));
}

#[test]
fn rows_are_28_px_tall() {
    let mut harness = builder(vec2(360.0, 140.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            TreeRow::new(0, "Figure").show(ui, Id::unique("height-row"));
            PropRow::new("Size", "1 KB").show(ui);
        });
    });
    harness.run();

    let tree = harness.get_by_role_and_label(Role::Button, "Figure").rect();
    let prop = harness.get_by_role_and_label(Role::Label, "Size").rect();
    assert_eq!(tree.height(), 28.0);
    // The key label is centred in its 28 px row: the row below the tree row starts 28 px lower.
    assert!(prop.top() >= tree.bottom(), "{prop:?} under {tree:?}");
    assert!(prop.height() <= 28.0);
    assert_eq!(tokens::ROW_H, 28.0);
}
