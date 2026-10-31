//! The small refinements of the inspector and the navigator: compact crumbs and badges, rows that
//! touch, targets that are the size of what they act on, the right cursor, and a focus that shows.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

mod common;

use common::{Region, app, nodes_in, open_fixture, the};
use egui::{CursorIcon, Role, Shape, pos2, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use pdfxray_app::components::{Badge, Crumb, PropRow, Tone, crumb, prop_row, section};
use pdfxray_app::theme::tokens;
use pretty_assertions::assert_eq;

fn builder<S>(size: egui::Vec2) -> HarnessBuilder<S> {
    pdfxray_app::theme::motion::set_reduce_motion(true);
    Harness::builder()
        .with_size(size)
        .with_context_setup(pdfxray_app::theme::install)
}

/// Every shape of the last frame, groups flattened.
fn shapes_of<S>(harness: &Harness<'_, S>) -> Vec<Shape> {
    fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| flatten(shape, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for clipped in &harness.output().shapes {
        flatten(&clipped.shape, &mut out);
    }
    out
}

fn cursor_over<S>(harness: &mut Harness<'_, S>, at: egui::Pos2) -> CursorIcon {
    harness.hover_at(at);
    harness.run_steps(2);
    harness.output().platform_output.cursor_icon
}

// ---- crumbs ----

fn three_crumbs(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        Crumb::new("Page 1", tokens::OK).show(ui);
        crumb::separator(ui);
        Crumb::new("Tj #3", tokens::KIND_CONTENT).show(ui);
        crumb::separator(ui);
        Crumb::new("glyph 0", tokens::KIND_CONTENT).current(true).show(ui);
    });
}

#[test]
fn crumbs_are_a_compact_pill_and_the_current_one_sits_on_the_same_baseline() {
    let mut harness = builder(vec2(420.0, 80.0)).build_ui(three_crumbs);
    harness.run();

    let crumbs: Vec<egui::Rect> = ["Page 1", "Tj #3", "glyph 0"]
        .iter()
        .map(|name| harness.get_by_role_and_label(Role::Button, name).rect())
        .collect();
    let texts: Vec<egui::Rect> = ["Page 1", "Tj #3", "glyph 0"]
        .iter()
        .map(|name| harness.get_by_role_and_label(Role::Label, name).rect())
        .collect();

    // Text height plus margins, not a 28 px control.
    for rect in &crumbs {
        assert!(rect.height() < 24.0, "{rect:?}");
    }
    // The same box for each, and the text of each at the same height: the border of the current
    // one does not push it down.
    for (rect, text) in crumbs.iter().zip(&texts) {
        assert_eq!(rect.height(), crumbs[0].height());
        assert_eq!(rect.min.y, crumbs[0].min.y);
        assert_eq!(text.min.y, texts[0].min.y);
    }
    // Smoke: the three still read left to right.
    assert!(crumbs[0].max.x < crumbs[1].min.x && crumbs[1].max.x < crumbs[2].min.x);
}

#[test]
fn a_crumb_under_the_pointer_gets_a_hover_plate_and_the_current_one_does_not() {
    let mut harness = builder(vec2(420.0, 80.0)).build_ui(three_crumbs);
    harness.run();
    let plain = harness.get_by_role_and_label(Role::Button, "Tj #3").rect();
    let current = harness.get_by_role_and_label(Role::Button, "glyph 0").rect();
    let plate = |harness: &Harness<'_, ()>, rect: egui::Rect| {
        shapes_of(harness).iter().any(|shape| {
            matches!(shape, Shape::Rect(r) if r.fill == tokens::NEUTRAL_HOVER
                && (r.rect.min - rect.min).length() < 0.6
                && (r.rect.max - rect.max).length() < 0.6)
        })
    };
    assert!(!plate(&harness, plain));

    harness.hover_at(plain.center());
    harness.run_steps(2);
    assert!(plate(&harness, plain));

    harness.hover_at(current.center());
    harness.run_steps(2);
    assert!(!plate(&harness, current));
    // Smoke: both show the hand.
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::PointingHand
    );
}

