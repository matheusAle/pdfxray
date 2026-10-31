//! Crumbs.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn crumbs_are_clickable_and_the_current_one_is_marked() {
    struct Path {
        clicked: Option<String>,
    }
    let mut harness = builder(vec2(420.0, 60.0)).build_ui_state(
        |ui, state: &mut Path| {
            on_surface(ui, |ui| {
                ui.horizontal(|ui| {
                    let names = ["page 1", "Figure", "MCID 9"];
                    for (i, name) in names.iter().enumerate() {
                        if i > 0 {
                            crumb::separator(ui);
                        }
                        let dot = [tokens::OK, tokens::KIND_TAG, tokens::KIND_CONTENT][i];
                        let shown = Crumb::new(name, dot).current(i == names.len() - 1);
                        assert!(shown.width(ui) > 20.0);
                        if shown.show(ui).clicked() {
                            state.clicked = Some((*name).to_owned());
                        }
                    }
                });
            });
        },
        Path { clicked: None },
    );
    harness.run();

    harness.get_by_role_and_label(Role::Button, "Figure").click();
    harness.run();

    assert_eq!(harness.state().clicked.as_deref(), Some("Figure"));
    for name in ["page 1", "Figure", "MCID 9"] {
        let _crumb = harness.get_by_role_and_label(Role::Button, name);
    }
}
