//! Property rows.

use egui::accesskit::Live;
use egui::{Event, Modifiers, Shape, StrokeKind};
use pdfxray_app::components::{context_menu, focus, long_value, prop_row, toast};
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
                if linked.followed {
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

    // A link is a link, and clicking its text is reported.
    assert!(!*harness.state());
    harness.get_by_role_and_label(Role::Label, "12 0 R").click();
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

// ---- copy without a pointer ("2.13 Spec") ----

/// What the last frame put on the clipboard.
fn copied<S>(harness: &Harness<'_, S>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

/// Presses `key` with `modifiers` held in one frame, so what it copies is in that frame's output.
fn press<S>(harness: &mut Harness<'_, S>, modifiers: Modifiers, key: Key) {
    harness.event(Event::Key {
        key,
        pressed: true,
        modifiers,
        repeat: false,
        physical_key: None,
    });
    harness.step();
}

/// The colour the text shape with exactly `text` is painted in.
fn ink_of(shapes: &[Shape], text: &str) -> Option<egui::Color32> {
    shapes.iter().find_map(|shape| match shape {
        Shape::Text(t) if t.galley.text() == text => Some(t.fallback_color),
        _ => None,
    })
}

/// The node the keyboard is on, by its name.
fn focused_name<S>(harness: &Harness<'_, S>) -> Option<String> {
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "the method path is not general enough over the node's lifetime"
    )]
    harness
        .query_by(|node| node.is_focused())
        .and_then(|node| node.accesskit_node().label())
}

/// Rows as an inspector draws them, with the value menu and the toast.
fn rows(ui: &mut egui::Ui, followed: &mut u32) {
    on_surface(ui, |ui| {
        PropRow::new("Width", "70.56 mm").copy_text("200 pt").show(ui);
        if PropRow::new("Parent", "3 0 R").link(true).show(ui).followed {
            *followed += 1;
        }
        PropRow::new("Size", "1 KB").show(ui);
    });
    if let Some(target) = prop_row::take_context(ui.ctx()) {
        context_menu::attach_value(&target, None);
    }
    toast::show(ui.ctx());
}

fn rows_harness() -> Harness<'static, u32> {
    let mut harness = builder(vec2(360.0, 260.0)).build_ui_state(rows, 0);
    harness.run();
    harness.remove_cursor();
    harness.run();
    harness
}

/// Tabs until the row `name` has the keyboard.
fn tab_to<S>(harness: &mut Harness<'_, S>, name: &str) {
    for _ in 0..8 {
        if focused_name(harness).as_deref() == Some(name) {
            return;
        }
        harness.key_press(Key::Tab);
        harness.run_steps(2);
    }
    panic!("Tab never reached {name}: on {:?}", focused_name(harness));
}

#[test]
fn the_row_is_the_tab_stop_and_looks_hovered_with_an_inset_ring_and_its_icon() {
    let mut harness = rows_harness();
    let row = harness.get_by_label("Width: 70.56 mm").rect();
    // Smoke: at rest, no icon and no ring.
    assert!(harness.query_by_label("Copy Width").is_none());

    harness.key_press(Key::Tab);
    harness.run_steps(2);

    assert_eq!(focused_name(&harness).as_deref(), Some("Width: 70.56 mm"));
    assert_eq!(
        harness.get_by_label("Width: 70.56 mm").accesskit_node().role(),
        Role::ListItem
    );
    // The icon shows without the pointer, but it is no stop of its own.
    assert!(harness.query_by_label("Copy Width").is_some());
    let shapes = shapes_of(&harness);
    let wash = rect_shapes_at(&shapes, row);
    assert!(
        wash.iter().any(|r| r.fill == tokens::NEUTRAL_HOVER),
        "the hover wash: {wash:?}"
    );
    let (ring, _) = focus::row_ring_rect(row, tokens::RADIUS_SM);
    let rings: Vec<_> = rect_shapes_at(&shapes, ring)
        .into_iter()
        .filter(|r| r.stroke.color == tokens::ACCENT)
        .collect();
    assert_eq!(rings.len(), 1, "one inset ring at {ring:?}");
    assert_eq!(
        (rings[0].stroke.width, rings[0].stroke_kind),
        (2.0, StrokeKind::Inside)
    );

    harness.key_press(Key::Tab);
    harness.run_steps(2);
    assert_eq!(focused_name(&harness).as_deref(), Some("Parent: 3 0 R"));
    // Smoke: the icon of a row the keyboard left is hidden again.
    assert!(harness.query_by_label("Copy Width").is_none());
}