#[test]
fn a_long_crumb_name_is_cut_to_a_width_and_the_folded_crumb_has_no_dot() {
    let long = "Ventura Community Partners_Ventura Community Partners - 2027 - English (1).pdf";
    let mut harness = builder(vec2(520.0, 80.0)).build_ui(move |ui| {
        ui.horizontal(|ui| {
            Crumb::new(long, tokens::TEXT_3).show(ui);
            Crumb::more().show(ui);
            Crumb::new("Tj", tokens::KIND_CONTENT).show(ui);
        });
    });
    harness.run();

    let long_rect = harness.get_by_role_and_label(Role::Button, long).rect();
    let more = harness.get_by_role_and_label(Role::Button, crumb::MORE).rect();
    let short = harness.get_by_role_and_label(Role::Button, "Tj").rect();

    // The name is cut at the cap (plus the margins and the dot), not as wide as the text.
    assert!(long_rect.width() <= crumb::MAX_TEXT_W + 30.0, "{long_rect:?}");
    assert!(long_rect.width() > crumb::MAX_TEXT_W, "{long_rect:?}");
    // No kind dot on the folded one: it is narrower than a crumb with a one-letter name.
    assert!(more.width() < short.width() + 18.0, "{more:?} {short:?}");
    // Smoke: the crumbs next to it keep their place after it.
    assert!(long_rect.max.x < more.min.x && more.max.x < short.min.x);
}

// ---- badge ----

#[test]
fn a_count_badge_is_a_sixteen_pixel_pill_as_wide_as_its_digits() {
    let mut harness = builder(vec2(200.0, 80.0)).build_ui(|ui| {
        ui.horizontal(|ui| {
            Badge::new(5).show(ui);
            Badge::new(377).tone(Tone::Warn).show(ui);
        });
    });
    harness.run();

    let pills: Vec<egui::Rect> = shapes_of(&harness)
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r) if r.rect.height() < 20.0 && r.fill != egui::Color32::TRANSPARENT => Some(r.rect),
            _ => None,
        })
        .collect();

    assert!(pills.len() >= 2, "{pills:?}");
    for pill in &pills {
        assert_eq!(pill.height(), 16.0);
    }
    // One digit makes a circle, three a pill.
    assert_eq!(pills[0].width(), 16.0);
    assert!(pills[1].width() > 20.0, "{pills:?}");
    // Smoke: the digits are still there to read.
    let _five = harness.get_by_role_and_label(Role::Label, "5");
    let _many = harness.get_by_role_and_label(Role::Label, "377");
}

// ---- sections and rows ----

#[test]
fn property_rows_touch_so_the_pointer_never_falls_between_two_of_them() {
    let mut harness = builder(vec2(340.0, 200.0)).build_ui(|ui| {
        section::show(ui, "polish-rows", "Rows", |ui| {
            PropRow::new("MediaBox", "[0 0 612 792]").show(ui);
            PropRow::new("CropBox", "[0 0 612 792]").show(ui);
            PropRow::new("Rotate", "0").show(ui);
        });
    });
    harness.run();

    let key = |name: &str| harness.get_by_role_and_label(Role::Label, name).rect();
    let (first, second, third) = (key("MediaBox"), key("CropBox"), key("Rotate"));

    // 28 px rows, 1 px apart.
    assert_eq!(second.center().y - first.center().y, tokens::ROW_H + 1.0);
    assert_eq!(third.center().y - second.center().y, tokens::ROW_H + 1.0);
    // Smoke: the keys share a column.
    assert_eq!(first.min.x, second.min.x);
}

#[test]
fn a_section_header_is_a_tall_target_that_shows_a_hand() {
    let mut harness = builder(vec2(340.0, 200.0)).build_ui(|ui| {
        section::show(ui, "polish-header", "Geometry", |ui| {
            ui.label("body");
        });
    });
    harness.run();
    let header = harness.get_by_role_and_label(Role::Button, "Geometry").rect();

    // Taller than its 10 px text and than the 18 px it is drawn in.
    assert!(header.height() >= 24.0, "{header:?}");
    assert_eq!(
        cursor_over(&mut harness, header.center()),
        CursorIcon::PointingHand
    );
    // Also in the padding just above and below the title.
    assert_eq!(
        cursor_over(&mut harness, pos2(header.center().x, header.min.y + 1.0)),
        CursorIcon::PointingHand
    );
    // Smoke: it still toggles.
    harness.get_by_role_and_label(Role::Button, "Geometry").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("body").is_none());
}

