//! The tag overlay of the canvas: every structure element's box in its role colour, the chips
//! that name them, the artifacts, and the reading order as numbered badges joined by a path.
//!
//! What shows is set in the View menu ([`TagShow`]): *All tag boxes* (`⇧T`) outlines every
//! element on the page and is what the labels, nesting and artifact options decorate; *Reading
//! order* (`R`) draws the badges and the path whether or not the boxes are on. Colours come from
//! [`crate::roles`], so a role reads the same here, in the Tags tree and in the legend.
//!
//! Chips and badges are buttons: a click selects the element they stand for. A click inside a
//! box selects the innermost element whose box holds the point, and hovering one outlines it.
//! The path draws from badge 1 to the last over [`motion::PATH`] when it is shown.

use std::sync::Arc;

use egui::{
    Align2, Color32, CursorIcon, Id, Painter, Pos2, Rect as UiRect, Role, Sense, Shape, Stroke, StrokeKind,
    Ui, Vec2, WidgetInfo, pos2, vec2,
};
use pdfxray_core::{
    NodeId, OrderStep, PageIx, Point, ReadingOrder, RoleClass, SpeechPreset, StepKind, TagBox,
};
use pdfxray_render::role_slot;

use super::connector;
use super::geometry::PageGeometry;
use crate::components::tooltip;
use crate::modes::{OrderSource, TagShow};
use crate::roles::role_color;
use crate::state::AppState;
use crate::theme::{fonts, motion, tokens};

/// Radius of a reading-order badge.
const BADGE_RADIUS: f32 = 9.0;

/// Space between a box's edge and its badge.
const BADGE_GAP: f32 = 2.0;

/// Side of the square a badge answers to: more than it paints, so it is easy to hit.
const BADGE_HIT: f32 = 24.0;

/// Width of the reading-order path.
const PATH_WIDTH: f32 = 2.0;

/// Dash and gap of the segment into a detour.
const DETOUR_DASH: (f32, f32) = (5.0, 4.0);

/// Name of the chip of an artifact sequence.
pub const ARTIFACT_CHIP: &str = "Art";

/// How many times a chip is moved down to clear the chips already there.
const MAX_NUDGES: usize = 8;

/// The accessible name of the badge of `step`, e.g. `Reading order step 4, goes back up`.
pub fn badge_label(step: &OrderStep) -> String {
    if step.backtrack {
        format!("Reading order step {}, goes back up", step.index)
    } else {
        format!("Reading order step {}", step.index)
    }
}

/// The note on the segment into a backtrack: `↰ goes back up: Caption read before Figure`.
pub fn backtrack_note(previous: &OrderStep, step: &OrderStep) -> String {
    format!("↰ goes back up: {} read before {}", previous.role, step.role)
}

/// The text of the chip of a tag box: the standard type it resolves to (`H1` for a `Heading1`
/// the role map sends there), else the type as written.
pub fn chip_text(tag: &TagBox) -> &str {
    if tag.standard.is_empty() {
        &tag.role
    } else {
        &tag.standard
    }
}

/// True when the options show the box of `tag`: containers only under *Nesting*, the PDF 2.0
/// `Artifact` element only under *Artifacts*.
fn shows(show: TagShow, tag: &TagBox) -> bool {
    match tag.class {
        RoleClass::Group => show.nesting,
        RoleClass::Artifact => show.artifacts,
        _ => true,
    }
}

/// An artifact sequence of the page: its number in the Artifacts tree and where it paints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ArtifactBox {
    seq: u32,
    rect: pdfxray_core::Rect,
}

/// What the tag overlay draws on one page.
#[derive(Debug, Default)]
pub(super) struct TagLayer {
    /// The boxes to outline; empty while *All tag boxes* is off.
    boxes: Arc<[TagBox]>,
    artifacts: Vec<ArtifactBox>,
    /// The order to number; `None` while *Reading order* is off.
    order: Option<Arc<ReadingOrder>>,
    show: TagShow,
    /// Where the order comes from, and for speech the reader: the path draws again when it changes.
    source: (OrderSource, Option<SpeechPreset>),
    /// The role colours, in the order of [`RoleClass::ALL`].
    colors: Vec<Color32>,
}

