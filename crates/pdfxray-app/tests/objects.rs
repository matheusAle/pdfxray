//! The Objects entry: the object view (decoded and raw), history, the References drawer, the
//! diagnostic cards and the revision timeline.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{
    Region, app, buttons_in, click_rail, click_title_chip, open_fixture, texts_in, the, title_chip,
};
use egui::{Key, Modifiers, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::{DrawerTab, ObjectMode, PdfxrayApp};
use pdfxray_core::{Document, NodeId, ObjRef, explain};
use pretty_assertions::assert_eq;

fn obj(n: u32) -> NodeId {
    NodeId::Object(ObjRef::new(n, 0))
}

/// Opens `fixture`, switches to the Objects entry and selects object `n` in the navigator.
fn objects_at(fixture: &str, n: u32) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    click_rail(&mut harness, "Objects");
    select(&mut harness, n);
    harness
}

fn select(harness: &mut Harness<'_, PdfxrayApp>, n: u32) {
    the(harness, Region::Navigator, Role::Button, &format!("{n} 0 obj")).click();
    harness.run_steps(3);
}

fn center(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    texts_in(harness, Region::Center, Role::Label)
}

/// The title line of the inspector: the label right before the id line.
fn title_of_inspector(harness: &Harness<'_, PdfxrayApp>) -> String {
    let labels = texts_in(harness, Region::Inspector, Role::Label);
    let at = labels
        .iter()
        .position(|t| t.starts_with("obj:"))
        .expect("an id line");
    labels[at - 1].clone()
}

fn doc<'h>(harness: &'h Harness<'_, PdfxrayApp>) -> &'h Document {
    &harness.state().state.document.as_ref().unwrap().doc
}

#[test]
fn the_objects_entry_shows_the_object_view_and_the_references_drawer() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");

    click_rail(&mut harness, "Objects");

    let state = &harness.state().state;
    assert_eq!(state.drawer.tab, DrawerTab::References);
    assert!(harness.query_by_label("Page 1 canvas").is_none());
    assert!(center(&harness).contains(&"Pick an object in the navigator to see it here.".to_owned()));
    let tabs: Vec<String> = texts_in(&harness, Region::Drawer, Role::Button);
    assert_eq!(&tabs[..3], ["References", "Diagnostics", "Hit stack"]);
    let navigator = texts_in(&harness, Region::Navigator, Role::Label);
    for text in [
        "Objects",
        "5 objects · 1 rev",
        "DOCUMENT",
        "RESOURCES · PAGE 1",
        "ALL OBJECTS",
    ] {
        assert!(navigator.contains(&text.to_owned()), "{text} in {navigator:?}");
    }
}

#[test]
fn a_content_stream_shows_its_instructions_with_a_plain_language_note_each() {
    let harness = objects_at("text-state", 4);

    let center = center(&harness);

    assert!(center.contains(&"4 0 obj".to_owned()), "{center:?}");
    assert!(
        center.contains(&"Page 1's drawing instructions · 67 operators".to_owned()),
        "{center:?}"
    );
    assert!(
        center.contains(&"FlateDecode · 315 B → 541 B".to_owned()),
        "{center:?}"
    );
    assert!(center.contains(&"Page 1 /Contents".to_owned()), "{center:?}");
    // The first instructions, as numbered lines: operands, operator, and the note.
    let captured = doc(&harness).captured(0).unwrap();
    let first: Vec<(String, String)> = captured
        .ops
        .iter()
        .take(4)
        .map(|op| (op.operator.clone(), explain::operator(&op.operator, &op.operands)))
        .collect();
    assert_eq!(
        first,
        [
            ("BT".to_owned(), "begin text".to_owned()),
            ("Tf".to_owned(), "font F1 at 12 pt".to_owned()),
            ("Td".to_owned(), "move to x 20, y 270".to_owned()),
            (
                "Tj".to_owned(),
                explain::operator_with_text("Tj", &["(Plain Tj)".to_owned()], Some("Plain Tj"))
            )
        ]
    );
    let at = center
        .iter()
        .position(|t| t == "#0")
        .expect("the first instruction");
    assert_eq!(center[at..at + 4], ["#0", "BT", "begin text", "#1"]);
    assert!(center.contains(&"font F1 at 12 pt".to_owned()));
    assert!(center.contains(&"move to x 20, y 270".to_owned()));
    assert!(center.contains(&"draw “Plain Tj”".to_owned()), "{center:?}");
    // Smoke: the dictionary is there too, with its line numbers.
    assert!(center.contains(&"/Filter ".to_owned()) || center.iter().any(|t| t.starts_with("/Filter")));
}