#[test]
fn command_c_copies_the_exact_value_and_shift_command_c_the_key_with_it() {
    let mut harness = rows_harness();
    // Smoke: without the focus, ⌘C copies nothing.
    press(&mut harness, Modifiers::COMMAND, Key::C);
    assert_eq!(copied(&harness), None);

    tab_to(&mut harness, "Width: 70.56 mm");
    press(&mut harness, Modifiers::COMMAND, Key::C);
    assert_eq!(copied(&harness).as_deref(), Some("200 pt"));

    // macOS sends ⌘C as a copy event.
    harness.run_steps(2);
    harness.event(Event::Copy);
    harness.step();
    assert_eq!(copied(&harness).as_deref(), Some("200 pt"));

    harness.run_steps(2);
    press(&mut harness, Modifiers::COMMAND | Modifiers::SHIFT, Key::C);
    assert_eq!(copied(&harness).as_deref(), Some("Width: 200 pt"));
}

#[test]
fn enter_on_a_link_row_follows_the_link_and_on_a_plain_row_does_nothing() {
    let mut harness = rows_harness();

    tab_to(&mut harness, "Size: 1 KB");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(*harness.state(), 0);

    tab_to(&mut harness, "Parent: 3 0 R");
    assert_eq!(
        harness.get_by_label("Parent: 3 0 R").accesskit_node().role(),
        Role::Link
    );
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(*harness.state(), 1);
    // Smoke: the row keeps the focus.
    assert_eq!(focused_name(&harness).as_deref(), Some("Parent: 3 0 R"));
}

/// The menu's lines: name and whether each one can be picked.
fn menu_lines<S>(harness: &Harness<'_, S>) -> Vec<(String, bool)> {
    harness
        .query_all_by_role(Role::MenuItem)
        .map(|n| {
            (
                n.accesskit_node().label().unwrap_or_default(),
                !n.accesskit_node().is_disabled(),
            )
        })
        .collect()
}

#[test]
fn shift_f10_opens_the_value_menu_on_its_first_line_with_follow_link_off_for_a_plain_value() {
    let mut harness = rows_harness();
    tab_to(&mut harness, "Size: 1 KB");
    // Smoke: no menu before the key.
    assert_eq!(menu_lines(&harness).len(), 0);

    press(&mut harness, Modifiers::SHIFT, Key::F10);
    harness.run_steps(2);

    assert_eq!(
        menu_lines(&harness),
        [
            (context_menu::COPY_VALUE.to_owned(), true),
            (context_menu::COPY_KEY_AND_VALUE.to_owned(), true),
            (context_menu::FOLLOW_LINK.to_owned(), false),
        ]
    );
    assert_eq!(focused_name(&harness).as_deref(), Some(context_menu::COPY_VALUE));
    // Its keys, quiet beside the names.
    for key in ["⌘C", "⇧⌘C", "↩"] {
        assert!(harness.query_by_label(key).is_some(), "{key}");
    }
    let shapes = shapes_of(&harness);
    assert_eq!(
        ink_of(&shapes, context_menu::FOLLOW_LINK),
        Some(tokens::TEXT_3),
        "Follow link is dimmed"
    );
    assert_eq!(ink_of(&shapes, context_menu::COPY_VALUE), Some(tokens::TEXT));

    press(&mut harness, Modifiers::NONE, Key::Enter);
    assert_eq!(copied(&harness).as_deref(), Some("1 KB"));
}

