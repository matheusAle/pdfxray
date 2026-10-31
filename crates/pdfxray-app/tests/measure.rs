//! Rulers, cursor readout, the measure tool, page boxes overlay and node comparison, end to end.

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

use common::{
    Region, all_label_texts, app, click_at, fixture_path, has_label, open_fixture, texts_in, the,
    wait_for_render,
};
use egui::{Key, Modifiers, Pos2, Role, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfxray_app::{PdfxrayApp, Unit};
use pdfxray_core::{BoxKind, Document, LayerMode, NodeId, Point};
use pdfxray_render::{PixelPoint, RenderSpec, render_page};
use pretty_assertions::assert_eq;

/// minimal-one-page at 100 %: the page is 200 x 100 pt shown 1:1.
fn minimal_at_100() -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, "minimal-one-page");
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    wait_for_render(&mut harness);
    harness
}

fn page_origin(harness: &Harness<'_, PdfxrayApp>, page: u32) -> Pos2 {
    harness
        .get_by_role_and_label(Role::Image, &format!("Page {} canvas", page + 1))
        .rect()
        .min
}

fn toggle_measure(harness: &mut Harness<'_, PdfxrayApp>) {
    the(harness, Region::Center, Role::Button, "Measure").click();
    harness.run();
}

fn click_unit(harness: &mut Harness<'_, PdfxrayApp>, unit: Unit) {
    the(harness, Region::Chrome, Role::Button, unit.label()).click();
    harness.run();
}

/// The page-space point under the screen position `pos` of a page shown 1:1, from the
/// transform the renderer produced for `fixture`.
fn expected_point(fixture: &str, page: u32, origin: Pos2, pos: Pos2) -> Point {
    let doc = Document::open(fixture_path(fixture)).unwrap();
    let rendered = render_page(&doc, page, &RenderSpec::new(1.0)).unwrap();
    rendered.pixel_to_page(PixelPoint::new(
        f64::from(pos.x - origin.x),
        f64::from(pos.y - origin.y),
    ))
}

/// The text rows of the measure card: hint and measurements.
fn rows(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    texts_in(harness, Region::Center, Role::Label)
        .into_iter()
        .filter(|t| t.contains(": "))
        .collect()
}

fn readout(harness: &Harness<'_, PdfxrayApp>, prefix: &str) -> Vec<String> {
    rows(harness)
        .into_iter()
        .filter(|t| t.starts_with(prefix))
        .collect()
}

#[test]
fn two_clicks_measure_the_distance_in_points_and_millimetres() {
    let mut harness = minimal_at_100();
    let origin = page_origin(&harness, 0);
    toggle_measure(&mut harness);
    assert!(has_label(
        &harness,
        "Measure: drag across the page, or click two points. Hold Shift to snap to 45°."
    ));

    // 30 points right and 40 down: a 3-4-5 triangle scaled by 10.
    let (a, b) = (origin + vec2(20.0, 20.0), origin + vec2(50.0, 60.0));
    click_at(&mut harness, a);
    // After one click the line is pending: nothing is measured yet.
    assert_eq!(harness.state().state.canvas.measure.offset(), None);
    click_at(&mut harness, b);

    // Expected values come from the render transform, not from the zoom.
    let (pa, pb) = (
        expected_point("minimal-one-page", 0, origin, a),
        expected_point("minimal-one-page", 0, origin, b),
    );
    assert_eq!((pa.x.round(), pa.y.round()), (20.0, 80.0));
    assert_eq!((pb.x.round(), pb.y.round()), (50.0, 40.0));
    let offset = harness.state().state.canvas.measure.offset().unwrap();
    assert!((offset.dx - (pb.x - pa.x)).abs() < 1e-3, "{offset:?}");
    assert!((offset.dy - (pb.y - pa.y)).abs() < 1e-3, "{offset:?}");
    assert!((offset.distance - (pb.x - pa.x).hypot(pb.y - pa.y)).abs() < 1e-3);
    assert_eq!(rows(&harness), ["Distance: 50 pt", "Δx: +30 pt", "Δy: -40 pt"]);
    // Measuring does not select anything: the hit stack stays empty.
    assert_eq!(harness.state().state.drawer.hit_stack, None);

    // 1 pt = 25.4 / 72 mm, so 50 pt = 17.6389 mm.
    click_unit(&mut harness, Unit::Mm);
    let mm = |pt: f64| format!("{:.2}", pt * 25.4 / 72.0);
    assert_eq!(
        rows(&harness),
        [
            format!("Distance: {} mm", mm(50.0)),
            format!("Δx: +{} mm", mm(30.0)),
            format!("Δy: -{} mm", mm(40.0)),
        ]
    );
    assert_eq!(readout(&harness, "Distance"), ["Distance: 17.64 mm"]);
    click_unit(&mut harness, Unit::In);
    assert_eq!(readout(&harness, "Distance"), ["Distance: 0.694 in"]);
    click_unit(&mut harness, Unit::Px);
    assert_eq!(readout(&harness, "Distance"), ["Distance: 66.67 px"]);
    // Smoke: back to points gives the original text.
    click_unit(&mut harness, Unit::Pt);
    assert_eq!(readout(&harness, "Distance"), ["Distance: 50 pt"]);
}

