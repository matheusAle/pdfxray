//! The tag overlay: role colours shared by the Tags tree, the canvas and the legend, the colour
//! editor and its saved overrides, tag boxes and labels, the reading-order badges and path, and
//! the inspector's "Page n · reading order".
//!
//! Update the snapshot with
//! `UPDATE_SNAPSHOTS=1 cargo test -p pdfxray-app --test tag_overlay snapshot_tag_overlay_reading_order`.

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
    Region, app, builder, buttons_in, click_at, fixture_path, nodes_in, open_fixture, texts_in, the,
    wait_for_render,
};
use egui::epaint::{ClippedShape, Shape};
use egui::{Key, Modifiers, Role, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfxray_app::center::canvas::role_legend;
use pdfxray_app::roles::TagRole;
use pdfxray_app::theme::tokens;
use pdfxray_app::{NavTree, OrderSource, PdfxrayApp, role_color};
use pdfxray_core::{Document, NodeId, Point, ReadingOrderSource, RoleClass, TreeKind, TreeNode};
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
    Document::open(fixture_path(fixture)).unwrap()
}

/// Turns an option of the tag overlay on with its key and lets the canvas draw it.
fn press(harness: &mut App, key: Key, modifiers: Modifiers) {
    harness.key_press_modifiers(modifiers, key);
    settle(harness);
}

/// Toggles the tag boxes (`⇧T`).
fn boxes_on(harness: &mut App) {
    press(harness, Key::T, Modifiers::SHIFT);
}

/// Toggles the tag labels (`L`).
fn labels_on(harness: &mut App) {
    press(harness, Key::L, Modifiers::NONE);
}

/// Toggles the tag boxes and their labels.
fn tags_on(harness: &mut App) {
    boxes_on(harness);
    labels_on(harness);
}

fn order_on(harness: &mut App) {
    press(harness, Key::R, Modifiers::NONE);
}

/// The accessible names of the buttons on the canvas that are chips or badges of the overlay.
fn canvas_buttons(harness: &App) -> Vec<String> {
    buttons_in(harness, Region::Center)
}

fn badges(harness: &App) -> Vec<String> {
    canvas_buttons(harness)
        .into_iter()
        .filter(|name| name.starts_with("Reading order step"))
        .collect()
}

fn chips(harness: &App) -> Vec<String> {
    let mut chips: Vec<String> = canvas_buttons(harness)
        .into_iter()
        .filter(|name| ["H1", "P", "Figure", "Caption", "Sect", "Document", "Art"].contains(&name.as_str()))
        .collect();
    chips.sort();
    chips
}

/// Every structure element of the Tags tree with the label it is shown under.
fn elements(nodes: Vec<TreeNode>, out: &mut Vec<(NodeId, String)>) {
    for node in nodes {
        if matches!(node.id, NodeId::StructElem(_)) {
            out.push((node.id.clone(), node.label.clone()));
        }
        elements(node.children, out);
    }
}

/// Screen position of a page-space point on the canvas, for a page of height `page_height`.
fn screen_of(harness: &App, point: Point, page_height: f64) -> egui::Pos2 {
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    let zoom = f64::from(harness.state().state.canvas.zoom);
    canvas.min + vec2((point.x * zoom) as f32, ((page_height - point.y) * zoom) as f32)
}

// ---- Role colours ----

#[test]
fn tree_dots_take_the_role_colour_of_their_class_and_a_custom_role_its_targets() {
    let mut harness = opened("struct-rich");
    common::click_rail(&mut harness, "Tags");
    let document = doc("struct-rich");
    let mut all = Vec::new();
    elements(document.tree(TreeKind::Tags), &mut all);

    // Every element's dot is the colour of its class, whatever the class is.
    let mut seen = std::collections::BTreeSet::new();
    for (id, label) in &all {
        let class = document.role_class(id).expect("a class");
        let dot = harness
            .state_mut()
            .state
            .tree_model(NavTree::Core(TreeKind::Tags))
            .unwrap()
            .dot_of(id)
            .unwrap_or_else(|| panic!("a dot for {label}"));
        assert_eq!(dot, role_color(class), "{label}");
        seen.insert(class);
    }
    // The fixture has headings, paragraphs, figures, tables and lists, each in its own colour.
    for class in [
        RoleClass::Heading,
        RoleClass::Paragraph,
        RoleClass::Figure,
        RoleClass::Table,
        RoleClass::List,
    ] {
        assert!(seen.contains(&class), "{class:?} in {seen:?}");
    }
    let colors: std::collections::BTreeSet<[u8; 4]> =
        seen.iter().map(|c| role_color(*c).to_array()).collect();
    assert_eq!(colors.len(), seen.len());
    // Smoke: the design's own colours are what the roles get by default.
    assert_eq!(role_color(RoleClass::Heading), tokens::ROLE_HEADING);
    assert_eq!(role_color(RoleClass::Figure), tokens::ROLE_FIGURE);
}

