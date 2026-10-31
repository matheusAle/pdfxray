//! Cards floating over the canvas: legends, measurements, the isolation chip and the paint-order
//! stepper.
//!
//! The cards form one column anchored at the canvas's top-right corner, [`MARGIN`] points in and
//! below the ruler (the canvas rectangle starts under it). They never sit over a page: when the
//! gap between the page and the canvas edge is narrower than the column, every card collapses to
//! a chip that expands while the pointer is on it. The X-ray legend is always a chip. A card
//! never runs below the canvas: it scrolls in the height that is left. The paint stepper is not
//! part of the column: it floats at the bottom centre, above the drawer, and what else floats
//! there keeps clear of it (`above_stepper`).

use egui::{
    Align2, Area, Color32, Context, Id, Label, Margin, Order, Rect, RichText, ScrollArea, Sense, Stroke, Ui,
    pos2, vec2,
};
use pdfxray_core::PageInfo;
use pdfxray_core::numeric::f32_from_usize;

use super::{MEASURE_HINT, overlay};
use crate::components::{Icon, StepTrack, ToolButton, tooltip};
use crate::modes::{LegendEntry, heatmap_legend, legend_entries};
use crate::state::{AppState, RailItem};
use crate::theme::{effects, fonts, tokens};

/// Name of the button that clears the isolate or hide chip.
pub const CLEAR_ISOLATION_LABEL: &str = "Clear isolation";

/// Title of the x-ray legend card.
pub const XRAY_TITLE: &str = "X-ray";

/// Title of the heat map legend card.
pub const HEATMAP_TITLE: &str = "Heat map";

/// Name of the button that steps one operator back.
pub const STEP_BACK_LABEL: &str = "Previous operator";

/// Name of the button that steps one operator forward.
pub const STEP_FORWARD_LABEL: &str = "Next operator";

/// The readout of the floating stepper, e.g. `step 11 / 80`.
pub fn mini_text(count: u32, total: u32) -> String {
    format!("step {count} / {total}")
}

/// The text of the x-ray legend chip: how many kinds of item the legend lists, e.g. `4 kinds`.
pub fn kinds_text(count: usize) -> String {
    format!("{count} kinds")
}

/// Width of the floating stepper's track.
const MINI_TRACK_W: f32 = 220.0;

/// Distance of the cards from the canvas edges.
pub const MARGIN: f32 = 14.0;

/// Width of the column of cards, margins included.
pub const COLUMN_W: f32 = 176.0;

/// Space between two cards of the column, and between the column and a page.
const GAP: f32 = 8.0;

/// Space around a card's content (see [`effects::floating_card`]).
const PADDING: f32 = 10.0;

/// The least height a card gets, however little of the canvas is left under the cards above it:
/// a title and a row or two.
const MIN_CARD_H: f32 = 72.0;

/// True when the column of full cards fits between the pages in view and the right edge of
/// `canvas`. `pages` are the screen rectangles of the pages drawn.
pub fn column_fits(canvas: Rect, pages: &[Rect]) -> bool {
    let right = pages
        .iter()
        .filter(|page| page.intersects(canvas))
        .map(|page| page.max.x)
        .fold(f32::NEG_INFINITY, f32::max);
    canvas.max.x - right >= COLUMN_W + MARGIN + GAP
}

/// What a card is called and how it shows collapsed.
#[derive(Debug, Clone)]
#[must_use]
pub(super) struct Card<'a> {
    name: &'a str,
    summary: Option<String>,
    dots: Vec<Color32>,
    chip: bool,
}

impl<'a> Card<'a> {
    /// A card named `name`; the name is also its chip's text and the key of its state.
    pub(super) const fn new(name: &'a str) -> Self {
        Self {
            name,
            summary: None,
            dots: Vec::new(),
            chip: false,
        }
    }

    /// The chip says `summary` instead of the name.
    pub(super) fn summary(mut self, summary: String) -> Self {
        self.summary = Some(summary);
        self
    }

    /// Colour dots in front of the chip's text.
    pub(super) fn dots(mut self, dots: Vec<Color32>) -> Self {
        self.dots = dots;
        self
    }

    /// The card is a chip even where there is room.
    pub(super) const fn always_chip(mut self) -> Self {
        self.chip = true;
        self
    }
}

/// The column of cards: each one is placed under the one before.
pub(super) struct Column {
    ctx: Context,
    canvas: Rect,
    right: f32,
    y: f32,
    collapsed: bool,
}

impl Column {
    fn new(ctx: &Context, canvas: Rect, collapsed: bool) -> Self {
        Self {
            ctx: ctx.clone(),
            canvas,
            right: canvas.max.x - MARGIN,
            y: canvas.min.y + MARGIN,
            collapsed,
        }
    }