#[test]
fn dragging_measures_too_and_the_third_click_starts_over() {
    let mut harness = minimal_at_100();
    let origin = page_origin(&harness, 0);
    toggle_measure(&mut harness);

    let (a, b) = (origin + vec2(10.0, 10.0), origin + vec2(70.0, 90.0));
    harness.hover_at(a);
    harness.drag_at(a);
    harness.hover_at(a + vec2(5.0, 5.0));
    harness.hover_at(b);
    harness.drop_at(b);
    harness.run();

    assert_eq!(rows(&harness), ["Distance: 100 pt", "Δx: +60 pt", "Δy: -80 pt"]);

    // A plain click afterwards begins a new line: the old measurement is gone.
    click_at(&mut harness, origin + vec2(100.0, 50.0));
    assert_eq!(harness.state().state.canvas.measure.offset(), None);
    assert!(has_label(
        &harness,
        "Measure: drag across the page, or click two points. Hold Shift to snap to 45°."
    ));
}

#[test]
fn the_m_key_toggles_the_tool_and_escape_forgets_the_line() {
    let mut harness = minimal_at_100();
    assert!(!harness.state().state.canvas.measure.active);

    harness.key_press(Key::M);
    harness.run();
    assert!(harness.state().state.canvas.measure.active);
    let origin = page_origin(&harness, 0);
    click_at(&mut harness, origin + vec2(10.0, 10.0));
    click_at(&mut harness, origin + vec2(40.0, 10.0));
    assert_eq!(readout(&harness, "Distance"), ["Distance: 30 pt"]);

    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(readout(&harness, "Distance"), Vec::<String>::new());
    assert!(harness.state().state.canvas.measure.active);

    harness.key_press(Key::M);
    harness.run();
    assert!(!harness.state().state.canvas.measure.active);
    assert!(!has_label(
        &harness,
        "Measure: drag across the page, or click two points. Hold Shift to snap to 45°."
    ));
}

#[test]
fn the_cursor_readout_shows_page_and_display_space() {
    let mut harness = minimal_at_100();
    let origin = page_origin(&harness, 0);

    harness.hover_at(origin + vec2(30.0, 20.0));
    harness.run();

    // y is up in page space and down in display space.
    assert!(has_label(&harness, "x 30  y 80 pt · display 30, 20"));
    click_unit(&mut harness, Unit::Mm);
    harness.hover_at(origin + vec2(72.0, 36.0));
    harness.run();
    // 72 pt = 25.4 mm; the page is 100 pt tall: 64 pt up, 36 pt down.
    assert!(
        has_label(&harness, "x 25.4  y 22.58 mm · display 25.4, 12.7"),
        "{:?}",
        all_label_texts(&harness)
    );
    // Smoke: the readout disappears when the pointer leaves the page.
    harness.remove_cursor();
    harness.run();
    assert_eq!(harness.state().state.canvas.cursor, None);
}

