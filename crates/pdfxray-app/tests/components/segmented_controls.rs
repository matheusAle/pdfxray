//! Segmented controls.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_segmented_control_picks_exactly_one_option() {
    let mut harness = builder(vec2(420.0, 60.0)).build_ui_state(
        |ui, state: &mut &'static str| {
            on_surface(ui, |ui| {
                let segments = [
                    Segment::new("all", "All"),
                    Segment::new("tags", "Tags").dot(tokens::KIND_TAG),
                    Segment::new("content", "Content").dot(tokens::KIND_CONTENT),
                ];
                segmented(ui, state, &segments);
            });
        },
        "all",
    );
    harness.run();
    let selected = |harness: &Harness<'_, &'static str>| {
        ["All", "Tags", "Content"]
            .into_iter()
            .filter(|name| is_on(&harness.get_by_role_and_label(Role::Button, name)))
            .collect::<Vec<_>>()
    };
    assert_eq!(selected(&harness), ["All"]);

    harness.get_by_role_and_label(Role::Button, "Content").click();
    harness.run();

    assert_eq!(*harness.state(), "content");
    assert_eq!(selected(&harness), ["Content"]);
    // Picking the selected one again changes nothing.
    harness.get_by_role_and_label(Role::Button, "Content").click();
    harness.run();
    assert_eq!(*harness.state(), "content");
}
