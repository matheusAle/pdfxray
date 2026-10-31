//! The Speech tab: the transcript of what a reader announces, its reader picker, the skipped
//! items, the link between rows and the canvas, and the speech order as a source of the reading
//! order. The footnote connectors and the table overlay are in `table_overlay.rs`.
//!
//! Update the snapshot with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test speech snapshot_speech_tab`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

mod common;
#[path = "common/view_tools.rs"]
mod view_tools;

use common::{
    Region, app, builder, buttons_in, click_drawer_tab, nodes_in, open_fixture, texts_in, the,
    wait_for_render,
};
use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::drawer::speech;
use pdfxray_app::{DrawerTab, OrderSource, PdfxrayApp};
use pdfxray_core::{Document, NodeId, ObjRef, ReadingOrderSource, SpeechPreset};
use pretty_assertions::assert_eq;

type App = Harness<'static, PdfxrayApp>;

fn settle(harness: &mut App) {
    harness.run_steps(4);
}

fn opened(fixture: &str) -> App {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    settle(&mut harness);
    wait_for_render(&mut harness);
    harness
}

fn doc(fixture: &str) -> Document {
    Document::open(common::fixture_path(fixture)).unwrap()
}

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

/// Opens the drawer on the Speech tab with room for a whole transcript.
fn open_speech(harness: &mut App) {
    harness.state_mut().state.drawer.height = 600.0;
    click_drawer_tab(harness, "Speech");
}

/// Picks a reader in the tab's picker.
fn pick(harness: &mut App, title: &str) {
    the(harness, Region::Drawer, Role::Button, title).click();
    settle(harness);
}

/// The rows of the transcript, in order.
fn rows(harness: &App) -> Vec<String> {
    buttons_in(harness, Region::Drawer)
        .into_iter()
        .filter(|name| name.starts_with("Item "))
        .collect()
}

/// The page separators of the transcript.
fn pages(harness: &App) -> Vec<String> {
    nodes_in(harness, Region::Drawer, Role::Heading)
        .into_iter()
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

fn badges(harness: &App) -> Vec<String> {
    buttons_in(harness, Region::Center)
        .into_iter()
        .filter(|name| name.starts_with("Reading order step"))
        .collect()
}

// ---- the transcript ----

#[test]
fn the_transcript_lists_what_the_spec_reader_announces_with_its_role_and_flags() {
    let mut harness = opened("speech-alt-hides");

    open_speech(&mut harness);

    // The Sect that /Alt replaces is one row and the two paragraphs under it are not there.
    assert_eq!(
        rows(&harness),
        [
            "Item 1: H1 · Quarterly report",
            "Item 2: Sect · Summary of results · Alt",
            "Item 5: Figure · Revenue chart · Alt",
            "Item 6: Figure · Cost chart · Alt",
            "Item 7: P · Closing remarks follow.",
        ]
    );
    assert_eq!(pages(&harness), ["Page 1"]);
    // Smoke: the tab is the active one, the drawer is open and its picker is on the spec reader.
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Speech);
    assert!(harness.state().state.drawer.open);
    assert!(common::is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Structure (spec)"
    )));
}

#[test]
fn each_reader_announces_what_core_says_it_does() {
    let mut harness = opened("speech-alt-hides");
    open_speech(&mut harness);
    let structure = rows(&harness);

    pick(&mut harness, "Firefox");
    let firefox = rows(&harness);
    pick(&mut harness, "Chrome/Edge");
    let chromium = rows(&harness);

    // pdf.js keeps the paragraphs under the Alt and replaces only a figure, and one has no name.
    assert_eq!(
        firefox,
        [
            "Item 1: H1 · Quarterly report",
            "Item 2: P · Revenue grew in every region.",
            "Item 3: P · Costs stayed flat this year.",
            "Item 4: Figure · Revenue chart · Alt",
            "Item 5: Figure · (no name)",
            "Item 6: P · Closing remarks follow.",
        ]
    );
    // Chrome ignores the tags: text in paint order, no roles.
    assert_eq!(
        chromium,
        [
            "Item 1: text · Quarterly report",
            "Item 2: text · Revenue grew in every region.",
            "Item 3: text · Costs stayed flat this year.",
            "Item 4: text · Closing remarks follow.",
        ]
    );
    assert_ne!(structure, firefox);
    assert_ne!(firefox, chromium);
    // Smoke: the counts are core's.
    let document = doc("speech-alt-hides");
    assert_eq!(
        [structure.len(), firefox.len(), chromium.len()],
        [
            SpeechPreset::Structure,
            SpeechPreset::FirefoxPdfJs,
            SpeechPreset::ChromiumDefault
        ]
        .map(|preset| document.speech_order(preset).spoken().count())
    );
}

