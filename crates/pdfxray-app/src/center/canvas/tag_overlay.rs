//! The tag overlay of the canvas: every structure element's box in its role colour, the chips
//! that name them, the artifacts, and the reading order as numbered badges joined by a path.
//!
//! What shows is set in the View menu ([`TagShow`]): *All tag boxes* (`⇧T`) outlines every
//! element on the page and *Tag labels* (`L`) puts a chip naming each one by its box, each
//! without the other; *Nesting* and *Artifacts* add the containers and the artifacts to both.
//! *Reading order* (`R`) draws the badges and the path whether or not the boxes are on. Colours
//! come from [`crate::roles`], so a role reads the same here, in the Tags tree and in the legend.
//!
//! Where every box, chip, badge and note goes is decided by [`pdfxray_render::annotate`], the
//! one text measurer and layout shared with the exported image: this module draws the shape list
//! it returns and binds interaction to the rects it was given.
//!
//! Chips and badges are buttons: a click selects the element they stand for. While the boxes are
//! on, a click inside one selects the innermost element whose box holds the point, and hovering
//! one outlines it.
//! The path draws from badge 1 to the last over [`motion::PATH`] when it is shown.
//!
//! The chip of a table header cell says which way the header heads, and hovering the header
//! echoes the cells it heads ([`super::header_scope`]).
//!
//! While the Tag roles card focuses some roles ([`crate::state::role_focus`]), the other roles
//! stay as boxes at 15 % without chips (a table's rows, headers and cells are roles apart), and the reading-order path dims to 30 % where it runs
//! through them; the badges stay. Focus is a view, not a filter: a click in a faint box still
//! picks it, and hovering it outlines it at full strength.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use egui::epaint::PathStroke;
use egui::{
    Align2, Color32, CursorIcon, FontId, Id, Painter, Pos2, Rect as UiRect, Role, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo, pos2,
};
use pdfxray_core::{NodeId, OrderStep, PageIx, Point, ReadingOrder, RoleClass, SpeechPreset, TagBox};
use pdfxray_render::annotate::{
    self, Anchor, Annotation, ArtifactBox, BADGE_RADIUS, Face, Font, Grid, Palette, TagView,
};
use pdfxray_render::{Edge, Ink, Line, Overlay, PixelPoint, PixelRect};

use super::geometry::PageGeometry;
use super::header_scope::{self, HeaderMark};
use crate::components::{focus, tooltip};
use crate::modes::{OrderSource, TagShow};
use crate::roles::{TagRole, role_color};
use crate::state::AppState;
use crate::theme::{fonts, motion};

/// Side of the square a badge answers to: more than it paints, so it is easy to hit.
const BADGE_HIT: f32 = 24.0;

/// The accessible name of the badge of `step`, e.g. `Reading order step 4, goes back up`.
pub fn badge_label(step: &OrderStep) -> String {
    if step.backtrack {
        format!("Reading order step {}, goes back up", step.index)
    } else {
        format!("Reading order step {}", step.index)
    }
}

/// The View menu's options as the layout reads them.
const fn tag_show(show: TagShow) -> annotate::TagShow {
    annotate::TagShow {
        boxes: show.all_boxes,
        labels: show.labels,
        nesting: show.nesting,
        artifacts: show.artifacts,
    }
}

/// What the tag overlay draws on one page.
#[derive(Debug, Default)]
pub(super) struct TagLayer {
    page: PageIx,
    /// The elements to outline or label; empty while *All tag boxes* and *Tag labels* are off.
    boxes: Arc<[TagBox]>,
    artifacts: Vec<ArtifactBox>,
    /// The order to number; `None` while *Reading order* is off.
    order: Option<Arc<ReadingOrder>>,
    show: TagShow,
    /// Where the order comes from, and for speech the reader: the path draws again when it changes.
    source: (OrderSource, Option<SpeechPreset>),
    /// The header cells of the page's tables, while the boxes or the labels are on.
    headers: HashMap<NodeId, HeaderMark>,
    /// The roles in focus; `None` when every role shows.
    focus: Option<BTreeSet<TagRole>>,
    /// The selection: a selected header's echo is the table overlay's to draw.
    selected: Option<NodeId>,
}

