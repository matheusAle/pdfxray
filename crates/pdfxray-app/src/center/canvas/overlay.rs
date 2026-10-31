//! Shapes drawn over the page image: page boxes, selection outlines and the measure tool.

use egui::{
    Align2, Area, Color32, Frame, Id, Label, Margin, Order, Painter, Pos2, Rect as UiRect, RichText, Shape,
    Stroke, StrokeKind, Ui, vec2,
};
use pdfxray_core::{BoxKind, BoxSource, PageBox, PageInfo, Point};

use super::geometry::PageGeometry;
use super::measure::{Comparison, Cursor, Line, MeasureTool, Offset};
use crate::theme::{effects, fonts, tokens};
use crate::units::Lengths;

/// Outline colour of the primary selection.
pub(crate) const SELECTION: Color32 = tokens::ACCENT;

/// Outline colour of the second selection.
pub(crate) const COMPARED: Color32 = tokens::PINK;

/// Colour of the measure line and of the gap markers.
pub(crate) const MEASURE: Color32 = tokens::ACCENT;

const DASH: f32 = 6.0;
const GAP: f32 = 4.0;

/// The overlay colour of a page box; every box has its own, the same in the Pages list. They are
/// the design's legend: Media red, Crop amber, Bleed green, Trim blue, Art violet.
pub fn box_color(kind: BoxKind) -> Color32 {
    match kind {
        BoxKind::Media => tokens::ERR,
        BoxKind::Crop => tokens::WARN,
        BoxKind::Bleed => tokens::OK,
        BoxKind::Trim => tokens::KIND_CONTENT,
        BoxKind::Art => tokens::KIND_TAG,
    }
}

/// True when the file does not set the box itself: it is inherited from nothing and falls back
/// to another box, so the overlay draws it dashed.
pub fn is_defaulted(source: BoxSource) -> bool {
    matches!(source, BoxSource::Default(_))
}

/// The name of a box without `Box`, as the legend and the cursor readout write it.
pub fn short_name(kind: BoxKind) -> &'static str {
    kind.key().strip_suffix("Box").unwrap_or(kind.key())
}

/// One row of the legend: the box name and its size (`612×792`), or what it falls back to
/// (`= Media`) when the file does not set it.
pub fn legend_row(page_box: &PageBox, lengths: &Lengths) -> (&'static str, String) {
    let size = match page_box.source {
        BoxSource::Default(of) if of != page_box.kind => format!("= {}", short_name(of)),
        BoxSource::Default(_) => "default".to_owned(),
        BoxSource::Own | BoxSource::Inherited(_) => format!(
            "{}×{}",
            lengths.number(page_box.rect.width()),
            lengths.number(page_box.rect.height())
        ),
    };
    (short_name(page_box.kind), size)
}

/// Draws the outlines of the boxes of `info` listed in `kinds`: solid when the file sets them,
/// dashed when they are defaults.
pub(crate) fn draw_boxes(painter: &Painter, geo: &PageGeometry, info: &PageInfo, kinds: &[BoxKind]) {
    for page_box in info.boxes.iter().filter(|b| kinds.contains(&b.kind)) {
        let rect = geo.rect_to_screen(page_box.rect);
        let stroke = Stroke::new(1.5, box_color(page_box.kind));
        if is_defaulted(page_box.source) {
            dashed_rect(painter, rect, stroke);
        } else {
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Middle);
        }
    }
}

fn dashed_rect(painter: &Painter, rect: UiRect, stroke: Stroke) {
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.extend(Shape::dashed_line(&corners, stroke, DASH, GAP));
}

/// Title of the boxes legend card.
pub const LEGEND_TITLE: &str = "Page boxes";

/// The legend: a title and one row per page box with its colour, name and size. Boxes whose
/// outline is hidden are dimmed.
pub(crate) fn legend_ui(
    ui: &mut Ui,
    info: &PageInfo,
    hidden: &std::collections::BTreeSet<BoxKind>,
    lengths: &Lengths,
) {
    ui.label(
        RichText::new(LEGEND_TITLE)
            .font(fonts::semibold(tokens::text::BODY))
            .color(tokens::TEXT),
    );
    egui::Grid::new("page-boxes-legend")
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for page_box in &info.boxes {
                let dim = hidden.contains(&page_box.kind);
                let color = box_color(page_box.kind);
                let (swatch, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().rect_stroke(
                    swatch.shrink(1.0),
                    2.0,
                    Stroke::new(1.5, if dim { color.gamma_multiply(0.35) } else { color }),
                    StrokeKind::Inside,
                );
                let (name, size) = legend_row(page_box, lengths);
                let ink = if dim { tokens::TEXT_3 } else { tokens::TEXT };
                ui.label(RichText::new(name).color(ink));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(size)
                            .font(fonts::mono(tokens::mono::SMALL))
                            .color(tokens::TEXT_3),
                    );
                });
                ui.end_row();
            }
        });
}

