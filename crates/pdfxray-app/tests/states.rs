//! The states of the window that are not the workspace: nothing open (the drop zone and the
//! recent files), a file opening on its own thread, an encrypted file and a damaged one.
//!
//! The pictures of the four states are in `snapshots_states.rs`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    Region, all_label_texts, app, fixture_path, has_label, is_selected, open_fixture, texts_in, the,
    wait_for_open, wait_until,
};
use egui::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::object_facts::human_bytes;
use pdfxray_app::open::{Observer, STAGES};
use pdfxray_app::states::{broken, empty, encrypted, opening};
use pdfxray_app::{DrawerTab, PdfxrayApp, RailItem, Recents, Screen, StageState};
use pdfxray_core::OpenStage;
use pretty_assertions::assert_eq;

/// 2026-10-29 12:00:00 UTC, the clock of every test.
const NOW: u64 = 1_793_275_200;

fn pinned() -> u64 {
    NOW
}

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfxray-states-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A copy of fixture `from` at `home/relative`.
fn put(home: &Path, relative: &str, from: &str) -> PathBuf {
    let path = home.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::copy(fixture_path(from), &path).unwrap();
    path
}

fn json_of(recent: &Path, name: &str, pages: usize, tagged: bool, opened_at: u64) -> serde_json::Value {
    serde_json::json!({
        "path": recent,
        "name": name,
        "pages": pages,
        "tagged": tagged,
        "opened_at": opened_at,
    })
}

/// The app with nothing open, its recent files in `data` and the clock stopped.
fn app_with_recents(data: &Path, home: &Path) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    let state = &mut harness.state_mut().state;
    state.recents = Recents::at(Some(data)).with_home(Some(home.to_path_buf()));
    state.clock = pinned;
    harness.run();
    harness
}

/// The labels after `RECENT`, `count` of them.
fn recent_labels(harness: &Harness<'_, PdfxrayApp>, count: usize) -> Vec<String> {
    let labels = all_label_texts(harness);
    let at = labels.iter().position(|t| t == empty::RECENT_HEADING).unwrap();
    labels[at + 1..at + 1 + count].to_vec()
}

#[test]
fn without_a_file_the_window_offers_the_drop_zone_and_nothing_else() {
    let harness = app();

    assert_eq!(harness.state().state.screen(), Screen::Empty);
    assert!(has_label(&harness, "Drop a PDF to inspect it"));
    assert!(has_label(
        &harness,
        "See its tags, layout boxes, drawing instructions and raw objects.\npdfxray only reads the file; it never changes it."
    ));
    let _open = harness.get_by_label("Open file…");
    let _sample = harness.get_by_label("Try a sample");
    // The title bar only names the app: no chip, no layer modes, no Validate.
    assert_eq!(texts_in(&harness, Region::Chrome, Role::Label)[0], "pdfxray");
    assert!(harness.query_by_label("Validate").is_none());
    assert!(harness.query_by_label("All").is_none());
    // No recent files, so no list.
    assert!(!has_label(&harness, "RECENT"));
    // Navigator, inspector and drawer are hidden.
    assert!(!has_label(&harness, "Structure"));
    assert!(harness.query_by_label("Hit stack").is_none());
    // The rail and the units switch are there but the rail does nothing yet.
    let tags = the(&harness, Region::Rail, Role::Button, "Tags");
    assert!(tags.accesskit_node().is_disabled());
    let _units = the(&harness, Region::Chrome, Role::Button, "mm");
}