/// Collects what the overlay needs for `page`: nothing while its options are off.
pub(super) fn collect(state: &mut AppState, page: PageIx) -> TagLayer {
    let show = state.canvas.view.show;
    let from = state.canvas.view.order_from;
    let source = (
        from,
        (from == OrderSource::Speech).then_some(state.drawer.speech.preset),
    );
    let tags = show.all_boxes || show.labels;
    if !tags && !show.reading_order {
        return TagLayer::default();
    }
    let boxes = if tags {
        state.tag_boxes(page)
    } else {
        Arc::from([])
    };
    let artifacts = match state.document.as_ref() {
        Some(loaded) if tags && show.artifacts => annotate::artifact_boxes(&loaded.doc, page),
        _ => Vec::new(),
    };
    let order = if show.reading_order {
        state.shown_order(page, from)
    } else {
        None
    };
    let headers = if tags {
        header_scope::marks_on(state, page)
    } else {
        HashMap::new()
    };
    TagLayer {
        page,
        boxes,
        artifacts,
        order,
        show,
        source,
        headers,
        focus: state.shown_roles(),
        selected: state.selection.primary().cloned(),
    }
}

impl TagLayer {
    fn visible(&self) -> impl Iterator<Item = &TagBox> {
        let show = tag_show(self.show);
        self.boxes.iter().filter(move |tag| show.shows(tag))
    }

    /// True when `role` is drawn at full strength: it is focused, or nothing is.
    fn focused(&self, role: TagRole) -> bool {
        self.focus.as_ref().is_none_or(|focus| focus.contains(&role))
    }

    /// The innermost shown element whose box holds `point`: the deepest, and of two as deep the
    /// smaller. Without the boxes there is none to click.
    pub(super) fn box_at(&self, point: Point) -> Option<&TagBox> {
        if !self.show.all_boxes {
            return None;
        }
        self.visible()
            .filter(|tag| tag.rect.contains(point))
            .min_by(|a, b| {
                b.depth
                    .cmp(&a.depth)
                    .then_with(|| a.rect.area().total_cmp(&b.rect.area()))
            })
    }

    /// The overlay laid out on `grid`: the shape list the canvas draws, and an export rasterises.
    pub(super) fn annotations(&self, grid: &Grid) -> Vec<Annotation> {
        let scope_of = |id: &NodeId| self.headers.get(id).map(|mark| &mark.scope);
        let in_focus = |class: RoleClass, standard: &str| self.focused(TagRole::of(class, standard));
        let view = TagView {
            page: self.page,
            tags: &self.boxes,
            artifacts: &self.artifacts,
            order: self.order.as_deref(),
            show: tag_show(self.show),
            scope_of: &scope_of,
            in_focus: &in_focus,
        };
        annotate::layout(grid, &Palette::DESIGN, &[annotate::tag_scene(&view)], None)
    }

    fn is_empty(&self) -> bool {
        self.boxes.is_empty() && self.artifacts.is_empty() && self.order.is_none()
    }
}