/// Fills and outlines a selected rectangle. `grown` (0..=1) is how far it has grown in: it starts
/// a little smaller and transparent and ends at its full size and strength.
pub(crate) fn draw_selection(painter: &Painter, rect: UiRect, color: Color32, grown: f32) {
    let scale = 0.94 + 0.06 * grown;
    let rect = UiRect::from_center_size(rect.center(), rect.size() * scale);
    let color = color.gamma_multiply(grown);
    painter.rect(
        rect,
        0.0,
        color.gamma_multiply(0.12),
        Stroke::new(2.0, color),
        StrokeKind::Inside,
    );
}

/// Draws the segments across the gaps between two compared boxes, each with a `gap 29.1 pt`
/// pill. The pills are labels of their own so they can be read without looking at pixels.
pub(crate) fn draw_gaps(ui: &Ui, geo: &PageGeometry, comparison: &Comparison, lengths: &Lengths) {
    let painter = ui.painter();
    for (n, (from, to)) in comparison.gap_segments().into_iter().enumerate() {
        let (a, b) = (geo.page_to_screen(from), geo.page_to_screen(to));
        painter.line_segment([a, b], Stroke::new(1.5, MEASURE));
        end_ticks(painter, a, b);
        let size = (to.x - from.x).abs().max((to.y - from.y).abs());
        pill(
            ui,
            ("gap", n),
            a.lerp(b, 0.5),
            &format!("gap {}", lengths.format(size)),
        );
    }
}

/// Short bars across both ends of the segment `a`-`b`.
fn end_ticks(painter: &Painter, a: Pos2, b: Pos2) {
    let along = (b - a).normalized();
    if !along.is_finite() {
        return;
    }
    let across = vec2(-along.y, along.x) * 4.0;
    for end in [a, b] {
        painter.line_segment([end - across, end + across], Stroke::new(1.5, MEASURE));
    }
}

/// Draws the measure line (or the pending start with `hover` as its provisional end) and its
/// distance pill.
pub(crate) fn draw_measure(ui: &Ui, geo: &PageGeometry, line: &Line, hover: Option<Pos2>, lengths: &Lengths) {
    let from = geo.page_to_screen(line.from);
    // Waiting for the second click, the line follows the pointer, snapped while Shift is held.
    let snap = ui.input(|i| i.modifiers.shift);
    let end = line.to.unwrap_or_else(|| {
        hover.map_or(line.from, |pos| {
            MeasureTool::end(line.from, geo.screen_to_page(pos), snap)
        })
    });
    let to = geo.page_to_screen(end);
    let painter = ui.painter();
    painter.line_segment([from, to], Stroke::new(1.5, MEASURE));
    end_ticks(painter, from, to);
    let offset = Offset::between(line.from, end);
    pill(
        ui,
        "measure",
        from.lerp(to, 0.5),
        &lengths.format(offset.distance),
    );
}

/// A label on an accent pill, centred above `at`. It floats over the canvas and stays inside the
/// visible part of it.
fn pill(ui: &Ui, key: impl std::hash::Hash + std::fmt::Debug, at: Pos2, text: &str) {
    Area::new(Id::unique(("canvas-pill", key)))
        .order(Order::Middle)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(at - vec2(0.0, 4.0))
        .interactable(false)
        .constrain_to(ui.clip_rect())
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(MEASURE)
                .corner_radius(tokens::RADIUS_SM)
                .inner_margin(Margin::symmetric(6, 1))
                .show(ui, |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(text)
                                .font(fonts::mono(tokens::mono::BODY))
                                .color(tokens::BG),
                        )
                        .selectable(false)
                        .extend(),
                    );
                });
        });
}

/// The two lines of the readout next to the crosshair: the position in the unit, and the boxes
/// the position is inside (the innermost first; a box that only repeats another one is left
/// out).
pub fn readout_lines(cursor: &Cursor, info: &PageInfo, lengths: &Lengths) -> [String; 2] {
    let position = format!(
        "x {}  y {} {}",
        lengths.number(cursor.page.x),
        lengths.number(cursor.page.y),
        lengths.unit.label()
    );
    [position, inside(cursor.page, info)]
}

fn inside(point: Point, info: &PageInfo) -> String {
    let mut names: Vec<&str> = info
        .boxes
        .iter()
        .rev()
        .filter(|b| !is_defaulted(b.source) && b.rect.contains(point))
        .map(|b| short_name(b.kind))
        .collect();
    // The MediaBox contains everything the others do; it only says something alone.
    if names.len() > 1 {
        names.retain(|name| *name != short_name(BoxKind::Media));
    }
    match names.as_slice() {
        [] if info.media_box.contains(point) => "inside Media".to_owned(),
        [] => "outside every box".to_owned(),
        names => format!("inside {}", names.join(" · ")),
    }
}

