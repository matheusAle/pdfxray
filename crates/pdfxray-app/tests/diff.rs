//! The comparison (diff mode): how B arrives (the `--compare` launch argument, drops, ⌘R), the
//! Changes list as the primary view, the composite canvas with its chips, per-side selection,
//! the geometry banner and its opt-in, and pixel snapshots of the new surfaces.
//!
//! Update the PNGs with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test diff`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use common::{Region, builder, buttons_in, drop_file, is_selected, the, wait_until};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::center::canvas::diff::{SCALE_B_LABEL, UNSCALE_LABEL, canvas_label};
use pdfxray_app::navigator::changes::UNCHANGED_LABEL;
use pdfxray_app::state::ChangeRow;
use pdfxray_app::{AppState, Launch, MenuCommand, PdfxrayApp, RailItem};
use pdfxray_core::NodeId;
use pdfxray_core::diff::{Side, Status};
use pdfxray_render::composite::{Fit, Mode};

/// The committed `(a.pdf, b.pdf)` of fixture pair `name`.
fn pair(name: &str) -> (PathBuf, PathBuf) {
    pdfxray_fixtures::pairs::pair(name)
}

/// The app launched as `pdfxray-app a --compare b`, run until the pairing and the first
/// composite are there.
fn launched(a: &Path, b: &Path) -> Harness<'static, PdfxrayApp> {
    let launch = Launch::parse(
        [a.as_os_str(), "--compare".as_ref(), b.as_os_str()]
            .into_iter()
            .map(Into::into),
    );
    let mut harness = builder().build_eframe(move |cc| {
        PdfxrayApp::new(cc)
            .with_document(launch.document.clone())
            .with_comparison(launch.compare.clone())
    });
    settle(&mut harness);
    harness
}

/// Runs until no file is opening, the pairing is done and the canvas shows its picture. A frame
/// runs first, so what the last input changed has been asked for.
fn settle(harness: &mut Harness<'_, PdfxrayApp>) {
    harness.step();
    wait_until(harness, "the comparison to settle", |s| {
        s.opening.job().is_none()
            && s.diff.as_ref().is_some_and(|d| {
                // The picture shown is the one of the page pair and mode the session is on.
                let current = d
                    .shown_picture()
                    .is_some_and(|(a, b, mode, _)| d.pages() == Some((a, b)) && mode == d.mode);
                d.pairing().is_some() && !d.is_drawing() && current
            })
    });
    // The status bar prints how long opening took; fixed, so snapshots are the same every run.
    let state = &mut harness.state_mut().state;
    if let Some(loaded) = state.document.as_mut() {
        loaded.opened_in = Duration::from_millis(310);
    }
    if let Some(diff) = state.diff.as_mut() {
        diff.b.opened_in = Duration::from_millis(310);
    }
    harness.run();
}

fn state<'h>(harness: &'h Harness<'_, PdfxrayApp>) -> &'h AppState {
    &harness.state().state
}