#[test]
fn on_a_rotated_page_display_space_is_turned_and_page_space_is_not() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    wait_for_render(&mut harness);
    // Page 1 is /Rotate 90 with the CropBox [10 10 390 290]: shown 280 wide and 380 tall.
    let origin = page_origin(&harness, 0);
    let image = harness.get_by_role_and_label(Role::Image, "Page 1 canvas").rect();
    assert_eq!((image.width(), image.height()), (280.0, 380.0));

    harness.hover_at(origin + vec2(3.0, 5.0));
    harness.run();

    // The bottom-left corner of the CropBox is the top-left of a page turned clockwise.
    assert!(
        has_label(&harness, "x 15  y 13 pt · display 3, 5"),
        "{:?}",
        all_label_texts(&harness)
    );
}

#[test]
fn a_page_with_a_user_unit_shows_physical_lengths_and_says_so() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    // Page 3 is 200 x 200 user units with /UserUnit 2: every length is twice its coordinate.
    harness.state_mut().state.go_to_page(2);
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    wait_for_render(&mut harness);
    let origin = page_origin(&harness, 2);

    harness.hover_at(origin + vec2(50.0, 50.0));
    harness.run();

    assert!(
        has_label(&harness, "x 100  y 300 pt · display 100, 100 · UserUnit 2"),
        "{:?}",
        all_label_texts(&harness)
    );
    harness.get_by_label("0 pt").hover();
    harness.run();
    harness.step();
    assert!(has_label(&harness, &pdfxray_app::rulers::origin_help(2.0)));
}

#[test]
fn the_ruler_corner_names_the_origin_and_follows_the_unit() {
    let mut harness = minimal_at_100();

    assert_eq!(
        texts_in(&harness, Region::Center, Role::Label)
            .iter()
            .filter(|t| t.starts_with("0 "))
            .collect::<Vec<_>>(),
        ["0 pt"]
    );
    // Hovering the corner explains the convention.
    harness.get_by_label("0 pt").hover();
    harness.run();
    harness.step();
    assert!(has_label(&harness, pdfxray_app::rulers::ORIGIN_HELP));
    assert!(pdfxray_app::rulers::ORIGIN_HELP.contains("top-left corner of the page as shown"));

    click_unit(&mut harness, Unit::Mm);
    let _corner = harness.get_by_label("0 mm");
    assert!(harness.query_by_label("0 pt").is_none());
}

#[test]
fn the_page_boxes_legend_gives_each_box_its_size_or_what_it_falls_back_to() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    // Room beside the page for the card to be in full.
    harness.state_mut().state.set_zoom(0.5);
    harness.run();
    wait_for_render(&mut harness);
    // Off by default on the Tags entry: no legend.
    let legend = |harness: &Harness<'_, PdfxrayApp>| {
        let labels = texts_in(harness, Region::Center, Role::Label);
        labels
            .iter()
            .position(|t| t == "Page boxes")
            .map(|at| labels[at + 1..at + 11].to_vec())
    };
    assert_eq!(legend(&harness), None);

    the(&harness, Region::Center, Role::Button, "Page boxes").click();
    harness.run();
    wait_for_render(&mut harness);

    // Page 1 sets all five boxes itself: names and sizes.
    assert!(harness.state().state.canvas.show_boxes);
    assert_eq!(
        legend(&harness).unwrap(),
        [
            "Media", "400×300", "Crop", "380×280", "Bleed", "360×260", "Trim", "340×240", "Art", "320×220"
        ]
    );
    // Page 2 inherits its MediaBox and defaults the rest; page 3 sets Media, Trim and defaults the rest.
    the(&harness, Region::Center, Role::Button, "Next").click();
    harness.run();
    wait_for_render(&mut harness);
    assert_eq!(
        legend(&harness).unwrap(),
        [
            "Media", "300×400", "Crop", "= Media", "Bleed", "= Crop", "Trim", "= Crop", "Art", "= Crop"
        ]
    );
    the(&harness, Region::Center, Role::Button, "Next").click();
    harness.run();
    wait_for_render(&mut harness);
    assert_eq!(
        legend(&harness).unwrap(),
        [
            "Media", "400×400", "Crop", "= Media", "Bleed", "= Crop", "Trim", "200×200", "Art", "= Crop"
        ]
    );
    // Sizes are physical: page 3 has a /UserUnit of 2, so 200 user units are 400 pt, and they
    // follow the unit.
    click_unit(&mut harness, Unit::Mm);
    assert_eq!(
        legend(&harness).unwrap()[..4],
        ["Media", "141.11×141.11", "Crop", "= Media"]
    );
    // Smoke: switching it off removes the legend again.
    the(&harness, Region::Center, Role::Button, "Page boxes").click();
    harness.run();
    wait_for_render(&mut harness);
    assert_eq!(legend(&harness), None);
}

