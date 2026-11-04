//! The canvas of a comparison: one composite bitmap of the page pair the Changes list is on,
//! side by side, overlaid or as a mask, with the diff's annotations over it.
//!
//! The bitmap is [`pdfxray_render::composite`]'s and the annotations are laid out by
//! [`pdfxray_render::composite::layout`] over the very scenes an export draws
//! ([`annotate::change_scenes`]), so the canvas and an exported PNG are one picture. Nothing here
//! decides a status, a box or a page correspondence: they come from the pairing.
//!
//! The canvas keeps both sides' transforms: a click picks on the side it landed on (side by
//! side), or on the active side (overlaid, where both sides share the pixels). Chips are buttons
//! named for their element, its status and its side.
//!
//! Pages that differ in size or `/Rotate` are never scaled silently: overlaid, a banner gives the
//! reason and the way to opt into scaling B to A.

use std::collections::HashMap;
use std::sync::Arc;

use egui::load::SizedTexture;
use egui::{
    Align, CursorIcon, Frame, Image, Layout, Margin, Panel, Pos2, Rect as UiRect, RichText, Role, ScrollArea,
    Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, vec2,
};
use pdfxray_core::diff::{Side, Status};
use pdfxray_core::numeric::f32_from_u32;
use pdfxray_core::{NodeId, PageIx, ReadingOrderSource, Rect, TagBox};
use pdfxray_render::annotate::{self, Anchor, Annotation, Grid, Palette, Scene, TagShow, TagView};
use pdfxray_render::composite::{self, Fit, Mode};
use pdfxray_render::score::VisualScore;
use pdfxray_render::{Overlay, PixelPoint, Transform, page_dimensions};

use super::tag_overlay::{paint, px_rect, ui_rect};
use super::{RENDERING_TEXT, message};
use crate::center::toolbar;
use crate::components::segmented::{Segment, segmented, segmented_mini};
use crate::components::{Icon, ToolButton, focus, tooltip};
use crate::diff_frames::{PictureKey, Placed, Refusal, Shown};
use crate::state::diff::{low_confidence, page_pair_label, pair_summary, status_word};
use crate::state::selection::hit_entries;
use crate::state::{AppState, HitStack};
use crate::theme::{fonts, tokens};
use crate::view::render_scale;

/// The modes, in the toolbar's order.
pub const MODES: [(Mode, &str); 3] = [
    (Mode::SideBySide, "Side by side"),
    (Mode::Overlay, "Overlay"),
    (Mode::Mask, "Mask"),
];

/// Name of the button that opts into scaling B onto A.
pub const SCALE_B_LABEL: &str = "Scale B to A";

/// Name of the button that takes the scaling back.
pub const UNSCALE_LABEL: &str = "Don’t scale";

/// Name of the button that ends the comparison.
pub const END_LABEL: &str = "End comparison";

/// Names of the buttons that step through the page pairs.
pub const PREVIOUS_PAGE_LABEL: &str = "Previous page pair";
/// See [`PREVIOUS_PAGE_LABEL`].
pub const NEXT_PAGE_LABEL: &str = "Next page pair";

/// Title of the banner of pages that differ in geometry.
pub const GEOMETRY_TITLE: &str = "The pages differ in size or rotation";

/// Largest side of a composite, pixels.
const MAX_SIDE: u32 = 8192;

/// Accessible name of the composite image.
pub fn canvas_label(a: Option<PageIx>, b: Option<PageIx>) -> String {
    format!("Comparison canvas, {}", page_pair_label(a, b))
}

/// The accessible name of a chip: what it says, the element's status and its side, e.g.
/// `H2, modified, B`.
pub fn chip_label(text: &str, status: Option<Status>, side: Side) -> String {
    let side = side_letter(side);
    match status {
        Some(status) => format!("{text}, {}, {side}", status_word(status)),
        None => format!("{text}, {side}"),
    }
}

const fn side_letter(side: Side) -> &'static str {
    match side {
        Side::A => "A",
        Side::B => "B",
    }
}