#[test]
fn the_explain_switch_hides_and_shows_the_notes() {
    let mut harness = objects_at("text-state", 4);
    assert!(harness.state().state.objects.explain);
    assert!(center(&harness).contains(&"begin text".to_owned()));

    the(&harness, Region::Center, Role::Switch, "Explain").click();
    harness.run_steps(3);

    assert!(!harness.state().state.objects.explain);
    let hidden = center(&harness);
    assert!(!hidden.contains(&"begin text".to_owned()), "{hidden:?}");
    assert!(!hidden.contains(&"font F1 at 12 pt".to_owned()));
    // The instructions themselves stay.
    assert!(hidden.contains(&"BT".to_owned()));
    assert!(hidden.contains(&"/F1 12".to_owned()), "{hidden:?}");

    the(&harness, Region::Center, Role::Switch, "Explain").click();
    harness.run_steps(3);
    assert!(center(&harness).contains(&"begin text".to_owned()));
}

#[test]
fn clicking_a_reference_navigates_and_back_and_forward_walk_the_history() {
    let mut harness = objects_at("minimal-one-page", 3);
    assert_eq!(harness.state().state.history.visited(), [obj(3)]);

    // The page refers to its content stream; the arrow is on the line of /Contents.
    the(&harness, Region::Center, Role::Link, "4 0 R").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(4)));
    assert_eq!(harness.state().state.history.visited(), [obj(3), obj(4)]);
    assert!(center(&harness).contains(&"4 0 obj".to_owned()));

    // Back, with the arrow of the header.
    the(&harness, Region::Center, Role::Button, "Back").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(3)));
    assert!(center(&harness).contains(&"3 0 obj".to_owned()));
    // Forward, with the key.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::CloseBracket);
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(4)));
    // Back, with the key; the history keeps its order.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::OpenBracket);
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(3)));
    assert_eq!(harness.state().state.history.visited(), [obj(3), obj(4)]);
}

#[test]
fn raw_bytes_show_the_hex_of_the_exact_file_range() {
    let mut harness = objects_at("minimal-one-page", 3);

    the(&harness, Region::Center, Role::Button, "Raw bytes").click();
    harness.run_steps(3);

    assert_eq!(harness.state().state.objects.mode, ObjectMode::Raw);
    let range = doc(&harness).object_range(ObjRef::new(3, 0)).unwrap();
    assert_eq!(range, 121..246);
    let shown = center(&harness);
    // `3 0 obj\n` is the first eight bytes of the object, on the first line, at offset 121.
    assert!(shown.contains(&"00000079".to_owned()), "{shown:?}");
    assert!(
        shown.contains(&"33 20 30 20 6f 62 6a 0a  3c 3c 20 2f 54 79 70 65".to_owned()),
        "{shown:?}"
    );
    assert!(shown.contains(&"3 0 obj.<< /Type".to_owned()), "{shown:?}");
    assert!(
        shown.contains(&"Bytes 121..246 · 125 bytes".to_owned()),
        "{shown:?}"
    );
    // Smoke: the bytes shown are the file's.
    let bytes = &doc(&harness).bytes()[range];
    assert!(bytes.starts_with(b"3 0 obj\n<< /Type"));
}

#[test]
fn a_diagnostic_card_jumps_to_the_bytes_it_is_about() {
    let mut harness = objects_at("broken-xref-offsets", 3);
    let expected = doc(&harness)
        .info()
        .diagnostics
        .iter()
        .find(|d| d.object == Some(ObjRef::new(3, 0)))
        .and_then(pdfxray_core::DiagnosticInfo::byte_range)
        .unwrap();
    assert_eq!(expected, 477..495);

    // The card names the problem in plain words.
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(
        inspector.contains(&"xref-offset-wrong".to_owned()),
        "{inspector:?}"
    );
    assert!(
        inspector
            .iter()
            .any(|t| t.starts_with("The index says an object is at byte 128")),
        "{inspector:?}"
    );
    assert_eq!(harness.state().state.objects.mode, ObjectMode::Decoded);

    the(&harness, Region::Inspector, Role::Button, "Show bytes").click();
    harness.run_steps(3);

    assert_eq!(harness.state().state.objects.mode, ObjectMode::Raw);
    assert_eq!(harness.state().state.objects.highlight, Some(expected.clone()));
    // The xref lies outside the object, so the whole file is listed, and the range is named.
    let shown = center(&harness);
    assert!(shown.contains(&"highlighted 477..495".to_owned()), "{shown:?}");
    assert!(
        shown.iter().any(|t| t.starts_with("Whole file 0..667")),
        "{shown:?}"
    );
    // The marked line (offset 0x1dd) is on screen: scrolling brought it in.
    assert!(shown.contains(&"000001d0".to_owned()), "{shown:?}");
    // Smoke: the help popover answers too.
    the(&harness, Region::Inspector, Role::Button, "What is the xref?").click();
    harness.run_steps(3);
    assert!(
        harness
            .query_by_label_contains("search the file for the object")
            .is_some()
    );
}