#[test]
fn selecting_a_far_away_node_scrolls_it_into_view() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    harness.state_mut().state.go_to_page(2);
    harness.state_mut().state.set_zoom(8.0);
    harness.run();
    wait_for_render(&mut harness);
    // Going to page 3 scrolls the continuous view down to its top edge.
    let top = harness.state().state.canvas.scroll_offset;
    assert_eq!(top.x, 0.0);
    assert!(top.y > 3000.0, "scrolled down to page 3: {top:?}");
    assert_eq!(harness.state().state.canvas.page, 2);

    // The TrimBox of page 3 is [150 150 250 250]: the top right of a 200 pt page, 1200 pt in.
    harness.state_mut().state.select(NodeId::PageBox {
        page: 2,
        kind: BoxKind::Trim,
    });
    harness.run();
    wait_for_render(&mut harness);

    let offset = harness.state().state.canvas.scroll_offset;
    assert!(offset.x > 600.0, "scrolled right: {offset:?}");
    // Centring the box keeps the view on page 3.
    let view = harness.state().state.canvas.viewport;
    assert!(
        (offset.y - top.y).abs() < view.y,
        "stayed on page 3: {offset:?} from {top:?}"
    );
    assert_eq!(harness.state().state.canvas.page, 2);
    // The outline is what the state says it is.
    let rect = harness.state().state.selection_rect().unwrap();
    assert_eq!((rect.page, rect.rect.x0, rect.rect.x1), (2, 150.0, 250.0));
    // Smoke: a second frame does not keep scrolling.
    harness.run();
    assert_eq!(harness.state().state.canvas.scroll_offset, offset);
}

#[test]
fn shift_click_in_the_hit_stack_compares_two_boxes() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    // The centre of page 1 is inside all five boxes; the smallest, the ArtBox, is selected.
    harness.state_mut().state.canvas.layer_mode = LayerMode::Boxes;
    // Five hits: give the drawer room for all of them.
    harness.state_mut().state.drawer.height = 300.0;
    harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .click();
    harness.run();
    assert_eq!(
        harness.state().state.selection.primary,
        Some(NodeId::PageBox {
            page: 0,
            kind: BoxKind::Art
        })
    );

    the(&harness, Region::Drawer, Role::Button, "MediaBox").click_modifiers(Modifiers::SHIFT);
    harness.run();

    // The first selection stays; the MediaBox is the second one.
    assert_eq!(
        harness.state().state.selection.secondary,
        Some(NodeId::PageBox {
            page: 0,
            kind: BoxKind::Media
        })
    );
    // ArtBox [40 40 360 260] inside MediaBox [0 0 400 300]: the second's edges are 40 further
    // out on the left and 40 further out on the right, and it covers the first one's width.
    let comparison = harness.state().state.comparison().unwrap();
    assert_eq!(
        (
            comparison.gap().distance,
            comparison.left_delta(),
            comparison.right_delta()
        ),
        (0.0, -40.0, 40.0)
    );
    let labels = texts_in(&harness, Region::Inspector, Role::Label);
    let at = labels.iter().position(|t| t == "Left edges").unwrap();
    assert_eq!(
        labels[at..at + 6],
        [
            "Left edges",
            "Δ -40 pt",
            "Right edges",
            "Δ +40 pt",
            "Horizontal",
            "B fully overlaps A"
        ]
    );
}

/// A page at 100 % with the measure tool on.
fn measuring(fixture: &str, page: u32) -> Harness<'static, PdfxrayApp> {
    let mut harness = app();
    open_fixture(&mut harness, fixture);
    harness.state_mut().state.go_to_page(page);
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    wait_for_render(&mut harness);
    toggle_measure(&mut harness);
    harness
}

