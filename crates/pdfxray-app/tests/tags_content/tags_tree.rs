//! The Tags tree rows: warnings, previews, the filter, mapped roles, right-click actions and the artifacts entry.

use pretty_assertions::assert_eq;

use super::*;

#[test]
fn a_figure_without_alt_is_warned_about_in_the_tree_the_hit_stack_and_the_inspector() {
    let source = std::fs::read(common::fixture_path("struct-rich")).unwrap();
    // `/Alt` becomes `/Alx`: same length, so every offset in the file still holds.
    let patched: Vec<u8> = String::from_utf8_lossy(&source)
        .replace("/Alt (", "/Alx (")
        .into_bytes();
    let path = std::env::temp_dir().join(format!("pdfxray-no-alt-{}.pdf", std::process::id()));
    std::fs::write(&path, &patched).unwrap();
    let mut harness = app();
    harness.state_mut().state.picker = Box::new({
        let path = path.clone();
        move || Some(path.clone())
    });
    harness.get_by_label("Open file…").click();
    harness.run();
    common::wait_for_open(&mut harness);
    wait_for_render(&mut harness);
    std::fs::remove_file(&path).ok();

    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run();
    let figure = NodeId::StructElem(ObjRef::new(14, 0));
    assert!(harness.state().state.tag_index().unwrap().lacks_alt(&figure));
    assert!(texts_in(&harness, Region::Navigator, Role::Label).contains(&"Alt".to_owned()));
    // Filtering by "alt missing" finds it.
    harness.state_mut().state.nav.filter = "alt missing".to_owned();
    harness.run();
    assert_eq!(
        texts_in(&harness, Region::Navigator, Role::Label)
            .iter()
            .filter(|t| *t == "Alt")
            .count(),
        1
    );
    harness.state_mut().state.nav.filter.clear();
    harness.state_mut().state.select(figure.clone());
    harness.run_steps(3);
    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    let alt = labels.iter().position(|t| t == "/Alt").expect("an /Alt row");
    assert_eq!(labels[alt + 1], "missing");
    // The chips under the title and the card above the sections say it too.
    let chip = labels.iter().position(|t| t == "no Alt").expect("a chip");
    let card = labels
        .iter()
        .position(|t| t == "No alternative text")
        .expect("a problem card");
    assert!(chip < card && card < alt, "{labels:?}");
    // The same warning in the hit stack of a click on it.
    harness
        .state_mut()
        .state
        .click_page_point(Point::new(60.0, 270.0));
    harness.state_mut().state.select(figure);
    harness.run_steps(3);
    let lines = pdfxray_app::drawer::hit_stack::lines(&harness.state().state);
    let figure_line = lines
        .iter()
        .find(|l| l.label == "Figure")
        .expect("the Figure row");
    assert_eq!(figure_line.detail, "14 0 R · /Alt missing");
    assert!(figure_line.warn);
}

/// Rests the pointer on `at` long enough for a tooltip.
fn rest_on(harness: &mut App, at: egui::Pos2) {
    harness.hover_at(at);
    harness.step();
    harness.remove_cursor();
    harness.hover_at(at);
    harness.step();
    harness.run_steps(3);
}

/// A label that is an object reference (`14 0 R`), as the tree shows it on a row.
fn is_reference(text: &str) -> bool {
    text.strip_suffix(" 0 R")
        .is_some_and(|num| !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()))
}

#[test]
fn a_tags_row_shows_a_text_preview_and_its_reference_only_when_hovered_or_selected() {
    let mut harness = expanded_tags();
    let doc = Document::open(common::fixture_path("struct-rich")).unwrap();
    let h1 = element("struct-rich", "H1");
    let preview = doc.text_preview(&h1, 28).expect("the H1 has text");

    // Quiet by default: the preview is there, no row shows an object reference.
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(labels.contains(&preview), "{preview} in {labels:?}");
    assert_eq!(labels.iter().filter(|t| is_reference(t)).count(), 0, "{labels:?}");

    // Pointing at the row swaps the preview for the reference.
    let at = the(&harness, Region::Navigator, Role::Button, "H1")
        .rect()
        .center();
    harness.hover_at(at);
    harness.run_steps(2);
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(labels.contains(&"12 0 R".to_owned()), "{labels:?}");
    assert!(!labels.contains(&preview), "{labels:?}");

    // The selected row keeps it while the pointer is elsewhere.
    harness.remove_cursor();
    harness.state_mut().state.select(element("struct-rich", "Figure"));
    harness.run_steps(3);
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(labels.contains(&"14 0 R".to_owned()), "{labels:?}");
    // Smoke: only the selected row shows one.
    assert_eq!(labels.iter().filter(|t| is_reference(t)).count(), 1, "{labels:?}");
}

