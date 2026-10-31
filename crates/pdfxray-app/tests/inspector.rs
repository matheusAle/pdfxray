//! The inspector: the facts under the title, the selection's problem first, folded default
//! values, sections that start collapsed where the mode does not need them, and the actions that
//! used to be buttons at the foot, now on right-click.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use common::fake::{FakeValidator, struct_rich_report};
use common::{
    Region, app, buttons_in, click_at, click_rail, is_selected, named, open_fixture, texts_in, the,
    wait_for_render, wait_until,
};
use egui::{Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::{Availability, PdfxrayApp, RailItem};
use pdfxray_core::{BoxKind, NodeId, ObjRef};
use pretty_assertions::assert_eq;

type App = Harness<'static, PdfxrayApp>;

fn opened(fixture: &str, rail: &str, node: &NodeId) -> App {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    click_rail(&mut harness, rail);
    wait_for_render(&mut harness);
    harness.state_mut().state.select(node.clone());
    harness.run_steps(4);
    harness
}

fn labels(harness: &App) -> Vec<String> {
    texts_in(harness, Region::Inspector, Role::Label)
}

/// The `count` labels that follow the id line `line` of the title: the chips of the strip.
fn chips(harness: &App, line: &str, count: usize) -> Vec<String> {
    let labels = labels(harness);
    let at = labels
        .iter()
        .position(|t| t == line)
        .unwrap_or_else(|| panic!("no `{line}` in {labels:?}"));
    labels[at + 1..at + 1 + count].to_vec()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn the_summary_strip_shows_three_or_four_facts_per_kind_of_node() {
    let element = |num| NodeId::StructElem(ObjRef::new(num, 0));
    let cases: [(&str, &str, NodeId, &str, &[&str]); 9] = [
        (
            "images",
            "Content",
            NodeId::Op { page: 0, ix: 2 },
            "op:0:2",
            &["2×2", "RGB", "100% visible"],
        ),
        (
            "minimal-one-page",
            "Content",
            NodeId::Op { page: 0, ix: 3 },
            "op:0:3 · op #3",
            &["13 glyphs", "/F1 · 18 pt", "100% visible"],
        ),
        (
            "minimal-one-page",
            "Content",
            NodeId::Glyph {
                page: 0,
                op: 3,
                glyph: 0,
            },
            "glyph:0:3:0 · in Tj op #3",
            &["U+0048", "/F1", "w 722"],
        ),
        (
            "struct-rich",
            "Tags",
            element(14),
            "struct:14:0 · struct-elem",
            &["/Figure", "page 1", "Alt"],
        ),
        (
            "struct-rich",
            "Tags",
            NodeId::MarkedContent { page: 0, mcid: 2 },
            "mc:0:2 · marked-content",
            &["MCID 2", "page 1", "/Figure"],
        ),
        (
            "struct-rich",
            "Pages",
            NodeId::Page(0),
            "page:0 · page",
            &["400 × 400 pt", "5 boxes"],
        ),
        (
            "artifacts",
            "Content",
            NodeId::Artifact { page: 0, seq: 0 },
            "artifact:0:0 · artifact",
            &["artifact", "page 1"],
        ),
        (
            "minimal-one-page",
            "Pages",
            NodeId::PageBox {
                page: 0,
                kind: BoxKind::Media,
            },
            "box:0:media · page-box",
            &["MediaBox", "200 × 100 pt", "own"],
        ),
        (
            "text-state",
            "Objects",
            NodeId::Object(ObjRef::new(4, 0)),
            "obj:4:0 · /Contents of page 1",
            &["Flate", "315 B → 541 B", "rev 1"],
        ),
    ];
    for (fixture, rail, node, line, expected) in cases {
        let harness = opened(fixture, rail, &node);
        assert_eq!(chips(&harness, line, expected.len()), strings(expected), "{node}");
        // Smoke: a strip is never longer than four chips.
        assert!(expected.len() <= 4);
    }
}

#[test]
fn the_problem_card_comes_first_and_details_opens_the_finding() {
    let mut fake = FakeValidator::new(Availability::Ready {
        version: "1.30.3".to_owned(),
    });
    fake.report = struct_rich_report();
    fake.run_gate.open();
    let mut harness = app();
    harness.state_mut().state.validator = Arc::new(fake);
    open_fixture(&mut harness, "struct-rich");
    the(&harness, Region::Chrome, Role::Button, "Validate").click();
    wait_until(&mut harness, "the validation result", |s| {
        s.document.as_ref().unwrap().doc.validation().is_some() && !s.validation.is_running()
    });
    click_rail(&mut harness, "Tags");
    // 12 0 R has no /Alt in the report.
    harness
        .state_mut()
        .state
        .select(NodeId::StructElem(ObjRef::new(12, 0)));
    harness.run_steps(4);

    let all = labels(&harness);
    let place = |text: &str| {
        all.iter()
            .position(|t| t == text)
            .unwrap_or_else(|| panic!("no `{text}` in {all:?}"))
    };
    let card = place("Figure has no alternative text");
    // After the title and the chips, before the first property section.
    assert!(place("struct:12:0 · struct-elem") < place("/H1"));
    assert!(place("/H1") < card);
    assert!(card < place("Page bbox"));
    assert!(card < place("Type"));
    // The card has a Details button; the finding bar is not there yet.
    let buttons = buttons_in(&harness, Region::Inspector);
    assert!(buttons.contains(&"Details".to_owned()), "{buttons:?}");
    assert!(!buttons.contains(&"Next finding".to_owned()));

    the(&harness, Region::Inspector, Role::Button, "Details").click();
    harness.run_steps(4);
    assert_eq!(harness.state().state.nav.rail, RailItem::Validation);
    assert!(harness.state().state.validation.selected_finding.is_some());
    let buttons = buttons_in(&harness, Region::Inspector);
    for bar in ["Previous", "Rule docs", "Next finding"] {
        assert!(buttons.contains(&bar.to_owned()), "{bar} in {buttons:?}");
    }
}

#[test]
fn untagged_content_gets_a_card_without_details_when_nothing_was_validated() {
    let harness = opened("images", "Content", &NodeId::Op { page: 0, ix: 2 });
    let all = labels(&harness);
    let card = all
        .iter()
        .position(|t| t == "Not tagged and not an artifact")
        .expect("a card for untagged content");
    assert!(card < all.iter().position(|t| t == "Page bbox").unwrap());
    assert!(named(&harness, Region::Inspector, Role::Button, "Details").is_empty());
    // Smoke: the card says why it matters.
    assert_eq!(all[card + 1], "Assistive technology cannot reach this content");
}

#[test]
fn default_values_fold_into_one_line_that_expands_when_clicked() {
    let mut harness = opened("images", "Content", &NodeId::Op { page: 0, ix: 2 });
    let line = "4 default values (Blend Normal, Alpha 1, no SMask, no clip)";
    let all = labels(&harness);
    assert!(all.contains(&line.to_owned()), "{all:?}");
    for folded in ["Blend", "Alpha", "SMask", "Clip"] {
        assert!(!all.contains(&folded.to_owned()), "{folded} in {all:?}");
    }

    the(&harness, Region::Inspector, Role::Button, "4 default values").click();
    harness.run_steps(3);
    let all = labels(&harness);
    for (key, value) in [
        ("Blend", "Normal"),
        ("Alpha", "ca 1.0 · CA 1.0"),
        ("SMask", "None"),
        ("Clip", "none"),
    ] {
        let at = all
            .iter()
            .position(|t| t == key)
            .unwrap_or_else(|| panic!("{key} in {all:?}"));
        assert_eq!(all[at + 1], value, "{key}");
    }
    // Smoke: the line keeps its text (the chevron shows it is open, not a sign in front of it),
    // and the fill and stroke rows never folded.
    assert!(all.contains(&line.to_owned()));
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "4 default values"
    )));
    assert!(all.contains(&"Fill".to_owned()) && all.contains(&"Stroke".to_owned()));

    the(&harness, Region::Inspector, Role::Button, "4 default values").click();
    harness.run_steps(3);
    assert!(labels(&harness).contains(&line.to_owned()));
    assert!(!is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "4 default values"
    )));
}

