//! The table overlay (grid, selected cell, header connectors, cells without a header), header
//! scope (the `TH` chips, the echo and the inspector's Scope rows), the inspector's Table section
//! and the footnote connectors.
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
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::center::canvas::header_scope;
use pdfxray_app::inspector::table;
use pdfxray_app::theme::tokens;
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
    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(22)));
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
    boxes_on(&mut harness);
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

    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(14)));
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
    assert!(
        links.contains(&"position: Q1 · row 1, col 2".to_owned()),
        "{links:?}"
    );
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
        ["Scope: Q1 · row 1, col 2", "Scope: North · row 2, col 1"]
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
            "Headers: South · row 4, col 1",
            "Headers: Q2 · row 2, col 3",
            "Headers: Sales · row 1, col 2"
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
    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(18)));
}

// ---- header scope ----

/// The tag boxes and their labels: `⇧T` then `L`.
fn boxes_on(harness: &mut App) {
    harness.key_press_modifiers(Modifiers::SHIFT, Key::T);
    harness.key_press(Key::L);
    settle(harness);
}

/// The chips of the header cells on the canvas, sorted.
fn header_chips(harness: &App) -> Vec<String> {
    on_canvas(harness, "TH")
}

/// Every shape painted last frame whose ink is `color`, by kind: rectangles outlined in it, and
/// meshes with a vertex of it.
fn rects_stroked(harness: &App, color: egui::Color32, width: f32) -> Vec<egui::Rect> {
    painted(harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rect) if rect.stroke.color == color && rect.stroke.width == width => Some(rect.rect),
            _ => None,
        })
        .collect()
}

/// The bands of the echo painted last frame: meshes that start at 25 % of the table colour.
fn echo_bands(harness: &App) -> usize {
    let near = tokens::ROLE_TABLE.gamma_multiply(0.25);
    painted(harness)
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Mesh(mesh) if mesh.vertices.iter().any(|v| v.color == near)))
        .count()
}

/// The chevrons of the echo painted last frame.
fn echo_chevrons(harness: &App) -> usize {
    painted(harness)
        .into_iter()
        .filter(|shape| {
            matches!(shape, Shape::Path(path)
                if path.points.len() == 3 && path.stroke.color == egui::epaint::ColorMode::Solid(tokens::ROLE_TABLE))
        })
        .count()
}

#[test]
fn a_th_chip_says_its_declared_scope_and_a_conflict_gets_an_amber_ring_and_a_mark() {
    let mut harness = opened("table-scope-conflict");
    assert_eq!(header_chips(&harness), Vec::<String>::new());

    boxes_on(&mut harness);

    assert_eq!(
        header_chips(&harness),
        [
            "TH · Both · declared",
            "TH · Column · declared",
            "TH · Column · declared",
            "TH · Column · declared",
            "TH · Column · declared, conflicts with position",
            "TH · Row · declared",
        ]
    );
    // The ring is on the one chip that conflicts, and the `!` is drawn: not colour alone.
    let south = the(
        &harness,
        Region::Center,
        Role::Button,
        "TH · Column · declared, conflicts with position",
    )
    .rect();
    let rings = rects_stroked(&harness, tokens::WARN, header_scope::CONFLICT_RING);
    assert_eq!(rings, [south]);
    let marks = painted(&harness)
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Text(t) if t.galley.text() == "!" && south.contains(t.pos)))
        .count();
    assert_eq!(marks, 1);
    // Smoke: labels off, the chips go.
    harness.key_press(Key::L);
    settle(&mut harness);
    assert_eq!(header_chips(&harness), Vec::<String>::new());
}

/// The row of the Tag roles card named `role`, e.g. `TH`.
fn role_row<'h>(harness: &'h App, role: &str) -> egui_kittest::Node<'h> {
    let prefix = format!("{role}, ");
    common::nodes_in(harness, Region::Center, Role::Button)
        .into_iter()
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .unwrap_or_else(|| panic!("no Tag roles row for {role}"))
}

#[test]
fn the_tag_roles_card_lists_rows_headers_and_cells_and_focuses_each_apart() {
    let mut harness = opened("table-scope-conflict");
    boxes_on(&mut harness);
    // Small enough for the card to be open beside the page.
    harness.state_mut().state.set_zoom(0.45);
    settle(&mut harness);
    wait_for_render(&mut harness);
    settle(&mut harness);
    // The chips of `tag` on the page: `TD`, or `TH · Column · declared`; not the card's rows.
    let chips = |harness: &App, tag: &str| {
        let scoped = format!("{tag} ·");
        buttons_in(harness, Region::Center)
            .into_iter()
            .filter(|name| name == tag || name.starts_with(&scoped))
            .count()
    };
    let cells = chips(&harness, "TD");
    assert!(cells > 0, "the fixture has data cells");

    role_row(&harness, "TH").click();
    settle(&mut harness);

    // The header cells alone keep their chips.
    assert_eq!(chips(&harness, "TH"), 6);
    assert_eq!(chips(&harness, "TD"), 0);
    assert_eq!(chips(&harness, "TR"), 0);
    assert!(
        role_row(&harness, "TH")
            .accesskit_node()
            .label()
            .is_some_and(|name| name.ends_with(", focused"))
    );

    // Shift adds the data cells; the rows and the table stay out.
    let pos = role_row(&harness, "TD").rect().center();
    harness.hover_at(pos);
    for pressed in [true, false] {
        harness.event_modifiers(
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::SHIFT,
            },
            Modifiers::SHIFT,
        );
    }
    settle(&mut harness);
    assert_eq!(chips(&harness, "TH"), 6);
    assert_eq!(chips(&harness, "TD"), cells);
    assert_eq!(chips(&harness, "TR"), 0);
    // Smoke: the table row of the card is a role of its own, out of focus.
    assert!(
        role_row(&harness, "Table")
            .accesskit_node()
            .label()
            .is_some_and(|name| name.ends_with(", out of focus"))
    );
}