#[test]
fn a_right_click_or_the_menu_key_opens_the_value_menu_and_follow_link_follows() {
    let mut harness = rows_harness();
    // A right-click on the key: anywhere on the row.
    harness
        .get_by_role_and_label(Role::Label, "Parent")
        .click_secondary();
    harness.run_steps(3);

    assert_eq!(
        menu_lines(&harness),
        [
            (context_menu::COPY_VALUE.to_owned(), true),
            (context_menu::COPY_KEY_AND_VALUE.to_owned(), true),
            (context_menu::FOLLOW_LINK.to_owned(), true),
        ]
    );
    harness
        .get_by_role_and_label(Role::MenuItem, context_menu::FOLLOW_LINK)
        .click();
    harness.run_steps(3);
    assert_eq!(*harness.state(), 1);
    assert_eq!(menu_lines(&harness).len(), 0, "the menu closed");

    // The context-menu key opens it too, and Copy key and value copies both.
    tab_to(&mut harness, "Width: 70.56 mm");
    harness.key_press(Key::Menu);
    harness.run_steps(3);
    harness
        .get_by_role_and_label(Role::MenuItem, context_menu::COPY_KEY_AND_VALUE)
        .click();
    harness.step();
    assert_eq!(copied(&harness).as_deref(), Some("Width: 200 pt"));
    // Smoke: nothing was followed again.
    assert_eq!(*harness.state(), 1);
}

#[test]
fn a_copy_shows_the_toast_for_1_6_s_and_announces_it() {
    let mut harness = rows_harness();
    tab_to(&mut harness, "Parent: 3 0 R");
    // Smoke: no toast before a copy.
    assert!(harness.query_by_role(Role::Status).is_none());

    press(&mut harness, Modifiers::COMMAND, Key::C);
    // The toast lays itself out on the frame of the copy and is read from the next one.
    harness.step();

    let toast = harness.get_by_role(Role::Status);
    assert_eq!(toast.accesskit_node().label().as_deref(), Some("Copied 3 0 R"));
    assert_eq!(toast.accesskit_node().live(), Live::Polite);
    assert!(harness.query_by_label("Copied").is_some(), "the word in the pill");
    // It sits at the foot of the panel the copy came from.
    assert!(toast.rect().max.y > 200.0, "{:?}", toast.rect());

    // Each step is a quarter of a second: 1.5 s after the copy it is there, at 1.75 s gone.
    harness.run_steps(5);
    assert!(harness.query_by_role(Role::Status).is_some(), "1.5 s later");
    harness.run_steps(1);
    assert!(harness.query_by_role(Role::Status).is_none(), "1.75 s later");
    assert_eq!(toast::TOAST_FOR, 1.6);
}

// ---- long values ----

fn widths(n: usize) -> String {
    let items: Vec<String> = (0..n).map(|i| (100 + i).to_string()).collect();
    format!("[{}]", items.join(" "))
}

fn long_rows(ui: &mut egui::Ui) {
    on_surface(ui, |ui| {
        PropRow::new("/Widths", &widths(224)).show(ui);
        PropRow::new("Size", "1 KB").show(ui);
    });
}

/// The painted text shapes whose text starts with a digit or a bracket: the value's lines, top to
/// bottom.
fn value_lines<S>(harness: &Harness<'_, S>) -> Vec<(String, egui::Pos2)> {
    let mut lines: Vec<(String, egui::Pos2)> = shapes_of(harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Text(t)
                if t.galley.text().starts_with(['[', '1', '2', '3'])
                    && !t.galley.text().chars().any(char::is_alphabetic) =>
            {
                Some((t.galley.text().to_owned(), t.pos))
            }
            _ => None,
        })
        .collect();
    lines.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    lines
}