#[test]
fn a_custom_role_gets_the_colour_of_the_role_its_rolemap_reaches() {
    let mut harness = opened("rolemap-two-hops");
    common::click_rail(&mut harness, "Tags");
    let document = doc("rolemap-two-hops");
    let mut all = Vec::new();
    elements(document.tree(TreeKind::Tags), &mut all);
    // `Headline` reaches H1 in two steps.
    let (custom, label) = all
        .iter()
        .find(|(_, label)| label.starts_with("Headline"))
        .expect("an element with the custom role Headline")
        .clone();

    let dot = harness
        .state_mut()
        .state
        .tree_model(NavTree::Core(TreeKind::Tags))
        .unwrap()
        .dot_of(&custom)
        .unwrap();

    assert_eq!(document.role_class(&custom), Some(RoleClass::Heading), "{label}");
    assert_eq!(dot, tokens::ROLE_HEADING);
}

#[test]
fn the_x_ray_by_tag_role_asks_for_tag_role_colouring_and_paints_a_new_picture() {
    let mut harness = opened("reading-order");
    view_tools::toggle(&mut harness, "X-ray");
    wait_for_render(&mut harness);
    let by_kind = common::digest(&harness);

    view_tools::pick(&mut harness, "Tag role");
    wait_for_render(&mut harness);

    let request = harness.state().state.shown_request().unwrap().clone();
    let xray = request.xray.expect("the x-ray is on");
    assert_eq!(xray.color_by, pdfxray_render::ColorBy::TagRole);
    // Smoke: the bitmap was asked for again and it is not the kind-coloured picture.
    assert_ne!(common::digest(&harness), by_kind);
}

#[test]
fn tag_boxes_and_tag_labels_show_each_without_the_other() {
    let mut harness = opened("reading-order");
    let paragraph = role_color(RoleClass::Paragraph);
    let all = ["Caption", "Document", "Figure", "H1", "P", "P"];
    assert_eq!(chips(&harness), Vec::<String>::new());

    // ⇧T: the boxes alone.
    boxes_on(&mut harness);
    assert_eq!(outlines_in(&harness, paragraph).len(), 2);
    assert_eq!(chips(&harness), Vec::<String>::new());

    // L as well: Heading1 is mapped to H1, so its chip says what a reader sees; the container is
    // a chip of its own at the box's top right (Nesting is on by default).
    labels_on(&mut harness);
    assert_eq!(chips(&harness), all);

    // ⇧T again: the labels stay without the boxes.
    boxes_on(&mut harness);
    assert_eq!(chips(&harness), all);
    assert_eq!(outlines_in(&harness, paragraph), []);
    // Smoke: the labels off as well takes everything away.
    labels_on(&mut harness);
    assert_eq!(chips(&harness), Vec::<String>::new());
}

#[test]
fn nesting_off_leaves_the_containers_out_and_artifacts_get_their_own_chips() {
    let mut harness = opened("artifacts");
    tags_on(&mut harness);
    let with_art = chips(&harness);
    assert!(with_art.contains(&"Art".to_owned()), "{with_art:?}");

    harness.state_mut().state.canvas.view.show.artifacts = false;
    harness.state_mut().state.canvas.view.show.nesting = false;
    settle(&mut harness);

    let bare = chips(&harness);
    assert!(!bare.contains(&"Art".to_owned()), "{bare:?}");
    assert!(!bare.contains(&"Document".to_owned()), "{bare:?}");
    assert!(!bare.contains(&"Sect".to_owned()), "{bare:?}");
}

/// Rests the pointer on the Tag roles chip so that the card opens, then lists its rows by their
/// accessible names (`P, 2 elements`) with whether the row is dimmed.
fn legend_rows(harness: &mut App) -> Vec<(String, bool)> {
    let chip = harness.get_by_label(role_legend::TITLE).rect();
    harness.hover_at(pos2(chip.max.x - 4.0, chip.center().y));
    harness.run_steps(4);
    nodes_in(harness, Region::Center, Role::Button)
        .into_iter()
        .filter_map(|node| {
            let name = node.accesskit_node().label()?;
            name.contains(" element")
                .then(|| (name, node.accesskit_node().is_disabled()))
        })
        .collect()
}

#[test]
fn the_tag_roles_legend_counts_the_classes_on_the_page_and_dims_the_unused_ones() {
    let mut harness = opened("reading-order");
    // Not shown until the tag boxes or the role colouring are on.
    assert!(harness.query_by_label(role_legend::TITLE).is_none());

    tags_on(&mut harness);
    let rows = legend_rows(&mut harness);

    let named: Vec<(&str, bool)> = rows
        .iter()
        .map(|(name, dimmed)| (name.as_str(), *dimmed))
        .collect();
    assert_eq!(
        named,
        [
            ("H1–H6, 1 element", false),
            ("P, 2 elements", false),
            ("Figure, 1 element", false),
            ("Caption, 1 element", false),
            ("Table, 0 elements", true),
            ("L / LI, 0 elements", true),
            ("Link, 0 elements", true),
            ("Span, 0 elements", true),
            ("Sect / Div, 1 element", false),
            ("Artifact, 0 elements", true),
        ]
    );
    // The same rows come from the legend's own function.
    let from_core: Vec<(RoleClass, usize)> = role_legend::rows(&mut harness.state_mut().state, 0)
        .into_iter()
        .map(|row| (row.role.class(), row.count))
        .collect();
    assert_eq!(from_core[0], (RoleClass::Heading, 1));
    assert_eq!(from_core[1], (RoleClass::Paragraph, 2));
}