#[test]
fn a_link_value_is_a_target_only_over_its_text() {
    let mut harness = builder(vec2(340.0, 120.0)).build_ui(|ui| {
        PropRow::new("Parent", "3 0 R").link(true).show(ui);
        PropRow::new("Kind", "Page").show(ui);
    });
    harness.run();
    let link = harness.get_by_role_and_label(Role::Link, "3 0 R").rect();

    assert!(link.width() < 60.0, "{link:?}");
    assert_eq!(cursor_over(&mut harness, link.center()), CursorIcon::PointingHand);
    // Empty space to the right of it, in the value column, is not a link.
    let beside = pos2(link.min.x + 120.0, link.center().y);
    assert_ne!(cursor_over(&mut harness, beside), CursorIcon::PointingHand);
    // Smoke: a plain value never shows the hand.
    let plain = harness.get_by_role_and_label(Role::Label, "Page").rect();
    assert_ne!(
        cursor_over(&mut harness, plain.center()),
        CursorIcon::PointingHand
    );
}

#[test]
fn a_long_key_fits_its_column_and_a_wrapped_value_lines_its_key_up_with_its_first_line() {
    let long_key = "Layout: Placement";
    let wrapped = "a value long enough that it has to wrap onto a second line in the column";
    let mut harness = builder(vec2(340.0, 200.0)).build_ui_state(
        move |ui, natural: &mut f32| {
            *natural = ui
                .painter()
                .layout_no_wrap(
                    long_key.to_owned(),
                    egui::TextStyle::Body.resolve(ui.style()),
                    egui::Color32::WHITE,
                )
                .size()
                .x;
            PropRow::new(long_key, "Block").show(ui);
            PropRow::new("Fill", wrapped).show(ui);
        },
        0.0,
    );
    harness.run();

    // The key is not cut: its label is as wide as its text, and the text fits the column.
    let drawn = harness.get_by_role_and_label(Role::Label, long_key).rect();
    let natural = *harness.state();
    assert!(natural < prop_row::KEY_W - 4.0, "{natural}");
    assert!(
        (drawn.width() - natural).abs() < 1.0,
        "{drawn:?} against {natural}"
    );

    let key = harness.get_by_role_and_label(Role::Label, "Fill").rect();
    let value = harness.get_by_role_and_label(Role::Label, wrapped).rect();
    // Two lines or more; the key is centred on the first.
    assert!(value.height() > 24.0, "{value:?}");
    let first_line = value.min.y + key.height() / 2.0;
    assert!(
        (key.center().y - first_line).abs() < 1.0,
        "key {key:?} against the first line at {first_line} of {value:?}"
    );
    // Smoke: the value column starts where the shared constant says.
    assert_eq!(value.min.x - key.min.x, prop_row::VALUE_X - 8.0);
}

// ---- navigator ----

#[test]
fn the_filter_is_as_tall_as_a_control_and_shows_the_accent_while_it_has_the_focus() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.run();

    let field = nodes_in(&harness, Region::Navigator, Role::TextInput)
        .into_iter()
        .next()
        .expect("the filter field")
        .rect();
    // The field, its 3 px margins and its border make a 26 px control.
    assert_eq!(field.height() + 2.0 * 3.0 + 2.0, tokens::CONTROL_H);
    let outlined = |harness: &Harness<'_, pdfxray_app::PdfxrayApp>, color: egui::Color32| {
        shapes_of(harness).iter().any(|shape| {
            matches!(shape, Shape::Rect(r) if r.stroke.color == color
                && r.stroke.width == 1.0
                && r.rect.height() < tokens::CONTROL_H + 1.0
                && r.rect.height() > tokens::CONTROL_H - 3.0
                && r.rect.width() > 200.0)
        })
    };
    assert!(outlined(&harness, tokens::BORDER_STRONG));
    assert!(!outlined(&harness, tokens::ACCENT));

    harness.state_mut().state.nav.focus_filter = true;
    harness.run_steps(3);

    assert!(outlined(&harness, tokens::ACCENT));
    assert!(!outlined(&harness, tokens::BORDER_STRONG));
}

// ---- inspector ----