#[test]
fn the_recent_files_are_listed_newest_first_and_a_missing_one_is_greyed() {
    let data = scratch("recents-data");
    let home = scratch("recents-home");
    let report = put(&home, "Documents/reports/annual-report-2025.pdf", "struct-rich");
    let gone = home.join("Downloads/invoice-4471.pdf");
    std::fs::write(
        data.join("recents.json"),
        serde_json::json!({"files": [
            json_of(&report, "annual-report-2025.pdf", 2, true, NOW - 120),
            json_of(&gone, "invoice-4471.pdf", 1, false, NOW - 86_400),
        ]})
        .to_string(),
    )
    .unwrap();

    let mut harness = app_with_recents(&data, &home);

    assert_eq!(
        recent_labels(&harness, 7),
        [
            "annual-report-2025.pdf",
            "~/Documents/reports · 2 pages",
            "Tagged",
            "2 min ago",
            "invoice-4471.pdf",
            "~/Downloads · 1 page",
            "not found",
        ]
    );
    // The file that exists opens; the one that is gone cannot be chosen.
    let missing = harness.get_by_label("Open invoice-4471.pdf");
    assert!(missing.accesskit_node().is_disabled());
    harness.get_by_label("Open invoice-4471.pdf").click();
    harness.run();
    assert!(harness.state().state.opening.job().is_none());
    assert!(harness.state().state.document.is_none());

    harness.get_by_label("Open annual-report-2025.pdf").click();
    harness.run();
    wait_for_open(&mut harness);

    let state = &harness.state().state;
    assert_eq!(state.document.as_ref().map(|d| d.path.clone()), Some(report));
    assert_eq!(state.screen(), Screen::Workspace);
    std::fs::remove_dir_all(&data).ok();
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_saved_list_that_cannot_be_read_is_reported_until_a_file_is_opened() {
    let data = scratch("recents-corrupt");
    let home = scratch("recents-corrupt-home");
    let saved = data.join("recents.json");
    std::fs::write(&saved, "{ not json").unwrap();

    let mut harness = app_with_recents(&data, &home);

    let problem = format!(
        "{}: {} is not a list of recent files: key must be a string at line 1 column 3",
        empty::RECENTS_NOT_READ,
        saved.display()
    );
    assert!(has_label(&harness, &problem), "{:?}", all_label_texts(&harness));
    assert!(!has_label(&harness, empty::RECENT_HEADING));

    // Opening a file saves a new list over the unreadable one: nothing is left to report, in this
    // run or the next.
    open_fixture(&mut harness, "page-boxes");
    let state = &harness.state().state;
    assert!(state.recents.load_error().is_none() && state.recents_error.is_none());
    let labels = all_label_texts(&app_with_recents(&data, &home));
    std::fs::remove_dir_all(&data).ok();
    std::fs::remove_dir_all(&home).ok();

    assert!(
        !labels.iter().any(|t| t.starts_with(empty::RECENTS_NOT_READ)),
        "{labels:?}"
    );
    // smoke: the new list is the one shown
    assert!(labels.contains(&"page-boxes.pdf".to_owned()), "{labels:?}");
}

#[test]
fn try_a_sample_opens_the_file_built_into_the_app() {
    let mut harness = app();

    harness.get_by_label("Try a sample").click();
    harness.run();
    wait_for_open(&mut harness);

    let state = &harness.state().state;
    let loaded = state.document.as_ref().unwrap();
    assert_eq!(loaded.info.name, "struct-rich.pdf");
    assert_eq!(state.page_count(), 2);
    assert!(loaded.tagged);
    assert_eq!(state.status, pdfxray_app::READY);
    assert_eq!(state.screen(), Screen::Workspace);
    // The sample is not a file of the user's: it is not remembered.
    assert_eq!(state.recents.entries(), []);
    // It is the same file as the generated fixture.
    assert_eq!(
        loaded.doc.bytes(),
        std::fs::read(fixture_path("struct-rich")).unwrap()
    );
    // The file validation reads is there.
    assert!(loaded.path.exists());
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)[0],
        "struct-rich.pdf"
    );
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.7 · Tagged · rev 1/1",
    );
}