#[test]
fn the_filter_still_finds_a_tag_by_its_object_number_and_its_text() {
    let mut harness = opened("struct-rich", "Tags");
    harness.state_mut().state.nav.filter = "14 0 R".to_owned();
    harness.run_steps(2);
    assert_eq!(
        named(&harness, Region::Navigator, Role::Button, "Figure").len(),
        1
    );
    assert!(named(&harness, Region::Navigator, Role::Button, "H1").is_empty());
    // The preview is searchable too: the Figure's is its /Alt, "A pale blue rectangle".
    harness.state_mut().state.nav.filter = "pale".to_owned();
    harness.run_steps(2);
    assert_eq!(
        named(&harness, Region::Navigator, Role::Button, "Figure").len(),
        1
    );
    // Smoke: a word nobody has matches nothing.
    harness.state_mut().state.nav.filter = "zebra".to_owned();
    harness.run_steps(2);
    assert!(named(&harness, Region::Navigator, Role::Button, "Figure").is_empty());
}

#[test]
fn a_mapped_role_says_what_it_stands_for_on_hover_and_there_is_no_role_map_footer() {
    let mut harness = opened("pdf2-namespaces", "Tags");
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run_steps(2);

    // The card under the tree is gone.
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(!labels.contains(&"RoleMap".to_owned()), "{labels:?}");
    assert!(
        !labels.iter().any(|t| t.starts_with("No custom types")),
        "{labels:?}"
    );
    assert!(!has_label(&harness, "Chapter → Sect"));

    // The row of the mapped type shows the type alone and explains itself on hover.
    let chapter = element("pdf2-namespaces", "Chapter → Sect");
    harness.state_mut().state.select(chapter);
    harness.run_steps(3);
    let at = the(&harness, Region::Navigator, Role::Button, "Chapter")
        .rect()
        .center();
    rest_on(&mut harness, at);
    let index = harness.state().state.tag_index().unwrap();
    let entry = index
        .role_map
        .iter()
        .find(|e| e.role == "Chapter")
        .expect("Chapter is in the role map");
    let expected = match &entry.namespace {
        Some(namespace) => format!("Chapter → Sect ({namespace})"),
        None => "Chapter → Sect".to_owned(),
    };
    assert!(has_label(&harness, &expected), "{expected}");
    // Smoke: it says which standard type it maps to.
    assert!(expected.starts_with("Chapter → Sect"));
}

#[test]
fn right_click_on_a_tags_row_lists_the_actions_and_show_in_tree_and_object_work() {
    let mut harness = opened("struct-rich", "Tags");
    let figure = element("struct-rich", "Figure");
    harness.state_mut().state.select(NodeId::Document);
    harness.state_mut().state.select(figure.clone());
    harness.run_steps(3);
    // Pointing elsewhere first, so the menu is for the row that was right-clicked.
    the(&harness, Region::Navigator, Role::Button, "Figure").click_secondary();
    harness.run_steps(3);

    let items: Vec<String> = harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(
        items,
        [
            "Isolate",
            "Hide",
            "Show in tree",
            "Object",
            "Raw bytes",
            "Copy id",
            "Copy JSON"
        ]
    );

    harness
        .get_by_role_and_label(Role::MenuItem, "Show in tree")
        .click();
    harness.run_steps(3);
    assert_eq!(selected_rows(&harness), ["Figure"]);
    assert_eq!(harness.state().state.selection.primary, Some(figure));

    the(&harness, Region::Navigator, Role::Button, "Figure").click_secondary();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::MenuItem, "Object").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(ObjRef::new(14, 0)))
    );
    // Smoke: the Objects entry is showing.
    assert_eq!(harness.state().state.nav.rail, pdfxray_app::RailItem::Objects);
}

#[test]
fn right_click_on_a_marked_content_row_does_nothing() {
    let mut harness = expanded_tags();
    mcid_row(&harness, "MCID 2").click_secondary();
    harness.run_steps(3);
    assert_eq!(harness.query_all_by_role(Role::MenuItem).count(), 0);
}

#[test]
fn right_click_on_a_content_row_lists_the_actions_of_the_operator() {
    let mut harness = opened("minimal-one-page", "Content");
    the(&harness, Region::Navigator, Role::Button, "Tj").click_secondary();
    harness.run_steps(3);
    let items: Vec<String> = harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(items, ["Isolate", "Hide", "Show in tree", "Copy id", "Copy JSON"]);

    harness.get_by_role_and_label(Role::MenuItem, "Isolate").click();
    harness.run_steps(3);
    // Smoke: the menu isolated the row it was opened on.
    assert!(harness.state().state.canvas.view.isolation.is_some());
}

#[test]
fn the_artifacts_entry_shows_only_artifacts_with_the_artifact_xray_on() {
    let mut harness = opened("artifacts", "Artifacts");
    wait_for_render(&mut harness);

    let state = &harness.state().state;
    assert!(state.canvas.view.xray);
    let rows = texts_in(&harness, Region::Navigator, Role::Label);
    assert!(
        rows.contains(&"Header".to_owned())
            && rows.contains(&"Layout".to_owned())
            && rows.contains(&"Footer".to_owned())
    );
    // The tagged paragraph is not listed.
    assert!(!rows.iter().any(|t| t.contains("Body text")), "{rows:?}");
    // Smoke: the unfiltered Content entry does list it.
    click_rail(&mut harness, "Content");
    assert!(
        texts_in(&harness, Region::Navigator, Role::Label)
            .iter()
            .any(|t| t.contains("Body text"))
    );
}