/// Collects what the overlay needs for `page`: nothing while its options are off.
pub(super) fn collect(state: &mut AppState, page: PageIx) -> TagLayer {
    let show = state.canvas.view.show;
    let from = state.canvas.view.order_from;
    let source = (
        from,
        (from == OrderSource::Speech).then_some(state.drawer.speech.preset),
    );
    if !show.all_boxes && !show.reading_order {
        return TagLayer::default();
    }
    let boxes = if show.all_boxes {
        state.tag_boxes(page)
    } else {
        Arc::from([])
    };
    let artifacts = if show.all_boxes && show.artifacts {
        artifact_boxes(state, page)
    } else {
        Vec::new()
    };
    let order = if show.reading_order {
        state.shown_order(page, from)
    } else {
        None
    };
    let colors = RoleClass::ALL.map(role_color).to_vec();
    TagLayer {
        boxes,
        artifacts,
        order,
        show,
        source,
        colors,
    }
}

/// The artifact sequences of `page` that paint something, numbered as the Artifacts tree does.
fn artifact_boxes(state: &AppState, page: PageIx) -> Vec<ArtifactBox> {
    let Some(captured) = state.document.as_ref().and_then(|l| l.doc.captured(page)) else {
        return Vec::new();
    };
    captured
        .marked
        .iter()
        .filter_map(|seq| seq.artifact.as_ref().map(|info| seq.bbox.or(info.bbox)))
        .zip(0u32..)
        .filter_map(|(rect, seq)| Some(ArtifactBox { seq, rect: rect? }))
        .collect()
}

impl TagLayer {
    fn color(&self, class: RoleClass) -> Color32 {
        self.colors
            .get(role_slot(class))
            .copied()
            .unwrap_or(tokens::TEXT_3)
    }

    fn visible(&self) -> impl Iterator<Item = &TagBox> {
        self.boxes.iter().filter(|tag| shows(self.show, tag))
    }

    /// The innermost shown element whose box holds `point`: the deepest, and of two as deep the
    /// smaller.
    pub(super) fn box_at(&self, point: Point) -> Option<&TagBox> {
        self.visible()
            .filter(|tag| tag.rect.contains(point))
            .min_by(|a, b| {
                b.depth
                    .cmp(&a.depth)
                    .then_with(|| a.rect.area().total_cmp(&b.rect.area()))
            })
    }
}

/// What the user did on the overlay this frame.
#[derive(Debug, Default)]
pub(super) struct Output {
    /// The element whose chip or badge was clicked.
    pub(super) clicked: Option<NodeId>,
}