#[test]
fn the_legend_follows_tag_role_colouring_and_offers_no_colour_editing() {
    let mut harness = opened("reading-order");
    view_tools::pick(&mut harness, "Tag role");
    settle(&mut harness);
    assert!(harness.query_by_label(role_legend::TITLE).is_some());
    // Open the card: it lists the roles in their fixed colours, with no palette button.
    let chip = harness.get_by_label(role_legend::TITLE).rect();
    harness.hover_at(pos2(chip.max.x - 4.0, chip.center().y));
    harness.run_steps(4);

    // Without the tag boxes the rows are a plain legend: there is nothing to focus.
    assert!(common::has_label(&harness, "H1–H6, 1 element"));
    assert!(
        !buttons_in(&harness, Region::Center)
            .iter()
            .any(|name| name.contains(" element"))
    );
    assert!(harness.query_by_label_contains("Edit tag colours").is_none());
    // Smoke: the View menu has no colour entry either.
    view_tools::open(&mut harness);
    assert!(view_tools::is_open(&harness));
    assert!(harness.query_by_label_contains("Edit tag colours").is_none());
}

#[test]
fn reading_order_numbers_the_five_elements_and_flags_the_backtrack() {
    let mut harness = opened("reading-order");
    assert_eq!(badges(&harness).len(), 0);

    order_on(&mut harness);

    assert_eq!(
        badges(&harness),
        [
            "Reading order step 1",
            "Reading order step 2",
            "Reading order step 3",
            "Reading order step 4, goes back up",
            "Reading order step 5",
        ]
    );
    // Exactly one is flagged, and the order is the one core reports.
    let flagged = badges(&harness)
        .iter()
        .filter(|name| name.ends_with("goes back up"))
        .count();
    assert_eq!(flagged, 1);
    let core = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);
    assert_eq!(core.backtracks, 1);
    assert_eq!(core.steps.len(), 5);
    // R again hides them.
    order_on(&mut harness);
    assert_eq!(badges(&harness).len(), 0);
}

#[test]
fn columns_and_footnotes_have_numbered_badges_but_none_is_amber() {
    for fixture in ["reading-order-columns", "reading-order-footnote"] {
        let mut harness = opened(fixture);

        order_on(&mut harness);

        let found = badges(&harness);
        let core = doc(fixture).reading_order(0, ReadingOrderSource::Structure);
        assert_eq!(found.len(), core.steps.len(), "{fixture}: {found:?}");
        assert!(!found.is_empty(), "{fixture}");
        assert!(
            found.iter().all(|name| !name.contains("goes back up")),
            "{fixture}: {found:?}"
        );
        assert_eq!(core.backtracks, 0, "{fixture}");
    }
}

#[test]
fn clicking_a_badge_selects_the_element_it_numbers() {
    let mut harness = opened("reading-order");
    order_on(&mut harness);
    let core = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);

    the(&harness, Region::Center, Role::Button, "Reading order step 3").click();
    settle(&mut harness);

    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(core.steps[2].id.clone())
    );
    // The backtrack badge selects the figure.
    the(
        &harness,
        Region::Center,
        Role::Button,
        "Reading order step 4, goes back up",
    )
    .click();
    settle(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(core.steps[3].id.clone())
    );
    assert_eq!(core.steps[3].role, "Figure");
}

#[test]
fn a_badge_sits_just_outside_the_right_edge_of_its_box_and_is_a_comfortable_target() {
    let mut harness = opened("reading-order");
    order_on(&mut harness);
    let core = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);

    for step in &core.steps {
        let edge = screen_of(
            &harness,
            Point::new(step.rect.x1, f64::midpoint(step.rect.y0, step.rect.y1)),
            400.0,
        );
        let badge = the(
            &harness,
            Region::Center,
            Role::Button,
            &pdfxray_app::center::canvas::tag_overlay::badge_label(step),
        )
        .rect();

        // Level with the middle of the box, and clear of the last glyph: the painted disc (18 px)
        // starts right of the edge.
        assert!(
            (badge.center().y - edge.y).abs() < 1.0,
            "step {}: {badge:?} {edge:?}",
            step.index
        );
        assert!(
            badge.center().x - 9.0 > edge.x,
            "step {}: {badge:?} {edge:?}",
            step.index
        );
        assert!(
            badge.width() >= 24.0 && badge.height() >= 24.0,
            "step {}: {badge:?}",
            step.index
        );
    }
    // Smoke: every step has its badge.
    assert_eq!(badges(&harness).len(), core.steps.len());
}

/// `shape` and what it holds, flattened, in paint order.
fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
    if let Shape::Vec(inner) = shape {
        for shape in inner {
            flatten(shape, out);
        }
    } else {
        out.push(shape.clone());
    }
}