#[test]
fn a_long_array_wraps_under_its_first_item_and_folds_past_three_lines() {
    let mut harness = builder(vec2(360.0, 260.0)).build_ui(long_rows);
    harness.run();

    let lines = value_lines(&harness);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].0.starts_with("[100 101"), "{lines:?}");
    // The hanging indent: the lines after the first start under its first item, after `[`.
    let bracket = harness
        .ctx
        .fonts_mut(|f| f.glyph_width(&fonts::mono(tokens::mono::BODY), '['));
    assert!((lines[1].1.x - (lines[0].1.x + bracket)).abs() < 0.5, "{lines:?}");
    assert_eq!(lines[1].1.x, lines[2].1.x);
    // The fold: its button, the closer and the count.
    let shown: usize = lines
        .iter()
        .map(|(text, _)| text.split_whitespace().count())
        .sum();
    let more = format!("{} more values", 224 - shown);
    let button = harness.get_by_role_and_label(Role::Button, &more);
    let shapes = shapes_of(&harness);
    let count = shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Text(t) if t.galley.text() == "224 numbers" => Some(t.clone()),
            _ => None,
        })
        .expect("the count");
    assert_eq!(count.fallback_color, tokens::TEXT_3);
    assert!(
        shapes
            .iter()
            .any(|shape| matches!(shape, Shape::Text(t) if t.galley.text() == "]")),
        "the closing bracket stays"
    );
    let plate = rect_shapes_at(&shapes, button.rect());
    assert!(
        plate.iter().any(|r| r.fill == tokens::SURFACE_2
            && r.stroke.color == tokens::FOLD_HAIRLINE
            && r.corner_radius.nw == 5),
        "{plate:?}"
    );
    // Smoke: the button sits on the indent, under the items.
    assert!((button.rect().min.x - lines[1].1.x).abs() < 0.5);

    button.click();
    harness.run_steps(2);

    let open = value_lines(&harness);
    assert!(open.len() > 3, "{open:?}");
    assert!(open.last().unwrap().0.ends_with("323]"), "{open:?}");
    assert!(harness.query_by_role_and_label(Role::Button, &more).is_none());
}

#[test]
fn enter_unfolds_a_long_array_and_copy_value_copies_all_of_it() {
    let mut harness = builder(vec2(360.0, 260.0)).build_ui(long_rows);
    harness.run();
    let value = widths(224);
    let name = harness
        .query_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .find(|label| label.ends_with("more values"))
        .expect("the fold button");

    harness.get_by_role_and_label(Role::Button, &name).focus();
    harness.run_steps(2);
    harness.key_press(Key::Enter);
    harness.run_steps(2);

    assert!(harness.query_by_role_and_label(Role::Button, &name).is_none());
    assert!(value_lines(&harness).len() > 3);
    // The row keeps the keyboard, and ⌘C copies the whole array, not what was shown.
    let row = format!("/Widths: {}…", &value[..80]);
    assert_eq!(focused_name(&harness).as_deref(), Some(row.as_str()));
    press(&mut harness, Modifiers::COMMAND, Key::C);
    assert_eq!(copied(&harness).as_deref(), Some(value.as_str()));
    // Smoke: the value as a whole is what assistive tech reads.
    assert!(harness.query_by_label(&value).is_some());
    assert_eq!(long_value::kind_of(&value), long_value::Kind::Array);
}

// ---- the "2.13 Spec" board, as rows of an inspector ----

/// The widths of a font: 224 numbers, as on the board.
fn font_widths() -> String {
    let pattern = [
        278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333,
    ];
    let items: Vec<String> = pattern
        .iter()
        .cycle()
        .take(224)
        .map(ToString::to_string)
        .collect();
    format!("[{}]", items.join(" "))
}

fn where_rows(ui: &mut egui::Ui, _: &mut ()) {
    on_surface(ui, |ui| {
        PropRow::new("Element", "/Figure 22 0 R").link(true).show(ui);
        PropRow::new("Content", "MCID 9 · Do /Im1 · page 1").show(ui);
        PropRow::new("veraPDF path", "…/SEFigure[0]").show(ui);
        PropRow::new("/Widths", &font_widths()).show(ui);
    });
    if let Some(target) = prop_row::take_context(ui.ctx()) {
        context_menu::attach_value(&target, None);
    }
    toast::show(ui.ctx());
}

#[test]
fn snapshot_copy_without_a_pointer_and_a_folded_array() {
    let _gpu = common::gpu_lock();
    let mut harness = builder(vec2(440.0, 300.0)).wgpu().build_ui_state(where_rows, ());
    harness.run();
    harness.remove_cursor();
    harness.run();
    tab_to(&mut harness, "Content: MCID 9 · Do /Im1 · page 1");
    press(&mut harness, Modifiers::SHIFT, Key::F10);
    harness.run_steps(2);

    harness.snapshot("prop_rows_value_menu");

    press(&mut harness, Modifiers::NONE, Key::Escape);
    harness.run_steps(2);
    tab_to(&mut harness, "Content: MCID 9 · Do /Im1 · page 1");
    press(&mut harness, Modifiers::COMMAND, Key::C);
    harness.run_steps(2);

    harness.snapshot("prop_rows_copied_toast");
}