#[test]
fn a_th_without_scope_has_an_outlined_chip_and_a_hash_when_cells_name_it_by_id() {
    let mut harness = opened("table-headers-ids");

    boxes_on(&mut harness);

    assert_eq!(
        header_chips(&harness),
        [
            "TH · Column · inferred",
            "TH · Column · inferred · IDs decide",
            "TH · Column · inferred · IDs decide",
            "TH · Column · inferred · IDs decide",
            "TH · Row · inferred · IDs decide",
            "TH · Row · inferred · IDs decide",
        ]
    );
    // Outlined: a 1 px border in the table colour and no fill, for each header chip.
    let chips: Vec<egui::Rect> = common::nodes_in(&harness, Region::Center, Role::Button)
        .into_iter()
        .filter(|node| node.accesskit_node().label().is_some_and(|l| l.starts_with("TH")))
        .map(|node| node.rect())
        .collect();
    let outlined: Vec<egui::Rect> = painted(&harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r)
                if r.stroke.color == tokens::ROLE_TABLE
                    && r.stroke.width == 1.0
                    && r.fill == egui::Color32::TRANSPARENT =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .collect();
    assert!(
        chips.iter().all(|chip| outlined.contains(chip)),
        "{chips:?} in {outlined:?}"
    );
    let hashes = painted(&harness)
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Text(t) if t.galley.text() == "#"))
        .count();
    assert_eq!(hashes, 5);
    // Smoke: nothing conflicts here.
    assert_eq!(
        rects_stroked(&harness, tokens::WARN, header_scope::CONFLICT_RING),
        []
    );
}

/// The texts of the inspector's labels.
fn inspector_labels(harness: &App) -> Vec<String> {
    texts_in(harness, Region::Inspector, Role::Label)
}

#[test]
fn the_inspector_says_each_scope_how_many_cells_it_governs_and_explains_a_conflict() {
    let mut harness = opened("table-scope-conflict");

    // Each header cell: its Scope row and how many cells it governs.
    for (cell, scope, governs) in [
        (13, "Both · declared", "5 cells"),
        (14, "Column · declared", "6 cells"),
        (15, "Column · declared", "2 cells"),
        (17, "Row · declared", "2 cells"),
        (20, "Column · declared, conflicts with position", "0 cells"),
    ] {
        select(&mut harness, elem(cell));
        let labels = inspector_labels(&harness);
        assert!(labels.contains(&scope.to_owned()), "{cell}: {labels:?}");
        assert!(labels.contains(&governs.to_owned()), "{cell}: {labels:?}");
    }
    // The conflict has its card, in plain words.
    let labels = inspector_labels(&harness);
    assert!(
        labels.contains(&table::SCOPE_CONFLICT_CODE.to_owned()),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|l| l.starts_with(
            "Says Column, but sits in the first column, so it reads as a row header."
        )),
        "{labels:?}"
    );
    // Smoke: a declared scope that agrees has no card, and a data cell governs nothing.
    select(&mut harness, elem(15));
    assert!(!inspector_labels(&harness).contains(&table::SCOPE_CONFLICT_CODE.to_owned()));
    select(&mut harness, elem(18));
    assert!(!inspector_labels(&harness).contains(&"Governs".to_owned()));
}

#[test]
fn the_inspector_lists_the_cells_whose_headers_name_an_inferred_header() {
    let mut harness = opened("table-headers-ids");

    // Q1: no Scope, named by 1.2 and 0.9.
    select(&mut harness, elem(15));

    let labels = inspector_labels(&harness);
    assert!(labels.contains(&"Column · inferred".to_owned()), "{labels:?}");
    assert!(labels.contains(&"2 cells".to_owned()), "{labels:?}");
    let links = texts_in(&harness, Region::Inspector, Role::Link);
    assert!(
        links.contains(&"/Headers: 1.2 · row 3, col 2".to_owned()),
        "{links:?}"
    );
    assert!(
        links.contains(&"/Headers: 0.9 · row 4, col 2".to_owned()),
        "{links:?}"
    );
    // A click on a cell that names it selects that cell.
    the(&harness, Region::Inspector, Role::Label, "0.9 · row 4, col 2").click();
    settle(&mut harness);
    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(21)));
}

