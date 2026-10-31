//! Keycaps.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_keycap_is_a_small_hairline_key_with_its_text_as_name() {
    let mut harness = builder(vec2(160.0, 60.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.horizontal(|ui| {
                Keycap::new("X").show(ui);
                Keycap::new("⇧T").show(ui);
            });
        });
    });
    harness.run();

    let x = harness.get_by_role_and_label(Role::Label, "X").rect();
    let shift_t = harness.get_by_role_and_label(Role::Label, "⇧T").rect();
    assert!(shift_t.width() > x.width(), "a longer key is a wider cap");
    // The cap is a rounded rectangle with the neutral fill and a hairline around the text.
    let shapes = shapes_of(&harness);
    let cap = shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Rect(r) if r.rect.contains_rect(x) && r.fill == tokens::NEUTRAL_ACTIVE => {
                Some(r.clone())
            }
            _ => None,
        })
        .expect("a filled cap around the X");
    assert_eq!(cap.stroke.color, tokens::POPOVER_HAIRLINE);
    assert_eq!(cap.stroke.width, 1.0);
    assert!(cap.rect.height() <= 18.0, "small: {:?}", cap.rect);
    // Mono 10.
    let key = shapes.iter().find_map(|shape| match shape {
        egui::Shape::Text(t) if t.galley.text() == "X" => {
            Some(t.galley.job.sections[0].format.font_id.clone())
        }
        _ => None,
    });
    assert_eq!(key, Some(fonts::mono(10.0)));
}

#[test]
fn tooltips_put_the_key_as_a_keycap_right_next_to_the_name_never_at_the_far_edge() {
    let mut harness = builder(vec2(420.0, 200.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let response = ToolButton::new("X-ray").icon(Icon::Xray).icon_only(true).show(ui);
            tooltip::named(
                response,
                "X-ray",
                "Outline every painted item: glyphs, paths, images, shadings",
                Some("X"),
            );
        });
    });
    harness.run();
    let button = harness
        .get_by_role_and_label(Role::Button, "X-ray")
        .rect()
        .center();
    harness.hover_at(button);
    harness.run_steps(4);

    let name = harness.get_by_role_and_label(Role::Label, "X-ray").rect();
    let key = harness.get_by_role_and_label(Role::Label, "X").rect();
    let detail = harness
        .get_by_role_and_label(
            Role::Label,
            "Outline every painted item: glyphs, paths, images, shadings",
        )
        .rect();

    // The name first, the cap right after it on the same line.
    assert!(key.left() > name.right(), "{key:?} after {name:?}");
    assert!(
        key.left() - name.right() < 12.0,
        "next to the name: {key:?} vs {name:?}"
    );
    assert!((key.center().y - name.center().y).abs() < 4.0, "same line");
    // The description is the wide line; the key is nowhere near its far edge.
    assert!(detail.width() > name.width() * 3.0);
    assert!(key.right() < detail.right() - 60.0, "{key:?} vs {detail:?}");
    // The cap is drawn: a hairline-framed neutral rectangle around the key.
    let shapes = shapes_of(&harness);
    assert!(shapes.iter().any(|shape| matches!(shape,
        egui::Shape::Rect(r) if r.rect.contains_rect(key)
            && r.fill == tokens::NEUTRAL_ACTIVE
            && r.stroke.color == tokens::POPOVER_HAIRLINE)));
    // And the tooltip itself is the frosted panel.
    assert!(shapes.iter().any(|shape| matches!(shape,
        egui::Shape::Rect(r) if r.fill == tokens::TOOLTIP_FILL && r.stroke.color == tokens::POPOVER_HAIRLINE)));
}

#[test]
fn the_shortcut_tooltip_shows_the_name_and_a_keycap_after_it() {
    let mut harness = builder(vec2(300.0, 100.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let response = ToolButton::new("Measure")
                .icon(Icon::Measure)
                .icon_only(true)
                .show(ui);
            tooltip::with_shortcut(response, "Measure distances", "⇧M");
        });
    });
    harness.run();
    harness.hover_at(
        harness
            .get_by_role_and_label(Role::Button, "Measure")
            .rect()
            .center(),
    );
    harness.run_steps(4);

    let name = harness
        .get_by_role_and_label(Role::Label, "Measure distances")
        .rect();
    let key = harness.get_by_role_and_label(Role::Label, "⇧M").rect();
    // The key's own padding and the gap are all that separate them.
    assert!(
        key.left() > name.right() && key.left() - name.right() < 14.0,
        "{key:?} {name:?}"
    );
}