/// Holds Shift down (`true`) or lets it go.
fn shift(harness: &mut Harness<'_, PdfxrayApp>, down: bool) {
    let modifiers = if down { Modifiers::SHIFT } else { Modifiers::NONE };
    harness.event(egui::Event::ModifiersChanged(modifiers));
}

#[test]
fn holding_shift_while_dragging_snaps_the_line_to_45_degrees() {
    let mut harness = measuring("page-boxes", 1);
    let origin = page_origin(&harness, 1);
    // 60 points right and 8 down, near the top of the page: the cards float beside it.
    let (a, b) = (origin + vec2(10.0, 20.0), origin + vec2(70.0, 28.0));

    shift(&mut harness, true);
    harness.hover_at(a);
    harness.drag_at(a);
    harness.hover_at(b);
    harness.drop_at(b);
    harness.run();
    shift(&mut harness, false);
    harness.run();

    // Horizontal: Δy is exactly 0 and Δx is what the pointer travelled across.
    let offset = harness.state().state.canvas.measure.offset().unwrap();
    assert_eq!((offset.dx.round(), offset.dy), (60.0, 0.0));
    assert_eq!(rows(&harness), ["Distance: 60 pt", "Δx: +60 pt", "Δy: 0 pt"]);

    // Smoke: the same drag without Shift keeps its slope.
    harness.hover_at(a);
    harness.drag_at(a);
    harness.hover_at(b);
    harness.drop_at(b);
    harness.run();
    let free = harness.state().state.canvas.measure.offset().unwrap();
    assert_eq!((free.dx.round(), free.dy.round()), (60.0, -8.0));
}

#[test]
fn holding_shift_for_the_second_click_snaps_it_to_the_anchor() {
    let mut harness = measuring("page-boxes", 1);
    let origin = page_origin(&harness, 1);

    // The anchor, then a second click 5 points right of straight down, with Shift held.
    click_at(&mut harness, origin + vec2(100.0, 150.0));
    shift(&mut harness, true);
    click_at(&mut harness, origin + vec2(105.0, 250.0));
    shift(&mut harness, false);
    harness.run();

    let offset = harness.state().state.canvas.measure.offset().unwrap();
    assert_eq!((offset.dx, offset.dy.round()), (0.0, -100.0));
    assert_eq!(rows(&harness)[0], "Distance: 100 pt");
}

fn centre_texts(harness: &Harness<'_, PdfxrayApp>) -> Vec<String> {
    texts_in(harness, Region::Center, Role::Label)
}

#[test]
fn a_dragged_line_has_a_pill_with_its_exact_length_in_the_unit() {
    let mut harness = measuring("page-boxes", 1);
    let origin = page_origin(&harness, 1);

    // 60 points right and 80 down: 100 points, on a page that is unrotated and 1:1.
    let (a, b) = (origin + vec2(10.0, 20.0), origin + vec2(70.0, 100.0));
    harness.hover_at(a);
    harness.drag_at(a);
    harness.hover_at(b);
    harness.drop_at(b);
    harness.run();

    let offset = harness.state().state.canvas.measure.offset().unwrap();
    assert_eq!(
        (offset.dx.round(), offset.dy.round(), offset.distance.round()),
        (60.0, -80.0, 100.0)
    );
    assert_eq!(rows(&harness), ["Distance: 100 pt", "Δx: +60 pt", "Δy: -80 pt"]);
    // The pill on the line says the same, once.
    assert_eq!(
        common::named(&harness, Region::Center, Role::Label, "100 pt").len(),
        1
    );

    click_unit(&mut harness, Unit::Mm);

    // 100 pt = 35.2778 mm.
    assert_eq!(rows(&harness)[0], "Distance: 35.28 mm");
    assert_eq!(
        common::named(&harness, Region::Center, Role::Label, "35.28 mm").len(),
        1
    );
    assert!(!centre_texts(&harness).contains(&"100 pt".to_owned()));
    // Smoke: turning the tool off takes the line and its pill away.
    toggle_measure(&mut harness);
    assert!(!centre_texts(&harness).contains(&"35.28 mm".to_owned()));
}