#[test]
fn every_tag_outline_is_painted_under_every_chip() {
    let mut harness = opened("reading-order-columns");
    tags_on(&mut harness);

    let mut shapes = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut shapes);
    }
    // The chips of the canvas: `P` and `H1` texts between the navigator and the inspector.
    let on_canvas = |shape: &Shape| {
        matches!(shape, Shape::Text(text)
            if ["P", "H1"].contains(&text.galley.text()) && (352.0..1100.0).contains(&text.pos.x))
    };
    let first_chip = shapes.iter().position(on_canvas).expect("a chip");
    // A tag box: a faint fill with a 1.5 px outline in the role colour, outside the box.
    let outlines: Vec<usize> = shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| {
            matches!(shape, Shape::Rect(rect)
                if rect.stroke.width == 1.5 && rect.stroke_kind == egui::StrokeKind::Outside && rect.fill.a() > 0)
        })
        .map(|(at, _)| at)
        .collect();

    // A box drawn after a chip would cut through its text.
    assert!(outlines.len() >= 6, "{outlines:?}");
    assert!(
        outlines.iter().all(|at| *at < first_chip),
        "{outlines:?} and the first chip at {first_chip}"
    );
    // Smoke: every element still has its chip.
    assert_eq!(chips(&harness), ["Document", "H1", "P", "P", "P", "P", "P"]);
}

#[test]
fn no_badge_covers_a_chip() {
    for fixture in ["reading-order-columns", "speech-footnote"] {
        let mut harness = opened(fixture);
        tags_on(&mut harness);
        order_on(&mut harness);

        let chips: Vec<egui::Rect> = nodes_in(&harness, Region::Center, Role::Button)
            .into_iter()
            .filter(|node| {
                node.accesskit_node()
                    .label()
                    .is_some_and(|label| !label.starts_with("Reading order step"))
            })
            .map(|node| node.rect())
            .filter(|rect| rect.width() < 80.0 && rect.height() < 24.0)
            .collect();
        // The painted disc is 18 px across; the hit area around it is bigger.
        let discs: Vec<egui::Rect> = nodes_in(&harness, Region::Center, Role::Button)
            .into_iter()
            .filter(|node| {
                node.accesskit_node()
                    .label()
                    .is_some_and(|label| label.starts_with("Reading order step"))
            })
            .map(|node| egui::Rect::from_center_size(node.rect().center(), vec2(18.0, 18.0)))
            .collect();

        assert!(chips.len() >= 4, "{fixture}: {chips:?}");
        for disc in &discs {
            for chip in &chips {
                assert!(
                    !disc.intersects(*chip),
                    "{fixture}: a badge at {disc:?} over a chip at {chip:?}"
                );
            }
        }
        // Smoke: every step of the order still has its badge.
        assert_eq!(discs.len(), badges(&harness).len());
        assert!(discs.len() >= 5, "{fixture}: {}", discs.len());
    }
}

#[test]
fn the_pointer_is_a_hand_over_a_badge_and_over_a_chip_and_a_plain_arrow_between() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    order_on(&mut harness);
    let cursor_at = |harness: &mut App, at: egui::Pos2| {
        harness.hover_at(at);
        harness.run_steps(2);
        harness.output().platform_output.cursor_icon
    };

    let badge = the(&harness, Region::Center, Role::Button, "Reading order step 1").rect();
    let chip = the(&harness, Region::Center, Role::Button, "Caption").rect();

    assert_eq!(
        cursor_at(&mut harness, badge.center()),
        egui::CursorIcon::PointingHand
    );
    assert_eq!(
        cursor_at(&mut harness, chip.center()),
        egui::CursorIcon::PointingHand
    );
    // Smoke: off both, on the grey around the page, it is the arrow.
    let canvas = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    assert_eq!(
        cursor_at(&mut harness, canvas.left_top() - vec2(4.0, 4.0)),
        egui::CursorIcon::Default
    );
}

#[test]
fn clicking_a_chip_selects_its_element_and_a_click_in_a_box_picks_the_innermost_one() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    let core = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);
    let caption = core.steps.iter().find(|s| s.role == "Caption").unwrap().clone();

    the(&harness, Region::Center, Role::Button, "Caption").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(caption.id.clone())
    );

    // A click in the middle of the figure's box (page 300 x 400) selects the Figure, not the
    // content operator under it.
    let figure = core.steps.iter().find(|s| s.role == "Figure").unwrap();
    let middle = Point::new(
        f64::midpoint(figure.rect.x0, figure.rect.x1),
        f64::midpoint(figure.rect.y0, figure.rect.y1),
    );
    let at = screen_of(&harness, middle, 400.0);
    click_at(&mut harness, at);
    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(figure.id.clone())
    );
}

#[test]
fn reading_order_from_paint_order_changes_the_numbers_and_loses_the_backtrack() {
    let mut harness = opened("reading-order");
    order_on(&mut harness);
    let structure = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);
    let paint = doc("reading-order").reading_order(0, ReadingOrderSource::Paint);
    assert_ne!(structure.steps[2].id, paint.steps[2].id);

    view_tools::pick(&mut harness, "Paint order");
    settle(&mut harness);

    assert_eq!(harness.state().state.canvas.view.order_from, OrderSource::Paint);
    // Painted in reading order: five badges, none going back.
    assert_eq!(badges(&harness).len(), 5);
    assert!(badges(&harness).iter().all(|name| !name.contains("goes back up")));
    // Badge 3 now stands for the element the paint order has third.
    the(&harness, Region::Center, Role::Button, "Reading order step 3").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(paint.steps[2].id.clone())
    );
    assert_eq!(paint.backtracks, 0);
}