#[test]
fn sections_the_mode_does_not_need_start_collapsed_and_the_users_choice_sticks() {
    // An image operator: its Image section is for Content, not for Tags.
    let mut harness = opened("images", "Content", &NodeId::Op { page: 0, ix: 2 });
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Image"
    )));
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Geometry"
    )));

    click_rail(&mut harness, "Tags");
    assert!(!is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Image"
    )));
    // What Tags is about stays open.
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Geometry"
    )));
    // One click opens it, and it stays open when the rail changes.
    the(&harness, Region::Inspector, Role::Button, "Image").click();
    harness.run_steps(2);
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Image"
    )));
    click_rail(&mut harness, "Content");
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Image"
    )));
}

#[test]
fn structure_is_collapsed_in_content_and_open_in_tags() {
    // Operator 17 is the Figure's content.
    let mut harness = opened("struct-rich", "Content", &NodeId::Op { page: 0, ix: 17 });
    assert!(!is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Structure"
    )));

    click_rail(&mut harness, "Tags");
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Structure"
    )));
    // Smoke: the structure facts are listed once it is open.
    assert!(labels(&harness).contains(&"Owner".to_owned()));
}

/// The text the last frame copied.
fn copied(harness: &App) -> Option<String> {
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

#[test]
fn the_footer_is_gone_and_the_title_and_the_values_have_a_menu() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    click_at(&mut harness, canvas.min + vec2(4.0, 4.0));

    let buttons = buttons_in(&harness, Region::Inspector);
    for gone in ["Raw object", "Stream bytes", "Copy JSON", "Copy id"] {
        assert!(!buttons.contains(&gone.to_owned()), "{gone} in {buttons:?}");
    }
    assert!(buttons.contains(&"Pin".to_owned()));

    // Right-click on the title lists the actions, Copy JSON among them.
    // The crumb, the title and the chip of the strip are all called MediaBox; the title is second.
    named(&harness, Region::Inspector, Role::Label, "MediaBox")[1].click_secondary();
    harness.run_steps(3);
    let items: Vec<String> = harness
        .query_all_by_role(Role::MenuItem)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert!(items.contains(&"Copy JSON".to_owned()), "{items:?}");
    harness.get_by_role_and_label(Role::MenuItem, "Copy JSON").click();
    harness.step();
    let json: serde_json::Value = serde_json::from_str(&copied(&harness).expect("JSON was copied")).unwrap();
    assert_eq!(json["title"], "MediaBox");
    // Smoke: it is the whole page box, not a value.
    assert_eq!(json["kind"], "page-box");

    // A value has the same menu: Copy id copies the id of the selection.
    harness.run_steps(2);
    the(&harness, Region::Inspector, Role::Label, "[0 0 200 100]").click_secondary();
    harness.run_steps(3);
    harness.get_by_role_and_label(Role::MenuItem, "Copy id").click();
    harness.step();
    assert_eq!(copied(&harness), Some("box:0:media".to_owned()));
}