/// A copy of `a` whose pages are 20 pt taller (`300 400` becomes `300 420`, the same number of
/// bytes, so every offset stays right): the same content on pages of another size.
fn taller(a: &Path) -> PathBuf {
    let bytes = std::fs::read(a).unwrap();
    let (from, to) = (b"/MediaBox [0 0 300 400]", b"/MediaBox [0 0 300 420]");
    assert!(
        bytes.windows(from.len()).any(|w| w == from),
        "the fixture's pages are 300 x 400"
    );
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(from) {
            out.extend_from_slice(to);
            at += from.len();
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    let dir = std::env::temp_dir().join(format!("pdfxray-diff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("taller.pdf");
    std::fs::write(&path, out).unwrap();
    path
}

#[test]
fn launch_arguments_name_the_document_and_what_to_compare_it_with() {
    let args = |list: &[&str]| Launch::parse(list.iter().map(Into::into));

    let both = args(&["a.pdf", "--compare", "b.pdf"]);
    assert_eq!(both.document, Some(PathBuf::from("a.pdf")));
    assert_eq!(both.compare, Some(PathBuf::from("b.pdf")));
    assert_eq!(args(&["--compare", "b.pdf", "a.pdf"]), both);
    assert_eq!(
        args(&["-psn_0_123", "a.pdf"]),
        Launch {
            document: Some(PathBuf::from("a.pdf")),
            compare: None,
        }
    );
    // smoke: no arguments, nothing to open
    assert_eq!(args(&[]), Launch::default());
}

#[test]
fn a_launch_with_compare_enters_side_by_side_on_the_changes_list() {
    let (a, b) = pair("change-heading-level");
    let harness = launched(&a, &b);

    let s = state(&harness);
    let diff = s.diff.as_ref().expect("diff mode");
    assert_eq!(diff.mode, Mode::SideBySide);
    assert_eq!(diff.fit, Fit::Refuse);
    assert_eq!(s.nav.rail, RailItem::Changes);
    assert_eq!(diff.b.path, b);
    assert_eq!(s.document.as_ref().map(|l| l.path.clone()), Some(a));
    let summary = diff.pairing().expect("paired").summary();
    assert_eq!((summary.modified, summary.added, summary.removed), (1, 0, 0));
    // The Changes entry leads the rail and is the one picked.
    let rail = buttons_in(&harness, Region::Rail);
    assert_eq!(rail.first().map(String::as_str), Some("Changes"));
    assert!(is_selected(&the(&harness, Region::Rail, Role::Button, "Changes")));
    // The canvas opens on the composite of the page pair the change is on: the second.
    assert_eq!(diff.pages(), Some((Some(1), Some(1))));
    assert!(harness.query_by_label(&canvas_label(Some(1), Some(1))).is_some());
    // smoke: the Changes list names the page of the change and the change itself
    let rows = buttons_in(&harness, Region::Navigator);
    assert!(rows.iter().any(|r| r.contains("Page 2")), "{rows:?}");
}

#[test]
fn the_changes_list_drives_the_canvas_and_the_inspector() {
    let (a, b) = pair("change-heading-level");
    let mut harness = launched(&a, &b);

    let index = {
        let diff = state(&harness).diff.as_ref().unwrap();
        diff.pairing()
            .unwrap()
            .pairs
            .iter()
            .position(|p| p.status == Status::Modified)
            .unwrap()
    };
    harness.state_mut().state.select_change(ChangeRow::Pair(index));
    harness.run();
    settle(&mut harness);

    let s = state(&harness);
    let diff = s.diff.as_ref().unwrap();
    assert_eq!(diff.selected_index(), Some(index));
    // The method heading is on the second page of both: the canvas followed.
    assert_eq!(diff.pages(), Some((Some(1), Some(1))));
    assert_eq!(
        diff.shown_picture().map(|(a, b, mode, _)| (a, b, mode)),
        Some((Some(1), Some(1), Mode::SideBySide))
    );
    // Each side's chip names its own element's role, coloured by status: one button per side.
    assert!(harness.query_by_label("H2, modified, A").is_some());
    assert!(harness.query_by_label("H3, modified, B").is_some());
    // smoke: the inspector shows the pair, with the typed change
    assert!(common::has_label(&harness, "Changed"));
    assert!(common::has_label(&harness, "Role"));
}

#[test]
fn a_chip_of_b_selects_b_s_element_even_when_a_has_the_same_object_number() {
    let (a, b) = pair("change-heading-level");
    let mut harness = launched(&a, &b);
    let index = state(&harness)
        .diff
        .as_ref()
        .unwrap()
        .pairing()
        .unwrap()
        .pairs
        .iter()
        .position(|p| p.status == Status::Modified)
        .unwrap();
    harness.state_mut().state.select_change(ChangeRow::Pair(index));
    settle(&mut harness);

    harness.get_by_label("H3, modified, B").click();
    harness.run();

    let s = state(&harness);
    let pair = s.diff.as_ref().unwrap().selected().unwrap().clone();
    // Both elements are object 17 in their files: only the side tells them apart.
    assert_eq!(pair.a, pair.b);
    assert_eq!(s.selection.side(), Side::B);
    assert_eq!(s.selection.primary(), pair.b.as_ref());
    // smoke: the A chip selects A's
    harness.get_by_label("H2, modified, A").click();
    harness.run();
    assert_eq!(state(&harness).selection.side(), Side::A);
}

#[test]
fn a_click_on_b_s_page_fills_the_hit_stack_from_b() {
    let (a, b) = pair("drop-alt");
    let mut harness = launched(&a, &b);
    // Small enough that both pages are in view.
    harness.state_mut().state.canvas.zoom = 0.5;
    settle(&mut harness);
    let (page_a, page_b) = state(&harness).diff.as_ref().unwrap().pages().unwrap();
    let canvas = harness.get_by_label(&canvas_label(page_a, page_b)).rect();

    // The right half of a side by side composite is B's page.
    let on_b = egui::pos2(canvas.min.x + canvas.width() * 0.75, canvas.center().y);
    common::click_at(&mut harness, on_b);

    let s = state(&harness);
    let stack = s.drawer.hit_stack.as_ref().expect("a hit stack");
    assert_eq!(stack.side, Side::B);
    assert_eq!(Some(stack.page), page_b);
    assert_eq!(s.selection.side(), Side::B);
    // smoke: B's page is under the pointer
    assert!(
        stack
            .entries
            .iter()
            .any(|e| Some(e.id.clone()) == page_b.map(NodeId::Page)),
        "{:?}",
        stack.entries
    );
}

#[test]
fn a_plain_drop_replaces_b_and_an_option_drop_replaces_a() {
    let (a, b) = pair("drop-alt");
    let (_, other) = pair("change-heading-level");
    let mut harness = launched(&a, &b);

    drop_file(&mut harness, &other);
    settle(&mut harness);
    let s = state(&harness);
    assert_eq!(s.diff.as_ref().map(|d| d.b.path.clone()), Some(other.clone()));
    assert_eq!(s.document.as_ref().map(|l| l.path.clone()), Some(a.clone()));

    // ⌥ held while the file is dropped.
    harness
        .input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::ALT));
    drop_file(&mut harness, &b);
    harness
        .input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    settle(&mut harness);
    let s = state(&harness);
    assert_eq!(s.document.as_ref().map(|l| l.path.clone()), Some(b));
    assert_eq!(s.diff.as_ref().map(|d| d.b.path.clone()), Some(other));
    // smoke: still comparing, on the Changes list
    assert_eq!(s.nav.rail, RailItem::Changes);
}

#[test]
fn reload_reads_b_again_and_pairs_again() {
    let (a, b) = pair("drop-alt");
    let mut harness = launched(&a, &b);
    let before = Arc::clone(&state(&harness).diff.as_ref().unwrap().b.doc);

    let ctx = harness.ctx.clone();
    MenuCommand::Reload.apply(&mut harness.state_mut().state, &ctx);
    settle(&mut harness);

    let diff = state(&harness).diff.as_ref().unwrap();
    assert!(!Arc::ptr_eq(&before, &diff.b.doc), "B was read again");
    assert_eq!(diff.b.path, b);
    // smoke: the new pairing is the same diff
    assert_eq!(diff.pairing().unwrap().summary().modified, 1);
}

#[test]
fn pages_of_another_size_are_not_overlaid_until_scaling_is_chosen() {
    let (a, _) = pair("drop-alt");
    let b = taller(&a);
    let mut harness = launched(&a, &b);
    let (wa, wb) = {
        let s = state(&harness);
        let dims = |doc: &pdfxray_core::Document| pdfxray_render::page_dimensions(doc, 0).unwrap();
        (
            dims(&s.document.as_ref().unwrap().doc),
            dims(&s.diff.as_ref().unwrap().b.doc),
        )
    };
    assert_ne!(wa, wb, "the fixtures must differ in page size");

    harness.state_mut().state.diff.as_mut().unwrap().mode = Mode::Overlay;
    harness.run();
    wait_until(&mut harness, "the refusal", |s| {
        !s.diff.as_ref().unwrap().is_drawing()
    });
    harness.run();
    assert!(common::has_label(
        &harness,
        pdfxray_app::center::canvas::diff::GEOMETRY_TITLE
    ));
    assert_eq!(state(&harness).diff.as_ref().unwrap().fit, Fit::Refuse);

    harness.get_by_label(SCALE_B_LABEL).click();
    settle(&mut harness);
    let diff = state(&harness).diff.as_ref().unwrap();
    assert_eq!(diff.fit, Fit::ScaleBToA);
    assert_eq!(
        diff.shown_picture().map(|(_, _, mode, _)| mode),
        Some(Mode::Overlay)
    );
    // smoke: scaled is said, with the way back
    assert!(harness.query_by_label(UNSCALE_LABEL).is_some());
}

#[test]
fn unchanged_pairs_hide_until_asked_and_artifact_changes_stay_one_row_per_page() {
    let (a, b) = pair("artifact-page-number");
    let mut harness = launched(&a, &b);

    let rows = buttons_in(&harness, Region::Navigator);
    let artifacts = rows.iter().filter(|r| r.starts_with("Artifacts")).count();
    let pages = state(&harness).page_count();
    assert_eq!(artifacts, pages, "{rows:?}");
    assert!(!state(&harness).diff.as_ref().unwrap().show_unchanged);

    harness.get_by_label(UNCHANGED_LABEL).click();
    harness.run();
    let shown = buttons_in(&harness, Region::Navigator);
    assert!(state(&harness).diff.as_ref().unwrap().show_unchanged);
    // smoke: the unchanged pairs add rows
    assert!(shown.len() > rows.len(), "{shown:?} vs {rows:?}");
}

#[test]
fn the_tag_overlay_keys_act_on_the_comparison_and_leave_a_s_view_alone() {
    let (a, b) = pair("drop-alt");
    let mut harness = launched(&a, &b);
    let before = state(&harness).canvas.view.show;

    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::T);
    harness.key_press(egui::Key::L);
    harness.key_press(egui::Key::R);
    harness.run();

    let s = state(&harness);
    let diff = s.diff.as_ref().unwrap();
    assert!(diff.show.unchanged, "⇧T outlines the unchanged elements too");
    assert!(!diff.show.labels, "L takes the chips away");
    assert!(diff.order, "R numbers the reading order");
    // smoke: the View menu's own options, which A alone is shown with, did not move
    assert_eq!(s.canvas.view.show, before);
}

