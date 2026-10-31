//! The table overlay (grid, selected cell, header connectors, cells without a header), the
//! inspector's Table section and the footnote connectors.
//!
//! Update the snapshot with `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test table_overlay snapshot_table_overlay`.

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

use std::path::PathBuf;

use common::{Region, app, buttons_in, is_selected, open_fixture, open_path, texts_in, the, wait_for_render};
use egui::epaint::{ClippedShape, Shape};
use egui::{Key, Modifiers, Role};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::{DrawerTab, PdfxrayApp};
use pdfxray_core::{NodeId, ObjRef};
use pdfxray_fixtures::{PdfVersion, PdfWriter, Trailer};
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

fn elem(n: u32) -> NodeId {
    NodeId::StructElem(ObjRef::new(n, 0))
}

fn select(harness: &mut App, node: NodeId) {
    harness.state_mut().state.select(node);
    settle(harness);
}

/// The names of the buttons on the canvas that start with `prefix`, sorted.
fn on_canvas(harness: &App, prefix: &str) -> Vec<String> {
    let mut found: Vec<String> = buttons_in(harness, Region::Center)
        .into_iter()
        .filter(|name| name.starts_with(prefix))
        .collect();
    found.sort();
    found
}

/// The connectors from a cell to its headers.
fn connectors(harness: &App) -> Vec<String> {
    on_canvas(harness, "Header via ")
}

/// The markers of the cells that cannot be read with a header.
fn unresolved(harness: &App) -> Vec<String> {
    on_canvas(harness, "No header")
}

// ---- header connectors ----

#[test]
fn a_selected_cell_is_joined_to_each_header_by_the_rule_that_found_it_pdf_2() {
    let mut harness = opened("table-scope-pdf2");

    // The TD `1.2`: the column header Q1 and the row header North, both by their Scope.
    select(&mut harness, elem(17));

    assert_eq!(
        connectors(&harness),
        ["Header via Scope: North", "Header via Scope: Q1"]
    );
    // Smoke: every cell can be read with a header, and a header cell of the first row has none.
    assert_eq!(unresolved(&harness), Vec::<String>::new());
    select(&mut harness, elem(13));
    assert_eq!(connectors(&harness), Vec::<String>::new());
}

#[test]
fn a_header_found_by_position_is_labelled_so_in_a_pdf_1_7_table() {
    let mut harness = opened("table-simple-pdf17");

    // The TD `1.2` sits under the TH `Q1`, which has no Scope.
    select(&mut harness, elem(16));

    assert_eq!(connectors(&harness), ["Header via position: Q1"]);
    assert_eq!(unresolved(&harness), Vec::<String>::new());
}

#[test]
fn explicit_headers_ids_are_solid_and_an_id_no_cell_has_marks_the_cell_amber() {
    let mut harness = opened("table-headers-ids");

    // The TD `1.2` names North, Q1 and Sales in its /Headers.
    select(&mut harness, elem(18));
    assert_eq!(
        connectors(&harness),
        [
            "Header via Headers: North",
            "Header via Headers: Q1",
            "Header via Headers: Sales"
        ]
    );
    // The TD `1.1` names an ID that no cell has: it is the one cell marked.
    assert_eq!(unresolved(&harness), ["No header: 1.1"]);

    // A header that is read with another by position: North under Region.
    select(&mut harness, elem(17));
    assert_eq!(connectors(&harness), ["Header via position: Region"]);
    // Smoke: the mark stays while anything of the table is selected.
    assert_eq!(unresolved(&harness), ["No header: 1.1"]);
}

#[test]
fn a_selected_table_shows_its_grid_and_marks_without_connectors_and_a_click_on_a_mark_selects_the_cell() {
    let mut harness = opened("table-headers-ids");

    select(&mut harness, elem(8));

    assert_eq!(connectors(&harness), Vec::<String>::new());
    assert_eq!(unresolved(&harness), ["No header: 1.1"]);
    the(&harness, Region::Center, Role::Button, "No header: 1.1").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.selection.primary, Some(elem(22)));
    // Now it has its connectors as well.
    assert_eq!(connectors(&harness).len(), 3);
}

/// The shapes painted last frame, flattened, in paint order.
fn painted(harness: &App) -> Vec<Shape> {
    fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
        if let Shape::Vec(shapes) = shape {
            for shape in shapes {
                flatten(shape, out);
            }
        } else {
            out.push(shape.clone());
        }
    }
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut out);
    }
    out
}