// ---- The inspector ----

#[test]
fn a_selected_page_lists_the_reading_order_with_its_backtrack_and_the_paint_comparison() {
    let mut harness = opened("reading-order");
    order_on(&mut harness);

    harness.state_mut().state.select(NodeId::Page(0));
    settle(&mut harness);

    let rows: Vec<String> = texts_in(&harness, Region::Inspector, Role::Button)
        .into_iter()
        .filter(|name| name.starts_with("Step "))
        .collect();
    assert_eq!(
        rows,
        [
            "Step 1: Heading1 · Quarterly report",
            "Step 2: P · Revenue grew in every region.",
            "Step 3: Caption · Figure 1: Revenue by region",
            "Step 4: Figure · Grey square · goes back up",
            "Step 5: P · Closing remarks follow.",
        ]
    );
    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(
        common::has_label(&harness, "Page 1 · reading order"),
        "{labels:?}"
    );
    assert!(
        common::has_label(&harness, "from structure tree · 5 elements · 1 backtrack"),
        "{labels:?}"
    );
    // The backtrack is explained in plain words.
    assert!(
        common::has_label(&harness, "reading-order-backtrack"),
        "{labels:?}"
    );
    // The comparison with the paint order.
    assert!(common::has_label(&harness, "3 of 5 elements"), "{labels:?}");
    assert!(
        common::has_label(&harness, "#3 Caption (painted after Figure)"),
        "{labels:?}"
    );
    // Smoke: the comparison section is open and names its rows.
    assert!(labels.contains(&"Same order".to_owned()), "{labels:?}");
    assert!(labels.contains(&"Differs at".to_owned()), "{labels:?}");
}

#[test]
fn a_row_of_the_inspector_list_selects_its_element() {
    let mut harness = opened("reading-order");
    order_on(&mut harness);
    harness.state_mut().state.select(NodeId::Page(0));
    settle(&mut harness);
    let core = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);

    the(
        &harness,
        Region::Inspector,
        Role::Button,
        "Step 2: P · Revenue grew in every region.",
    )
    .click();
    settle(&mut harness);

    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(core.steps[1].id.clone())
    );
    assert_eq!(core.steps[1].role, "P");
}

#[test]
fn the_list_follows_the_source_and_a_page_without_backtrack_has_no_card() {
    let mut harness = opened("reading-order");
    harness.state_mut().state.select(NodeId::Page(0));
    order_on(&mut harness);
    view_tools::pick(&mut harness, "Paint order");
    settle(&mut harness);

    let rows: Vec<String> = texts_in(&harness, Region::Inspector, Role::Button)
        .into_iter()
        .filter(|name| name.starts_with("Step "))
        .collect();

    // Painted: heading, paragraph, figure, caption, paragraph.
    assert_eq!(rows.len(), 5);
    assert!(rows[2].starts_with("Step 3: Figure"), "{rows:?}");
    assert!(rows[3].starts_with("Step 4: Caption"), "{rows:?}");
    assert!(!common::has_label(&harness, "reading-order-backtrack"));
    assert!(common::has_label(&harness, "from paint order · 5 elements"));
}

#[test]
fn an_untagged_file_has_no_reading_order_section() {
    let mut harness = opened("minimal-one-page");
    harness.state_mut().state.select(NodeId::Page(0));
    order_on(&mut harness);
    settle(&mut harness);

    let steps = texts_in(&harness, Region::Inspector, Role::Button)
        .into_iter()
        .filter(|name| name.starts_with("Step "))
        .count();

    assert_eq!(steps, 0);
    assert!(!common::has_label(&harness, "Same order"));
}

// ---- Tag roles focus ----

/// Makes the page small enough for the column of cards to fit beside it, so the Tag roles card
/// is open rather than a chip.
fn card_open(harness: &mut App) {
    harness.state_mut().state.set_zoom(0.45);
    settle(harness);
    wait_for_render(harness);
    settle(harness);
}

/// The row of the Tag roles card whose name starts with `role`, e.g. `H1–H6`.
fn role_row<'h>(harness: &'h App, role: &str) -> egui_kittest::Node<'h> {
    let prefix = format!("{role}, ");
    let mut found: Vec<_> = nodes_in(harness, Region::Center, Role::Button)
        .into_iter()
        .filter(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect();
    assert_eq!(found.len(), 1, "one row for {role}");
    found.remove(0)
}

fn row_name(harness: &App, role: &str) -> String {
    role_row(harness, role).accesskit_node().label().unwrap()
}

fn click_row(harness: &mut App, role: &str) {
    role_row(harness, role).click();
    settle(harness);
}

/// Clicks the row of `role` with Shift held.
fn shift_click_row(harness: &mut App, role: &str) {
    let pos = role_row(harness, role).rect().center();
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
    settle(harness);
}

fn focused(harness: &App) -> Vec<TagRole> {
    harness.state().state.focused_roles().into_iter().collect()
}