#[test]
fn opening_a_file_puts_it_first_in_the_saved_recent_files() {
    let data = scratch("saved-data");
    let mut harness = app();
    {
        let state = &mut harness.state_mut().state;
        state.recents = Recents::at(Some(&data));
        state.clock = pinned;
    }
    harness.run();

    open_fixture(&mut harness, "page-boxes");
    let first = std::path::absolute(fixture_path("page-boxes")).unwrap();
    let saved = |harness: &Harness<'_, PdfxrayApp>| -> serde_json::Value {
        let _ = harness;
        serde_json::from_slice(&std::fs::read(data.join("recents.json")).unwrap()).unwrap()
    };
    assert_eq!(
        saved(&harness),
        serde_json::json!({"files": [json_of(&first, "page-boxes.pdf", 3, false, NOW)]})
    );

    open_fixture(&mut harness, "struct-rich");
    open_fixture(&mut harness, "page-boxes");

    // Opened again, a file moves to the front instead of appearing twice.
    let second = std::path::absolute(fixture_path("struct-rich")).unwrap();
    assert_eq!(
        saved(&harness),
        serde_json::json!({"files": [
            json_of(&first, "page-boxes.pdf", 3, false, NOW),
            json_of(&second, "struct-rich.pdf", 2, true, NOW),
        ]})
    );
    // A new run of the app reads them back, in the same order.
    let reread = Recents::at(Some(&data));
    assert_eq!(
        reread
            .entries()
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["page-boxes.pdf", "struct-rich.pdf"]
    );
    // A file that fails to open is not remembered.
    let not_a_pdf = data.join("notes.txt");
    std::fs::write(&not_a_pdf, "hello").unwrap();
    harness.state_mut().state.open_path(&not_a_pdf);
    harness.run();
    wait_for_open(&mut harness);
    assert_eq!(harness.state().state.recents.entries().len(), 2);
    std::fs::remove_dir_all(&data).ok();
}

/// What the opening thread reported, in order.
type Reports = Arc<Mutex<Vec<(OpenStage, usize, usize)>>>;

/// An observer that holds the opening thread while `held` says so, recording every report.
fn holding(held: Arc<AtomicBool>, at: fn(OpenStage, usize, usize) -> bool) -> (Observer, Reports) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let observer: Observer = Arc::new(move |stage, done, total| {
        log.lock().unwrap().push((stage, done, total));
        if at(stage, done, total) {
            while held.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    });
    (observer, seen)
}

fn first_page(stage: OpenStage, done: usize, _total: usize) -> bool {
    stage == OpenStage::CaptureContent && done >= 1
}

/// Starts opening `long-doc` with the card showing at once, held after its first captured page.
fn opening_long_doc(harness: &mut Harness<'_, PdfxrayApp>, held: &Arc<AtomicBool>) -> Reports {
    let (observer, seen) = holding(Arc::clone(held), first_page);
    let path = fixture_path("long-doc");
    let state = &mut harness.state_mut().state;
    state.picker = Box::new(move || Some(path.clone()));
    state.opening.card_delay = Duration::ZERO;
    state.opening.spinner_turns = false;
    state.opening.observer = Some(observer);
    harness.get_by_label("Open file…").click();
    harness.run();
    wait_until(harness, "the first captured page", |state| {
        state
            .open_progress()
            .and_then(|progress| progress.detail(OpenStage::CaptureContent))
            .is_some_and(|detail| detail == "1 / 40 pages")
    });
    seen
}