#[test]
fn references_list_who_uses_the_object_and_the_resources_it_uses() {
    let mut harness = objects_at("minimal-one-page", 4);

    let refs = doc(&harness).object_refs(ObjRef::new(4, 0));
    assert_eq!(refs.used_by, [(ObjRef::new(3, 0), "/Contents".to_owned())]);
    assert_eq!(refs.uses.len(), 1);
    let drawer = texts_in(&harness, Region::Drawer, Role::Label);
    for text in [
        "Used by",
        "1 object",
        "Page 1",
        "3 0 R",
        "/Contents",
        "Resources it uses",
        "1 name",
        "/F1",
        "5 0 R",
    ] {
        assert!(drawer.contains(&text.to_owned()), "{text} in {drawer:?}");
    }
    assert!(drawer.contains(&refs.uses[0].summary), "{drawer:?}");
    assert_eq!(
        harness
            .state()
            .state
            .object_facts()
            .map(|f| f.used_by.len() + f.uses.len()),
        Some(2)
    );
    let badge = texts_in(&harness, Region::Drawer, Role::Label);
    assert!(badge.contains(&"2".to_owned()), "the tab count: {badge:?}");

    // A reference in the drawer navigates, and the history follows.
    the(&harness, Region::Drawer, Role::Link, "3 0 R").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(3)));
    assert_eq!(harness.state().state.history.visited(), [obj(4), obj(3)]);
}

#[test]
fn the_revision_chip_lists_the_revisions_and_picking_one_shows_the_old_object() {
    let mut harness = objects_at("incremental-update", 4);
    // The newest content is shown as instructions.
    assert!(center(&harness).contains(&"(Hello pdfxray rev 2)".to_owned()));

    let chip = title_chip(&harness);
    assert!(chip.ends_with(" · rev 2/2"), "{chip}");
    click_title_chip(&harness);
    harness.run_steps(3);

    // The timeline: newest first, each with where its xref is and how many objects it wrote.
    let rows: Vec<String> = harness
        .query_all_by_role(Role::Button)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| l == "rev 1" || l == "rev 2")
        .collect();
    assert_eq!(rows, ["rev 2", "rev 1"]);
    let popover: Vec<String> = common::all_label_texts(&harness);
    for text in [
        "latest",
        "xref table @ 826 · 2 objects",
        "xref table @ 410 · 5 objects",
    ] {
        assert!(popover.contains(&text.to_owned()), "{text} in {popover:?}");
    }

    harness.get_by_role_and_label(Role::Button, "rev 1").click();
    harness.run_steps(3);

    assert_eq!(harness.state().state.viewed_revision(), Some(0));
    assert_eq!(
        title_chip(&harness),
        chip.replace("rev 2/2", "rev 1/2 · read-only")
    );
    // The popover closed; the object is the one as written in revision 1, as plain text.
    assert!(harness.query_by_role_and_label(Role::Button, "rev 2").is_none());
    let shown = center(&harness);
    assert!(
        shown.contains(&"BT /F1 18 Tf 20 40 Td (Hello pdfxray) Tj ET".to_owned()),
        "{shown:?}"
    );
    assert!(!shown.iter().any(|t| t.contains("rev 2)")), "{shown:?}");
    // Objects added later do not exist then: 6 0 obj is gone from the list.
    assert!(harness.query_by_role_and_label(Role::Button, "6 0 obj").is_none());
    assert!(
        texts_in(&harness, Region::Navigator, Role::Label).contains(&"5 objects · rev 1 of 2".to_owned())
    );
    // The drawer says references are not tracked per revision.
    assert!(
        texts_in(&harness, Region::Drawer, Role::Label)
            .contains(&"References are only tracked for the newest revision.".to_owned())
    );

    // Back to the newest revision.
    click_title_chip(&harness);
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::Button, "rev 2").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.viewed_revision(), None);
    assert_eq!(title_chip(&harness), chip);
    assert!(harness.query_by_role_and_label(Role::Button, "6 0 obj").is_some());
}