#[test]
fn a_click_focuses_a_role_a_second_click_releases_and_shift_adds_or_removes_one() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    card_open(&mut harness);

    click_row(&mut harness, "H1–H6");

    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Heading)]);
    assert_eq!(chips(&harness), ["H1"]);
    assert_eq!(row_name(&harness, "H1–H6"), "H1–H6, 1 element, focused");
    assert_eq!(row_name(&harness, "P"), "P, 2 elements, out of focus");
    assert!(common::has_label(&harness, "1 of 10 shown"));
    assert!(common::is_selected(&role_row(&harness, "H1–H6")));

    // The focused row again: every role shows.
    click_row(&mut harness, "H1–H6");
    assert_eq!(focused(&harness), []);
    assert_eq!(chips(&harness), ["Caption", "Document", "Figure", "H1", "P", "P"]);
    assert_eq!(row_name(&harness, "P"), "P, 2 elements");
    assert!(harness.query_by_label(role_legend::SHOW_ALL).is_none());

    // Shift adds a role, and removes one; removing the last releases.
    click_row(&mut harness, "H1–H6");
    shift_click_row(&mut harness, "Figure");
    assert_eq!(
        focused(&harness),
        [
            TagRole::Class(RoleClass::Heading),
            TagRole::Class(RoleClass::Figure)
        ]
    );
    assert_eq!(chips(&harness), ["Figure", "H1"]);
    assert!(common::has_label(&harness, "2 of 10 shown"));
    shift_click_row(&mut harness, "H1–H6");
    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Figure)]);
    shift_click_row(&mut harness, "Figure");
    assert_eq!(focused(&harness), []);

    // Smoke: a role the page does not use cannot be picked.
    assert!(role_row(&harness, "Table").accesskit_node().is_disabled());
    click_row(&mut harness, "Table");
    assert_eq!(focused(&harness), []);
}

#[test]
fn esc_and_show_all_release_the_focus_and_the_card_only_picks_while_the_boxes_or_labels_are_on() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    card_open(&mut harness);
    harness.state_mut().state.select(NodeId::Page(0));
    settle(&mut harness);

    // Esc releases the focus before it clears the selection.
    click_row(&mut harness, "H1–H6");
    press(&mut harness, Key::Escape, Modifiers::NONE);
    assert_eq!(focused(&harness), []);
    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(NodeId::Page(0))
    );

    click_row(&mut harness, "Caption");
    the(&harness, Region::Center, Role::Button, role_legend::SHOW_ALL).click();
    settle(&mut harness);
    assert_eq!(focused(&harness), []);

    // With the boxes and the labels off the focus waits, Esc leaves it alone, and the card does
    // not pick.
    click_row(&mut harness, "Caption");
    tags_on(&mut harness);
    press(&mut harness, Key::Escape, Modifiers::NONE);
    harness
        .state_mut()
        .state
        .click_role(TagRole::Class(RoleClass::Figure), false);
    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Caption)]);
    assert_eq!(harness.state().state.shown_roles(), None);
    // Smoke: on again, the focus is back.
    tags_on(&mut harness);
    assert_eq!(chips(&harness), ["Caption"]);
}

/// The outlines painted last frame whose stroke is `color`.
fn outlines_in(harness: &App, color: egui::Color32) -> Vec<egui::Rect> {
    let mut shapes = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut shapes);
    }
    shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rect) if rect.stroke.color == color && rect.stroke.width == 1.5 => Some(rect.rect),
            _ => None,
        })
        .collect()
}

#[test]
fn roles_out_of_focus_stay_as_faint_boxes_without_labels_and_a_click_still_picks_them() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    card_open(&mut harness);
    let faint = |class| role_color(class).gamma_multiply(pdfxray_render::annotate::UNFOCUSED_BOX);
    assert_eq!(outlines_in(&harness, role_color(RoleClass::Paragraph)).len(), 2);

    click_row(&mut harness, "Figure");

    // The figure at full strength with its chip; the paragraphs faint, without theirs.
    assert_eq!(outlines_in(&harness, role_color(RoleClass::Figure)).len(), 1);
    assert_eq!(outlines_in(&harness, faint(RoleClass::Paragraph)).len(), 2);
    assert_eq!(outlines_in(&harness, role_color(RoleClass::Paragraph)), []);
    assert_eq!(chips(&harness), ["Figure"]);
    // Under the pointer a faint box is outlined at full strength.
    let core = doc("reading-order").reading_order(0, ReadingOrderSource::Structure);
    let caption = core.steps.iter().find(|s| s.role == "Caption").unwrap();
    let middle = Point::new(
        f64::midpoint(caption.rect.x0, caption.rect.x1),
        f64::midpoint(caption.rect.y0, caption.rect.y1),
    );
    let at = screen_of(&harness, middle, 400.0);
    harness.hover_at(at);
    settle(&mut harness);
    assert_eq!(rects_hovered(&harness, role_color(RoleClass::Caption)), 1);
    // A click in it picks it: focus is a view, not a filter.
    click_at(&mut harness, at);
    assert_eq!(
        harness.state().state.selection.primary().cloned(),
        Some(caption.id.clone())
    );
    // Smoke: the focus is untouched by the click.
    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Figure)]);
}