    /// The height a full card may take: what is left of the canvas under the cards above it.
    fn room(&self) -> f32 {
        (self.canvas.max.y - MARGIN - self.y).max(MIN_CARD_H)
    }

    /// Places `card` and draws `content` in it, or only its chip while it is collapsed and the
    /// pointer is not on it.
    pub(super) fn card(&mut self, card: &Card<'_>, content: impl FnOnce(&mut Ui)) {
        let id = Id::unique(("canvas-card", card.name));
        let chip = card.chip || self.collapsed;
        let open: bool = self.ctx.data(|data| data.get_temp(id)).unwrap_or(false);
        let room = self.room();
        let shown = Area::new(id)
            .order(Order::Middle)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(pos2(self.right, self.y))
            .constrain_to(self.canvas)
            .show(&self.ctx, |ui| {
                if chip && !open {
                    summary_chip(ui, card);
                } else {
                    full(ui, id, room, content);
                }
            });
        let rect = shown.response.rect;
        if chip {
            let hovered = self.ctx.rect_contains_pointer(shown.response.layer_id, rect);
            self.ctx.data_mut(|data| data.insert_temp(id, hovered));
            if hovered != open {
                self.ctx.request_repaint();
            }
        }
        self.y += rect.height() + GAP;
    }
}

/// The frame of a card around `content`, at most `room` tall: what does not fit scrolls. `id` is
/// the card's, under which the height `content` takes is kept.
///
/// A scroll area answers with last frame's content size, which would leave the cards below a
/// card that grew on top of it. So `content` is laid out plainly until it is known to be too tall
/// (the frame before said so), and the scroll area only takes it over then.
fn full(ui: &mut Ui, id: Id, room: f32, content: impl FnOnce(&mut Ui)) {
    let frame = effects::floating_card();
    // The frame's margins and its hairline are inside `room`.
    let inner = room - frame.total_margin().sum().y;
    let key = id.with("natural-height");
    let natural: Option<f32> = ui.data(|data| data.get_temp(key));
    frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.set_max_width(COLUMN_W - PADDING * 2.0);
        let height = if natural.is_some_and(|height| height > inner) {
            ScrollArea::vertical()
                .max_height(inner)
                .show(ui, content)
                .content_size
                .y
        } else {
            content(ui);
            ui.min_rect().height()
        };
        ui.data_mut(|data| data.insert_temp(key, height));
    });
}

/// A floating card that is not part of the column.
fn floating(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    effects::floating_card().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        content(ui);
    });
}

/// The collapsed look of a card: its colours, its name and a chevron. It can sit over the page,
/// so it keeps the small shadow of the bars that float there.
fn summary_chip(ui: &mut Ui, card: &Card<'_>) {
    effects::floating_card()
        .shadow(effects::ON_PAGE_SHADOW)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                for color in &card.dots {
                    let (rect, _) = ui.allocate_exact_size(vec2(6.0, 6.0), Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, *color);
                }
                ui.label(
                    RichText::new(card.summary.as_deref().unwrap_or(card.name))
                        .font(fonts::medium(tokens::text::SMALL))
                        .color(tokens::TEXT_2),
                );
                Icon::ChevronDown.show(ui, 10.0, tokens::TEXT_3);
            });
        });
}

pub(super) fn title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(fonts::semibold(tokens::text::BODY))
            .color(tokens::TEXT),
    );
}

/// Draws all cards over `canvas`, the area the pages are drawn in. `pages` are the screen
/// rectangles of the pages in view.
pub(super) fn show(ui: &mut Ui, state: &mut AppState, canvas: Rect, info: &PageInfo, pages: &[Rect]) {
    let mut column = Column::new(ui.ctx(), canvas, !column_fits(canvas, pages));
    isolation_chip(&mut column, state);
    if state.nav.rail == RailItem::Validation {
        super::validation_card::show(&mut column, state);
    }
    xray_cards(&mut column, state);
    super::role_legend::show(&mut column, state);
    if state.canvas.view.heatmap {
        column.card(&Card::new(HEATMAP_TITLE), |ui| {
            title(ui, HEATMAP_TITLE);
            legend(ui, &heatmap_legend());
        });
    }
    if state.canvas.view.stepper {
        column.card(
            &Card::new(super::paint_order::LEGEND_TITLE),
            super::paint_order::legend,
        );
    }
    if state.canvas.show_boxes {
        let lengths = state.lengths();
        column.card(&Card::new(overlay::LEGEND_TITLE), |ui| {
            overlay::legend_ui(ui, info, &state.canvas.hidden_boxes, &lengths);
        });
    }
    measure_card(&mut column, state);
    let stepper_rect = if state.canvas.view.stepper {
        let shown = Area::new(Id::unique("canvas-stepper"))
            .order(Order::Middle)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(canvas.center().x, canvas.max.y - MARGIN))
            .show(ui.ctx(), |ui| stepper(ui, state));
        Some(shown.response.rect)
    } else {
        None
    };
    ui.data_mut(|data| data.insert_temp(stepper_key(), stepper_rect));
}