/// Draws the comparison's centre: its toolbar, then the composite of the page pair.
pub fn ui(ui: &mut Ui, state: &mut AppState) {
    Panel::top("diff_toolbar")
        .exact_size(toolbar::HEIGHT)
        .frame(
            Frame::new()
                .fill(tokens::TOOLBAR_BG)
                .inner_margin(Margin::symmetric(10, 6)),
        )
        .show(ui, |ui| bar(ui, state));
    let Some(diff) = state.diff.as_ref() else {
        return;
    };
    if let Some(why) = diff.pairing_error() {
        message(ui, &format!("The documents could not be paired: {why}"));
        return;
    }
    if let Some(running) = diff.pairing_for() {
        pairing_card(ui, state, running);
        return;
    }
    let Some((a, b)) = diff.pages() else {
        message(ui, "Neither document has a page to compare.");
        return;
    };
    picture(ui, state, (a, b));
}

/// The comparison's toolbar: mode, active side, page pair, zoom, the way out.
fn bar(ui: &mut Ui, state: &mut AppState) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if let Some(diff) = state.diff.as_mut() {
            mode_controls(ui, diff);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ToolButton::new(END_LABEL).compact(true).show(ui).clicked() {
                state.end_diff();
                return;
            }
            let zoom_in = ToolButton::new("Zoom in")
                .icon(Icon::ZoomIn)
                .icon_only(true)
                .show(ui);
            ui.label(
                RichText::new(format!("{:.0} %", state.canvas.zoom * 100.0))
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_2),
            );
            let zoom_out = ToolButton::new("Zoom out")
                .icon(Icon::ZoomOut)
                .icon_only(true)
                .show(ui);
            if zoom_in.clicked() {
                state.zoom_in();
            }
            if zoom_out.clicked() {
                state.zoom_out();
            }
        });
    });
}

/// Why the modes cannot compare the page pair `(a, b)`: a page only one side has. `None` when
/// both sides have a page.
pub fn nothing_to_compare(a: Option<PageIx>, b: Option<PageIx>) -> Option<String> {
    let (page, side, other) = match (a, b) {
        (Some(_), Some(_)) | (None, None) => return None,
        (Some(page), None) => (page, "A", "B"),
        (None, Some(page)) => (page, "B", "A"),
    };
    Some(format!(
        "Page {} of {side} has no page in {other} it corresponds to, so there is nothing to \
         compare it with: it is shown alone.",
        page + 1
    ))
}

/// The mode, the active side while overlaid, and the page pair with its arrows. On a page only
/// one side has, the modes are disabled and say why: side by side, overlaid and masked, one
/// page shows the same, and enabled buttons that change nothing would pretend otherwise.
fn mode_controls(ui: &mut Ui, diff: &mut crate::state::DiffSession) {
    let alone = diff.pages().and_then(|(a, b)| nothing_to_compare(a, b));
    let modes = ui.add_enabled_ui(alone.is_none(), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let segments = MODES.map(|(mode, label)| Segment::new(mode, label));
        segmented(ui, &mut diff.mode, &segments);
        if diff.mode.shares_grid() {
            let sides = [Segment::new(Side::A, "A"), Segment::new(Side::B, "B")];
            let toggle = ui
                .scope(|ui| segmented_mini(ui, &mut diff.active, &sides))
                .response;
            tooltip::text(
                toggle,
                "The side whose reading order is drawn and whose content a click picks",
            );
        }
    });
    if let Some(why) = alone {
        tooltip::text(modes.response, why);
    }
    let count = diff.pairing().map_or(0, |p| p.pages.len());
    let previous = ToolButton::new(PREVIOUS_PAGE_LABEL)
        .icon(Icon::ChevronLeft)
        .icon_only(true)
        .show(ui);
    if let Some((a, b)) = diff.pages() {
        ui.label(RichText::new(page_pair_label(a, b)).font(fonts::mono(tokens::mono::SMALL)));
    }
    let next = ToolButton::new(NEXT_PAGE_LABEL)
        .icon(Icon::ChevronRight)
        .icon_only(true)
        .show(ui);
    if previous.clicked() {
        diff.page = diff.page.saturating_sub(1);
    }
    if next.clicked() && diff.page + 1 < count {
        diff.page += 1;
    }
}