/// The crumbs of the inspector: its buttons in front of Pin.
fn crumbs_of(harness: &Harness<'_, pdfxray_app::PdfxrayApp>) -> Vec<(String, egui::Rect)> {
    nodes_in(harness, Region::Inspector, Role::Button)
        .into_iter()
        .map(|node| (node.accesskit_node().label().unwrap_or_default(), node.rect()))
        .take_while(|(name, _)| name != "Pin")
        .collect()
}

#[test]
fn a_long_file_name_is_cut_in_the_first_crumb_so_the_selection_stays_in_the_row() {
    let dir = std::env::temp_dir().join(format!("pdfxray-crumbs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let name = "Ventura Community Partners_Ventura Community Partners - 2027 - English (1).pdf";
    let path = dir.join(name);
    std::fs::copy(common::fixture_path("minimal-one-page"), &path).unwrap();
    let mut harness = app();
    common::open_path(&mut harness, path);
    harness.state_mut().state.select(pdfxray_core::NodeId::Page(0));
    harness.run_steps(3);

    let crumbs = crumbs_of(&harness);
    let first = crumbs.first().expect("the file crumb");
    let last = crumbs.last().expect("the selection crumb");
    let right_edge = common::WINDOW.x;

    // The file name is the first crumb, cut to the room the selection leaves it (more than the
    // width it is cut at when the row is short); the whole of it is in its tooltip.
    assert_eq!(first.0, name);
    assert!(first.1.width() > 150.0, "{first:?}");
    assert!(first.1.width() < 300.0, "{first:?}");
    // The selection stays inside the panel, and every crumb is on one row.
    assert!(last.1.max.x <= right_edge - 8.0, "{last:?}");
    assert!(
        crumbs
            .iter()
            .all(|(_, rect)| (rect.center().y - first.1.center().y).abs() < 1.0),
        "{crumbs:?}"
    );
    // Smoke: no crumb is as tall as the old 34 px pill.
    assert!(crumbs.iter().all(|(_, rect)| rect.height() < 24.0), "{crumbs:?}");
    std::fs::remove_file(dir.join(name)).ok();
}

#[test]
fn a_figure_without_alt_is_marked_once_in_its_tree_row() {
    let source = std::fs::read(common::fixture_path("struct-rich")).unwrap();
    // `/Alt` becomes `/Alx`: same length, so every offset in the file still holds.
    let patched: Vec<u8> = String::from_utf8_lossy(&source)
        .replace("/Alt (", "/Alx (")
        .into_bytes();
    let dir = std::env::temp_dir().join(format!("pdfxray-alt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("no-alt.pdf");
    std::fs::write(&path, patched).unwrap();
    let mut harness = app();
    common::open_path(&mut harness, path.clone());
    std::fs::remove_file(&path).ok();
    the(&harness, Region::Navigator, Role::Button, "Expand all").click();
    harness.run();

    let marks = common::texts_in(&harness, Region::Navigator, Role::Label)
        .into_iter()
        .filter(|text| text == "Alt")
        .count();

    assert_eq!(marks, 1);
}

// ---- batch B: inspector and navigator ----

/// A copy of a fixture under `file_name`, opened in a new window.
fn opened_as(fixture: &str, file_name: &str) -> Harness<'static, pdfxray_app::PdfxrayApp> {
    let dir = std::env::temp_dir().join(format!(
        "pdfxray-batch-b-{}-{}",
        std::process::id(),
        file_name.len()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(file_name);
    std::fs::copy(common::fixture_path(fixture), &path).unwrap();
    let mut harness = app();
    common::open_path(&mut harness, path.clone());
    std::fs::remove_file(&path).ok();
    harness
}

#[test]
fn a_file_name_that_fits_the_row_is_not_cut_in_the_breadcrumb() {
    let name = "reading-order-columns-extra.pdf";
    let mut harness = opened_as("minimal-one-page", name);
    harness.state_mut().state.select(pdfxray_core::NodeId::Document);
    harness.run_steps(3);

    let alone = crumbs_of(&harness);
    // The only crumb gets the row, not the 140 pt it was cut at.
    assert_eq!(alone.len(), 1, "{alone:?}");
    assert_eq!(alone[0].0, name);
    assert!(alone[0].1.width() > 200.0, "{alone:?}");

    // With the page after it, the name keeps what the page crumb leaves: still all of it.
    let shorter = "reading-order-columns.pdf";
    let mut harness = opened_as("minimal-one-page", shorter);
    harness.state_mut().state.select(pdfxray_core::NodeId::Page(0));
    harness.run_steps(3);
    let crumbs = crumbs_of(&harness);
    assert_eq!(crumbs.len(), 2, "{crumbs:?}");
    assert!(crumbs[0].1.width() > 170.0, "{crumbs:?}");
    assert!(crumbs[1].1.max.x <= common::WINDOW.x - 12.0, "{crumbs:?}");
}

#[test]
fn a_long_title_takes_two_lines_then_an_ellipsis_and_the_whole_name_is_its_tooltip() {
    let name = "Quarterly Accessibility Remediation Report - Northwind Traders and Associated Subsidiaries - 2027.pdf";
    let mut harness = opened_as("minimal-one-page", name);
    harness.state_mut().state.select(pdfxray_core::NodeId::Document);
    harness.run_steps(3);

    // The crumb carries the name too, on one line; the title is the tall one.
    let title = common::nodes_in(&harness, Region::Inspector, Role::Label)
        .into_iter()
        .filter(|node| node.value().as_deref() == Some(name))
        .map(|node| node.rect())
        .find(|rect| rect.height() > 20.0)
        .expect("the title");
    // Two lines of the 17 px title, not the four or five it took whole.
    assert!(title.height() < 50.0, "{title:?}");
    assert!(title.height() > 30.0, "{title:?}");

    harness.hover_at(title.center());
    harness.run_steps(40);
    let shown = common::all_label_texts(&harness)
        .into_iter()
        .filter(|text| text == name)
        .count();
    // The title itself and its tooltip.
    assert!(shown >= 2, "{shown}");
}

#[test]
fn a_wrapped_row_keeps_its_first_line_where_a_one_line_row_has_it() {
    let wrapped = "a value long enough that it has to wrap onto a second line in the column";
    let mut harness = builder(vec2(340.0, 240.0)).build_ui(move |ui| {
        section::show(ui, "wrapped-pitch", "Rows", |ui| {
            PropRow::new("Alpha", "1").show(ui);
            PropRow::new("Fill", wrapped).show(ui);
            PropRow::new("Beta", "2").show(ui);
        });
    });
    harness.run();

    let key = |name: &str| harness.get_by_role_and_label(Role::Label, name).rect();
    // The pitch from a one-line row to the first line of a wrapped one is a row and its gap.
    assert_eq!(
        key("Fill").center().y - key("Alpha").center().y,
        tokens::ROW_H + 1.0
    );
    // The lines that wrap go below, with the room above the first one at their foot.
    let value = harness.get_by_role_and_label(Role::Label, wrapped).rect();
    assert!(value.height() > 24.0, "{value:?}");
    assert!(key("Beta").center().y - key("Fill").center().y > tokens::ROW_H + 1.0);
}

#[test]
fn rows_and_section_headers_wash_the_same_width_with_the_same_corners() {
    let mut harness = builder(vec2(340.0, 200.0)).build_ui(|ui| {
        section::show(ui, "wash-extent", "Geometry", |ui| {
            PropRow::new("Rotate", "0").show(ui);
        });
    });
    harness.run();
    let wash = |harness: &Harness<'_, ()>| {
        shapes_of(harness)
            .into_iter()
            .find_map(|shape| match shape {
                Shape::Rect(r) if r.fill == tokens::NEUTRAL_HOVER => Some(r),
                _ => None,
            })
            .expect("a hover wash")
    };
    let header = harness.get_by_role_and_label(Role::Button, "Geometry").rect();
    harness.hover_at(header.center());
    harness.run_steps(2);
    let on_header = wash(&harness);
    let key = harness.get_by_role_and_label(Role::Label, "Rotate").rect();
    harness.hover_at(key.center());
    harness.run_steps(2);
    let on_row = wash(&harness);

    assert_eq!(on_header.rect.x_range(), on_row.rect.x_range());
    assert_eq!(on_header.corner_radius, on_row.corner_radius);
    // Smoke: the corners are the token's, not square.
    assert_eq!(on_row.corner_radius, egui::CornerRadius::from(tokens::RADIUS_SM));
}

#[test]
fn the_page_box_swatch_is_centred_on_the_line_of_its_name() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    common::click_rail(&mut harness, "Pages");
    harness.run_steps(3);

    let name = the(&harness, Region::Navigator, Role::Label, "MediaBox").rect();
    let swatch = shapes_of(&harness)
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Rect(r)
                if r.stroke.width == 1.5
                    && r.stroke.color
                        == pdfxray_app::center::canvas::overlay::box_color(pdfxray_core::BoxKind::Media)
                    && r.rect.width() == 12.0 =>
            {
                Some(r.rect)
            }
            _ => None,
        })
        .expect("the MediaBox swatch");

    assert!(
        (swatch.center().y - name.center().y).abs() <= 1.0,
        "swatch {swatch:?} against its name {name:?}"
    );
    // Smoke: it sits in front of the name, not over it.
    assert!(swatch.max.x < name.min.x, "{swatch:?} {name:?}");
}

#[test]
fn the_provenance_line_of_every_page_box_row_ends_inside_its_row() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    common::click_rail(&mut harness, "Pages");
    harness.run_steps(3);

    let rows: Vec<(String, egui::Rect)> = ["MediaBox", "CropBox", "BleedBox", "TrimBox", "ArtBox"]
        .into_iter()
        .map(|kind| {
            let row = harness.get_by_role_and_label(Role::Button, kind).rect();
            (kind.to_owned(), row)
        })
        .collect();
    let provenance: Vec<(String, egui::Rect)> = common::nodes_in(&harness, Region::Navigator, Role::Label)
        .into_iter()
        .filter_map(|node| {
            let text = node.value().unwrap_or_default();
            ["own", "inherited", "not set"]
                .iter()
                .any(|start| text.starts_with(start))
                .then(|| (text, node.rect()))
        })
        .collect();

    // One provenance line per row, each wholly inside the row it belongs to.
    assert_eq!(provenance.len(), rows.len(), "{provenance:?}");
    for (kind, row) in &rows {
        let (text, line) = provenance
            .iter()
            .find(|(_, line)| row.y_range().contains(line.min.y))
            .unwrap_or_else(|| panic!("no provenance line in the {kind} row {row:?}"));
        assert!(
            line.max.y <= row.max.y - 1.0,
            "{kind}: {text:?} {line:?} ends below {row:?}"
        );
    }
    // Smoke: the rows follow each other, touching (1 pt shared, no gap), never stacking up.
    for pair in rows.windows(2) {
        assert!((pair[1].1.min.y - pair[0].1.max.y).abs() <= 1.0, "{pair:?}");
    }
}