#[test]
fn ending_the_comparison_keeps_a() {
    let (a, b) = pair("drop-alt");
    let mut harness = launched(&a, &b);

    harness
        .get_by_label(pdfxray_app::center::canvas::diff::END_LABEL)
        .click();
    harness.run();

    let s = state(&harness);
    assert!(s.diff.is_none());
    assert_eq!(s.nav.rail, RailItem::Tags);
    assert_eq!(s.document.as_ref().map(|l| l.path.clone()), Some(a));
    // smoke: the Changes entry is gone from the rail
    assert!(!buttons_in(&harness, Region::Rail).contains(&"Changes".to_owned()));
}

#[test]
fn snapshot_diff_side_by_side() {
    let _gpu = common::gpu_lock();
    let (a, b) = pair("change-heading-level");
    let mut harness = launched(&a, &b);
    let index = state(&harness)
        .diff
        .as_ref()
        .unwrap()
        .pairing()
        .unwrap()
        .pairs
        .iter()
        .position(|p| p.status == Status::Modified)
        .unwrap();
    harness.state_mut().state.select_change(ChangeRow::Pair(index));
    settle(&mut harness);
    harness.snapshot("app_diff_side_by_side");
}

#[test]
fn snapshot_diff_overlay_with_reading_order() {
    let _gpu = common::gpu_lock();
    let (a, b) = pair("move-paragraph-3pt");
    let mut harness = launched(&a, &b);
    {
        let diff = harness.state_mut().state.diff.as_mut().unwrap();
        diff.mode = Mode::Overlay;
        diff.order = true;
    }
    settle(&mut harness);
    harness.snapshot("app_diff_overlay");
}