/// The card while the two documents are being paired.
fn pairing_card(ui: &mut Ui, state: &AppState, running: std::time::Duration) {
    let names = state
        .document
        .as_ref()
        .zip(state.diff.as_ref())
        .map(|(a, d)| format!("{} ↔ {}", a.info.name, d.b.info.name))
        .unwrap_or_default();
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new().size(18.0));
            ui.label(
                RichText::new(crate::navigator::changes::PAIRING_TEXT)
                    .font(fonts::semibold(tokens::text::BODY))
                    .color(tokens::TEXT),
            );
            ui.label(
                RichText::new(format!("{names} · {:.1} s", running.as_secs_f32()))
                    .font(fonts::mono(tokens::mono::SMALL))
                    .color(tokens::TEXT_3),
            );
        });
    });
}

/// The banner of pages whose geometry differs: the reason, and the explicit opt-in.
fn geometry_banner(ui: &mut Ui, state: &mut AppState, why: &str, scaled: bool) {
    let mut clicked = false;
    Frame::new()
        .fill(tokens::WARN.gamma_multiply(0.10))
        .stroke(Stroke::new(1.0, tokens::WARN.gamma_multiply(0.30)))
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                Icon::Warning.show(ui, 16.0, tokens::WARN);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(GEOMETRY_TITLE)
                            .font(fonts::semibold(tokens::text::BODY))
                            .color(tokens::TEXT),
                    );
                    ui.label(
                        RichText::new(why)
                            .font(fonts::ui(tokens::text::BODY))
                            .color(tokens::TEXT_2),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if scaled { UNSCALE_LABEL } else { SCALE_B_LABEL };
                    clicked = ToolButton::new(label).secondary().show(ui).clicked();
                });
            });
        });
    if clicked && let Some(diff) = &mut state.diff {
        diff.fit = if scaled { Fit::Refuse } else { Fit::ScaleBToA };
    }
}

/// The composite of the page pair `(a, b)` with its annotations, or why there is none.
fn picture(ui: &mut Ui, state: &mut AppState, (a, b): (Option<PageIx>, Option<PageIx>)) {
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let Some(doc_a) = state.document.as_ref().map(|l| Arc::clone(&l.doc)) else {
        return;
    };
    let Some(diff) = state.diff.as_mut() else {
        return;
    };
    let doc_b = Arc::clone(&diff.b.doc);
    let dims = match (a, b) {
        (Some(page), _) => page_dimensions(&doc_a, page),
        (None, Some(page)) => page_dimensions(&doc_b, page),
        (None, None) => return,
    }
    .map_or(Vec2::splat(612.0), |(w, h)| vec2(w, h));
    let wide = if diff.mode == Mode::SideBySide { 2.0 } else { 1.0 };
    let key = PictureKey {
        a,
        b,
        mode: diff.mode,
        scale: render_scale(state.canvas.zoom, ppp, vec2(dims.x * wide, dims.y), MAX_SIDE),
        fit: diff.fit,
    };
    let shown = diff.frames.get(&ctx, &doc_a, &doc_b, key);
    if diff.frames.is_pending() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
    let picture = match shown {
        Shown::Ready(picture) | Shown::Stale(picture) => picture,
        Shown::Drawing => return message(ui, RENDERING_TEXT),
        Shown::Refused(Refusal::Geometry(why)) => {
            geometry_banner(ui, state, &why, false);
            return message(ui, "Not overlaid: side by side shows both pages as they are.");
        }
        Shown::Refused(Refusal::Render(why)) => return message(ui, &why),
    };
    let (b_scale, visual) = match picture.placed.as_ref() {
        Placed::Both(c) => (c.b_scale, c.delta.as_ref().and_then(|d| d.visual()).copied()),
        Placed::One(..) => (None, None),
    };
    if let Some(by) = b_scale {
        geometry_banner(
            ui,
            state,
            &format!("B is drawn scaled by {by:.3} to fit A's page; its geometry is not A's."),
            true,
        );
    }
    let (scenes, sides) = scenes(state, picture.placed.as_ref(), (a, b));
    let legend = legend(state, (a, b), b_scale, visual.as_ref());
    let Some(diff) = state.diff.as_ref() else {
        return;
    };
    let mode = diff.mode;
    let size = vec2(
        f32_from_u32(picture.size.0) / ppp,
        f32_from_u32(picture.size.1) / ppp,
    );
    let transform = match picture.placed.as_ref() {
        Placed::Both(c) => c.a.to_pixel,
        Placed::One(_, t) => *t,
    };
    let grid_at = |origin: Pos2| -> Option<Grid> {
        let image = UiRect::from_min_size(origin, size);
        Some(Grid {
            to_out: transform.onto(
                (1.0 / f64::from(ppp), 1.0 / f64::from(ppp)),
                (f64::from(origin.x), f64::from(origin.y)),
            )?,
            unit: 1.0,
            pixel: 1.0 / ppp,
            bounds: px_rect(image),
        })
    };
    let band = grid_at(Pos2::ZERO).map_or(0.0, |g| annotate::legend_extent(&g, &legend).1);
    #[allow(clippy::cast_possible_truncation)]
    let total = vec2(size.x, size.y + band as f32);

    let mut clicked: Option<(Side, NodeId)> = None;
    let mut picked: Option<Pos2> = None;
    ScrollArea::both()
        .id_salt("diff-canvas")
        .auto_shrink(false)
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(total, Sense::hover());
            let image_rect = UiRect::from_min_size(rect.min, size);
            let image = Image::from_texture(SizedTexture::new(picture.texture.id(), size))
                .sense(Sense::click())
                .alt_text(canvas_label(a, b));
            let response = ui.put(image_rect, image);
            if response.clicked() {
                picked = response.interact_pointer_pos();
            }
            let Some(grid) = grid_at(image_rect.min) else {
                return;
            };
            let (shapes, _) = composite::layout(&grid, &Palette::DESIGN, &scenes, &legend);
            let painter = ui.painter().with_clip_rect(ui.clip_rect());
            outline_selection(&painter, state, picture.placed.as_ref(), &grid, (a, b));
            for shape in &shapes {
                paint(&painter, &shape.shape);
                if let Some(hit) = interact(ui, &painter, state, shape, &sides) {
                    clicked = Some(hit);
                }
            }
            if let Some(pos) = picked {
                let px = (pos - image_rect.min) * ppp;
                pick(
                    state,
                    picture.placed.as_ref(),
                    mode,
                    (a, b),
                    PixelPoint::new(f64::from(px.x), f64::from(px.y)),
                );
            }
        });
    if let Some((side, id)) = clicked {
        state.select_in(side, id);
    }
}

