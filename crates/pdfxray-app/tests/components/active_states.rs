//! Neutral active states.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn an_active_tool_button_is_neutral_white_on_a_white_8_percent_fill_with_no_accent() {
    let mut harness = builder(vec2(360.0, 60.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ui.horizontal(|ui| {
                ToolButton::new("Active").icon(Icon::Xray).active(true).show(ui);
                ToolButton::new("Idle").icon(Icon::Xray).show(ui);
            });
        });
    });
    harness.run();
    let shapes = shapes_of(&harness);
    let active = harness.get_by_role_and_label(Role::Button, "Active").rect();
    let idle = harness.get_by_role_and_label(Role::Button, "Idle").rect();

    let fills = rect_shapes_at(&shapes, active);
    assert_eq!(fills.len(), 1, "one rectangle: {fills:?}");
    assert_eq!(fills[0].fill, tokens::NEUTRAL_ACTIVE);
    assert_eq!(fills[0].stroke.width, 0.0, "no stroke at all, not an accent one");
    assert_eq!(text_ink(&shapes, "Active"), Some(egui::Color32::WHITE));
    // The idle one has no fill and a dimmer label.
    assert!(
        rect_shapes_at(&shapes, idle)
            .iter()
            .all(|r| r.fill == egui::Color32::TRANSPARENT)
    );
    assert_eq!(text_ink(&shapes, "Idle"), Some(tokens::TEXT_2));
    // Nothing on screen is drawn in the accent or its dim fill.
    let accent = |c: egui::Color32| c == tokens::ACCENT || c == tokens::ACCENT_DIM;
    assert!(!shapes.iter().any(|shape| match shape {
        egui::Shape::Rect(r) => accent(r.fill) || accent(r.stroke.color),
        _ => false,
    }));
    // Smoke: still a toggle for assistive tech.
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "Active")));
}

#[test]
fn the_selected_segment_is_neutral_too() {
    let mut harness = builder(vec2(300.0, 60.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let mut mode = 1;
            segmented(
                ui,
                &mut mode,
                &[
                    Segment::new(0, "Kind"),
                    Segment::new(1, "Tag role"),
                    Segment::new(2, "Artifact"),
                ],
            );
        });
    });
    harness.run();
    let shapes = shapes_of(&harness);
    let selected = harness.get_by_role_and_label(Role::Button, "Tag role").rect();

    let fills = rect_shapes_at(&shapes, selected);
    assert_eq!(fills.len(), 1, "{fills:?}");
    assert_eq!(fills[0].fill, tokens::NEUTRAL_ACTIVE);
    assert_eq!(text_ink(&shapes, "Tag role"), Some(egui::Color32::WHITE));
    assert_eq!(text_ink(&shapes, "Kind"), Some(tokens::TEXT_2));
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "Tag role")));
    assert!(!is_on(&harness.get_by_role_and_label(Role::Button, "Kind")));
}

#[test]
fn the_primary_button_is_the_cyan_gradient_with_a_glow_and_dark_ink() {
    let mut harness = builder(vec2(200.0, 80.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            ToolButton::new("Validate")
                .icon(Icon::Validation)
                .primary()
                .tall()
                .show(ui);
        });
    });
    harness.run();
    let shapes = shapes_of(&harness);
    let button = harness.get_by_role_and_label(Role::Button, "Validate").rect();

    // The gradient is a mesh inside the button, top colour at its top edge, bottom at its bottom.
    let mesh = shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Mesh(mesh) if mesh.vertices.iter().all(|v| button.expand(1.0).contains(v.pos)) => {
                Some(mesh.clone())
            }
            _ => None,
        })
        .expect("a gradient mesh over the button");
    let top = mesh.vertices.iter().map(|v| v.pos.y).fold(f32::MAX, f32::min);
    let bottom = mesh.vertices.iter().map(|v| v.pos.y).fold(f32::MIN, f32::max);
    assert!(
        mesh.vertices
            .iter()
            .filter(|v| v.pos.y == top)
            .all(|v| v.color == tokens::PRIMARY_TOP)
    );
    assert!(
        mesh.vertices
            .iter()
            .filter(|v| v.pos.y == bottom)
            .all(|v| v.color == tokens::PRIMARY_BOTTOM)
    );
    // The glow is a cyan shadow shape behind it, and the label is the dark ink.
    assert!(shapes.iter().any(|shape| matches!(shape,
        egui::Shape::Rect(r) if r.blur_width > 0.0 && r.fill.b() > r.fill.r() && r.fill.a() < 0x40)));
    assert_eq!(text_ink(&shapes, "Validate"), Some(tokens::ON_ACCENT));
}