/// The crosshair at the pointer and the card that reads the position out. Both are for the
/// measure tool; the card is made of labels so tests can read it.
pub(crate) fn draw_crosshair(ui: &Ui, at: Pos2, info: &PageInfo, geo: &PageGeometry, lengths: &Lengths) {
    let painter = ui.painter();
    for stroke in [
        Stroke::new(3.0, tokens::BG.gamma_multiply(0.63)),
        Stroke::new(1.0, tokens::ACTIVE_INK),
    ] {
        painter.line_segment([at - vec2(10.0, 0.0), at + vec2(10.0, 0.0)], stroke);
        painter.line_segment([at - vec2(0.0, 10.0), at + vec2(0.0, 10.0)], stroke);
    }
    let point = geo.screen_to_page(at);
    let cursor = Cursor {
        page: point,
        display: geo.screen_to_display(at),
    };
    let [position, boxes] = readout_lines(&cursor, info, lengths);
    Area::new(Id::unique("canvas-readout"))
        .order(Order::Middle)
        .fixed_pos(at + vec2(16.0, 16.0))
        .interactable(false)
        .constrain_to(ui.clip_rect())
        .show(ui.ctx(), |ui| {
            effects::frosted_panel()
                .inner_margin(Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (text, ink) in [(position, tokens::TEXT), (boxes, tokens::TEXT_3)] {
                        ui.add(
                            Label::new(
                                RichText::new(text)
                                    .font(fonts::mono(tokens::mono::BODY))
                                    .color(ink),
                            )
                            .selectable(false)
                            .extend(),
                        );
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use pdfxray_core::ObjRef;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::fixture;
    use crate::units::Unit;

    const PT: Lengths = Lengths::new(Unit::Pt, 1.0);

    fn page(name: &str, ix: u32) -> PageInfo {
        fixture(name).page(ix).unwrap().clone()
    }

    fn at(x: f64, y: f64) -> Cursor {
        Cursor {
            page: Point::new(x, y),
            display: Point::new(x, y),
        }
    }

    #[test]
    fn every_box_has_its_own_colour() {
        let colors: HashSet<Color32> = BoxKind::ALL.into_iter().map(box_color).collect();

        assert_eq!(colors.len(), 5);
    }

    #[test]
    fn legend_rows_give_sizes_and_what_defaulted_boxes_fall_back_to() {
        let own = page("page-boxes", 0);
        let inherited = page("page-boxes", 1);

        let rows = |info: &PageInfo, lengths: &Lengths| -> Vec<(&str, String)> {
            info.boxes.iter().map(|b| legend_row(b, lengths)).collect()
        };

        assert_eq!(
            rows(&own, &PT),
            [
                ("Media", "400×300".to_owned()),
                ("Crop", "380×280".to_owned()),
                ("Bleed", "360×260".to_owned()),
                ("Trim", "340×240".to_owned()),
                ("Art", "320×220".to_owned()),
            ]
        );
        // Page 2 inherits its MediaBox (a set value, drawn solid) and defaults the rest.
        assert_eq!(
            rows(&inherited, &PT),
            [
                ("Media", "300×400".to_owned()),
                ("Crop", "= Media".to_owned()),
                ("Bleed", "= Crop".to_owned()),
                ("Trim", "= Crop".to_owned()),
                ("Art", "= Crop".to_owned()),
            ]
        );
        assert_eq!(
            rows(&own, &Lengths::new(Unit::In, 1.0))[0],
            ("Media", "5.556×4.167".to_owned())
        );
        assert!(!is_defaulted(BoxSource::Inherited(ObjRef::new(2, 0))));
        assert_eq!(short_name(BoxKind::Trim), "Trim");
    }

    #[test]
    fn the_readout_names_the_position_and_the_boxes_around_it() {
        let own = page("page-boxes", 0);
        let inherited = page("page-boxes", 1);

        // The centre is inside every box; the MediaBox is implied by the others.
        assert_eq!(
            readout_lines(&at(200.0, 150.0), &own, &PT),
            [
                "x 200  y 150 pt".to_owned(),
                "inside Art · Trim · Bleed · Crop".to_owned()
            ]
        );
        // Between the MediaBox and the CropBox only the MediaBox is around.
        assert_eq!(readout_lines(&at(5.0, 5.0), &own, &PT)[1], "inside Media");
        assert_eq!(readout_lines(&at(-5.0, 5.0), &own, &PT)[1], "outside every box");
        // Defaulted boxes repeat another box and are not listed.
        assert_eq!(
            readout_lines(&at(100.0, 100.0), &inherited, &PT)[1],
            "inside Media"
        );
        // The position follows the unit.
        assert_eq!(
            readout_lines(&at(72.0, 36.0), &own, &Lengths::new(Unit::Mm, 1.0))[0],
            "x 25.4  y 12.7 mm"
        );
    }
}