/// The breadcrumb buttons of the inspector, in order: everything before the pin.
fn crumb_nodes(harness: &egui_kittest::Harness<'_, pdfxray_app::PdfxrayApp>) -> Vec<(String, egui::Rect)> {
    common::nodes_in(harness, common::Region::Inspector, egui::Role::Button)
        .into_iter()
        .map(|node| (node.accesskit_node().label().unwrap_or_default(), node.rect()))
        .take_while(|(name, _)| name != "Pin")
        .collect()
}

#[test]
fn the_breadcrumb_stays_on_one_line_and_folds_its_middle_when_it_is_long() {
    let mut harness = common::app();
    common::open_fixture(&mut harness, "struct-rich");
    // The deepest element: LBody, under Document › Sect › L › LI.
    harness
        .state_mut()
        .state
        .select(pdfxray_core::NodeId::StructElem(pdfxray_core::ObjRef::new(22, 0)));
    harness.run_steps(3);

    let crumbs = crumb_nodes(&harness);
    let names: Vec<&str> = crumbs.iter().map(|(name, _)| name.as_str()).collect();
    // One line: every crumb on the same row.
    let row = crumbs[0].1.center().y;
    // (The selected crumb has a 1 pt stroke around it.)
    assert!(
        crumbs
            .iter()
            .all(|(_, rect)| (rect.center().y - row).abs() <= 2.0),
        "{crumbs:?}"
    );
    // The file is first, the selection last; what did not fit is behind "…".
    assert_eq!(names.first(), Some(&"struct-rich.pdf"), "{names:?}");
    assert_eq!(names.last(), Some(&"LBody"));
    assert!(names.contains(&pdfxray_app::components::crumb::MORE), "{names:?}");
    // Smoke: a short path does not fold.
    harness.state_mut().state.select(pdfxray_core::NodeId::Page(0));
    harness.run_steps(3);
    assert!(
        !crumb_nodes(&harness)
            .iter()
            .any(|(name, _)| name == pdfxray_app::components::crumb::MORE)
    );
}