#[test]
fn a_table_cell_says_the_headers_it_is_read_with() {
    let mut harness = opened("table-scope-pdf2");
    open_speech(&mut harness);

    let found = rows(&harness);

    assert_eq!(found[3], "Item 4: TH · North · header: Region");
    assert_eq!(found[4], "Item 5: TD · 1.2 · header: Q1 · header: North");
    // Smoke: a header cell of the first row has none.
    assert_eq!(found[0], "Item 1: TH · Region");
}

#[test]
fn language_changes_links_and_notes_have_chips() {
    let mut lang = opened("speech-lang");
    open_speech(&mut lang);
    assert_eq!(
        rows(&lang),
        [
            "Item 1: P · The German word",
            "Item 2: Span · Gesamtumsatz · lang: de-DE",
            "Item 3: P · means total revenue. · lang: en-US",
            "Item 4: P · Bonjour tout le monde. · lang: fr-FR",
            "Item 5: P · Back in English. · lang: en-US",
        ]
    );

    let mut links = opened("speech-link-objr");
    open_speech(&mut links);
    assert_eq!(
        rows(&links),
        [
            "Item 1: P · Please read the following.",
            "Item 2: Link · Annual report · link → https://example.com/report",
            "Item 3: Link · (no name) · link → p1",
            "Item 4: Form · Your name · widget",
            "Item 5: P · Thank you.",
        ]
    );

    let mut notes = opened("speech-footnote");
    open_speech(&mut notes);
    let found = rows(&notes);
    assert_eq!(found[1], "Item 2: Reference · 1 · note ↗ 1 apart");
    assert_eq!(found[3], "Item 4: Lbl · 1 · note ↙ 1 apart");
}

// ---- show skipped ----

#[test]
fn show_skipped_lists_what_is_not_announced_with_the_reason() {
    let mut harness = opened("speech-alt-hides");
    open_speech(&mut harness);
    assert_eq!(rows(&harness).len(), 5);

    the(&harness, Region::Drawer, Role::Switch, "Show skipped").click();
    settle(&mut harness);

    let found = rows(&harness);
    assert_eq!(found.len(), 7);
    assert_eq!(
        found[2],
        "Item 3 (not announced: hidden by Alt on Sect): P · Revenue grew in every region."
    );
    assert_eq!(
        found[3],
        "Item 4 (not announced: hidden by Alt on Sect): P · Costs stayed flat this year."
    );
    // Smoke: the spoken ones are unchanged, and the switch says it is on.
    assert_eq!(found[1], "Item 2: Sect · Summary of results · Alt");
    assert!(common::is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Switch,
        "Show skipped"
    )));

    // Off again, they are gone.
    the(&harness, Region::Drawer, Role::Switch, "Show skipped").click();
    settle(&mut harness);
    assert_eq!(rows(&harness).len(), 5);
}

#[test]
fn artifacts_are_skipped_with_their_reason_and_chrome_reads_them() {
    let mut harness = opened("speech-artifacts");
    open_speech(&mut harness);
    the(&harness, Region::Drawer, Role::Switch, "Show skipped").click();
    settle(&mut harness);

    assert_eq!(
        rows(&harness),
        [
            "Item 1 (not announced: artifact): Artifact · Annual Report 2025",
            "Item 2: H1 · Results",
            "Item 3: P · Revenue grew in every region.",
            "Item 4 (not announced: artifact): Artifact · Page 1 of 9",
        ]
    );
    pick(&mut harness, "Chrome/Edge");
    // Chrome reads the artifacts like any text, so nothing is skipped.
    assert_eq!(
        rows(&harness),
        [
            "Item 1: text · Annual Report 2025",
            "Item 2: text · Results",
            "Item 3: text · Revenue grew in every region.",
            "Item 4: text · Page 1 of 9",
        ]
    );
}

// ---- the approximation label ----

#[test]
fn an_approximation_says_so_and_a_tooltip_lists_its_rules() {
    let mut harness = opened("speech-alt-hides");
    open_speech(&mut harness);
    // The spec reader is not an approximation: there is no label.
    assert!(harness.query_by_label("approximation").is_none());

    pick(&mut harness, "Acrobat (NVDA/JAWS)");
    let label = harness.get_by_label("approximation").rect().center();
    harness.hover_at(label);
    harness.run_steps(6);

    let rules = SpeechPreset::AcrobatWindows.rules();
    assert_eq!(rules.len(), 1);
    for rule in rules {
        assert!(harness.query_by_label(rule.topic).is_some(), "{}", rule.topic);
        assert!(
            harness.query_by_label(rule.behaviour).is_some(),
            "{}",
            rule.behaviour
        );
        assert!(
            harness.query_by_label(rule.evidence).is_some(),
            "{}",
            rule.evidence
        );
    }
    assert!(
        harness
            .query_by_label("Approximation of Acrobat on Windows (NVDA, JAWS)")
            .is_some()
    );
    // Smoke: it also says it was not recorded from a screen reader.
    assert!(harness.query_by_label(speech::NOT_RECORDED).is_some());
}