#[test]
fn the_selected_cell_outline_is_painted_under_the_tag_chips_not_over_them() {
    let mut harness = opened("table-headers-ids");
    harness.key_press_modifiers(Modifiers::SHIFT, Key::T);
    // The TD `1.2`: its outline is the accent one the table overlay paints.
    select(&mut harness, elem(18));

    let shapes = painted(&harness);
    let chip = shapes
        .iter()
        // On the canvas, not the Tags tree row of the same name.
        .position(|shape| {
            matches!(shape, Shape::Text(text) if text.galley.text() == "TD" && (352.0..1100.0).contains(&text.pos.x))
        })
        .expect("the chip of a cell");
    let outlines: Vec<usize> = shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| {
            matches!(shape, Shape::Rect(rect)
                if rect.stroke.color == pdfxray_app::theme::tokens::ACCENT && rect.stroke.width == 2.0)
        })
        .map(|(at, _)| at)
        .collect();

    // Every accent outline of the selection is under the first chip.
    assert!(outlines.len() >= 2, "{outlines:?}");
    assert!(
        outlines.iter().all(|at| *at < chip),
        "{outlines:?} and a chip at {chip}"
    );
    // Smoke: the overlay still marks the cell without a header.
    assert_eq!(unresolved(&harness), ["No header: 1.1"]);
}

#[test]
fn clicking_a_connector_label_selects_the_header_it_points_to() {
    let mut harness = opened("table-scope-pdf2");
    select(&mut harness, elem(17));

    the(&harness, Region::Center, Role::Button, "Header via Scope: Q1").click();
    settle(&mut harness);

    assert_eq!(harness.state().state.selection.primary, Some(elem(14)));
    // Selecting a header cell keeps the table: its own headers show.
    assert_eq!(connectors(&harness), Vec::<String>::new());
}

#[test]
fn a_connector_label_is_a_hand_and_lights_its_border_under_the_pointer() {
    let mut harness = opened("table-scope-pdf2");
    select(&mut harness, elem(17));
    let label = the(&harness, Region::Center, Role::Button, "Header via Scope: Q1").rect();
    // The strongest strokes of the label's rectangle: full alpha under the pointer, softer off it.
    let border_alpha = |harness: &App| {
        harness
            .output()
            .shapes
            .iter()
            .flat_map(|clipped| match &clipped.shape {
                egui::Shape::Vec(shapes) => shapes.clone(),
                other => vec![other.clone()],
            })
            .filter_map(|shape| match shape {
                egui::Shape::Rect(rect)
                    if rect.rect.size().x > 20.0 && label.expand(0.5).contains_rect(rect.rect) =>
                {
                    Some(rect.stroke.color.a())
                }
                _ => None,
            })
            .max()
            .unwrap_or(0)
    };
    harness.hover_at(label.center() - egui::vec2(0.0, 80.0));
    harness.run_steps(2);
    let resting = border_alpha(&harness);

    harness.hover_at(label.center());
    harness.run_steps(2);

    assert_eq!(
        harness.output().platform_output.cursor_icon,
        egui::CursorIcon::PointingHand
    );
    assert!(
        border_alpha(&harness) > resting,
        "{resting} -> {}",
        border_alpha(&harness)
    );
}

#[test]
fn what_is_inside_a_cell_counts_as_the_cell_and_what_is_outside_a_table_has_no_overlay() {
    let mut harness = opened("table-headers-ids");

    // The marked content the TD `1.1` was painted by.
    select(&mut harness, NodeId::MarkedContent { page: 0, mcid: 9 });
    assert_eq!(connectors(&harness).len(), 3);

    let mut outside = opened("reading-order");
    select(&mut outside, elem(9));
    assert_eq!(connectors(&outside), Vec::<String>::new());
    assert_eq!(unresolved(&outside), Vec::<String>::new());
    // Smoke: no Table section either.
    assert!(
        outside
            .query_all_by_role_and_label(Role::Button, "Table")
            .next()
            .is_none()
    );
}

// ---- the inspector ----

/// The cells of the inspector's mini grid, in order.
fn grid(harness: &App) -> Vec<String> {
    buttons_in(harness, Region::Inspector)
        .into_iter()
        .filter(|name| name.starts_with("Cell row") || name.starts_with("Header cell row"))
        .collect()
}