/// Draws the overlay over the page at `geo`. `hover` is the pointer over the page, if it is.
pub(super) fn draw(
    ui: &Ui,
    geo: &PageGeometry,
    page: PageIx,
    layer: &TagLayer,
    hover: Option<Pos2>,
) -> Output {
    let mut out = Output::default();
    if layer.boxes.is_empty() && layer.artifacts.is_empty() && layer.order.is_none() {
        return out;
    }
    // Chips sit outside their boxes, so they may leave the page.
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    let mut hovered: Option<&NodeId> = None;
    let under_pointer = hover.and_then(|pos| layer.box_at(geo.screen_to_page(pos)));

    // Every outline first, then the chips: a later box must not cut through an earlier chip.
    for tag in layer.visible() {
        let rect = geo.rect_to_screen(tag.rect);
        let color = layer.color(tag.class);
        if tag.class == RoleClass::Group {
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, color.gamma_multiply(0.55)),
                StrokeKind::Outside,
            );
        } else {
            painter.rect(
                rect,
                0.0,
                color.gamma_multiply(0.06),
                Stroke::new(1.5, color),
                StrokeKind::Outside,
            );
        }
    }
    for art in &layer.artifacts {
        painter.rect_stroke(
            geo.rect_to_screen(art.rect),
            0.0,
            Stroke::new(1.0, layer.color(RoleClass::Artifact)),
            StrokeKind::Outside,
        );
    }
    let mut placed: Vec<UiRect> = Vec::new();
    for tag in layer.visible() {
        let wanted = match tag.class {
            RoleClass::Group => layer.show.nesting.then_some(Corner::TopRight),
            _ => layer.show.labels.then_some(Corner::TopLeft),
        };
        if let Some(corner) = wanted {
            let response = Chip {
                rect: geo.rect_to_screen(tag.rect),
                corner,
                text: chip_text(tag),
                fill: layer.color(tag.class),
                id: ui.make_persistent_id(("tag-chip", page, &tag.id)),
            }
            .show(ui, &painter, &mut placed);
            if response.clicked() {
                out.clicked = Some(tag.id.clone());
            }
            if response.hovered() {
                hovered = Some(&tag.id);
            }
        }
    }
    for art in &layer.artifacts {
        let response = Chip {
            rect: geo.rect_to_screen(art.rect),
            corner: Corner::TopLeft,
            text: ARTIFACT_CHIP,
            fill: layer.color(RoleClass::Artifact),
            id: ui.make_persistent_id(("art-chip", page, art.seq)),
        }
        .show(ui, &painter, &mut placed);
        if response.clicked() {
            out.clicked = Some(NodeId::Artifact { page, seq: art.seq });
        }
    }
    if let Some(order) = &layer.order {
        let (clicked, badge_hover) = draw_order(ui, &painter, geo, page, layer, order, &mut placed);
        out.clicked = clicked.or(out.clicked.take());
        hovered = badge_hover.or(hovered);
    }
    // The element under the pointer, or whose chip or badge is: outlined and tinted over the rest.
    let preview = hovered.or_else(|| under_pointer.map(|tag| &tag.id));
    if let Some(tag) = preview.and_then(|id| layer.boxes.iter().find(|t| &t.id == id)) {
        let color = layer.color(tag.class);
        painter.rect(
            geo.rect_to_screen(tag.rect),
            0.0,
            color.gamma_multiply(0.18),
            Stroke::new(2.0, color),
            StrokeKind::Outside,
        );
    }
    out
}

/// Which corner of a box its chip is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    /// Left of the box, level with its top edge.
    TopLeft,
    /// Above the box, level with its right edge.
    TopRight,
}

/// A chip of bold `text` on `fill` by the `corner` of `rect` (screen), a button named `text`.
struct Chip<'a> {
    /// The box the chip labels.
    rect: UiRect,
    /// Which corner of `rect` it sits by.
    corner: Corner,
    /// The label, also the button's accessible name.
    text: &'a str,
    /// Background colour.
    fill: Color32,
    /// Interaction id.
    id: Id,
}

impl Chip<'_> {
    /// Draws the chip clear of the chips in `placed`, records where it went in `placed` and
    /// returns its response.
    fn show(self, ui: &Ui, painter: &Painter, placed: &mut Vec<UiRect>) -> egui::Response {
        let Self {
            rect,
            corner,
            text,
            fill,
            id,
        } = self;
        let galley = painter.layout_no_wrap(text.to_owned(), fonts::bold(tokens::text::SMALL), tokens::BG);
        let size = galley.size() + vec2(8.0, 3.0);
        let wanted = match corner {
            Corner::TopLeft => UiRect::from_min_size(pos2(rect.min.x - size.x - 3.0, rect.min.y), size),
            Corner::TopRight => {
                UiRect::from_min_size(pos2(rect.max.x - size.x, rect.min.y - size.y - 1.0), size)
            }
        };
        let at = clear_of(placed, wanted);
        placed.push(at);
        painter.rect_filled(at, tokens::RADIUS_SM, fill);
        painter.rect_stroke(
            at,
            tokens::RADIUS_SM,
            Stroke::new(1.0, tokens::BG),
            StrokeKind::Outside,
        );
        painter.galley(at.min + vec2(4.0, 1.5), galley, tokens::BG);
        let response = ui
            .interact(at, id, Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, text));
        response
    }
}