#[test]
fn every_approximation_carries_the_label_and_the_header_note_counts_what_is_read() {
    let mut harness = opened("speech-alt-hides");
    open_speech(&mut harness);
    let document = doc("speech-alt-hides");

    for preset in [
        SpeechPreset::Structure,
        SpeechPreset::AcrobatWindows,
        SpeechPreset::FirefoxPdfJs,
        SpeechPreset::ChromiumDefault,
    ] {
        pick(&mut harness, speech::preset_title(preset));
        let order = document.speech_order(preset);
        let spoken = order.spoken().count();
        let skipped = order.items.len() - spoken;
        let mut expected = vec![speech::preset_title(preset).to_owned()];
        if preset.is_approximation() {
            expected.push("approximation".to_owned());
        }
        expected.push(format!("{spoken} spoken"));
        expected.push(format!("{skipped} skipped"));

        assert_eq!(
            speech::header_note(&harness.state().state),
            Some(expected.join(" · ")),
            "{preset:?}"
        );
        assert_eq!(
            harness.query_by_label("approximation").is_some(),
            preset.is_approximation(),
            "{preset:?}"
        );
    }
    // Smoke: the one the issue names.
    pick(&mut harness, "Acrobat (NVDA/JAWS)");
    assert_eq!(
        speech::header_note(&harness.state().state).as_deref(),
        Some("Acrobat (NVDA/JAWS) · approximation · 5 spoken · 2 skipped")
    );
    // When the tabs leave too little room the bar falls back to the note without the reader.
    assert_eq!(
        speech::header_notes(&harness.state().state),
        [
            "Acrobat (NVDA/JAWS) · approximation · 5 spoken · 2 skipped",
            "approximation · 5 spoken · 2 skipped"
        ]
    );
}

// ---- rows and the canvas ----

#[test]
fn hovering_a_row_previews_its_node_and_clicking_selects_it_and_scrolls_the_canvas_there() {
    let mut harness = opened("struct-rich");
    open_speech(&mut harness);
    let rows_before = rows(&harness);
    assert_eq!(rows_before.len(), 9);
    assert_eq!(pages(&harness), ["Page 1", "Page 2"]);
    assert_eq!(harness.state().state.canvas.page, 0);

    // Resting on the Figure of page 1 previews it on the canvas.
    let figure = rows_before[2].clone();
    assert_eq!(
        figure,
        "Item 3: Figure · A pale blue rectangle · Alt · lang: en-US"
    );
    let at = the(&harness, Region::Drawer, Role::Button, &figure)
        .rect()
        .center();
    harness.hover_at(at);
    harness.run_steps(3);
    assert_eq!(harness.state().state.canvas.drawn_preview, Some(elem(14)));
    harness.remove_cursor();
    harness.run_steps(3);
    assert_eq!(harness.state().state.canvas.drawn_preview, None);

    let callout = rows_before[8].clone();

    the(&harness, Region::Drawer, Role::Button, &callout).click();
    settle(&mut harness);

    assert_eq!(harness.state().state.selection.primary, Some(elem(24)));
    assert_eq!(harness.state().state.canvas.page, 1);
    assert!(
        harness.state().state.canvas.scroll_offset.y > 0.0,
        "{:?}",
        harness.state().state.canvas.scroll_offset
    );
    // The row is the selected one.
    assert!(common::is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        &callout
    )));
}

#[test]
fn selecting_a_node_elsewhere_highlights_its_row_and_scrolls_the_transcript_to_it() {
    let mut harness = opened("long-doc");
    open_speech(&mut harness);
    harness.state_mut().state.drawer.height = 200.0;
    settle(&mut harness);
    // Chapter 8 is far below what a short drawer shows.
    let row = "Item 29: H1 · Chapter 8";
    assert!(harness.query_by_label(row).is_none());

    harness.state_mut().state.select(elem(59));
    settle(&mut harness);

    let found = the(&harness, Region::Drawer, Role::Button, row);
    assert!(common::is_selected(&found));
    // Smoke: it is the only selected row.
    let selected = nodes_in(&harness, Region::Drawer, Role::Button)
        .into_iter()
        .filter(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Item "))
        })
        .filter(common::is_selected)
        .count();
    assert_eq!(selected, 1);
}

