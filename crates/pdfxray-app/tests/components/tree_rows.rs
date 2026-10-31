//! Tree rows.

use pretty_assertions::assert_eq;

use super::*;

struct RowState {
    selected: bool,
    open: bool,
    toggled: u32,
    clicked: u32,
}

#[test]
fn a_tree_row_shows_its_parts_and_reports_clicks_and_toggles_separately() {
    let state = RowState {
        selected: false,
        open: false,
        toggled: 0,
        clicked: 0,
    };
    let mut harness = builder(vec2(320.0, 60.0)).build_ui_state(
        |ui, state: &mut RowState| {
            on_surface(ui, |ui| {
                let annotations = [Annotation::Text("MCID 4".to_owned())];
                let out = TreeRow::new(1, "P")
                    .detail(Some("12 0 R"))
                    .dot(tokens::KIND_TAG)
                    .expandable(state.open)
                    .selected(state.selected)
                    .annotations(&annotations)
                    .show(ui, Id::unique("row"));
                if out.toggled {
                    state.toggled += 1;
                    state.open = !state.open;
                } else if out.row.clicked() {
                    state.clicked += 1;
                    state.selected = true;
                }
            });
        },
        state,
    );
    harness.run();

    // The label, the dim id and the annotation are all there, and the chevron says "Expand".
    for text in ["P", "12 0 R", "MCID 4"] {
        let _label = harness.get_by_role_and_label(Role::Label, text);
    }
    assert!(!is_on(&harness.get_by_role_and_label(Role::Button, "P")));

    // The chevron toggles and does not select.
    harness.get_by_role_and_label(Role::Button, "Expand P").click();
    harness.run();
    assert_eq!((harness.state().toggled, harness.state().clicked), (1, 0));
    assert!(harness.state().open);
    let _collapse = harness.get_by_role_and_label(Role::Button, "Collapse P");

    // The row selects, and selected rows say so.
    harness.get_by_role_and_label(Role::Button, "P").click();
    harness.run();
    assert_eq!((harness.state().toggled, harness.state().clicked), (1, 1));
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "P")));
}

#[test]
fn a_content_reference_row_is_quiet_text_with_an_arrow_that_nothing_can_click() {
    let state = RowState {
        selected: false,
        open: false,
        toggled: 0,
        clicked: 0,
    };
    let mut harness = builder(vec2(360.0, 80.0)).build_ui_state(
        |ui, state: &mut RowState| {
            on_surface(ui, |ui| {
                let tag = TreeRow::new(1, "Figure")
                    .expandable(true)
                    .selected(state.selected)
                    .show(ui, Id::unique("tag"));
                let out = TreeRow::new(2, "MCID 2")
                    .detail(Some("p1"))
                    .content_ref()
                    .show(ui, Id::unique("ref"));
                if out.toggled {
                    state.toggled += 1;
                }
                if out.row.clicked() || out.row.double_clicked() || out.row.has_focus() {
                    state.clicked += 1;
                }
                if tag.row.clicked() {
                    state.selected = true;
                }
            });
        },
        state,
    );
    harness.run();

    // The label and the page are text; the row is no button, so it is not focusable either.
    for text in ["MCID 2", "p1"] {
        let _label = harness.get_by_role_and_label(Role::Label, text);
    }
    assert!(harness.query_by_role_and_label(Role::Button, "MCID 2").is_none());
    // Clicks, double clicks and Tab do nothing to it.
    let at = harness
        .get_by_role_and_label(Role::Label, "MCID 2")
        .rect()
        .center();
    harness.hover_at(at);
    harness.run();
    for _ in 0..2 {
        harness.drag_at(at);
        harness.drop_at(at);
        harness.run();
    }
    harness.key_press(Key::Tab);
    harness.run();
    assert_eq!((harness.state().toggled, harness.state().clicked), (0, 0));
    // The tag row beside it is still a button and still selects.
    harness.get_by_role_and_label(Role::Button, "Figure").click();
    harness.run();
    assert!(harness.state().selected);
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "Figure")));
    // Resting on the row explains it after 400 ms.
    harness.remove_cursor();
    harness.run();
    harness.hover_at(at);
    harness.run_steps(4);
    let tip = pdfxray_app::components::tree_row::CONTENT_REF_TIP;
    let _tooltip = harness.get_by_role_and_label(Role::Label, tip);
    // Smoke: the row is still there, with its text.
    let _label = harness.get_by_role_and_label(Role::Label, "MCID 2");
}