/// `rect` moved down until it covers none of `placed`.
pub(super) fn clear_of(placed: &[UiRect], rect: UiRect) -> UiRect {
    let mut at = rect;
    for _ in 0..MAX_NUDGES {
        if !placed.iter().any(|other| other.intersects(at)) {
            break;
        }
        at = at.translate(vec2(0.0, rect.height() + 1.0));
    }
    at
}

/// Where the badge of a box at `rect` (screen) goes: level with the middle of the box, just
/// outside its right edge, so it does not cover the last glyph. Where that is taken by one of
/// `placed` (a chip of the box beside it, another badge) it goes above the right end of the box,
/// else below it; with none free it stays beside the box.
fn badge_spot(rect: UiRect, placed: &[UiRect]) -> Pos2 {
    let beside = pos2(rect.max.x + BADGE_RADIUS + BADGE_GAP, rect.center().y);
    let above = pos2(rect.max.x - BADGE_RADIUS, rect.min.y - BADGE_RADIUS - BADGE_GAP);
    let below = pos2(rect.max.x - BADGE_RADIUS, rect.max.y + BADGE_RADIUS + BADGE_GAP);
    [beside, above, below]
        .into_iter()
        .find(|at| {
            let disc = UiRect::from_center_size(*at, Vec2::splat(BADGE_RADIUS * 2.0));
            !placed.iter().any(|other| other.intersects(disc))
        })
        .unwrap_or(beside)
}

/// How much of each segment of the path through `points` is drawn when `progress` (0 to 1) of
/// its whole length is: 1 for a segment that is done, 0 for one not reached.
fn reveal(points: &[Pos2], progress: f32) -> Vec<f32> {
    let lengths: Vec<f32> = points.windows(2).map(|pair| pair[0].distance(pair[1])).collect();
    if progress >= 1.0 {
        // Subtracting lengths from their sum would leave the last one a hair short.
        return vec![1.0; lengths.len()];
    }
    let mut left = lengths.iter().sum::<f32>() * progress.clamp(0.0, 1.0);
    lengths
        .iter()
        .map(|&length| {
            if length <= f32::EPSILON {
                return if left >= 0.0 { 1.0 } else { 0.0 };
            }
            let done = (left / length).clamp(0.0, 1.0);
            left -= length;
            done
        })
        .collect()
}