// ---- speech as the reading order ----

#[test]
fn speech_as_the_reading_order_source_changes_the_badges_on_the_canvas() {
    let mut harness = opened("speech-alt-hides");
    view_tools::in_menu(&mut harness, |_| {});
    // R shows the reading order, from the structure tree.
    harness.key_press(egui::Key::R);
    settle(&mut harness);
    let structure = doc("speech-alt-hides").reading_order(0, ReadingOrderSource::Structure);
    assert_eq!(badges(&harness).len(), structure.steps.len());

    view_tools::pick(&mut harness, "Speech");
    settle(&mut harness);

    assert_eq!(harness.state().state.canvas.view.order_from, OrderSource::Speech);
    // The Sect that /Alt replaces is one step and the paragraphs under it take none.
    let document = doc("speech-alt-hides");
    let speech_order = document.speech_reading_order(0, &document.speech_order(SpeechPreset::Structure));
    assert_eq!(speech_order.steps.len(), 5);
    assert_eq!(
        badges(&harness),
        (1..=5)
            .map(|n| format!("Reading order step {n}"))
            .collect::<Vec<_>>()
    );
    assert_ne!(structure.steps.len(), speech_order.steps.len());
    // Badge 2 is the Sect.
    the(&harness, Region::Center, Role::Button, "Reading order step 2").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.selection.primary, Some(elem(9)));
    // Choosing it brought the transcript up.
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Speech);
}

#[test]
fn the_reading_order_follows_the_reader_of_the_speech_tab() {
    let mut harness = opened("speech-alt-hides");
    harness.key_press(egui::Key::R);
    view_tools::pick(&mut harness, "Speech");
    settle(&mut harness);
    assert_eq!(badges(&harness).len(), 5);

    pick(&mut harness, "Chrome/Edge");

    // Chrome reads four items, so there are four numbers.
    assert_eq!(badges(&harness).len(), 4);
}

#[test]
fn the_inspector_lists_the_speech_order_of_a_selected_page() {
    let mut harness = opened("speech-alt-hides");
    harness.key_press(egui::Key::R);
    view_tools::pick(&mut harness, "Speech");
    settle(&mut harness);

    harness.state_mut().state.select(NodeId::Page(0));
    settle(&mut harness);

    let rows: Vec<String> = texts_in(&harness, Region::Inspector, Role::Button)
        .into_iter()
        .filter(|name| name.starts_with("Step "))
        .collect();
    assert_eq!(
        rows,
        [
            "Step 1: H1 · Quarterly report",
            "Step 2: Sect · Summary of results",
            "Step 3: Figure · Revenue chart",
            "Step 4: Figure · Cost chart",
            "Step 5: P · Closing remarks follow.",
        ]
    );
    assert!(common::has_label(&harness, "from speech order · 5 elements"));
}

// ---- the drawer ----

#[test]
fn the_speech_tab_is_the_fifth_and_cmd_5_shows_it() {
    let mut harness = opened("speech-alt-hides");
    assert_eq!(
        buttons_in(&harness, Region::Drawer)[..5],
        ["Hit stack", "Diagnostics", "Validation", "Operators", "Speech"]
    );

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num5);
    settle(&mut harness);

    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Speech);
    assert!(harness.state().state.drawer.open);
    // Smoke: the strip had no count for it before the document was read, the tab has one now.
    assert_eq!(
        speech::header_note(&harness.state().state).as_deref(),
        Some("Structure (spec) · 5 spoken · 2 skipped")
    );
}

#[test]
fn a_file_without_structure_announces_nothing_to_the_tag_readers_and_its_text_to_chrome() {
    let mut harness = opened("minimal-one-page");
    open_speech(&mut harness);

    assert!(
        harness
            .query_by_label(&format!(
                "{} {}",
                speech::NOTHING_ANNOUNCED,
                speech::UNTAGGED_NOTE
            ))
            .is_some()
    );

    pick(&mut harness, "Chrome/Edge");
    assert_eq!(rows(&harness), ["Item 1: text · Hello pdfxray"]);
    assert!(harness.query_by_label(speech::NOTHING_ANNOUNCED).is_none());
}

#[test]
fn snapshot_speech_tab() {
    let _gpu = common::gpu_lock();
    let mut harness = builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "speech-footnote");
    harness.state_mut().state.drawer.speech.preset = SpeechPreset::AcrobatWindows;
    harness.state_mut().state.drawer.height = 300.0;
    click_drawer_tab(&mut harness, "Speech");
    wait_for_render(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_speech_tab");
}