#[test]
fn opening_shows_the_stages_as_they_finish_and_the_pages_as_they_are_captured() {
    let mut harness = app();
    let held = Arc::new(AtomicBool::new(true));

    let seen = opening_long_doc(&mut harness, &held);

    // Three stages are done, the fourth is counting pages, the last has not begun.
    let state = &harness.state().state;
    assert_eq!(state.screen(), Screen::Opening);
    let progress = state.open_progress().unwrap();
    assert_eq!(
        STAGES.map(|stage| progress.state(stage)),
        [
            StageState::Done,
            StageState::Done,
            StageState::Done,
            StageState::Running,
            StageState::Pending
        ]
    );
    let reports = seen.lock().unwrap().clone();
    let objects = reports
        .iter()
        .find(|(stage, done, total)| *stage == OpenStage::IndexObjects && done == total && *total > 0)
        .unwrap()
        .2;
    let size = std::fs::metadata(fixture_path("long-doc")).unwrap().len();
    assert_eq!(
        progress.facts(),
        format!("{} · {objects} objects · 40 pages", human_bytes(size as usize))
    );
    let counted = progress.detail(OpenStage::CaptureContent).unwrap();
    assert!(counted.ends_with(" / 40 pages"), "{counted}");

    // The card says so, in order: headline, facts, then each stage and what it found.
    let labels = all_label_texts(&harness);
    let at = labels.iter().position(|t| *t == "Opening long-doc.pdf").unwrap();
    assert_eq!(
        labels[at..at + 12],
        [
            "Opening long-doc.pdf".to_owned(),
            progress.facts(),
            "Read file structure".to_owned(),
            "xref + 1 revision".to_owned(),
            "Index objects".to_owned(),
            format!("{objects} objects"),
            "Read page tree".to_owned(),
            "40 pages".to_owned(),
            "Capture page content".to_owned(),
            counted,
            "Build structure tree".to_owned(),
            opening::PAGES_NOTE.to_owned(),
        ]
    );
    let _cancel = harness.get_by_label("Cancel");
    // The title bar names the file; the rail is shown but the workspace is not.
    assert_eq!(texts_in(&harness, Region::Chrome, Role::Label)[0], "long-doc.pdf");
    assert!(harness.query_by_label("Validate").is_none());
    assert!(!has_label(&harness, "Structure"));
    assert!(harness.query_by_label("Hit stack").is_none());
    assert!(
        the(&harness, Region::Rail, Role::Button, "Tags")
            .accesskit_node()
            .is_disabled()
    );
    // Smoke: the bar is part-way.
    let fraction = progress.fraction();
    assert!(fraction > 0.6 && fraction < 1.0, "{fraction}");

    // Let it finish: the stages ran in order, once each began, and the workspace takes over.
    held.store(false, Ordering::SeqCst);
    wait_for_open(&mut harness);
    let state = &harness.state().state;
    assert_eq!(state.screen(), Screen::Workspace);
    assert_eq!(state.page_count(), 40);
    let mut order: Vec<OpenStage> = seen.lock().unwrap().iter().map(|(stage, _, _)| *stage).collect();
    order.dedup();
    assert_eq!(order, STAGES);
    assert_eq!(seen.lock().unwrap()[0], (OpenStage::ReadStructure, 0, 0));
}

#[test]
fn cancelling_an_open_goes_back_to_the_drop_zone_and_the_file_never_opens() {
    let mut harness = app();
    let held = Arc::new(AtomicBool::new(true));
    let seen = opening_long_doc(&mut harness, &held);
    assert_eq!(harness.state().state.screen(), Screen::Opening);

    harness.get_by_label("Cancel").click();
    harness.run();

    let state = &harness.state().state;
    assert_eq!(state.screen(), Screen::Empty);
    assert!(state.document.is_none());
    assert!(state.opening.job().is_none());
    assert_eq!(state.status, "Cancelled opening long-doc.pdf");
    assert!(has_label(&harness, "Drop a PDF to inspect it"));
    // The thread is told to stop: it ends without a document and nothing appears.
    held.store(false, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(300));
    harness.run_steps(3);
    assert!(harness.state().state.document.is_none());
    assert_eq!(harness.state().state.screen(), Screen::Empty);
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .any(|(stage, _, _)| *stage == OpenStage::BuildStructureTree),
        "the thread went on to build the structure tree"
    );
}