/// Draws the path and the badges, clear of the chips in `placed` and their notes too. Returns the
/// element whose badge was clicked and the one whose badge the pointer is on.
fn draw_order<'a>(
    ui: &Ui,
    painter: &Painter,
    geo: &PageGeometry,
    page: PageIx,
    layer: &TagLayer,
    order: &'a ReadingOrder,
    placed: &mut Vec<UiRect>,
) -> (Option<NodeId>, Option<&'a NodeId>) {
    let ctx = ui.ctx();
    // The path starts over when it is shown again, and when its source changes.
    let id = Id::unique(("tag-order-path", page));
    let source_key = id.with("source");
    let last: Option<(OrderSource, Option<SpeechPreset>)> = ctx.data(|data| data.get_temp(source_key));
    if last != Some(layer.source) {
        motion::reset(ctx, id);
        ctx.data_mut(|data| data.insert_temp(source_key, layer.source));
    }
    let progress = motion::appear(ctx, id, motion::PATH);

    // The badges take their place among the chips, and the notes keep clear of both.
    let points: Vec<Pos2> = order
        .steps
        .iter()
        .map(|step| {
            let at = badge_spot(geo.rect_to_screen(step.rect), placed);
            placed.push(UiRect::from_center_size(at, Vec2::splat(BADGE_RADIUS * 2.0)));
            at
        })
        .collect();
    let done = reveal(&points, progress);
    for (n, fraction) in done.iter().enumerate() {
        let (from, to) = (points[n], points[n + 1]);
        let step = &order.steps[n + 1];
        let color = segment_color(step);
        let end = from + (to - from) * *fraction;
        if *fraction <= 0.0 {
            continue;
        }
        let stroke = Stroke::new(PATH_WIDTH, color);
        if step.kind == StepKind::Detour {
            painter.extend(Shape::dashed_line(
                &[from, end],
                stroke,
                DETOUR_DASH.0,
                DETOUR_DASH.1,
            ));
        } else {
            painter.line_segment([from, end], stroke);
        }
        if step.backtrack && *fraction >= 1.0 {
            note(
                painter,
                placed,
                (from, to),
                &backtrack_note(&order.steps[n], step),
            );
        }
    }
    let mut clicked = None;
    let mut hovered = None;
    for (n, step) in order.steps.iter().enumerate() {
        if n > 0 && done[n - 1] < 1.0 {
            break;
        }
        let response = badge(ui, painter, page, points[n], step);
        let preview = if step.preview.is_empty() {
            step.role.clone()
        } else {
            format!("{} · {}", step.role, step.preview)
        };
        let response = tooltip::text(response, preview);
        if response.clicked() {
            clicked = Some(step.id.clone());
        }
        if response.hovered() {
            hovered = Some(&step.id);
        }
    }
    (clicked, hovered)
}

/// The colour of the segment into `step` and of its badge: amber for a backtrack, else accent.
fn segment_color(step: &OrderStep) -> Color32 {
    if step.backtrack {
        tokens::WARN
    } else {
        tokens::ACCENT
    }
}