#[test]
fn only_objects_redefined_in_a_later_revision_carry_the_changed_badge() {
    let mut harness = objects_at("incremental-update", 4);

    let rows = harness.state_mut().state.object_rows();

    let changed: Vec<(u32, Option<u32>)> = rows.iter().map(|r| (r.r.num, r.changed_in)).collect();
    // Object 4 was written in revision 1 and again in revision 2; object 6 only exists since 2.
    assert_eq!(
        changed,
        [
            (1, None),
            (2, None),
            (3, None),
            (4, Some(2)),
            (5, None),
            (6, None)
        ]
    );
    // The row shows it, and so does the header of the object view.
    let navigator = texts_in(&harness, Region::Navigator, Role::Label);
    let all = navigator.iter().position(|t| t == "ALL OBJECTS").unwrap();
    assert_eq!(
        navigator[all..].iter().filter(|t| t.as_str() == "rev 2").count(),
        1,
        "{navigator:?}"
    );
    assert!(center(&harness).contains(&"changed in rev 2".to_owned()));
    // Smoke: an object that was not redefined has no such chip.
    select(&mut harness, 3);
    assert!(!center(&harness).contains(&"changed in rev 2".to_owned()));
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(
        inspector.contains(&"rev 1 (original)".to_owned()),
        "{inspector:?}"
    );
}

#[test]
fn the_inspector_describes_the_object_and_right_click_on_its_title_opens_the_raw_bytes() {
    let mut harness = objects_at("text-state", 4);

    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    for text in [
        "Content stream",
        "obj:4:0 · /Contents of page 1",
        "4 0",
        "stream · page drawing",
        "byte 257 · index ok",
        "rev 1 (original)",
    ] {
        assert!(inspector.contains(&text.to_owned()), "{text} in {inspector:?}");
    }
    let crumbs = harness.state().state.object_facts().unwrap().crumbs.clone();
    assert_eq!(crumbs.last(), Some(&ObjRef::new(4, 0)));
    assert!(crumbs.len() >= 2, "{crumbs:?}");

    // The button footer is gone: its actions are on the title's right-click menu.
    let buttons = buttons_in(&harness, Region::Inspector);
    for gone in ["Raw object", "Stream bytes", "Copy JSON"] {
        assert!(!buttons.contains(&gone.to_owned()), "{gone} in {buttons:?}");
    }
    the(&harness, Region::Inspector, Role::Label, "Content stream").click_secondary();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::MenuItem, "Raw bytes").click();
    harness.run_steps(3);

    assert_eq!(harness.state().state.objects.mode, ObjectMode::Raw);
    assert_eq!(harness.state().state.objects.highlight, None);
    // Smoke: the object stays selected.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(ObjRef::new(4, 0)))
    );
}

/// A one-page file whose content stream declares `/Length 10` but holds 20 bytes.
fn wrong_length_file() -> std::path::PathBuf {
    use pdfxray_fixtures::writer::{PdfVersion, PdfWriter, Trailer};
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>",
    );
    w.stream_with_length(4, "", b"0 0 10 10 re f % ok  ", 10);
    w.finish_table(&Trailer::new(1)).unwrap();
    let path = std::env::temp_dir().join("pdfxray-objects-wrong-length.pdf");
    std::fs::write(&path, w.into_output().unwrap().bytes).unwrap();
    path
}