#[test]
fn the_inspector_draws_the_grid_with_headers_and_the_selection_and_names_the_pdf_1_7_rules() {
    let mut harness = opened("table-simple-pdf17");

    select(&mut harness, elem(16));

    assert_eq!(
        grid(&harness),
        [
            "Header cell row 1, column 1: Region",
            "Header cell row 1, column 2: Q1",
            "Header cell row 1, column 3: Q2",
            "Cell row 2, column 1: North",
            "Cell row 2, column 2: 1.2",
            "Cell row 2, column 3: 1.4",
            "Cell row 3, column 1: South",
            "Cell row 3, column 2: 0.9",
            "Cell row 3, column 3: 1.1",
        ]
    );
    assert!(is_selected(&the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Cell row 2, column 2: 1.2"
    )));
    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(labels.contains(&"3 rows × 3 columns".to_owned()), "{labels:?}");
    assert!(labels.contains(&"PDF 1.7 rules".to_owned()), "{labels:?}");
    // The header read with the cell, and the rule that found it.
    let links = texts_in(&harness, Region::Inspector, Role::Link);
    assert!(links.contains(&"Q1 · row 1, col 2".to_owned()), "{links:?}");
    // Only the one header the cell is read with is a link of the grid section.
    assert_eq!(
        links.iter().filter(|l| l.contains("row ")).count(),
        1,
        "{links:?}"
    );
    assert!(labels.contains(&"position".to_owned()), "{labels:?}");
}

#[test]
fn the_inspector_names_the_pdf_2_algorithm_and_lists_both_headers() {
    let mut harness = opened("table-scope-pdf2");

    select(&mut harness, elem(17));

    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(labels.contains(&"PDF 2.0 algorithm".to_owned()), "{labels:?}");
    assert!(!labels.contains(&"PDF 1.7 rules".to_owned()), "{labels:?}");
    assert_eq!(
        texts_in(&harness, Region::Inspector, Role::Link),
        ["Q1 · row 1, col 2", "North · row 2, col 1"]
    );
    // The row header is a header cell of the grid, in this table.
    assert!(grid(&harness).contains(&"Header cell row 2, column 1: North".to_owned()));
    // Smoke: the rule of each is Scope.
    assert_eq!(labels.iter().filter(|l| *l == "Scope").count(), 2);
}

#[test]
fn a_header_id_no_cell_has_is_named_in_the_inspector_and_a_click_in_the_grid_selects_the_cell() {
    let mut harness = opened("table-headers-ids");
    select(&mut harness, elem(22));

    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(
        labels.contains(&"h-missing: no cell has this ID".to_owned()),
        "{labels:?}"
    );
    assert_eq!(
        texts_in(&harness, Region::Inspector, Role::Link),
        [
            "South · row 4, col 1",
            "Q2 · row 2, col 3",
            "Sales · row 1, col 2"
        ]
    );
    // Spans are named: Sales covers two columns, Region two rows.
    assert!(grid(&harness).contains(&"Header cell row 1, column 2, 2 columns wide: Sales".to_owned()));
    assert!(grid(&harness).contains(&"Header cell row 1, column 1, 2 rows tall: Region".to_owned()));

    the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Cell row 3, column 2: 1.2",
    )
    .click();
    settle(&mut harness);
    assert_eq!(harness.state().state.selection.primary, Some(elem(18)));
}

// ---- footnote connectors ----

fn footnote_links(harness: &App) -> Vec<String> {
    on_canvas(harness, "Footnote link")
}

#[test]
fn a_footnote_reference_is_joined_to_its_note_with_how_far_apart_they_are_read() {
    let mut harness = opened("speech-footnote");
    assert_eq!(footnote_links(&harness), Vec::<String>::new());

    // The reading order on.
    harness.key_press(egui::Key::R);
    settle(&mut harness);

    assert_eq!(footnote_links(&harness), ["Footnote link: 1 apart"]);
    // Off again, it goes.
    harness.key_press(egui::Key::R);
    settle(&mut harness);
    assert_eq!(footnote_links(&harness), Vec::<String>::new());
}

#[test]
fn the_speech_tab_or_a_selected_reference_or_note_shows_the_connector_and_other_elements_do_not() {
    let mut harness = opened("speech-footnote");

    // An unrelated paragraph.
    select(&mut harness, elem(10));
    assert_eq!(footnote_links(&harness), Vec::<String>::new());
    // The reference, then the note's label.
    select(&mut harness, elem(9));
    assert_eq!(footnote_links(&harness), ["Footnote link: 1 apart"]);
    select(&mut harness, elem(12));
    assert_eq!(footnote_links(&harness), ["Footnote link: 1 apart"]);
    // The Speech tab shows them whatever is selected.
    select(&mut harness, elem(10));
    harness.state_mut().state.drawer.show_tab(DrawerTab::Speech);
    settle(&mut harness);
    assert_eq!(footnote_links(&harness), ["Footnote link: 1 apart"]);
}