/// The scenes of the page pair, each with its side: the diff's boxes and chips for A and B (B's
/// carried into A's page space when both are on one composite), and the reading order of the
/// sides that show it.
fn scenes(
    state: &mut AppState,
    placed: &Placed,
    (a, b): (Option<PageIx>, Option<PageIx>),
) -> (Vec<Scene>, Vec<Side>) {
    let boxes_a: Arc<[TagBox]> = a.map_or_else(|| Arc::from([]), |page| state.tag_boxes(page));
    let order_a = match (a, state.diff.as_ref().map(|d| d.order)) {
        (Some(page), Some(true)) => state.reading_order(page, ReadingOrderSource::Structure),
        _ => None,
    };
    let Some(diff) = state.diff.as_mut() else {
        return (Vec::new(), Vec::new());
    };
    let boxes_b: Arc<[TagBox]> = b.map_or_else(|| Arc::from([]), |page| diff.b_boxes(page));
    let order_b = match b {
        Some(page) if diff.order => Some(diff.b.doc.reading_order(page, ReadingOrderSource::Structure)),
        _ => None,
    };
    let Some(pairing) = diff.pairing().cloned() else {
        return (Vec::new(), Vec::new());
    };
    let by_id = |boxes: &Arc<[TagBox]>| -> HashMap<NodeId, usize> {
        boxes.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect()
    };
    let (ids_a, ids_b) = (by_id(&boxes_a), by_id(&boxes_b));
    let tag_of = |side: Side, id: &NodeId| -> Option<&TagBox> {
        match side {
            Side::A => ids_a.get(id).map(|i| &boxes_a[*i]),
            Side::B => ids_b.get(id).map(|i| &boxes_b[*i]),
        }
    };
    let mode = diff.mode;
    let [scene_a, scene_b] = annotate::change_scenes(&pairing, (a, b), mode, diff.show, &tag_of);
    let to_a = |scene: Scene| match placed {
        Placed::Both(c) => scene.mapped(|r| c.b_in_a(r)),
        Placed::One(..) => scene,
    };
    let order_scene = |page: PageIx, order: &pdfxray_core::ReadingOrder| {
        annotate::tag_scene(&TagView {
            page,
            tags: &[],
            artifacts: &[],
            order: Some(order),
            show: TagShow::default(),
            scope_of: &|_| None,
            in_focus: &|_, _| true,
        })
    };
    let mut scenes = vec![scene_a, to_a(scene_b)];
    let mut sides = vec![Side::A, Side::B];
    let wants = |side: Side| mode == Mode::SideBySide || diff.active == side;
    if let (Some(page), Some(order)) = (a, order_a.as_deref())
        && wants(Side::A)
    {
        scenes.push(order_scene(page, order));
        sides.push(Side::A);
    }
    if let (Some(page), Some(order)) = (b, order_b.as_ref())
        && wants(Side::B)
    {
        scenes.push(to_a(order_scene(page, order)));
        sides.push(Side::B);
    }
    (scenes, sides)
}