#[test]
fn a_wrong_stream_length_is_marked_on_its_key_line_with_a_button_to_the_bytes() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    let path = wrong_length_file();
    harness.state_mut().state.open_path(&path);
    harness.run();
    common::wait_for_open(&mut harness);
    click_rail(&mut harness, "Objects");
    select(&mut harness, 4);

    let facts = harness.state().state.object_facts().unwrap().clone();
    let diagnostic = facts.length_diagnostic().unwrap().clone();
    assert_eq!(diagnostic.code, "stream-length-wrong");
    let stream = facts.stream.clone().unwrap();
    assert_eq!((stream.declared_length, stream.encoded_len), (Some(10), 21));
    let shown = center(&harness);
    assert!(
        shown.contains(&"Declared 10 B, actual 21 B".to_owned()),
        "{shown:?}"
    );
    // The marker is on the /Length line: its key, then its value, then the marker.
    let key = shown.iter().position(|t| t == "/Length ").expect("the key");
    assert_eq!(shown[key + 1], "10");
    assert_eq!(shown[key + 2], "Declared 10 B, actual 21 B");
    // The inspector says it in plain words, with both figures.
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(
        inspector
            .iter()
            .any(|t| t.contains("says it is 10 bytes long") && t.contains("really is 21")),
        "{inspector:?}"
    );
    assert!(
        inspector.contains(&"stream-length-wrong".to_owned()),
        "{inspector:?}"
    );

    the(&harness, Region::Center, Role::Button, "Show bytes").click();
    harness.run_steps(3);

    assert_eq!(harness.state().state.objects.mode, ObjectMode::Raw);
    assert_eq!(harness.state().state.objects.highlight, diagnostic.byte_range());
    assert!(harness.state().state.objects.highlight.is_some());
}

#[test]
fn the_resources_of_the_page_are_listed_by_category_and_open_their_object() {
    let mut harness = app();
    open_fixture(&mut harness, "resources-inheritance");
    click_rail(&mut harness, "Objects");

    let navigator = texts_in(&harness, Region::Navigator, Role::Label);
    for text in [
        "RESOURCES · PAGE 1",
        "Font",
        "XObject",
        "ExtGState",
        "/F1",
        "/F2",
        "/F3",
        "/Fm1",
        "×2",
    ] {
        assert!(navigator.contains(&text.to_owned()), "{text} in {navigator:?}");
    }
    // Fonts and forms are open; the other categories are folded until opened.
    assert!(navigator.contains(&"/Fm2".to_owned()));
    assert!(!navigator.contains(&"/GS1".to_owned()), "{navigator:?}");
    the(&harness, Region::Navigator, Role::Button, "ExtGState").click();
    harness.run_steps(3);
    assert!(texts_in(&harness, Region::Navigator, Role::Label).contains(&"/GS1".to_owned()));

    // A font row opens the font object: /F1 of page 1 is 9 0 obj.
    the(&harness, Region::Navigator, Role::Button, "/F1").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(9)));
    let shown = center(&harness);
    assert!(shown.contains(&"9 0 obj".to_owned()), "{shown:?}");
    assert!(shown.contains(&"Font · Helvetica".to_owned()), "{shown:?}");
}

#[test]
fn the_filter_narrows_every_section_and_the_document_rows_open_the_catalog_and_trailer() {
    let mut harness = app();
    open_fixture(&mut harness, "resources-inheritance");
    click_rail(&mut harness, "Objects");

    harness.state_mut().state.nav.filter = "form".to_owned();
    harness.run_steps(3);

    // Objects whose type says Form, and the Form resources of the page.
    let rows = common::nav_rows(&harness, 0);
    assert_eq!(
        rows,
        [
            "XObject",
            "Collapse XObject",
            "/Fm1",
            "/Fm2",
            "12 0 obj",
            "13 0 obj"
        ]
    );

    harness.state_mut().state.nav.filter = "no such thing".to_owned();
    harness.run_steps(3);
    let labels = texts_in(&harness, Region::Navigator, Role::Label);
    let empty = labels
        .iter()
        .position(|t| t == "No objects match “no such thing”.");
    // Under the heading of the list, not above the rows it sits beside.
    assert!(
        empty.is_some_and(|at| labels[..at].contains(&"ALL OBJECTS".to_owned())),
        "{labels:?}"
    );

    harness.state_mut().state.nav.filter = String::new();
    harness.run_steps(3);
    the(&harness, Region::Navigator, Role::Button, "/Catalog").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(obj(1)));
    assert!(center(&harness).contains(&"1 0 obj".to_owned()));
    assert_eq!(title_of_inspector(&harness), "Catalog");
    the(&harness, Region::Navigator, Role::Button, "trailer").click();
    harness.run_steps(3);
    assert_eq!(harness.state().state.selection.primary, Some(NodeId::Trailer));
    let shown = center(&harness);
    assert!(shown.contains(&"Trailer".to_owned()), "{shown:?}");
    assert!(shown.contains(&"/Size ".to_owned()), "{shown:?}");
}

