//! Property rows.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_prop_row_shows_its_copy_icon_only_on_hover_and_flips_to_a_check_for_1_2_s() {
    let mut harness = builder(vec2(360.0, 80.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            PropRow::new("MediaBox", "[0 0 612 792]").show(ui);
            PropRow::new("Size", "1 KB").show(ui);
        });
    });
    harness.run();

    // Hidden: neither row has a copy button while the pointer is elsewhere.
    harness.remove_cursor();
    harness.run();
    assert!(harness.query_by_label("Copy MediaBox").is_none());
    assert!(harness.query_by_label("Copy Size").is_none());

    // Hovering the row shows the icon of that row only.
    let row = harness.get_by_role_and_label(Role::Label, "MediaBox").rect();
    harness.hover_at(row.center());
    harness.run();
    let _copy = harness.get_by_role_and_label(Role::Button, "Copy MediaBox");
    assert!(harness.query_by_label("Copy Size").is_none());

    // Clicking copies the value and flips the icon.
    harness
        .get_by_role_and_label(Role::Button, "Copy MediaBox")
        .click();
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
    assert_eq!(copied.as_deref(), Some("[0 0 612 792]"));
    // The next frame draws the check.
    harness.step();
    assert!(harness.query_by_label("Copied MediaBox").is_some());
    assert!(harness.query_by_label("Copy MediaBox").is_none());

    // The check stays for 1.2 s even when the pointer leaves, then the icon is hidden again.
    // Each step of the harness is a quarter of a second.
    harness.remove_cursor();
    harness.run_steps(3);
    assert!(harness.query_by_label("Copied MediaBox").is_some(), "1 s later");
    harness.run_steps(3);
    assert!(
        harness.query_by_label("Copied MediaBox").is_none(),
        "1.75 s later"
    );
    assert!(harness.query_by_label("Copy MediaBox").is_none());
}

#[test]
fn a_prop_row_can_copy_something_else_link_warn_and_wrap() {
    let mut harness = builder(vec2(360.0, 160.0)).build_ui_state(
        |ui, followed: &mut bool| {
            on_surface(ui, |ui| {
                PropRow::new("Width", "70.56 mm").copy_text("200 pt").show(ui);
                let linked = PropRow::new("Owner", "12 0 R").link(true).show(ui);
                if linked.value.clicked() {
                    *followed = true;
                }
                PropRow::new("/Alt", "missing").warn(true).show(ui);
                PropRow::new(
                    "Long",
                    "a value that is much longer than the column so that it has to wrap onto lines",
                )
                .show(ui);
            });
        },
        false,
    );
    harness.run();

    // The copy text is not the shown text.
    harness.hover_at(
        harness
            .get_by_role_and_label(Role::Label, "Width")
            .rect()
            .center(),
    );
    harness.run();
    harness.get_by_role_and_label(Role::Button, "Copy Width").click();
    harness.step();
    assert!(
        harness
            .output()
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::CopyText(text) if text == "200 pt"))
    );

    // A link is a link, and clicking it is reported.
    assert!(!*harness.state());
    harness.get_by_role_and_label(Role::Link, "12 0 R").click();
    harness.run();
    assert!(*harness.state());

    // A long value wraps: the row is taller than a one-line row.
    let long = harness
        .get_by_role_and_label(
            Role::Label,
            "a value that is much longer than the column so that it has to wrap onto lines",
        )
        .rect();
    assert!(long.height() > 30.0, "{long:?}");
}