#[test]
fn cancelling_while_a_document_is_open_keeps_that_document() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    let held = Arc::new(AtomicBool::new(true));
    let (observer, _seen) = holding(Arc::clone(&held), first_page);
    {
        let state = &mut harness.state_mut().state;
        state.opening.card_delay = Duration::ZERO;
        state.opening.observer = Some(observer);
    }
    // A drop replaces the document of this window.
    common::drop_file(&mut harness, &fixture_path("long-doc"));
    wait_until(&mut harness, "the card", |state| {
        state.screen() == Screen::Opening
    });
    // While the new file opens, the title bar names it, not the old one.
    assert_eq!(texts_in(&harness, Region::Chrome, Role::Label)[0], "long-doc.pdf");

    harness.get_by_label("Cancel").click();
    harness.run();
    held.store(false, Ordering::SeqCst);

    let state = &harness.state().state;
    assert_eq!(state.screen(), Screen::Workspace);
    assert_eq!(
        state.document.as_ref().map(|d| d.info.name.clone()),
        Some("minimal-one-page.pdf".to_owned())
    );
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)[0],
        "minimal-one-page.pdf"
    );
}

#[test]
fn a_quick_open_does_not_flash_the_card() {
    let mut harness = app();
    let held = Arc::new(AtomicBool::new(true));
    let (observer, _seen) = holding(Arc::clone(&held), |stage, done, _| {
        stage == OpenStage::ReadStructure && done == 0
    });
    let path = fixture_path("minimal-one-page");
    {
        let state = &mut harness.state_mut().state;
        state.picker = Box::new(move || Some(path.clone()));
        state.opening.card_delay = Duration::from_millis(250);
        state.opening.observer = Some(observer);
    }
    harness.get_by_label("Open file…").click();
    harness.run();

    // The job runs, but until the delay has passed the window still shows the drop zone.
    assert!(harness.state().state.opening.job().is_some());
    assert_eq!(harness.state().state.screen(), Screen::Empty);
    assert!(!has_label(&harness, "Opening minimal-one-page.pdf"));

    // A slow open does get its card.
    wait_until(&mut harness, "the card", |state| {
        state.screen() == Screen::Opening
    });
    assert!(has_label(&harness, "Opening minimal-one-page.pdf"));
    held.store(false, Ordering::SeqCst);
    wait_for_open(&mut harness);
    assert_eq!(harness.state().state.screen(), Screen::Workspace);
}

#[test]
fn an_encrypted_file_gets_an_explainer_with_its_settings_until_a_rail_entry_is_picked() {
    let mut harness = app();

    open_fixture(&mut harness, "encrypted-stub");

    assert_eq!(harness.state().state.screen(), Screen::Encrypted);
    let labels = all_label_texts(&harness);
    let at = labels.iter().position(|t| t == encrypted::TITLE).unwrap();
    assert_eq!(
        labels[at..at + 18],
        [
            encrypted::TITLE,
            encrypted::SUBTITLE,
            encrypted::EXPLANATION,
            "/Encrypt  6 0 R",
            "Filter",
            "/Standard (password based)",
            "Version",
            "V 1 · R 2 · RC4, 40-bit",
            "User password",
            "required",
            "Permissions",
            "print ✓ · copy ✓ · edit ×",
            "Metadata",
            "encrypted",
            "Objects & xref",
            "Page boxes",
            "Rendering",
            "Tags & text",
        ]
    );
    assert_eq!(
        encrypted::CAPABILITIES,
        [
            ("Objects & xref", true),
            ("Page boxes", true),
            ("Rendering", false),
            ("Tags & text", false)
        ]
    );
    let _browse = harness.get_by_role_and_label(Role::Button, "Browse objects");
    let _boxes = harness.get_by_role_and_label(Role::Button, "Page boxes");
    // The title bar chip says it is encrypted, and claims nothing about tags.
    assert_eq!(
        texts_in(&harness, Region::Chrome, Role::Label)[0],
        "encrypted-stub.pdf"
    );
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.7 · Encrypted · rev 1/1",
    );
    // No navigator, inspector or drawer yet, and no layer modes.
    assert!(!has_label(&harness, "Structure"));
    assert!(harness.query_by_label("Hit stack").is_none());
    assert!(harness.query_by_label("All").is_none());
    // Smoke: the table is what the core reads from the file.
    let info = harness
        .state()
        .state
        .document
        .as_ref()
        .unwrap()
        .encryption
        .clone()
        .unwrap();
    assert_eq!(info.algorithm, "RC4 40-bit");
    assert_eq!(info.user_password_empty, Some(false));

    harness
        .get_by_role_and_label(Role::Button, "Browse objects")
        .click();
    harness.run();

    let state = &harness.state().state;
    assert_eq!(state.screen(), Screen::Workspace);
    assert_eq!(state.nav.rail, RailItem::Objects);
    assert!(is_selected(&the(&harness, Region::Rail, Role::Button, "Objects")));
    assert!(has_label(&harness, "Objects"));
    assert!(!has_label(&harness, encrypted::TITLE));
}