#[test]
fn the_crosshair_reads_out_the_position_and_the_boxes_around_it() {
    let mut harness = measuring("page-boxes", 0);
    let origin = page_origin(&harness, 0);

    // The middle of the turned page 1 is the middle of its CropBox, (200, 150), inside every box.
    let middle = origin + vec2(140.0, 190.0);
    harness.hover_at(middle);
    harness.run();

    let point = expected_point("page-boxes", 0, origin, middle);
    assert_eq!((point.x.round(), point.y.round()), (200.0, 150.0));
    let texts = centre_texts(&harness);
    assert!(texts.contains(&"x 200  y 150 pt".to_owned()), "{texts:?}");
    assert!(
        texts.contains(&"inside Art · Trim · Bleed · Crop".to_owned()),
        "{texts:?}"
    );

    // At the corner of the page only the CropBox is around (the page shows the CropBox, so
    // the MediaBox margin is never under the pointer). The bottom corner.
    let bottom_left = harness
        .get_by_role_and_label(Role::Image, "Page 1 canvas")
        .rect()
        .left_bottom();
    harness.hover_at(bottom_left + vec2(1.0, -1.0));
    harness.run();
    assert!(centre_texts(&harness).contains(&"inside Crop".to_owned()));
    // The readout follows the unit.
    click_unit(&mut harness, Unit::Mm);
    harness.hover_at(origin + vec2(72.0, 236.0));
    harness.run();
    assert!(
        centre_texts(&harness)
            .iter()
            .any(|t| t.ends_with(" mm") && t.starts_with("x ")),
        "{:?}",
        centre_texts(&harness)
    );
    // Smoke: with the tool off there is no readout.
    toggle_measure(&mut harness);
    assert!(!centre_texts(&harness).iter().any(|t| t.starts_with("inside")));
}

/// Shift-click at the page-space point (`x`, `y`) of a 400 x 400 page shown 1:1.
fn shift_click_page(harness: &mut Harness<'_, PdfxrayApp>, origin: Pos2, x: f32, y: f32) {
    let pos = origin + vec2(x, 400.0 - y);
    harness.event(egui::Event::ModifiersChanged(Modifiers::SHIFT));
    click_at(harness, pos);
    harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    harness.run();
}

fn op(ix: u32) -> NodeId {
    NodeId::Op { page: 0, ix }
}

#[test]
fn shift_clicking_a_second_node_shows_the_gap_the_alignment_and_the_print_boxes() {
    let mut harness = app();
    open_fixture(&mut harness, "graphics-state");
    harness.state_mut().state.set_zoom(1.0);
    harness.run();
    wait_for_render(&mut harness);
    let origin = page_origin(&harness, 0);

    // The orange square (operator 49) and, 10 pt to its left, the green one (operator 38).
    click_at(&mut harness, origin + vec2(225.0, 400.0 - 105.0));
    // The click may open the drawer, which moves the page: measure from where it is now.
    let origin = page_origin(&harness, 0);
    shift_click_page(&mut harness, origin, 190.0, 140.0);

    assert_eq!(harness.state().state.selection.primary, Some(op(49)));
    assert_eq!(harness.state().state.selection.secondary, Some(op(38)));
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    let from = inspector.iter().position(|t| t == "2 selected · gap").unwrap();
    assert_eq!(
        inspector[from..],
        [
            "2 selected · gap",
            "shift-click adds to the selection",
            "10 pt",
            "horizontal gap, left of A → right of B",
            "3.53",
            "mm",
            "0.139",
            "in",
            "13.33",
            "px",
            "Left edges",
            "Δ -50 pt",
            "Right edges",
            "Δ -110 pt",
            "Horizontal",
            "apart 10 pt",
            "Bleed",
            "not set",
            "Trim size",
            "400 × 400 pt · 141.11 × 141.11 mm",
            "Safe area",
            "no ArtBox set",
        ]
    );
    // The crumbs list both nodes, the second after a plus.
    assert_eq!(inspector[..from], ["Page 1", "f #49", "+", "f #38"]);
    // The canvas marks the gap with a pill.
    assert!(centre_texts(&harness).contains(&"gap 10 pt".to_owned()));

    // Everything follows the unit.
    click_unit(&mut harness, Unit::Mm);
    assert!(centre_texts(&harness).contains(&"gap 3.53 mm".to_owned()));
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    assert!(inspector.contains(&"3.53 mm".to_owned()), "{inspector:?}");
    assert!(inspector.contains(&"Δ -17.64 mm".to_owned()), "{inspector:?}");

    // Shift-clicking the second node again removes it: back to the single-node inspector.
    shift_click_page(&mut harness, origin, 190.0, 140.0);
    assert_eq!(harness.state().state.selection.secondary, None);
    assert!(!has_label(&harness, "2 selected · gap"));
}

