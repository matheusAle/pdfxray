//! The "components" snapshot.

use super::*;

/// The frame "Components" of the design, row by row, and the rest of the set below it.
fn components_sheet(ui: &mut egui::Ui) {
    on_surface(ui, |ui| {
        ui.set_width(496.0);
        ui.spacing_mut().item_spacing.y = 14.0;
        let annotations = [Annotation::Text(String::new())];
        let _ = &annotations;
        TreeRow::new(0, "P")
            .detail(Some("12 0 R"))
            .dot(tokens::KIND_TAG)
            .expandable(false)
            .show(ui, Id::unique("components-row"));
        PropRow::new("MediaBox", "[0 0 612 792]").show(ui);
        ui.horizontal(|ui| {
            ToolButton::new("X-ray").icon(Icon::Xray).show(ui);
        });
        ui.horizontal(|ui| {
            ToolButton::new("X-ray").icon(Icon::Xray).active(true).show(ui);
        });
        ui.horizontal(|ui| {
            ToolButton::new("Validate")
                .icon(Icon::Validation)
                .primary()
                .tall()
                .show(ui);
            Keycap::new("X").show(ui);
            Keycap::new("⇧T").show(ui);
        });
        ui.horizontal(|ui| {
            Chip::new("/P").show(ui);
        });
        ui.horizontal(|ui| {
            Chip::new("glyph").dot(tokens::KIND_CONTENT).show(ui);
        });
        ui.separator();
        ui.horizontal(|ui| {
            for (name, tone) in [
                ("Tagged · UA-2", Tone::Ok),
                ("Untagged", Tone::Warn),
                ("damaged xref", Tone::Err),
                ("/Figure", Tone::Kind(tokens::KIND_TAG)),
            ] {
                Chip::new(name).tone(tone).show(ui);
            }
            Chip::new("rev 3 of 3").show(ui);
        });
        ui.horizontal(|ui| {
            crumb::separator(ui);
            Crumb::new("page 1", tokens::OK).show(ui);
            crumb::separator(ui);
            Crumb::new("Figure", tokens::KIND_TAG).show(ui);
            crumb::separator(ui);
            Crumb::new("Do /Im1", tokens::KIND_CONTENT).current(true).show(ui);
        });
        let mut layer = 1;
        segmented(
            ui,
            &mut layer,
            &[
                Segment::new(0, "All"),
                Segment::new(1, "Tags").dot(tokens::KIND_TAG),
                Segment::new(2, "Content").dot(tokens::KIND_CONTENT),
                Segment::new(3, "Annots").dot(tokens::KIND_ANNOT),
            ],
        );
        ui.horizontal(|ui| {
            ui.label("Hit stack");
            Badge::new(5).show(ui);
            ui.label("Operators");
            Badge::new(377).show(ui);
            ui.label("Diagnostics");
            Badge::new(7).tone(Tone::Warn).show(ui);
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for icon in Icon::ALL {
                icon.show(ui, 16.0, tokens::TEXT_2);
            }
        });
    });
}

#[test]
fn snapshot_components() {
    let _gpu = common::gpu_lock();
    let mut harness = builder(vec2(520.0, 520.0)).wgpu().build_ui(components_sheet);
    harness.run();

    harness.snapshot("components");
}

#[test]
fn a_tooltip_beside_opens_to_the_right_level_with_its_widget_and_leaves_the_next_one_free() {
    let mut harness = builder(vec2(300.0, 200.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.vertical(|ui| {
                let first = ToolButton::new("Content")
                    .icon(Icon::Content)
                    .icon_only(true)
                    .show(ui);
                tooltip::text_beside(first, "Content");
                ToolButton::new("Artifacts")
                    .icon(Icon::Artifacts)
                    .icon_only(true)
                    .show(ui);
            });
        });
    });
    harness.run();
    let first = harness.get_by_role_and_label(Role::Button, "Content").rect();
    let second = harness.get_by_role_and_label(Role::Button, "Artifacts").rect();

    harness.hover_at(first.center());
    harness.run_steps(4);

    let tip = harness.get_by_role_and_label(Role::Label, "Content").rect();
    assert!(tip.left() >= first.right(), "{tip:?} beside {first:?}");
    assert!(!tip.intersects(second), "{tip:?} over {second:?}");
    assert!(
        (tip.center().y - first.center().y).abs() < 6.0,
        "{tip:?} level with {first:?}"
    );
    // Smoke: it is the frosted panel, not a bare label.
    assert!(shapes_of(&harness).iter().any(|shape| matches!(shape,
        egui::Shape::Rect(r) if r.fill == tokens::TOOLTIP_FILL && r.rect.contains_rect(tip))));
}
