//! Tool buttons.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_tool_button_toggles_shows_it_and_keeps_its_name_when_only_the_icon_is_drawn() {
    struct Tools {
        xray: bool,
        disabled_clicks: u32,
    }
    let mut harness = builder(vec2(360.0, 60.0)).build_ui_state(
        |ui, state: &mut Tools| {
            on_surface(ui, |ui| {
                ui.horizontal(|ui| {
                    if ToolButton::new("X-ray")
                        .icon(Icon::Xray)
                        .active(state.xray)
                        .show(ui)
                        .clicked()
                    {
                        state.xray = !state.xray;
                    }
                    ToolButton::new("Measure")
                        .icon(Icon::Measure)
                        .icon_only(true)
                        .show(ui);
                    ui.add_enabled_ui(false, |ui| {
                        if ToolButton::new("Isolate").icon(Icon::Isolate).show(ui).clicked() {
                            state.disabled_clicks += 1;
                        }
                    });
                });
            });
        },
        Tools {
            xray: false,
            disabled_clicks: 0,
        },
    );
    harness.run();

    assert!(!is_on(&harness.get_by_role_and_label(Role::Button, "X-ray")));
    harness.get_by_role_and_label(Role::Button, "X-ray").click();
    harness.run();
    assert!(harness.state().xray);
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "X-ray")));
    // Icon only: no text is drawn but the name is there, and the button is square.
    let measure = harness.get_by_role_and_label(Role::Button, "Measure").rect();
    assert_eq!(measure.width(), tokens::CONTROL_H);
    assert!(harness.query_by_role_and_label(Role::Label, "Measure").is_none());
    // A disabled button is disabled and does nothing.
    let isolate = harness.get_by_role_and_label(Role::Button, "Isolate");
    assert!(isolate.accesskit_node().is_disabled());
    isolate.click();
    harness.run();
    assert_eq!(harness.state().disabled_clicks, 0);
}