/// The legend: what each colour means, the mask's visual score, and what the picture must not
/// be mistaken for (a degraded page, B scaled, pairs matched on weak evidence).
fn legend(
    state: &AppState,
    (a, b): (Option<PageIx>, Option<PageIx>),
    b_scale: Option<f64>,
    visual: Option<&VisualScore>,
) -> annotate::Legend {
    let Some(diff) = state.diff.as_ref() else {
        return annotate::Legend::default();
    };
    let mut warnings = Vec::new();
    if let Some(pairing) = diff.pairing() {
        for (side, info, page) in [("A", &pairing.sides.a, a), ("B", &pairing.sides.b, b)] {
            if let Some(page) = page
                && info.degraded_pages.contains(&page)
            {
                warnings.push(format!("{side} page {}: capture degraded", page + 1));
            }
        }
        let weak = pairing
            .pairs
            .iter()
            .filter(|p| (a.is_some() && p.page_a == a) || (b.is_some() && p.page_b == b))
            .filter(|p| low_confidence(p))
            .count();
        if weak > 0 {
            warnings.push(format!("{weak} pair(s) here matched with low confidence"));
        }
    }
    if let Some(by) = b_scale {
        warnings.push(format!("B scaled to A by {by:.3}"));
    }
    annotate::change_legend(diff.mode, diff.show, visual, &warnings, Palette::DESIGN.warn)
}

/// What the selection outlines: the selected pair's boxes on the sides it is on, and — when
/// nothing paired was selected — the selected nodes' own boxes on the side they belong to, so a
/// click on an operator, a glyph or an artifact is ringed here exactly as it is on the ordinary
/// canvas. An untagged comparison has no pairs at all, so without this nothing is ever ringed.
fn outline_selection(
    painter: &egui::Painter,
    state: &AppState,
    placed: &Placed,
    grid: &Grid,
    (a, b): (Option<PageIx>, Option<PageIx>),
) {
    let mut rects = Vec::new();
    if let Some(pair) = state.diff.as_ref().and_then(|d| d.selected()) {
        match placed {
            Placed::Both(c) => {
                if let Some(r) = pair.bbox_a.filter(|_| pair.page_a.is_some() && pair.page_a == a) {
                    rects.push(r);
                }
                if let Some(r) = pair.bbox_b.filter(|_| pair.page_b.is_some() && pair.page_b == b) {
                    rects.push(c.b_in_a(r));
                }
            }
            Placed::One(Side::A, _) => rects.extend(pair.bbox_a.filter(|_| pair.page_a == a)),
            Placed::One(Side::B, _) => rects.extend(pair.bbox_b.filter(|_| pair.page_b == b)),
        }
    }
    if rects.is_empty() {
        rects = selected_boxes(state, placed, (a, b));
    }
    for rect in rects {
        painter.rect_stroke(
            ui_rect(grid.to_out.rect_to_pixel(rect)),
            0.0,
            Stroke::new(2.0, tokens::ACCENT),
            StrokeKind::Outside,
        );
    }
}