/// A numbered badge at `center`; a button named for its step.
fn badge(ui: &Ui, painter: &Painter, page: PageIx, center: Pos2, step: &OrderStep) -> egui::Response {
    let color = segment_color(step);
    painter.circle_filled(center, BADGE_RADIUS, color);
    painter.circle_stroke(center, BADGE_RADIUS, Stroke::new(1.5, tokens::BG));
    painter.text(
        center,
        Align2::CENTER_CENTER,
        step.index.to_string(),
        fonts::bold(tokens::text::SMALL),
        tokens::ON_ACCENT,
    );
    let hit = UiRect::from_center_size(center, Vec2::splat(BADGE_HIT));
    let response = ui
        .interact(
            hit,
            ui.make_persistent_id(("order-badge", page, step.index)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    let label = badge_label(step);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    response
}

/// The amber note on the backtrack `segment`, beside it, clear of the chips in `placed` and
/// inside what the painter may draw on.
fn note(painter: &Painter, placed: &mut Vec<UiRect>, segment: (Pos2, Pos2), text: &str) {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        fonts::mono_medium(tokens::mono::SMALL),
        tokens::WARN,
    );
    let size = galley.size() + vec2(14.0, 8.0);
    let at = connector::clear_beside(segment.0, segment.1, size);
    let rect = clear_of(placed, UiRect::from_center_size(at, size));
    let clip = painter.clip_rect();
    let shift = vec2(
        (clip.min.x - rect.min.x).max(0.0) + (clip.max.x - rect.max.x).min(0.0),
        (clip.min.y - rect.min.y).max(0.0) + (clip.max.y - rect.max.y).min(0.0),
    );
    let rect = rect.translate(shift);
    placed.push(rect);
    painter.rect_filled(rect, tokens::RADIUS_SM, tokens::BG.gamma_multiply(0.92));
    painter.rect_stroke(
        rect,
        tokens::RADIUS_SM,
        Stroke::new(1.0, tokens::WARN.gamma_multiply(0.6)),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(7.0, 4.0), galley, tokens::WARN);
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use pdfxray_core::Rect;
    use pretty_assertions::assert_eq;

    use super::*;

    fn tag(role: &str, standard: &str, class: RoleClass, depth: u32, rect: Rect) -> TagBox {
        TagBox {
            id: NodeId::Document,
            role: role.to_owned(),
            standard: standard.to_owned(),
            class,
            rect,
            depth,
        }
    }

    fn step(index: usize, role: &str, kind: StepKind) -> OrderStep {
        OrderStep {
            index,
            id: NodeId::Document,
            role: role.to_owned(),
            class: RoleClass::Paragraph,
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            kind,
            backtrack: kind == StepKind::Backtrack,
            preview: String::new(),
        }
    }

    #[test]
    fn a_badge_sits_beside_its_box_unless_a_chip_is_there_and_then_goes_above_or_below() {
        let line = UiRect::from_min_max(pos2(100.0, 100.0), pos2(200.0, 112.0));
        let beside = pos2(200.0 + BADGE_RADIUS + BADGE_GAP, 106.0);
        let disc = |at: Pos2| UiRect::from_center_size(at, Vec2::splat(BADGE_RADIUS * 2.0));

        assert_eq!(badge_spot(line, &[]), beside);

        // The chip of the box to the right takes the place beside the line.
        let chip = UiRect::from_min_size(pos2(205.0, 100.0), vec2(16.0, 14.0));
        let above = badge_spot(line, &[chip]);
        assert_eq!(above.y, 100.0 - BADGE_RADIUS - BADGE_GAP);
        assert!(!disc(above).intersects(chip));

        // With the room above taken as well, it goes below.
        let taken = [chip, disc(above)];
        let below = badge_spot(line, &taken);
        assert_eq!(below.y, 112.0 + BADGE_RADIUS + BADGE_GAP);

        // With none free it stays beside the box, where the badge always was.
        let all = [chip, disc(above), disc(below)];
        assert_eq!(badge_spot(line, &all), beside);
    }

    #[test]
    fn a_chip_says_the_standard_type_else_the_role() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);

        assert_eq!(chip_text(&tag("Heading1", "H1", RoleClass::Heading, 1, r)), "H1");
        assert_eq!(chip_text(&tag("Mystery", "", RoleClass::Other, 1, r)), "Mystery");
    }

    #[test]
    fn badges_and_notes_say_what_goes_wrong() {
        let caption = step(3, "Caption", StepKind::Forward);
        let figure = step(4, "Figure", StepKind::Backtrack);

        assert_eq!(badge_label(&caption), "Reading order step 3");
        assert_eq!(badge_label(&figure), "Reading order step 4, goes back up");
        assert_eq!(
            backtrack_note(&caption, &figure),
            "↰ goes back up: Caption read before Figure"
        );
    }

    #[test]
    fn containers_show_under_nesting_and_artifact_elements_under_artifacts() {
        let r = Rect::new(0.0, 0.0, 1.0, 1.0);
        let group = tag("Sect", "Sect", RoleClass::Group, 0, r);
        let artifact = tag("Artifact", "Artifact", RoleClass::Artifact, 0, r);
        let para = tag("P", "P", RoleClass::Paragraph, 1, r);
        let all = TagShow::default();
        let bare = TagShow {
            nesting: false,
            artifacts: false,
            ..all
        };

        assert_eq!(
            [&group, &artifact, &para].map(|t| shows(all, t)),
            [true, true, true]
        );
        assert_eq!(
            [&group, &artifact, &para].map(|t| shows(bare, t)),
            [false, false, true]
        );
    }

    #[test]
    fn a_point_picks_the_innermost_box_and_skips_hidden_containers() {
        let outer = tag(
            "Sect",
            "Sect",
            RoleClass::Group,
            0,
            Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        let wide = tag(
            "P",
            "P",
            RoleClass::Paragraph,
            1,
            Rect::new(10.0, 10.0, 90.0, 60.0),
        );
        let small = tag(
            "Span",
            "Span",
            RoleClass::Inline,
            1,
            Rect::new(20.0, 20.0, 40.0, 30.0),
        );
        let layer = |nesting| TagLayer {
            boxes: Arc::from(vec![outer.clone(), wide.clone(), small.clone()]),
            show: TagShow {
                nesting,
                ..TagShow::default()
            },
            ..TagLayer::default()
        };

        // As deep: the smaller box wins.
        assert_eq!(
            layer(true)
                .box_at(Point::new(25.0, 25.0))
                .map(|t| t.role.as_str()),
            Some("Span")
        );
        assert_eq!(
            layer(true)
                .box_at(Point::new(80.0, 50.0))
                .map(|t| t.role.as_str()),
            Some("P")
        );
        assert_eq!(
            layer(true).box_at(Point::new(5.0, 95.0)).map(|t| t.role.as_str()),
            Some("Sect")
        );
        // Without nesting the container is not there to click.
        assert_eq!(layer(false).box_at(Point::new(5.0, 95.0)), None);
    }

    #[test]
    fn the_path_draws_segment_after_segment() {
        let points = [pos2(0.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 30.0)];

        assert_eq!(reveal(&points, 0.0), [0.0, 0.0]);
        assert_eq!(reveal(&points, 0.25), [1.0, 0.0]);
        assert_eq!(reveal(&points, 0.625), [1.0, 0.5]);
        assert_eq!(reveal(&points, 1.0), [1.0, 1.0]);
        // Lengths that do not add up exactly still finish every segment.
        let slanted = [pos2(0.0, 0.0), pos2(3.3, 1.7), pos2(9.1, 2.9), pos2(4.7, 8.3)];
        assert_eq!(reveal(&slanted, 1.0), [1.0, 1.0, 1.0]);
        // Smoke: one point has no segments.
        assert_eq!(reveal(&points[..1], 1.0), Vec::<f32>::new());
    }

    #[test]
    fn a_chip_moves_down_clear_of_the_ones_there() {
        let at = UiRect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 10.0));

        assert_eq!(clear_of(&[], at), at);
        assert_eq!(
            clear_of(&[at], at),
            UiRect::from_min_size(pos2(0.0, 11.0), vec2(20.0, 10.0))
        );
    }

    #[test]
    fn a_note_sits_beside_its_segment_clear_of_what_is_placed_and_inside_what_may_be_drawn_on() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        // The fonts of the theme are there from the second frame.
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let clip = UiRect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let mut placed = vec![UiRect::from_center_size(pos2(450.0, 200.0), vec2(40.0, 14.0))];
        let steep = (pos2(300.0, 100.0), pos2(300.0, 300.0));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().with_clip_rect(clip);
            note(
                &painter,
                &mut placed,
                steep,
                "↰ goes back up: Caption read before Figure",
            );
            note(
                &painter,
                &mut placed,
                steep,
                "↰ goes back up: Figure read before Table",
            );
            let above = (pos2(250.0, -20.0), pos2(350.0, -20.0));
            note(
                &painter,
                &mut placed,
                above,
                "↰ goes back up: Table read before Note",
            );
        });
        output.textures_delta.clear();

        // The chip that was there, and one note each.
        assert_eq!(placed.len(), 4);
        // The first note leaves the steep line clear: the line is at x = 300.
        assert!(
            placed[1].max.x < 300.0 || placed[1].min.x > 300.0,
            "{:?}",
            placed[1]
        );
        for (n, rect) in placed.iter().enumerate() {
            for other in &placed[n + 1..] {
                assert!(!rect.intersects(*other), "{rect:?} overlaps {other:?}");
            }
        }
        // Smoke: the note asked for above the clip comes down into it.
        assert!(
            placed[1..].iter().all(|rect| clip.contains_rect(*rect)),
            "{placed:?}"
        );
    }
}
