//! Sections.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_section_collapses_and_remembers_it_per_key() {
    let mut harness = builder(vec2(360.0, 200.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            section::show(ui, "components-test-a", "Geometry", |ui| {
                ui.label("inside a");
            });
            section::show(ui, "components-test-b", "Font", |ui| {
                ui.label("inside b");
            });
        });
    });
    harness.run();
    // Sections start open.
    assert!(harness.query_by_label("inside a").is_some());
    assert!(harness.query_by_label("inside b").is_some());
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "Geometry")));

    harness.get_by_role_and_label(Role::Button, "Geometry").click();
    harness.run();

    // The one that was clicked is closed; the other is untouched.
    assert!(harness.query_by_label("inside a").is_none());
    assert!(harness.query_by_label("inside b").is_some());
    assert!(!is_on(&harness.get_by_role_and_label(Role::Button, "Geometry")));
    assert!(harness.ctx.data_mut(|d| {
        !*d.get_persisted_mut_or(Id::unique(("pdfxray-section", "components-test-a")), true)
    }));
    // And it opens again.
    harness.get_by_role_and_label(Role::Button, "Geometry").click();
    harness.run();
    assert!(harness.query_by_label("inside a").is_some());
}

#[test]
fn a_section_that_starts_collapsed_stays_as_the_user_leaves_it() {
    let mut harness = builder(vec2(360.0, 200.0)).build_ui(|ui| {
        on_surface(ui, |ui| {
            section::show_with(ui, "components-test-quiet", "Image", false, |ui| {
                ui.label("inside quiet");
            });
        });
    });
    harness.run();
    assert!(harness.query_by_label("inside quiet").is_none());
    assert!(!is_on(&harness.get_by_role_and_label(Role::Button, "Image")));

    harness.get_by_role_and_label(Role::Button, "Image").click();
    harness.run();
    assert!(harness.query_by_label("inside quiet").is_some());
    assert!(is_on(&harness.get_by_role_and_label(Role::Button, "Image")));
    // Smoke: only the click is remembered, not the default.
    assert!(harness.ctx.data_mut(|d| {
        d.get_persisted::<bool>(Id::unique(("pdfxray-section", "components-test-quiet"))) == Some(true)
    }));
}

#[test]
fn a_tree_row_swaps_its_detail_for_the_hot_one_when_selected_or_pointed_at() {
    let mut harness = builder(vec2(320.0, 60.0)).build_ui_state(
        |ui, selected: &mut bool| {
            on_surface(ui, |ui| {
                TreeRow::new(0, "H1")
                    .detail(Some("Annual Report"))
                    .detail_when_hot(Some("12 0 R"))
                    .selected(*selected)
                    .show(ui, Id::unique("hot-row"));
            });
        },
        false,
    );
    harness.run();
    // At rest: the preview, no reference.
    assert!(harness.query_by_label("Annual Report").is_some());
    assert!(harness.query_by_label("12 0 R").is_none());

    // Pointing at the row shows the reference instead.
    let at = harness.get_by_role_and_label(Role::Button, "H1").rect().center();
    harness.hover_at(at);
    harness.run();
    assert!(harness.query_by_label("12 0 R").is_some());
    assert!(harness.query_by_label("Annual Report").is_none());

    // Selected, it keeps the reference with the pointer gone.
    harness.remove_cursor();
    harness.run();
    assert!(harness.query_by_label("Annual Report").is_some());
    *harness.state_mut() = true;
    harness.run();
    assert!(harness.query_by_label("12 0 R").is_some());
}

#[test]
fn a_section_has_no_divider_and_14_px_of_padding_above_and_below() {
    let top = std::rc::Rc::new(std::cell::Cell::new(0.0_f32));
    let seen = top.clone();
    let mut harness = builder(vec2(360.0, 200.0)).build_ui(move |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, tokens::PANEL);
        ui.spacing_mut().item_spacing.y = 0.0;
        seen.set(ui.cursor().min.y);
        section::show(ui, "v1-padding-a", "Geometry", |ui| {
            ui.label("row a");
        });
        section::show(ui, "v1-padding-b", "Font", |ui| {
            ui.label("row b");
        });
    });
    harness.run();
    let shapes = shapes_of(&harness);

    // No horizontal line anywhere.
    assert!(
        !shapes.iter().any(
            |shape| matches!(shape, egui::Shape::LineSegment { points, .. } if points[0].y == points[1].y)
        ),
        "{shapes:?}"
    );
    // The first header starts 14 px below the top, and the next section 14 + 14 below the end of
    // the first one's body. The target of a header reaches 4 px into the padding, so it starts
    // 10 px below the top and the second one 24 px below the body.
    let first = harness.get_by_role_and_label(Role::Button, "Geometry").rect();
    assert_eq!(first.top() - top.get(), 14.0 - 4.0);
    let row_a = harness.get_by_label("row a").rect();
    let second = harness.get_by_role_and_label(Role::Button, "Font").rect();
    assert!(
        (second.top() - row_a.bottom() - (28.0 - 4.0)).abs() < 2.5,
        "{second:?} after {row_a:?}"
    );
    // The header is the tracked 10 px one, upper-cased, in text-3.
    assert!(shapes.iter().any(|shape| matches!(shape,
        egui::Shape::Text(t) if t.galley.text() == "GEOMETRY"
            && t.galley.job.sections[0].format.extra_letter_spacing == 0.9)));
    // Smoke: it still collapses on click.
    harness.get_by_role_and_label(Role::Button, "Geometry").click();
    harness.run();
    assert!(harness.query_by_label("row a").is_none());
}