#[test]
fn with_the_labels_alone_the_card_picks_which_labels_show() {
    let mut harness = opened("reading-order");
    labels_on(&mut harness);
    card_open(&mut harness);

    click_row(&mut harness, "Figure");

    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Figure)]);
    assert_eq!(chips(&harness), ["Figure"]);
    // No box is drawn, at full strength or faint.
    let faint = role_color(RoleClass::Paragraph).gamma_multiply(pdfxray_render::annotate::UNFOCUSED_BOX);
    assert_eq!(outlines_in(&harness, role_color(RoleClass::Figure)), []);
    assert_eq!(outlines_in(&harness, faint), []);
    // Smoke: Shift adds the headings' labels.
    shift_click_row(&mut harness, "H1–H6");
    assert_eq!(chips(&harness), ["Figure", "H1"]);
}

/// How many outlines of the element under the pointer (2 px, in `color`) were painted last frame.
fn rects_hovered(harness: &App, color: egui::Color32) -> usize {
    let mut shapes = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut shapes);
    }
    shapes
        .iter()
        .filter(|shape| matches!(shape, Shape::Rect(rect) if rect.stroke.color == color && rect.stroke.width == 2.0))
        .count()
}

/// The reading-order segments painted last frame: `(full, faint)` counts.
fn path_segments(harness: &App) -> (usize, usize) {
    let mut shapes = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut shapes);
    }
    let faint = pdfxray_render::annotate::UNFOCUSED_PATH;
    let segments: Vec<egui::Color32> = shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { stroke, .. } if stroke.width == 2.0 => Some(stroke.color),
            _ => None,
        })
        .collect();
    let full = segments
        .iter()
        .filter(|c| [tokens::ACCENT, tokens::WARN].contains(c))
        .count();
    let dim = segments
        .iter()
        .filter(|c| {
            [
                tokens::ACCENT.gamma_multiply(faint),
                tokens::WARN.gamma_multiply(faint),
            ]
            .contains(c)
        })
        .count();
    (full, dim)
}

#[test]
fn the_reading_order_path_dims_through_roles_out_of_focus_and_keeps_its_badges() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    order_on(&mut harness);
    card_open(&mut harness);
    let (full, dim) = path_segments(&harness);
    assert_eq!((full, dim), (4, 0));

    click_row(&mut harness, "Figure");
    shift_click_row(&mut harness, "Caption");

    // Five steps: H1, P, Caption, Figure, P. Only Caption → Figure (the amber backtrack) joins two
    // focused roles; every other segment touches a role out of focus.
    assert_eq!(path_segments(&harness), (1, 3));
    assert_eq!(badges(&harness).len(), 5);
    // Smoke: released, the path is whole again.
    press(&mut harness, Key::Escape, Modifiers::NONE);
    assert_eq!(path_segments(&harness), (4, 0));
}

#[test]
fn the_rows_take_the_keyboard_arrows_space_and_shift_space() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    card_open(&mut harness);
    role_row(&harness, "H1–H6").focus();
    settle(&mut harness);
    assert!(role_row(&harness, "H1–H6").is_focused());

    press(&mut harness, Key::Space, Modifiers::NONE);
    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Heading)]);

    press(&mut harness, Key::ArrowDown, Modifiers::NONE);
    assert!(role_row(&harness, "P").is_focused());
    press(&mut harness, Key::Space, Modifiers::SHIFT);
    assert_eq!(
        focused(&harness),
        [
            TagRole::Class(RoleClass::Heading),
            TagRole::Class(RoleClass::Paragraph)
        ]
    );

    // Down past Figure and Caption skips the roles the page does not use.
    for _ in 0..3 {
        press(&mut harness, Key::ArrowDown, Modifiers::NONE);
    }
    assert!(role_row(&harness, "Sect / Div").is_focused());
    press(&mut harness, Key::ArrowUp, Modifiers::NONE);
    assert!(role_row(&harness, "Caption").is_focused());
    // Space on a row out of focus focuses that role alone.
    press(&mut harness, Key::Space, Modifiers::NONE);
    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Caption)]);
    // Smoke: the row with the keyboard focus has the 2 px accent ring.
    let row = role_row(&harness, "Caption").rect();
    let mut shapes = Vec::new();
    for ClippedShape { shape, .. } in &harness.output().shapes {
        flatten(shape, &mut shapes);
    }
    assert!(shapes.iter().any(|shape| matches!(shape, Shape::Rect(r)
        if r.rect == row && r.stroke.color == tokens::ACCENT && r.stroke.width == 2.0)));
}

#[test]
fn the_focus_belongs_to_its_document_for_the_session() {
    let mut harness = opened("reading-order");
    tags_on(&mut harness);
    card_open(&mut harness);
    click_row(&mut harness, "Figure");

    open_fixture(&mut harness, "reading-order-columns");
    settle(&mut harness);
    assert_eq!(focused(&harness), []);

    open_fixture(&mut harness, "reading-order");
    settle(&mut harness);
    assert_eq!(focused(&harness), [TagRole::Class(RoleClass::Figure)]);
}

// ---- Snapshot ----

/// Design frame 12 on `reading-order`: tag boxes, labels and the reading order, the backtrack in
/// amber with its note, the Tag roles card and the inspector's list.
#[test]
fn snapshot_tag_overlay_reading_order() {
    let _gpu = common::gpu_lock();
    let mut harness = builder()
        .wgpu()
        .build_eframe(|cc| pdfxray_app::PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "reading-order");
    common::click_rail(&mut harness, "Tags");
    tags_on(&mut harness);
    order_on(&mut harness);
    harness.state_mut().state.select(NodeId::Page(0));
    harness.run_steps(4);
    wait_for_render(&mut harness);

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_tag_overlay_reading_order");
}

