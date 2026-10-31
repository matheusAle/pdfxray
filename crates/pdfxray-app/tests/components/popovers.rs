//! Popovers.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_popover_opens_on_click_and_closes_on_escape_and_on_a_click_outside() {
    let mut harness = builder(vec2(420.0, 260.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let chip = ui.button("rev 3 of 3");
            popover::show(&chip, egui::RectAlign::BOTTOM_START, 180.0, |ui| {
                ui.label("Revisions");
            });
            ui.add_space(120.0);
            ui.label("elsewhere");
        });
    });
    harness.run();
    assert!(harness.query_by_label("Revisions").is_none());

    harness.get_by_role_and_label(Role::Button, "rev 3 of 3").click();
    harness.run();
    assert!(harness.query_by_label("Revisions").is_some());

    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.query_by_label("Revisions").is_none());

    // Again, closed by a click outside.
    harness.get_by_role_and_label(Role::Button, "rev 3 of 3").click();
    harness.run();
    assert!(harness.query_by_label("Revisions").is_some());
    let outside = harness.get_by_label("elsewhere").rect().center();
    harness.hover_at(outside);
    harness.drag_at(outside);
    harness.drop_at(outside);
    harness.run();
    assert!(harness.query_by_label("Revisions").is_none());
}

#[test]
fn a_popover_is_the_frosted_panel() {
    let mut harness = builder(vec2(420.0, 260.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let chip = ui.button("rev 3 of 3");
            popover::show(&chip, egui::RectAlign::BOTTOM_START, 180.0, |ui| {
                ui.label("Revisions");
            });
        });
    });
    harness.run();
    harness.get_by_role_and_label(Role::Button, "rev 3 of 3").click();
    harness.run();

    let content = harness.get_by_label("Revisions").rect();
    let shapes = shapes_of(&harness);
    let frame = shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Rect(r) if r.fill == tokens::POPOVER_FILL && r.rect.contains_rect(content) => {
                Some(r.clone())
            }
            _ => None,
        })
        .expect("the popover's fill");
    assert_eq!(frame.stroke.color, tokens::POPOVER_HAIRLINE);
    // Smoke: it is below the chip, and the shadow behind it is the deep one.
    assert!(
        frame.rect.top()
            >= harness
                .get_by_role_and_label(Role::Button, "rev 3 of 3")
                .rect()
                .bottom()
    );
    assert!(shapes.iter().any(|shape| matches!(shape,
        egui::Shape::Rect(r) if r.blur_width >= 44.0 && r.rect.height() >= frame.rect.height())));
}