#[test]
fn nodes_apart_in_both_directions_have_a_diagonal_gap_with_a_pill_on_each_side() {
    let mut harness = app();
    open_fixture(&mut harness, "graphics-state");
    harness.state_mut().state.select(op(38));
    harness.state_mut().state.select_additional(op(3));
    harness.run();
    wait_for_render(&mut harness);

    // Operator 38 is (170,120)-(210,160), operator 3 (20,20)-(100,80): 70 across, 40 up.
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    let from = inspector.iter().position(|t| t == "80.62 pt").unwrap();
    assert_eq!(inspector[from + 1], "diagonal gap, corner of A → corner of B");
    assert!(centre_texts(&harness).contains(&"gap 70 pt".to_owned()));
    assert!(centre_texts(&harness).contains(&"gap 40 pt".to_owned()));
}

#[test]
fn edges_that_line_up_are_aligned_and_a_box_inside_another_has_no_gap() {
    let mut harness = app();
    open_fixture(&mut harness, "page-boxes");
    // Page 3 has no CropBox: it is the MediaBox, so their edges coincide.
    harness.state_mut().state.select(NodeId::PageBox {
        page: 2,
        kind: BoxKind::Media,
    });
    harness.state_mut().state.select_additional(NodeId::PageBox {
        page: 2,
        kind: BoxKind::Crop,
    });
    harness.run();

    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    let from = inspector.iter().position(|t| t == "0 pt").unwrap();
    assert_eq!(inspector[from..from + 2], ["0 pt", "A and B overlap, no gap"]);
    let at = inspector.iter().position(|t| t == "Left edges").unwrap();
    assert_eq!(
        inspector[at..at + 6],
        [
            "Left edges",
            "aligned · Δ 0 pt",
            "Right edges",
            "aligned · Δ 0 pt",
            "Horizontal",
            "A and B span the same width"
        ]
    );
    // The page has a UserUnit of 2 and a TrimBox of 100 user units: 200 pt, in the print boxes.
    assert!(inspector.contains(&"Trim size".to_owned()));
    assert!(
        inspector.contains(&"200 × 200 pt · 70.56 × 70.56 mm".to_owned()),
        "{inspector:?}"
    );
}

#[test]
fn side_by_side_nodes_have_a_horizontal_gap_and_edges_that_are_off() {
    let mut harness = app();
    open_fixture(&mut harness, "graphics-state");
    harness.state_mut().state.select(op(3));
    harness.state_mut().state.select_additional(op(12));
    harness.run();

    // Operator 3 is (20,20)-(100,80); operator 12 is stroked 4 wide, so it spans (118,18)-(202,82).
    let inspector = texts_in(&harness, Region::Inspector, Role::Label);
    let from = inspector.iter().position(|t| t == "18 pt").unwrap();
    assert_eq!(
        inspector[from..from + 2],
        ["18 pt", "horizontal gap, right of A → left of B"]
    );
    let at = inspector.iter().position(|t| t == "Left edges").unwrap();
    assert_eq!(
        inspector[at..at + 6],
        [
            "Left edges",
            "Δ +98 pt",
            "Right edges",
            "Δ +102 pt",
            "Horizontal",
            "apart 18 pt"
        ]
    );
    assert_eq!(harness.state().state.comparison().unwrap().gap_x(), 18.0);
}