#[test]
fn the_paper_name_of_the_display_size_wraps_as_a_whole_and_copies_with_plain_spaces() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    common::click_rail(&mut harness, "Pages");
    harness.run_steps(3);

    let labels = common::texts_in(&harness, Region::Navigator, Role::Label);
    let size = labels
        .iter()
        .find(|text| text.contains("US\u{a0}Letter"))
        .unwrap_or_else(|| panic!("a Display size with the paper name in {labels:?}"));
    assert!(size.starts_with("612 × 792 pt"), "{size}");
    // Smoke: no plain space left inside the name.
    assert!(!size.contains("US Letter"), "{size}");
}

#[test]
fn the_thumbnail_under_the_pointer_takes_a_wash_and_a_hand() {
    let mut harness = app();
    open_fixture(&mut harness, "long-doc");
    common::click_rail(&mut harness, "Pages");
    common::wait_for_thumbnails(&mut harness);
    harness.remove_cursor();
    harness.run_steps(2);
    let washes = |harness: &Harness<'_, pdfxray_app::PdfxrayApp>| {
        shapes_of(harness)
            .into_iter()
            .filter(|shape| {
                matches!(shape, Shape::Rect(r) if r.fill == tokens::NEUTRAL_HOVER
                && r.rect.width() == 62.0)
            })
            .count()
    };
    assert_eq!(washes(&harness), 0);

    let second = the(&harness, Region::Navigator, Role::Button, "Page 2").rect();
    let at = second.center();
    assert_eq!(cursor_over(&mut harness, at), CursorIcon::PointingHand);

    assert_eq!(washes(&harness), 1);
}