/// Design board 2.11 on `reading-order`: the tag boxes with H1–H6 and Figure focused, the other
/// roles faint without labels, and the Tag roles card with its focused, out-of-focus and unused
/// rows.
#[test]
fn snapshot_tag_roles_focus() {
    let _gpu = common::gpu_lock();
    let mut harness = builder()
        .wgpu()
        .build_eframe(|cc| pdfxray_app::PdfxrayApp::new(cc));
    harness.run();
    open_fixture(&mut harness, "reading-order");
    common::click_rail(&mut harness, "Tags");
    tags_on(&mut harness);
    // As large as the page gets with the card still open beside it.
    harness.state_mut().state.set_zoom(0.9);
    harness.run_steps(4);
    harness
        .state_mut()
        .state
        .click_role(TagRole::Class(RoleClass::Heading), false);
    harness
        .state_mut()
        .state
        .click_role(TagRole::Class(RoleClass::Figure), true);
    harness.run_steps(4);
    wait_for_render(&mut harness);

    harness.remove_cursor();
    harness.run_steps(2);
    harness.snapshot("app_tag_roles_focus");
}

#[test]
fn the_reading_order_rows_show_a_hand_and_the_backtrack_card_is_inset_like_the_problem_cards() {
    let mut harness = opened("reading-order");
    order_on(&mut harness);
    harness.state_mut().state.select(NodeId::Page(0));
    settle(&mut harness);

    let step = |harness: &App, n: usize| {
        let name = texts_in(harness, Region::Inspector, Role::Button)
            .into_iter()
            .find(|name| name.starts_with(&format!("Step {n}:")))
            .expect("the step");
        the(harness, Region::Inspector, Role::Button, &name).rect()
    };
    let (plain, backtrack) = (step(&harness, 2), step(&harness, 4));
    let cursor_at = |harness: &mut App, at: egui::Pos2| {
        harness.hover_at(at);
        harness.run_steps(2);
        harness.output().platform_output.cursor_icon
    };

    assert_eq!(
        cursor_at(&mut harness, plain.center()),
        egui::CursorIcon::PointingHand
    );
    assert_eq!(
        cursor_at(&mut harness, backtrack.center()),
        egui::CursorIcon::PointingHand
    );
    // The amber of a backtrack row deepens under the pointer.
    let amber = |harness: &App| {
        harness
            .output()
            .shapes
            .iter()
            .flat_map(|clipped| match &clipped.shape {
                egui::Shape::Vec(shapes) => shapes.clone(),
                other => vec![other.clone()],
            })
            .filter_map(|shape| match shape {
                egui::Shape::Rect(r)
                    if (r.rect.min.y - backtrack.min.y).abs() < 0.6
                        && (r.rect.width() - backtrack.width()).abs() < 0.6 =>
                {
                    Some(r.fill.a())
                }
                _ => None,
            })
            .max()
            .unwrap_or(0)
    };
    let hovered = amber(&harness);
    harness.hover_at(plain.center());
    harness.run_steps(2);
    assert!(hovered > amber(&harness), "{hovered} against the resting amber");

    // The card that explains the backtrack starts inside the panel, not at its edge.
    let inspector_left = common::WINDOW.x - pdfxray_app::theme::tokens::INSPECTOR_W;
    let code = the(
        &harness,
        Region::Inspector,
        Role::Label,
        "reading-order-backtrack",
    )
    .rect();
    assert!(
        code.min.x - inspector_left >= 12.0 + 10.0,
        "{code:?} in {inspector_left}"
    );
}

/// The order of the layer on top at `pos`: a floating card is `Middle`, the panels `Background`.
fn order_at(harness: &App, pos: egui::Pos2) -> Option<egui::Order> {
    harness.ctx.layer_id_at(pos).map(|layer| layer.order)
}

#[test]
fn a_tall_legend_card_stays_inside_the_canvas_and_does_not_cover_the_drawer() {
    let mut harness = opened("reading-order-columns");
    harness.state_mut().state.set_zoom(0.4);
    // A drawer this tall leaves the canvas about 290 px: shorter than the ten rows of the card.
    harness.state_mut().state.drawer.height = 560.0;
    harness.state_mut().state.drawer.open = true;
    tags_on(&mut harness);
    settle(&mut harness);
    let drawer_top = common::WINDOW.y - tokens::STATUS_H - 560.0;
    let x = 1000.0;

    // The card is there, under the title and in the canvas.
    assert!(common::has_label(&harness, role_legend::TITLE));
    assert_eq!(order_at(&harness, pos2(x, 200.0)), Some(egui::Order::Middle));
    // Past the canvas bottom it is the drawer that is under the pointer, not the card.
    assert_eq!(
        order_at(&harness, pos2(x, drawer_top + 20.0)),
        Some(egui::Order::Background)
    );
    assert_eq!(
        order_at(&harness, pos2(x, drawer_top + 120.0)),
        Some(egui::Order::Background)
    );
}
