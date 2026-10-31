//! Tooltips.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_tooltip_shows_after_400_ms_of_resting_on_the_widget() {
    let mut harness = builder(vec2(300.0, 100.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let response = ToolButton::new("Measure")
                .icon(Icon::Measure)
                .icon_only(true)
                .show(ui);
            tooltip::with_shortcut(response, "Measure distances", "M");
        });
    });
    harness.run();
    let button = harness
        .get_by_role_and_label(Role::Button, "Measure")
        .rect()
        .center();

    harness.hover_at(button);
    // One step is 250 ms: too early.
    harness.step();
    harness.step();
    harness.remove_cursor();
    harness.hover_at(button);
    harness.step();
    assert!(
        harness.query_by_label("Measure distances").is_none(),
        "after 250 ms"
    );

    // 750 ms: shown, with its shortcut.
    harness.run_steps(3);
    assert!(
        harness.query_by_label("Measure distances").is_some(),
        "after 750 ms"
    );
    let _key = harness.get_by_label("M");
}

#[test]
fn a_named_tooltip_leads_with_the_name_then_the_key_and_what_it_does() {
    let mut harness = builder(vec2(400.0, 160.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            let response = ToolButton::new("X-ray").icon(Icon::Xray).icon_only(true).show(ui);
            tooltip::named(response, "X-ray", "Outline every painted item", Some("X"));
        });
    });
    harness.run();
    let button = harness
        .get_by_role_and_label(Role::Button, "X-ray")
        .rect()
        .center();
    // Smoke: only the button carries the name before the pointer rests on it.
    assert!(harness.query_by_label("Outline every painted item").is_none());

    harness.hover_at(button);
    harness.run_steps(4);

    let labels: Vec<String> = harness
        .query_all_by_role(Role::Label)
        .filter_map(|node| node.value())
        .collect();
    assert_eq!(labels, ["X-ray", "X", "Outline every painted item"]);
}