#[test]
fn the_objects_count_follows_the_filter() {
    let mut harness = app();
    open_fixture(&mut harness, "rolemap-two-hops");
    common::click_rail(&mut harness, "Objects");
    harness.run_steps(3);
    let count = |harness: &Harness<'_, pdfxray_app::PdfxrayApp>| {
        let labels = common::texts_in(harness, Region::Navigator, Role::Label);
        let at = labels
            .iter()
            .position(|text| text == "ALL OBJECTS")
            .unwrap_or_else(|| panic!("the section header in {labels:?}"));
        labels[at + 1].clone()
    };
    let total = count(&harness);
    assert_ne!(total, "0");

    harness.state_mut().state.nav.filter = "zzq".to_owned();
    harness.run_steps(3);

    assert_eq!(count(&harness), "0");
    // Smoke: the line under it is the filter's own message.
    assert!(
        common::has_label(&harness, "No objects match “zzq”."),
        "{:?}",
        common::texts_in(&harness, Region::Navigator, Role::Label)
    );
}

#[test]
fn revisions_are_numbered_from_one_like_the_chip_and_the_newest_says_so() {
    let mut harness = app();
    open_fixture(&mut harness, "incremental-update");
    common::click_rail(&mut harness, "Revisions");
    harness.run_steps(3);

    let rows: Vec<String> = common::texts_in(&harness, Region::Navigator, Role::Label)
        .into_iter()
        .filter(|text| text.starts_with("Revision ") || text == "latest")
        .collect();

    assert_eq!(rows, ["Revision 1", "Revision 2", "latest"]);
    // Smoke: the title chip counts the same way.
    assert!(
        common::title_chip(&harness).contains("rev 2/2"),
        "{}",
        common::title_chip(&harness)
    );
}