#[test]
fn the_page_boxes_button_of_the_explainer_opens_the_pages_entry() {
    let mut harness = app();
    open_fixture(&mut harness, "encrypted-stub");

    harness.get_by_role_and_label(Role::Button, "Page boxes").click();
    harness.run();

    assert_eq!(harness.state().state.screen(), Screen::Workspace);
    assert_eq!(harness.state().state.nav.rail, RailItem::Pages);
    assert_eq!(harness.state().state.drawer.tab, DrawerTab::Diagnostics);
}

#[test]
fn any_rail_entry_leaves_the_explainer() {
    let mut harness = app();
    open_fixture(&mut harness, "encrypted-stub");

    common::click_rail(&mut harness, "Tags");

    assert_eq!(harness.state().state.screen(), Screen::Workspace);
    assert!(harness.state().state.document.is_some());
    assert!(has_label(&harness, "Structure"));
}

/// `broken-xref-offsets` with a repeated `/Type` in the catalog (a note, which comes first in the
/// file) and every later object moved (warnings).
fn damaged_file() -> PathBuf {
    let source = std::fs::read(fixture_path("broken-xref-offsets")).unwrap();
    let from = b"<< /Type /Catalog /Pages 2 0 R >>";
    let at = source.windows(from.len()).position(|w| w == from).unwrap();
    let mut bytes = source[..at].to_vec();
    bytes.extend_from_slice(b"<< /Type /Catalog /Type /Catalog /Pages 2 0 R >>");
    bytes.extend_from_slice(&source[at + from.len()..]);
    let path = std::env::temp_dir().join(format!("pdfxray-states-damaged-{}.pdf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn drawer_codes(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    texts_in(harness, Region::Drawer, Role::Label)
        .into_iter()
        .filter(|t| {
            matches!(
                t.as_str(),
                "xref-offset-wrong" | "startxref-wrong" | "duplicate-dict-key"
            )
        })
        .collect()
}

#[test]
fn a_damaged_index_gets_a_banner_a_chip_and_a_button_to_the_worst_problems_first() {
    let path = damaged_file();
    let mut harness = app();
    harness.state_mut().state.drawer.height = 320.0;
    harness.state_mut().state.picker = Box::new({
        let path = path.clone();
        move || Some(path.clone())
    });
    harness.get_by_label("Open file…").click();
    harness.run();
    wait_for_open(&mut harness);
    common::wait_for_render(&mut harness);
    let info = harness.state().state.document.as_ref().unwrap().info.clone();
    let damage = broken::damage(&info).unwrap();
    assert_eq!((damage.moved, damage.problems), (4, 6));
    // The banner says what is wrong, with the count, and where to look.
    let centre = texts_in(&harness, Region::Center, Role::Label);
    assert!(
        centre.contains(&"This file's index is damaged".to_owned()),
        "{centre:?}"
    );
    assert!(centre.contains(
        &"4 objects aren't where the xref says. Other viewers silently search for them; pdfxray shows the damage."
            .to_owned()
    ));
    let _view = harness.get_by_label("View 6 problems");
    // The title bar chip carries a compact warning, and its popover spells the damage out.
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(texts_in(&harness, Region::Chrome, Role::Label)[0], name);
    the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.4 · Untagged · damaged · rev 1/1",
    )
    .click();
    harness.run();
    let labels = common::all_label_texts(&harness);
    let at = labels.iter().position(|t| t == "Damage").unwrap();
    assert_eq!(labels[at + 1], "damaged xref · 6 problems");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.query_by_label("FILE").is_none());

    // Before the button, the drawer lists the file's order: the note first. A damaged file
    // opens the drawer on its problems by itself.
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Diagnostics"
    )));
    assert_eq!(
        drawer_codes(&harness),
        [
            "duplicate-dict-key",
            "xref-offset-wrong",
            "xref-offset-wrong",
            "xref-offset-wrong",
            "xref-offset-wrong",
            "startxref-wrong"
        ]
    );
    assert_eq!(
        pdfxray_app::drawer::diagnostics::header_note(&harness.state().state),
        None
    );
    common::click_drawer_tab(&mut harness, "Hit stack");

    harness.get_by_label("View 6 problems").click();
    harness.run();

    let state = &harness.state().state;
    assert_eq!(state.drawer.tab, DrawerTab::Diagnostics);
    assert!(state.drawer.sorted_by_severity);
    assert!(is_selected(&the(
        &harness,
        Region::Drawer,
        Role::Button,
        "Diagnostics"
    )));
    assert_eq!(
        pdfxray_app::drawer::diagnostics::header_note(&harness.state().state).as_deref(),
        Some("sorted by severity")
    );
    // The warnings come first, in the file's order, and the note last.
    assert_eq!(
        drawer_codes(&harness),
        [
            "xref-offset-wrong",
            "xref-offset-wrong",
            "xref-offset-wrong",
            "xref-offset-wrong",
            "startxref-wrong",
            "duplicate-dict-key"
        ]
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_sound_file_has_no_banner_and_no_warning_in_its_chip() {
    let mut harness = app();

    open_fixture(&mut harness, "minimal-one-page");

    assert!(harness.query_by_label("View 0 problems").is_none());
    assert!(!has_label(&harness, broken::TITLE));
    let _chip = the(
        &harness,
        Region::Chrome,
        Role::Button,
        "PDF 1.4 · Untagged · rev 1/1",
    );
}

/// The baseline of the text shape that draws `text`, in window coordinates.
fn baseline_of(harness: &Harness<'_, PdfxrayApp>, text: &str) -> f32 {
    fn find(shape: &egui::Shape, text: &str) -> Option<f32> {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, text)),
            egui::Shape::Text(t) if t.galley.text() == text => {
                let row = t.galley.rows.first()?;
                Some(t.pos.y + row.pos.y + row.glyphs.first()?.pos.y)
            }
            _ => None,
        }
    }
    harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| find(&clipped.shape, text))
        .unwrap_or_else(|| panic!("no text {text:?}"))
}

#[test]
fn the_encrypt_table_sets_each_value_on_the_baseline_of_its_label() {
    let mut harness = app();

    open_fixture(&mut harness, "encrypted-stub");

    for (key, value) in [
        ("Filter", "/Standard (password based)"),
        ("Permissions", "print ✓ · copy ✓ · edit ×"),
        ("Metadata", "encrypted"),
    ] {
        let (label, text) = (baseline_of(&harness, key), baseline_of(&harness, value));
        assert!(
            (label - text).abs() < 0.01,
            "{key}: baseline {label} against {text}"
        );
    }
    // Smoke: the rows are still a row apart.
    let (first, second) = (baseline_of(&harness, "Filter"), baseline_of(&harness, "Version"));
    assert!((second - first - 28.0).abs() < 8.0, "{first} {second}");
}