#[test]
fn a_binary_stream_is_summarised_and_an_image_names_its_size() {
    let mut harness = app();
    open_fixture(&mut harness, "xref-stream-objstm");
    click_rail(&mut harness, "Objects");
    let xref = harness
        .state_mut()
        .state
        .object_rows()
        .iter()
        .find(|r| r.detail.contains("/XRef"))
        .map(|r| r.r)
        .expect("a cross-reference stream");

    select(&mut harness, xref.num);

    let shown = center(&harness);
    assert!(shown.contains(&"Binary data".to_owned()), "{shown:?}");
    assert!(
        shown.iter().any(|t| t.contains("Bytes are not listed here")),
        "{shown:?}"
    );

    // An image is pictured instead (see `object_previews.rs`); its subtitle names its size.
    open_fixture(&mut harness, "images");
    click_rail(&mut harness, "Objects");
    select(&mut harness, 5);
    let shown = center(&harness);
    assert!(shown.contains(&"Image · 2×2".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"Binary data".to_owned()), "{shown:?}");
}

#[test]
fn right_click_on_an_object_row_lists_the_actions_and_copy_id_copies_its_id() {
    let mut harness = objects_at("text-state", 4);
    the(&harness, Region::Navigator, Role::Button, "3 0 obj").click_secondary();
    harness.run_steps(3);

    let items: Vec<String> = harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(
        items,
        ["Show in tree", "Object", "Raw bytes", "Copy id", "Copy JSON"]
    );

    harness.get_by_role_and_label(Role::MenuItem, "Copy id").click();
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
    assert_eq!(copied, Some("obj:3:0".to_owned()));
    // Smoke: the menu is for the row that was clicked, not for the selected object.
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::Object(ObjRef::new(4, 0)))
    );
}

#[test]
fn stream_bytes_opens_the_raw_view_with_the_streams_data_marked_and_only_streams_offer_it() {
    let mut harness = objects_at("minimal-one-page", 4);
    let range = harness
        .state()
        .state
        .object_facts()
        .and_then(|facts| facts.stream.as_ref())
        .map(|stream| stream.file_range.clone())
        .expect("object 4 is a stream");

    the(&harness, Region::Inspector, Role::Label, "Content stream").click_secondary();
    harness.run_steps(3);
    let items: Vec<String> = harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert!(items.contains(&"Stream bytes".to_owned()), "{items:?}");
    harness
        .get_by_role_and_label(Role::MenuItem, "Stream bytes")
        .click();
    harness.run_steps(3);

    assert_eq!(harness.state().state.objects.mode, ObjectMode::Raw);
    assert_eq!(harness.state().state.objects.highlight, Some(range.clone()));
    // The range is the stream's data, inside the object, which stays selected.
    let object = doc(&harness).object_range(ObjRef::new(4, 0)).unwrap();
    assert!(
        object.start < range.start && range.end <= object.end,
        "{object:?} {range:?}"
    );
    assert_eq!(harness.state().state.selection.primary, Some(obj(4)));
    // Smoke: the range really is the bytes between `stream` and `endstream`.
    let data = &doc(&harness).bytes()[range];
    assert!(data.starts_with(b"BT"), "{:?}", String::from_utf8_lossy(data));

    // A dictionary object has no stream data to mark.
    select(&mut harness, 3);
    the(&harness, Region::Inspector, Role::Label, "Page").click_secondary();
    harness.run_steps(3);
    let items: Vec<String> = harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert!(items.contains(&"Raw bytes".to_owned()), "{items:?}");
    assert!(!items.contains(&"Stream bytes".to_owned()), "{items:?}");
}

#[test]
fn the_decoded_and_raw_switch_sits_at_the_right_end_of_the_object_header() {
    let harness = objects_at("images", 5);
    let title = the(&harness, Region::Center, Role::Label, "5 0 obj").rect();
    let decoded = the(&harness, Region::Center, Role::Button, "Decoded").rect();
    let raw = the(&harness, Region::Center, Role::Button, "Raw bytes").rect();
    let column_right = common::WINDOW.x - pdfxray_app::theme::tokens::INSPECTOR_W;

    // Its last option ends at the header's right margin, not just after the chips.
    assert!(
        (column_right - 14.0 - raw.max.x).abs() <= 6.0,
        "{raw:?} should end near {}",
        column_right - 14.0
    );
    assert!(
        decoded.min.x > title.max.x + 200.0,
        "{decoded:?} right after {title:?}"
    );
    // Smoke: it still reads Decoded, then Raw bytes, on the title's line.
    assert!(decoded.max.x <= raw.min.x, "{decoded:?} {raw:?}");
    assert!((decoded.center().y - title.center().y).abs() < 6.0);
}