#[test]
fn the_glyph_in_its_tile_stands_on_the_baseline_line() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    common::click_rail(&mut harness, "Content");
    harness.state_mut().state.select(pdfxray_core::NodeId::Glyph {
        page: 0,
        op: 3,
        glyph: 0,
    });
    harness.run_steps(3);

    let shapes = shapes_of(&harness);
    let baseline = shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::LineSegment { points, stroke } if stroke.color == tokens::PINK.gamma_multiply(0.7) => {
                Some(points[0].y)
            }
            _ => None,
        })
        .expect("the baseline line of the tile");
    let glyph = shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::Text(text)
                if text.galley.text() == "H" && text.galley.job.sections[0].format.font_id.size == 34.0 =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .expect("the glyph of the tile");
    let row = &glyph.galley.rows[0];
    let stands_on = glyph.pos.y + row.pos.y + row.glyphs[0].pos.y;

    // The text's own baseline lies on the pink line, not a descent above it.
    assert!(
        (stands_on - baseline).abs() < 0.6,
        "baseline {stands_on} against line {baseline}"
    );
    // Smoke: the line box reaches below the baseline, which is what lifted the glyph.
    assert!(glyph.pos.y + glyph.galley.size().y > baseline + 2.0);
}

#[test]
fn a_cell_of_the_table_grid_lights_up_under_the_pointer_whatever_its_outline() {
    let mut harness = app();
    open_fixture(&mut harness, "table-headers-ids");
    harness
        .state_mut()
        .state
        .select(pdfxray_core::NodeId::StructElem(pdfxray_core::ObjRef::new(18, 0)));
    harness.run_steps(4);
    harness.remove_cursor();
    harness.run_steps(2);

    let cells: Vec<(String, egui::Rect)> = common::nodes_in(&harness, Region::Inspector, Role::Button)
        .into_iter()
        .map(|node| (node.accesskit_node().label().unwrap_or_default(), node.rect()))
        .filter(|(name, _)| name.starts_with("Cell row") || name.starts_with("Header cell row"))
        .collect();
    assert!(cells.len() > 4, "{cells:?}");
    let lit = |harness: &Harness<'_, pdfxray_app::PdfxrayApp>, rect: egui::Rect| {
        shapes_of(harness)
            .iter()
            .any(|shape| matches!(shape, Shape::Rect(r) if r.fill == tokens::NEUTRAL_HOVER && r.rect == rect))
    };
    let mut warned = 0;
    for (name, rect) in &cells {
        harness.remove_cursor();
        harness.run_steps(2);
        assert!(!lit(&harness, *rect), "{name} is lit without the pointer");
        harness.hover_at(rect.center());
        harness.run_steps(2);
        if shapes_of(&harness)
            .iter()
            .any(|shape| matches!(shape, Shape::Rect(r) if r.rect == *rect && r.stroke.color == tokens::WARN))
        {
            warned += 1;
        }
        // The selected cell keeps its accent; every other one takes the wash.
        let selected = shapes_of(&harness).iter().any(
            |shape| matches!(shape, Shape::Rect(r) if r.rect == *rect && r.stroke.color == tokens::ACCENT),
        );
        assert!(selected || lit(&harness, *rect), "{name} has no hover wash");
    }
    // Smoke: the table has a cell outlined as missing its header, and it was among them.
    assert!(warned > 0, "no cell with a WARN outline in {cells:?}");
}