/// Where the rectangle the stepper was drawn in is kept.
fn stepper_key() -> Id {
    Id::unique("canvas-stepper-rect")
}

/// `clip` without the strip the floating stepper takes at its bottom and the space above it, so
/// that what floats at the selection (the action bar) does not sit on the stepper. The stepper's
/// place is the one it was drawn at the frame before; `clip` as it is while the stepper is off.
pub(super) fn above_stepper(ctx: &Context, clip: Rect) -> Rect {
    let stepper: Option<Option<Rect>> = ctx.data(|data| data.get_temp(stepper_key()));
    stepper.flatten().map_or(clip, |stepper| {
        Rect::from_min_max(clip.min, pos2(clip.max.x, clip.max.y.min(stepper.min.y - GAP)))
    })
}

fn isolation_chip(column: &mut Column, state: &mut AppState) {
    let Some(isolation) = state
        .canvas
        .view
        .isolation
        .as_ref()
        .filter(|i| i.page == state.canvas.page)
    else {
        return;
    };
    let text = format!("{}: {}", isolation.mode.title(), isolation.title);
    let mut clear = false;
    column.card(&Card::new(isolation.mode.title()), |ui| {
        ui.horizontal(|ui| {
            let close = ToolButton::new(CLEAR_ISOLATION_LABEL)
                .icon(Icon::Close)
                .icon_only(true);
            // The button keeps its place; a long title is cut with a `…` to what is left.
            let room = (ui.available_width() - close.width(ui) - ui.spacing().item_spacing.x).max(0.0);
            ui.scope(|ui| {
                ui.set_max_width(room);
                ui.add(
                    Label::new(
                        RichText::new(&text)
                            .font(fonts::medium(tokens::text::BODY))
                            .color(tokens::ACCENT),
                    )
                    .truncate(),
                )
                .on_hover_text(&text);
            });
            let close = close.show(ui);
            clear = close.clicked();
            tooltip::text(close, CLEAR_ISOLATION_LABEL);
        });
    });
    if clear {
        state.clear_isolation();
    }
}

fn measure_card(column: &mut Column, state: &AppState) {
    if !state.canvas.measure.active {
        return;
    }
    let lengths = state.lengths();
    column.card(&Card::new("Measure"), |ui| {
        title(ui, "Measure");
        match state.canvas.measure.offset() {
            Some(offset) => {
                for (name, value) in offset.rows(&lengths) {
                    ui.label(
                        RichText::new(format!("{name}: {value}"))
                            .font(fonts::mono(tokens::mono::BODY))
                            .color(tokens::TEXT_2),
                    );
                }
            }
            None => {
                ui.label(RichText::new(MEASURE_HINT).color(tokens::TEXT_3));
            }
        }
    });
}

/// Title of the content card of the Content entry.
pub const CONTENT_TITLE: &str = "Content";

/// What the page paints, for the counts in the legends.
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    tagged: u32,
    artifact: u32,
    untagged: u32,
    text: u32,
    image: u32,
    path: u32,
    shading: u32,
    marked: u32,
}

impl Counts {
    fn of(state: &mut AppState) -> Self {
        let page = state.canvas.page;
        state.page_stats(page).map_or_else(Self::default, |s| Self {
            tagged: s.coverage.tagged,
            artifact: s.coverage.artifact,
            untagged: s.coverage.untagged,
            text: s.text,
            image: s.image,
            path: s.path,
            shading: s.shading,
            marked: s.marked,
        })
    }

    /// The count a legend entry of the x-ray colourings stands for.
    fn of_entry(&self, label: &str) -> Option<u32> {
        match label {
            "Tagged" => Some(self.tagged),
            "Artifact" => Some(self.artifact),
            "Untagged" | "No tag" => Some(self.untagged),
            "Glyph" => Some(self.text),
            "Path" => Some(self.path),
            "Image" => Some(self.image),
            "Shading" => Some(self.shading),
            _ => None,
        }
    }
}

/// The x-ray's card: on the Content entry the content legend, elsewhere the colour legend as a
/// chip (`4 kinds`) that opens while the pointer is on it. The colouring and the other options
/// are in the View menu.
fn xray_cards(column: &mut Column, state: &mut AppState) {
    if !state.canvas.view.xray {
        return;
    }
    let counts = Counts::of(state);
    if state.nav.rail == RailItem::Content {
        column.card(&Card::new(CONTENT_TITLE), |ui| content_legend(ui, &counts));
        return;
    }
    let view = &state.canvas.view;
    let entries = legend_entries(view.color_by, view.invisible);
    // The Tag roles card is the legend of the role colouring.
    if entries.is_empty() {
        return;
    }
    let dots = entries
        .iter()
        .filter_map(|entry| entry.colors.first())
        .take(4)
        .map(|[r, g, b, a]| Color32::from_rgba_unmultiplied(*r, *g, *b, *a))
        .collect();
    let card = Card::new(XRAY_TITLE)
        .summary(kinds_text(entries.len()))
        .dots(dots)
        .always_chip();
    column.card(&card, |ui| {
        title(ui, XRAY_TITLE);
        counted_legend(ui, &entries, &counts);
    });
}