/// The boxes of the selected nodes on the side they were selected on, when that side's page is
/// the one shown. The page-space box comes from that side's own document, so B's nodes are read
/// from B.
fn selected_boxes(state: &AppState, placed: &Placed, (a, b): (Option<PageIx>, Option<PageIx>)) -> Vec<Rect> {
    let side = state.selection.side();
    if let Placed::One(shown, _) = placed
        && *shown != side
    {
        return Vec::new();
    }
    let (Some(doc), Some(page)) = (state.doc_of(side), if side == Side::A { a } else { b }) else {
        return Vec::new();
    };
    state
        .selection
        .nodes()
        .iter()
        .filter_map(|id| crate::selection::selection_rect(doc, id).filter(|s| s.page == page))
        .map(|s| match (placed, side) {
            (Placed::Both(c), Side::B) => c.b_in_a(s.rect),
            _ => s.rect,
        })
        .collect()
}

/// The interaction of one shape: a chip or a badge is a button that selects its element on its
/// side. Returns what was clicked.
fn interact(
    ui: &Ui,
    painter: &egui::Painter,
    state: &mut AppState,
    shape: &Annotation,
    sides: &[Side],
) -> Option<(Side, NodeId)> {
    let side = *sides.get(shape.scene?)?;
    match (&shape.anchor, &shape.shape) {
        (Anchor::Chip(node), Overlay::Text { rect, text, .. }) => {
            let at = ui_rect(*rect);
            let response = ui
                .interact(
                    at,
                    ui.make_persistent_id(("diff-chip", side, node)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            focus::page_ring(painter, &response, at, annotate::CHIP_RADIUS);
            let diff = state.diff.as_mut()?;
            let pair = diff
                .pair_of(side, node)
                .and_then(|i| diff.pairing()?.pairs.get(i).cloned());
            let label = chip_label(text, pair.as_ref().map(|p| p.status), side);
            response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
            let response = match &pair {
                Some(pair) => tooltip::text(response, pair_summary(pair)),
                None => response,
            };
            response.clicked().then(|| (side, node.clone()))
        }
        (Anchor::Badge { step, node }, Overlay::Badge { center, .. }) => {
            #[allow(clippy::cast_possible_truncation)]
            let center = Pos2::new(center.x as f32, center.y as f32);
            let hit = UiRect::from_center_size(center, Vec2::splat(24.0));
            let response = ui
                .interact(
                    hit,
                    ui.make_persistent_id(("diff-badge", side, *step)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand);
            let label = format!("Reading order step {}, {}", step + 1, side_letter(side));
            response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
            response.clicked().then(|| (side, node.clone()))
        }
        _ => None,
    }
}

/// A click at `px` (the composite's pixels): the side it is on picks what is under it, which
/// becomes the selection and the hit stack.
fn pick(
    state: &mut AppState,
    placed: &Placed,
    mode: Mode,
    (a, b): (Option<PageIx>, Option<PageIx>),
    px: PixelPoint,
) {
    let inside = |r: &pdfxray_render::PixelRect| px.x >= r.x0 && px.x <= r.x1 && px.y >= r.y0 && px.y <= r.y1;
    let active = state.diff.as_ref().map_or(Side::B, |d| d.active);
    let (side, to_pixel): (Side, Transform) = match placed {
        Placed::One(side, t) => (*side, *t),
        Placed::Both(c) if mode.shares_grid() => match active {
            Side::A => (Side::A, c.a.to_pixel),
            Side::B => (Side::B, c.b.to_pixel),
        },
        Placed::Both(c) if inside(&c.a.bounds) => (Side::A, c.a.to_pixel),
        Placed::Both(c) if inside(&c.b.bounds) => (Side::B, c.b.to_pixel),
        Placed::Both(_) => return,
    };
    let Some(page) = (match side {
        Side::A => a,
        Side::B => b,
    }) else {
        return;
    };
    let point = to_pixel.to_page(px);
    let Some(doc) = state.doc_of(side).cloned() else {
        return;
    };
    let entries = hit_entries(&doc, page, point, state.canvas.layer_mode);
    let first = entries.first().map(|e| e.id.clone());
    state.drawer.show_hits(HitStack {
        side,
        page,
        point,
        entries,
    });
    if let Some(id) = first {
        state.select_in(side, id);
    }
}