#[test]
fn a_new_selection_starts_the_inspector_at_the_top_and_a_scrolled_one_shows_a_hairline() {
    let mut harness = app();
    open_fixture(&mut harness, "struct-rich");
    common::click_rail(&mut harness, "Tags");
    harness.set_size(vec2(1440.0, 420.0));
    let element = |n: u32| pdfxray_core::NodeId::StructElem(pdfxray_core::ObjRef::new(n, 0));
    harness.state_mut().state.select(element(14));
    harness.run_steps(4);
    let first_section = |harness: &Harness<'_, pdfxray_app::PdfxrayApp>| {
        common::nodes_in(harness, Region::Inspector, Role::Button)
            .into_iter()
            .skip_while(|node| node.accesskit_node().label().as_deref() != Some("Pin"))
            .nth(1)
            .expect("a section header")
            .rect()
            .min
            .y
    };
    let hairlines = |harness: &Harness<'_, pdfxray_app::PdfxrayApp>| {
        shapes_of(harness)
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::LineSegment { stroke, points }
                if stroke.color == tokens::BORDER && points[0].x > 1100.0 && points[0].y == points[1].y)
            })
            .count()
    };
    let top = first_section(&harness);
    let before = hairlines(&harness);
    // What another node has of its own, before anything is scrolled.
    harness
        .state_mut()
        .state
        .select(pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 2 });
    harness.run_steps(4);
    let other = hairlines(&harness);
    harness.state_mut().state.select(element(14));
    harness.run_steps(4);

    harness.hover_at(pos2(1260.0, 250.0));
    for _ in 0..3 {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -120.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(2);
    }
    // The wheel's smoothing runs out.
    harness.run_steps(10);
    let scrolled = first_section(&harness);
    assert!(scrolled < top - 30.0, "did not scroll: {top} then {scrolled}");
    assert!(
        hairlines(&harness) > before,
        "no hairline under the header once scrolled"
    );

    harness
        .state_mut()
        .state
        .select(pdfxray_core::NodeId::MarkedContent { page: 0, mcid: 2 });
    harness.run_steps(4);

    // Another node is at the top again, and the hairline is gone.
    assert!(
        first_section(&harness) > top - 30.0,
        "{top} then {}",
        first_section(&harness)
    );
    assert_eq!(hairlines(&harness), other);
}

#[test]
fn the_transform_chain_starts_where_its_label_and_the_keys_do() {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    common::click_rail(&mut harness, "Content");
    harness
        .state_mut()
        .state
        .select(pdfxray_core::NodeId::Op { page: 0, ix: 3 });
    harness.run_steps(4);

    let label = the(&harness, Region::Inspector, Role::Label, "Transform chain").rect();
    let key = the(&harness, Region::Inspector, Role::Label, "CTM").rect();
    let first = common::texts_in(&harness, Region::Inspector, Role::Label)
        .into_iter()
        .skip_while(|text| text != "Transform chain")
        .nth(1)
        .expect("a chip after the label");
    let text = common::nodes_in(&harness, Region::Inspector, Role::Label)
        .into_iter()
        .find(|node| node.value().as_deref() == Some(first.as_str()) && node.rect().min.y > label.max.y)
        .expect("the first chip's text")
        .rect();
    // The chip is the smallest filled rectangle around its text.
    let chip = shapes_of(&harness)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(r) if r.rect.contains_rect(text) && r.rect.height() < 30.0 => Some(r.rect),
            _ => None,
        })
        .min_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("the chip of the first space");

    assert!(
        (chip.min.x - label.min.x).abs() <= 1.0,
        "chip {chip:?} against label {label:?}"
    );
    // Smoke: the keys of the rows above share that edge.
    assert!((key.min.x - label.min.x).abs() <= 1.0, "{key:?} {label:?}");
}