/// The legend card of the Content entry: what the page paints and how it is marked, as
/// counts, in the colours the tree and the canvas use for each.
fn content_legend(ui: &mut Ui, counts: &Counts) {
    title(ui, CONTENT_TITLE);
    egui::Grid::new("content-legend")
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for (name, count, color) in [
                ("Text", counts.text, tokens::KIND_CONTENT),
                ("Image", counts.image, tokens::KIND_CONTENT),
                ("Artifact", counts.artifact, tokens::KIND_ARTIFACT),
                ("Marked", counts.marked, tokens::KIND_TAG),
                ("Untagged", counts.untagged, tokens::ERR),
            ] {
                let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                ui.painter().rect_stroke(
                    rect.shrink(1.0),
                    2.0,
                    Stroke::new(1.5, color),
                    egui::StrokeKind::Inside,
                );
                ui.label(name);
                count_cell(ui, count);
                ui.end_row();
            }
        });
}

fn count_cell(ui: &mut Ui, count: u32) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(
            RichText::new(count.to_string())
                .font(fonts::mono(tokens::mono::SMALL))
                .color(tokens::TEXT_3),
        );
    });
}

fn legend(ui: &mut Ui, entries: &[LegendEntry]) {
    for entry in entries {
        ui.horizontal(|ui| {
            swatch(ui, entry);
            ui.label(entry.label);
        });
    }
}

/// A legend with the number of painted items of each entry that has one.
fn counted_legend(ui: &mut Ui, entries: &[LegendEntry], counts: &Counts) {
    egui::Grid::new("xray-legend")
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for entry in entries {
                swatch(ui, entry);
                ui.label(entry.label);
                match counts.of_entry(entry.label) {
                    Some(count) => count_cell(ui, count),
                    None => {
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
}

fn swatch(ui: &mut Ui, entry: &LegendEntry) {
    const HEIGHT: f32 = 12.0;
    // A lone colour is a square; a palette is a strip of narrower cells.
    let cell = if entry.colors.len() > 1 { 8.0 } else { HEIGHT };
    // At most a handful of colours per entry.
    let count = f32_from_usize(entry.colors.len());
    let (rect, _) = ui.allocate_exact_size(vec2(cell * count, HEIGHT), Sense::hover());
    let mut min = rect.min;
    for [r, g, b, a] in entry.colors.iter().copied() {
        ui.painter().rect_filled(
            Rect::from_min_size(min, vec2(cell, HEIGHT)),
            2.0,
            Color32::from_rgba_unmultiplied(r, g, b, a),
        );
        min.x += cell;
    }
}

/// The floating paint-order stepper: previous, a track to scrub, `step 11 / 80`, next. It
/// follows the same position as the Paint order tab of the drawer.
fn stepper(ui: &mut Ui, state: &mut AppState) {
    let Some(last) = state.last_op(state.canvas.page) else {
        return;
    };
    let step = state.canvas.view.step.min(last);
    let (can_back, can_forward) = (
        state.next_target(false).is_some(),
        state.next_target(true).is_some(),
    );
    let mut target = None;
    let mut next = None;
    floating(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_enabled_ui(can_back, |ui| {
                let back = ToolButton::new(STEP_BACK_LABEL)
                    .icon(Icon::StepBack)
                    .icon_only(true)
                    .show(ui);
                if back.clicked() {
                    next = Some(false);
                }
                tooltip::with_shortcut(back, "Previous operator", "←");
            });
            ui.allocate_ui(vec2(MINI_TRACK_W, 20.0), |ui| {
                target = StepTrack::new(step, last).height(20.0).show(ui);
            });
            ui.label(
                RichText::new(mini_text(step + 1, state.op_total()))
                    .font(fonts::mono(tokens::mono::BODY))
                    .color(tokens::TEXT_2),
            );
            ui.add_enabled_ui(can_forward, |ui| {
                let forward = ToolButton::new(STEP_FORWARD_LABEL)
                    .icon(Icon::StepForward)
                    .icon_only(true)
                    .show(ui);
                if forward.clicked() {
                    next = Some(true);
                }
                tooltip::with_shortcut(forward, "Next operator", "→");
            });
        });
    });
    if let Some(ix) = target {
        state.step_to(ix);
    } else if let Some(forward) = next {
        state.step_next(forward);
    }
}