/// The grid of the page at `geo` on a screen of `pixels_per_point`, with annotations allowed
/// anywhere in `clip`: page space onto screen points, one annotation unit a point.
pub(super) fn grid(geo: &PageGeometry, pixels_per_point: f32, clip: UiRect) -> Option<Grid> {
    let display = geo.display();
    let (w, h) = (f64::from(geo.bitmap.0), f64::from(geo.bitmap.1));
    let to_out = geo.to_pixel.onto(
        (f64::from(display.x) / w, f64::from(display.y) / h),
        (f64::from(geo.rect.min.x), f64::from(geo.rect.min.y)),
    )?;
    Some(Grid {
        to_out,
        unit: 1.0,
        pixel: 1.0 / pixels_per_point,
        bounds: px_rect(clip),
    })
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
    if layer.is_empty() {
        return out;
    }
    // Chips sit outside their boxes, so they may leave the page.
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    let Some(grid) = grid(geo, ui.ctx().pixels_per_point(), painter.clip_rect()) else {
        return out;
    };
    let annotations = layer.annotations(&grid);
    let mut hovered: Option<&NodeId> = None;
    let under_pointer = hover.and_then(|pos| layer.box_at(geo.screen_to_page(pos)));
    let progress = path_progress(ui.ctx(), page, layer.source);
    let done = reveal(&badge_centers(&annotations), progress);

    // The echo of a hovered header goes over the boxes and under the chips; which header is
    // hovered is known once the chips are drawn.
    let mut echo_at = None;
    for annotation in &annotations {
        let shape = &annotation.shape;
        if echo_at.is_none() && !matches!(annotation.anchor, Anchor::Box(_)) {
            echo_at = Some(painter.add(Shape::Noop));
        }
        match &annotation.anchor {
            Anchor::None | Anchor::Box(_) | Anchor::Part(_) => paint(&painter, shape),
            Anchor::Chip(node) => {
                paint(&painter, shape);
                let Some(response) = chip(ui, &painter, page, node, shape, layer) else {
                    continue;
                };
                if response.clicked() {
                    out.clicked = Some(node.clone());
                }
                // Hovering an element's chip previews its box; an artifact sequence has none.
                if response.hovered() {
                    hovered = layer.boxes.iter().find(|t| &t.id == node).map(|t| &t.id);
                }
            }
            Anchor::Segment(step) => {
                let fraction = step
                    .checked_sub(1)
                    .and_then(|n| done.get(n))
                    .copied()
                    .unwrap_or(0.0);
                paint_segment(&painter, shape, fraction);
            }
            Anchor::Badge { step, .. } => {
                if *step > 0 && done.get(step - 1).is_none_or(|d| *d < 1.0) {
                    continue;
                }
                let Some(order_step) = layer.order.as_ref().and_then(|o| o.steps.get(*step)) else {
                    continue;
                };
                paint(&painter, shape);
                let response = badge(ui, &painter, page, shape, order_step);
                if response.clicked() {
                    out.clicked = Some(order_step.id.clone());
                }
                if response.hovered() {
                    hovered = Some(&order_step.id);
                }
            }
        }
    }
    let echo_at = echo_at.unwrap_or_else(|| painter.add(Shape::Noop));
    // The element under the pointer, or whose chip or badge is: outlined and tinted over the rest,
    // at full strength even when its role is out of focus.
    let preview = hovered.or_else(|| under_pointer.map(|tag| &tag.id));
    if let Some(mark) = preview
        .filter(|id| layer.selected.as_ref() != Some(*id))
        .and_then(|id| layer.headers.get(id))
    {
        let echo = header_scope::echo_shapes(geo, &mark.echo, role_color(RoleClass::Table));
        painter.set(echo_at, Shape::Vec(echo));
    }
    if let Some(tag) = preview.and_then(|id| layer.boxes.iter().find(|t| &t.id == id)) {
        let color = role_color(tag.class);
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

/// How far the path has drawn, 0 to 1: it starts over when it is shown again, and when its
/// source changes.
fn path_progress(ctx: &egui::Context, page: PageIx, source: (OrderSource, Option<SpeechPreset>)) -> f32 {
    let id = Id::unique(("tag-order-path", page));
    let source_key = id.with("source");
    let last: Option<(OrderSource, Option<SpeechPreset>)> = ctx.data(|data| data.get_temp(source_key));
    if last != Some(source) {
        motion::reset(ctx, id);
        ctx.data_mut(|data| data.insert_temp(source_key, source));
    }
    motion::appear(ctx, id, motion::PATH)
}

/// The centres of the badges of `annotations`, in path order.
fn badge_centers(annotations: &[Annotation]) -> Vec<Pos2> {
    annotations
        .iter()
        .filter_map(|a| match (&a.anchor, &a.shape) {
            (Anchor::Badge { .. }, Overlay::Badge { center, .. }) => Some(pos(*center)),
            _ => None,
        })
        .collect()
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

/// A segment of the path drawn `fraction` of the way; its note once it is whole.
fn paint_segment(painter: &Painter, shape: &Overlay, fraction: f32) {
    if fraction <= 0.0 {
        return;
    }
    match shape {
        Overlay::Polyline { points, line } if points.len() == 2 => {
            let (from, to) = (pos(points[0]), pos(points[1]));
            painter.line_segment([from, from + (to - from) * fraction], stroke(*line));
        }
        Overlay::Dashed {
            from,
            to,
            line,
            dash,
            gap,
        } => {
            let (from, to) = (pos(*from), pos(*to));
            painter.extend(Shape::dashed_line(
                &[from, from + (to - from) * fraction],
                stroke(*line),
                *dash,
                *gap,
            ));
        }
        note if fraction >= 1.0 => paint(painter, note),
        _ => {}
    }
}

/// The interaction of the chip of `node` whose name is `shape`: a button over the whole chip,
/// named for the element (and a header's scope), with the header's hint.
fn chip(
    ui: &Ui,
    painter: &Painter,
    page: PageIx,
    node: &NodeId,
    shape: &Overlay,
    layer: &TagLayer,
) -> Option<egui::Response> {
    let Overlay::Text { rect, text, .. } = shape else {
        return None;
    };
    let at = ui_rect(*rect);
    let response = ui
        .interact(
            at,
            ui.make_persistent_id(("tag-chip", page, node)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    focus::page_ring(painter, &response, at, annotate::CHIP_RADIUS);
    let header = layer.headers.get(node);
    let name = header.map_or_else(|| text.clone(), |mark| header_scope::chip_name(text, &mark.scope));
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
    Some(match header {
        Some(mark) => tooltip::named(
            response,
            &header_scope::hint_title(text, &mark.text, &mark.scope),
            &header_scope::hint(&mark.scope),
            None,
        ),
        None => response,
    })
}

/// The interaction of the badge `shape` of `step`: a button bigger than the disc, named for its
/// step, with the element's role and text as its hint.
fn badge(ui: &Ui, painter: &Painter, page: PageIx, shape: &Overlay, step: &OrderStep) -> egui::Response {
    let center = match shape {
        Overlay::Badge { center, .. } => pos(*center),
        _ => Pos2::ZERO,
    };
    let hit = UiRect::from_center_size(center, Vec2::splat(BADGE_HIT));
    let response = ui
        .interact(
            hit,
            ui.make_persistent_id(("order-badge", page, step.index)),
            Sense::click(),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    focus::page_ring(
        painter,
        &response,
        UiRect::from_center_size(center, Vec2::splat(BADGE_RADIUS * 2.0)),
        BADGE_RADIUS,
    );
    let label = badge_label(step);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    let preview = if step.preview.is_empty() {
        step.role.clone()
    } else {
        format!("{} · {}", step.role, step.preview)
    };
    tooltip::text(response, preview)
}

/// Paints one resolved shape with egui.
pub(super) fn paint(painter: &Painter, shape: &Overlay) {
    match shape {
        Overlay::Frame {
            rect,
            radius,
            fill,
            stroke: outline,
        } => {
            let (line, kind) = outline.map_or((Stroke::NONE, StrokeKind::Outside), |line| {
                (stroke(line), stroke_kind(line.edge))
            });
            painter.rect(
                ui_rect(*rect),
                *radius,
                fill.map_or(Color32::TRANSPARENT, color),
                line,
                kind,
            );
        }
        Overlay::Text {
            rect,
            origin,
            text,
            font,
            ink,
        } => {
            // Clipped to the rect the layout gave it, so a glyph wider here than where it was
            // measured cannot leave its chip.
            let clipped = painter.with_clip_rect(painter.clip_rect().intersect(ui_rect(*rect)));
            let galley = painter.layout_no_wrap(text.clone(), font_id(*font), color(*ink));
            clipped.galley(pos(*origin), galley, color(*ink));
        }
        Overlay::Polyline { points, line } => {
            let points: Vec<Pos2> = points.iter().copied().map(pos).collect();
            if let [from, to] = points[..] {
                painter.line_segment([from, to], stroke(*line));
            } else {
                painter.line(points, PathStroke::from(stroke(*line)));
            }
        }
        Overlay::Dashed {
            from,
            to,
            line,
            dash,
            gap,
        } => painter.extend(Shape::dashed_line(
            &[pos(*from), pos(*to)],
            stroke(*line),
            *dash,
            *gap,
        )),
        Overlay::Badge {
            center,
            radius,
            fill,
            ring,
            label,
            font,
            ink,
        } => {
            let center = pos(*center);
            painter.circle_filled(center, *radius, color(*fill));
            painter.circle_stroke(center, *radius, stroke(*ring));
            painter.text(center, Align2::CENTER_CENTER, label, font_id(*font), color(*ink));
        }
    }
}

fn color(ink: Ink) -> Color32 {
    let [r, g, b, a] = ink.premultiplied();
    Color32::from_rgba_premultiplied(r, g, b, a)
}

fn stroke(line: Line) -> Stroke {
    Stroke::new(line.width, color(line.ink))
}

const fn stroke_kind(edge: Edge) -> StrokeKind {
    match edge {
        Edge::Inside => StrokeKind::Inside,
        Edge::Centre => StrokeKind::Middle,
        Edge::Outside => StrokeKind::Outside,
    }
}

fn font_id(font: Font) -> FontId {
    match font.face {
        Face::Bold => fonts::bold(font.size),
        Face::Mono => fonts::mono_medium(font.size),
        Face::MonoBold => fonts::mono_bold(font.size),
    }
}

// Screen coordinates are small; f32 is what the painter uses anyway.
#[allow(clippy::cast_possible_truncation)]
fn pos(p: PixelPoint) -> Pos2 {
    pos2(p.x as f32, p.y as f32)
}

pub(super) fn ui_rect(r: PixelRect) -> UiRect {
    UiRect::from_min_max(pos(PixelPoint::new(r.x0, r.y0)), pos(PixelPoint::new(r.x1, r.y1)))
}

pub(super) fn px_rect(r: UiRect) -> PixelRect {
    PixelRect {
        x0: f64::from(r.min.x),
        y0: f64::from(r.min.y),
        x1: f64::from(r.max.x),
        y1: f64::from(r.max.y),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use egui::vec2;
    use pdfxray_core::{Rect, StepKind};
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
    fn badges_say_what_goes_wrong() {
        let caption = step(3, "Caption", StepKind::Forward);
        let figure = step(4, "Figure", StepKind::Backtrack);

        assert_eq!(badge_label(&caption), "Reading order step 3");
        assert_eq!(badge_label(&figure), "Reading order step 4, goes back up");
        // Smoke: the note on the segment is the layout's.
        assert_eq!(
            annotate::backtrack_note(&caption, &figure),
            "↰ goes back up: Caption read before Figure"
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
        let shown = |all_boxes, nesting| TagLayer {
            boxes: Arc::from(vec![outer.clone(), wide.clone(), small.clone()]),
            show: TagShow {
                all_boxes,
                labels: true,
                nesting,
                ..TagShow::default()
            },
            ..TagLayer::default()
        };
        let layer = |nesting| shown(true, nesting);

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
        // Labels without the boxes: there is no box to click, only the chips.
        assert_eq!(shown(false, true).box_at(Point::new(25.0, 25.0)), None);
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
    fn the_grid_maps_page_space_onto_the_screen_like_the_page_geometry() {
        let geo = super::super::geometry::fixture_geometry("minimal-one-page", 0, 2.0);
        let clip = UiRect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));

        let grid = grid(&geo, 2.0, clip).unwrap();

        let page = pdfxray_core::Rect::new(20.0, 10.0, 60.0, 40.0);
        assert_eq!(ui_rect(grid.to_out.rect_to_pixel(page)), geo.rect_to_screen(page));
        assert_eq!((grid.unit, grid.pixel), (1.0, 0.5));
        // Smoke: the bounds are the clip.
        assert_eq!(ui_rect(grid.bounds), clip);
    }

    #[test]
    fn a_resolved_shape_paints_as_the_egui_shape_the_canvas_always_drew() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let frame = Overlay::Frame {
            rect: px_rect(UiRect::from_min_size(pos2(10.0, 10.0), vec2(40.0, 20.0))),
            radius: 0.0,
            fill: Some(Ink::new([0x5A, 0xA9, 0xFF, 255]).fade(0.06)),
            stroke: Some(Line::new(1.5, Ink::new([0x5A, 0xA9, 0xFF, 255])).on(Edge::Outside)),
        };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint(ui.painter(), &frame);
        });
        output.textures_delta.clear();

        let rects: Vec<(UiRect, Stroke, StrokeKind, Color32)> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(r) => Some((r.rect, r.stroke, r.stroke_kind, r.fill)),
                _ => None,
            })
            .collect();
        assert_eq!(
            rects,
            [(
                UiRect::from_min_size(pos2(10.0, 10.0), vec2(40.0, 20.0)),
                Stroke::new(1.5, role_color(RoleClass::Paragraph)),
                StrokeKind::Outside,
                role_color(RoleClass::Paragraph).gamma_multiply(0.06),
            )]
        );
        // Smoke: one shape for one frame.
        assert_eq!(output.shapes.len(), 1);
    }

    #[test]
    fn the_canvas_draws_and_the_export_rasterises_the_same_shape_list() {
        use pdfxray_core::ReadingOrderSource;
        use pdfxray_render::{RenderSpec, render_page};

        let doc = crate::test_support::fixture("reading-order");
        let plain = render_page(&doc, 0, &RenderSpec::new(1.0)).unwrap();
        let (w, h) = (plain.width, plain.height);
        // The page shown 1:1 at the screen's origin, on a 1x screen, may be annotated anywhere on
        // it: the canvas's grid is the bitmap's.
        let size = vec2(
            pdfxray_core::numeric::f32_from_u32(w),
            pdfxray_core::numeric::f32_from_u32(h),
        );
        let geo = PageGeometry {
            rect: UiRect::from_min_size(Pos2::ZERO, size),
            bitmap: (w, h),
            to_pixel: plain.page_to_pixel,
        };
        let on_screen = grid(&geo, 1.0, geo.rect).unwrap();
        let exported = Grid::bitmap(plain.page_to_pixel, w, h, 1.0);
        assert_eq!(on_screen, exported);
        let layer = TagLayer {
            page: 0,
            boxes: doc.tag_boxes(0).into(),
            order: Some(Arc::new(doc.reading_order(0, ReadingOrderSource::Structure))),
            show: TagShow {
                all_boxes: true,
                labels: true,
                reading_order: true,
                ..TagShow::default()
            },
            ..TagLayer::default()
        };
        let list = layer.annotations(&exported);

        // The canvas: every frame, every label and every badge of the list where the list says.
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let mut shapes = Vec::new();
        for time in [0.0, 10.0] {
            let input = egui::RawInput {
                time: Some(time),
                screen_rect: Some(geo.rect),
                ..egui::RawInput::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                draw(ui, &geo, 0, &layer, None);
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        let drawn: Vec<Shape> = shapes.into_iter().map(|clipped| clipped.shape).collect();
        let frames: Vec<UiRect> = drawn
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect(r) => Some(r.rect),
                _ => None,
            })
            .collect();
        let texts: Vec<(Pos2, String)> = drawn
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(t) => Some((t.pos, t.galley.text().to_owned())),
                _ => None,
            })
            .collect();
        let discs: Vec<Pos2> = drawn
            .iter()
            .filter_map(|shape| match shape {
                Shape::Circle(c) if c.fill != Color32::TRANSPARENT => Some(c.center),
                _ => None,
            })
            .collect();
        let mut badges = 0;
        for annotation in &list {
            match &annotation.shape {
                Overlay::Frame { rect, .. } => assert!(frames.contains(&ui_rect(*rect)), "{rect:?}"),
                Overlay::Text { origin, text, .. } => {
                    assert!(
                        texts.contains(&(pos(*origin), text.clone())),
                        "{text} at {origin:?}"
                    );
                }
                Overlay::Badge { center, .. } => {
                    badges += 1;
                    assert!(discs.contains(&pos(*center)), "{center:?}");
                }
                _ => {}
            }
        }

        // The export: the same list, rasterised over the page.
        let spec = RenderSpec {
            overlays: annotate::overlays(&list),
            ..RenderSpec::new(1.0)
        };
        let annotated = render_page(&doc, 0, &spec).unwrap();
        let accent = Palette::DESIGN.accent;
        for annotation in &list {
            if let Overlay::Badge {
                center, radius, fill, ..
            } = &annotation.shape
            {
                // Inside the disc, clear of its number.
                let (x, y) = (center.x, center.y - f64::from(*radius) * 0.75);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let at = ((y as usize) * w as usize + x as usize) * 4;
                assert_eq!(annotated.rgba[at..at + 4], fill.rgba, "{center:?}");
            }
        }
        assert_eq!(badges, 5);
        // Smoke: the first badge is the accent colour; an unannotated page has none of it.
        assert!(
            list.iter()
                .any(|a| matches!(&a.shape, Overlay::Badge { fill, .. } if fill.rgba == accent))
        );
        assert!(!plain.rgba.chunks(4).any(|px| px == accent));
    }

    #[test]
    fn no_label_the_canvas_draws_is_wider_than_the_rect_the_layout_measured_for_it() {
        use pdfxray_core::{Document, ReadingOrderSource};
        use pdfxray_render::{RenderSpec, render_page};

        // The layout measures without kerning, which only ever tightens: the canvas's text, set
        // with kerning, must fit the room measured for it, on a 1x and a 2x screen.
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let mut checked = 0;
        for fixture in pdfxray_fixtures::all() {
            let Ok(doc) = Document::open(pdfxray_fixtures::pdf_path(fixture.name)) else {
                continue;
            };
            let Ok(page) = render_page(&doc, 0, &RenderSpec::new(1.0)) else {
                continue;
            };
            let layer = TagLayer {
                page: 0,
                boxes: doc.tag_boxes(0).into(),
                artifacts: annotate::artifact_boxes(&doc, 0),
                order: Some(Arc::new(doc.reading_order(0, ReadingOrderSource::Structure))),
                show: TagShow {
                    all_boxes: true,
                    labels: true,
                    nesting: true,
                    artifacts: true,
                    reading_order: true,
                },
                ..TagLayer::default()
            };
            for ppp in [1.0, 2.0] {
                let mut grid = Grid::bitmap(page.page_to_pixel, page.width, page.height, 1.0);
                grid.pixel = 1.0 / ppp;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        viewports: std::iter::once((
                            egui::ViewportId::ROOT,
                            egui::ViewportInfo {
                                native_pixels_per_point: Some(ppp),
                                ..egui::ViewportInfo::default()
                            },
                        ))
                        .collect(),
                        ..egui::RawInput::default()
                    },
                    |ui| {
                        for annotation in layer.annotations(&grid) {
                            let (text, font, room) = match (&annotation.anchor, &annotation.shape) {
                                (
                                    Anchor::Chip(_),
                                    Overlay::Text {
                                        rect,
                                        origin,
                                        text,
                                        font,
                                        ..
                                    },
                                ) => {
                                    // The name starts at `origin`; the chip's pad is on both sides.
                                    let pad = origin.x - rect.x0;
                                    (text, font, rect.width() - 2.0 * pad)
                                }
                                (
                                    Anchor::Badge { .. },
                                    Overlay::Badge {
                                        radius, label, font, ..
                                    },
                                ) => (label, font, f64::from(*radius) * 2.0),
                                _ => continue,
                            };
                            let galley =
                                ui.painter()
                                    .layout_no_wrap(text.clone(), font_id(*font), Color32::WHITE);
                            assert!(
                                f64::from(galley.size().x) <= room + 1e-3,
                                "{}: `{text}` is {} wide in {room} at {ppp}x",
                                fixture.name,
                                galley.size().x
                            );
                            checked += 1;
                        }
                    },
                );
                output.textures_delta.clear();
            }
        }
        // Smoke: the fixture set has labels to check.
        assert!(checked > 50, "{checked}");
    }
}