#[test]
fn a_footnote_label_goes_to_the_other_end() {
    let mut harness = opened("speech-footnote");
    select(&mut harness, elem(9));

    the(&harness, Region::Center, Role::Button, "Footnote link: 1 apart").click();
    settle(&mut harness);
    // From the reference to the note.
    assert_eq!(harness.state().state.selection.primary, Some(elem(11)));

    the(&harness, Region::Center, Role::Button, "Footnote link: 1 apart").click();
    settle(&mut harness);
    // And back.
    assert_eq!(harness.state().state.selection.primary, Some(elem(9)));
}

/// A two-page file whose footnote reference is on page 1 and whose note is on page 2.
fn footnote_across_pages() -> PathBuf {
    let text = |tag: &str, mcid: u32, size: u32, x: u32, y: u32, text: &str| {
        format!("/{tag} << /MCID {mcid} >> BDC BT /F1 {size} Tf {x} {y} Td ({text}) Tj ET EMC")
    };
    let page1 = [
        text("P", 0, 12, 40, 340, "Revenue grew in every region."),
        text("Reference", 1, 8, 220, 344, "1"),
        text("P", 2, 12, 40, 310, "Closing remarks follow."),
    ]
    .join("\n");
    let page2 = [
        text("Lbl", 0, 8, 40, 60, "1"),
        text("P", 1, 8, 50, 60, "Source: annual report."),
    ]
    .join("\n");
    let mut w = PdfWriter::new(PdfVersion::V1_7);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R 15 0 R] /Count 2 >>");
    let page = |parent_key: u32, contents: u32| {
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /StructParents {parent_key} \
             /Resources << /Font << /F1 5 0 R >> >> /Contents {contents} 0 R >>"
        )
    };
    w.object(3, page(0, 4));
    w.stream(4, "", page1.as_bytes());
    w.object(5, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 7 0 R /ParentTree 14 0 R /ParentTreeNextKey 2 >>",
    );
    w.object(
        7,
        "<< /Type /StructElem /S /Document /P 6 0 R /K [8 0 R 10 0 R 11 0 R] >>",
    );
    w.object(8, "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K [0 9 0 R] >>");
    w.object(9, "<< /Type /StructElem /S /Reference /P 8 0 R /Pg 3 0 R /K 1 >>");
    w.object(10, "<< /Type /StructElem /S /P /P 7 0 R /Pg 3 0 R /K 2 >>");
    w.object(11, "<< /Type /StructElem /S /Note /P 7 0 R /K [12 0 R 13 0 R] >>");
    w.object(12, "<< /Type /StructElem /S /Lbl /P 11 0 R /Pg 15 0 R /K 0 >>");
    w.object(13, "<< /Type /StructElem /S /P /P 11 0 R /Pg 15 0 R /K 1 >>");
    w.object(14, "<< /Nums [0 [8 0 R 9 0 R 10 0 R] 1 [12 0 R 13 0 R]] >>");
    w.object(15, page(1, 16));
    w.stream(16, "", page2.as_bytes());
    w.finish_table(&Trailer::new(1)).unwrap();
    let bytes = w.into_output().unwrap().bytes;
    let path = std::env::temp_dir().join(format!("pdfxray-footnote-pages-{}.pdf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn a_note_on_another_page_is_joined_on_both_pages_and_says_which() {
    let mut harness = app();
    open_path(&mut harness, footnote_across_pages());
    // Both pages in view.
    harness.state_mut().state.set_zoom(0.45);
    harness.key_press(egui::Key::R);
    settle(&mut harness);
    wait_for_render(&mut harness);
    settle(&mut harness);

    assert_eq!(
        footnote_links(&harness),
        ["Footnote link: from p1 · 1 apart", "Footnote link: p2 · 1 apart"]
    );
    // The label on the reference's page goes to the note, on the other page.
    the(
        &harness,
        Region::Center,
        Role::Button,
        "Footnote link: p2 · 1 apart",
    )
    .click();
    settle(&mut harness);
    assert_eq!(harness.state().state.selection.primary, Some(elem(11)));
    assert_eq!(harness.state().state.canvas.page, 1);
}

#[test]
fn snapshot_table_overlay() {
    let _gpu = common::gpu_lock();
    let mut harness = common::builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "table-headers-ids");
    common::click_rail(&mut harness, "Tags");
    harness.state_mut().state.select(elem(22));
    harness.run_steps(4);
    wait_for_render(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_table_overlay");
}