#[test]
fn a_selected_th_echoes_the_cells_it_governs_and_nothing_else_does() {
    let mut harness = opened("table-scope-conflict");
    assert_eq!(echo_bands(&harness), 0);

    // Q1 heads the two cells below it: one band, a chevron on each of their rows.
    select(&mut harness, elem(15));
    assert_eq!((echo_bands(&harness), echo_chevrons(&harness)), (1, 2));

    // The corner heads cells beside it and below it: two bands.
    select(&mut harness, elem(13));
    assert_eq!(echo_bands(&harness), 2);

    // A data cell, or the table, has none.
    select(&mut harness, elem(18));
    assert_eq!((echo_bands(&harness), echo_chevrons(&harness)), (0, 0));
    select(&mut harness, elem(8));
    assert_eq!(echo_bands(&harness), 0);
}

#[test]
fn hovering_a_th_chip_echoes_its_cells_until_the_pointer_leaves() {
    let mut harness = opened("table-scope-conflict");
    boxes_on(&mut harness);
    harness.remove_cursor();
    settle(&mut harness);
    assert_eq!(echo_bands(&harness), 0);

    let corner = the(&harness, Region::Center, Role::Button, "TH · Both · declared").rect();
    harness.hover_at(corner.center());
    settle(&mut harness);
    assert_eq!(echo_bands(&harness), 2);

    harness.remove_cursor();
    settle(&mut harness);
    assert_eq!(echo_bands(&harness), 0);

    // Hovering the selected header does not paint its echo twice.
    select(&mut harness, elem(13));
    harness.hover_at(corner.center());
    settle(&mut harness);
    assert_eq!(echo_bands(&harness), 2);
}

#[test]
fn snapshot_table_header_scope() {
    let _gpu = common::gpu_lock();
    let mut harness = common::builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "table-scope-conflict");
    common::click_rail(&mut harness, "Tags");
    boxes_on(&mut harness);
    // Sales selected: its echo over the quarters and the figures; the hint of the conflict. Zoomed
    // in so the table is as large as on the design board.
    harness.state_mut().state.set_zoom(2.2);
    harness.state_mut().state.select(elem(14));
    harness.run_steps(4);
    wait_for_render(&mut harness);
    let south = the(
        &harness,
        Region::Center,
        Role::Button,
        "TH · Column · declared, conflicts with position",
    )
    .rect();
    harness.hover_at(south.center());
    harness.run_steps(6);
    harness.snapshot("app_table_header_scope");
}

#[test]
fn snapshot_table_header_scope_by_ids() {
    let _gpu = common::gpu_lock();
    let mut harness = common::builder().wgpu().build_eframe(|cc| PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "table-headers-ids");
    common::click_rail(&mut harness, "Tags");
    boxes_on(&mut harness);
    // Q1 selected: inferred, named by two cells.
    harness.state_mut().state.set_zoom(2.2);
    harness.state_mut().state.select(elem(15));
    harness.run_steps(4);
    wait_for_render(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_table_header_scope_ids");
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
    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(11)));

    the(&harness, Region::Center, Role::Button, "Footnote link: 1 apart").click();
    settle(&mut harness);
    // And back.
    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(9)));
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
    assert_eq!(harness.state().state.selection.primary().cloned(), Some(elem(11)));
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

#[test]
fn the_sub_headers_of_the_table_section_are_in_natural_case_on_the_section_headers_text_edge() {
    let mut harness = opened("table-headers-ids");
    // A header cell named by two cells, then a data cell read with its headers.
    select(&mut harness, elem(15));
    let header = the(&harness, Region::Inspector, Role::Button, table::TITLE).rect();
    let named = the(&harness, Region::Inspector, Role::Label, table::NAMED_BY).rect();

    assert_eq!(table::NAMED_BY, "Named by /Headers");
    assert!(
        (named.min.x - (header.min.x + pdfxray_app::components::section::TEXT_X)).abs() < 0.5,
        "{named:?} against the header {header:?}"
    );
    let shape = painted(&harness)
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Text(t) if t.galley.text() == table::NAMED_BY => Some(t),
            _ => None,
        })
        .expect("the sub-header is painted");
    let format = &shape.galley.job.sections[0].format;
    assert_eq!(
        (format.font_id.size, format.color),
        (tokens::text::SMALL, tokens::TEXT_2)
    );
    assert!(
        shape.galley.job.sections[0].format.font_id.family
            == pdfxray_app::theme::fonts::semibold(11.0).family,
        "Inter 600"
    );

    select(&mut harness, elem(22));
    let cell = the(
        &harness,
        Region::Inspector,
        Role::Label,
        table::HEADERS_OF_THE_CELL,
    )
    .rect();
    assert_eq!(table::HEADERS_OF_THE_CELL, "Headers of the cell");
    assert!((cell.min.x - named.min.x).abs() < 0.5, "{cell:?} {named:?}");
    // Smoke: the shouted versions are gone.
    let labels = inspector_labels(&harness);
    assert!(
        !labels
            .iter()
            .any(|l| l == "HEADERS OF THE CELL" || l == "NAMED BY /HEADERS")
    );
}
