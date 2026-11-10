//! The Paint order tab: the stepper over the paint order of the whole document. Buttons (page
//! start, previous, play, next, page end), the position `211 / 1 120 ops` with the page and
//! operator it is at (`page 3 · op #42`), the "Unpainted boxes", "Follow step" and "Skip
//! state-only ops" switches, a scrubbable track over every page's operators with marks and a
//! hairline where each page starts, and a row of cards for the operators around the current one
//! on its page: four before, the current one, two after.
//!
//! *Page start* goes to the first operator of the page, and from there to the first of the page
//! before; *Page end* likewise to the last operator, then the last of the page after (Home and
//! End do the same).
//!
//! Clicking a card or the track moves the stepper there; the canvas, the tree, the breadcrumb
//! and the inspector follow because the stepper selects the operator it stops at.

use egui::text::LayoutJob;
use egui::{
    Align, Color32, CursorIcon, Layout, Rect, RichText, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    pos2, vec2,
};
use pdfxray_core::capture::{CapturedPage, OpRecord};
use pdfxray_core::diff::Side;
use pdfxray_core::explain;
use pdfxray_core::numeric::{f32_from_i64, f32_from_u32, usize_from_u32};
use pdfxray_core::{OpPosition, PageIx};

use super::operators::text_of;
use crate::components::segmented::{Segment, segmented_mini, segmented_mini_width};
use crate::components::step_track::{PageMark, StepTrack, Tick, TickKind};
use crate::components::{Icon, ToolButton, focus, switch_named, switch_width, tooltip};
use crate::state::AppState;
use crate::theme::{fonts, tokens};

/// Name of the button that goes to the start of the page.
pub const PAGE_START_LABEL: &str = "Page start";

/// Name of the button that steps one operator back.
pub const PREVIOUS_LABEL: &str = "Previous operator";

/// Name of the play button while paused.
pub const PLAY_LABEL: &str = "Play";

/// Name of the play button while playing.
pub const PAUSE_LABEL: &str = "Pause";

/// Name of the button that steps one operator forward.
pub const NEXT_LABEL: &str = "Next operator";

/// Name of the button that goes to the end of the page.
pub const PAGE_END_LABEL: &str = "Page end";

/// The tooltip of a page-edge button: its name, and where pressing it at the edge goes, e.g.
/// `Page start (previous page)`.
pub fn page_edge_hint(forward: bool, crosses: bool) -> String {
    match (forward, crosses) {
        (false, false) => PAGE_START_LABEL.to_owned(),
        (false, true) => format!("{PAGE_START_LABEL} (previous page)"),
        (true, false) => PAGE_END_LABEL.to_owned(),
        (true, true) => format!("{PAGE_END_LABEL} (next page)"),
    }
}

/// Name of the switch that makes stepping skip operators that paint nothing.
pub const SKIP_LABEL: &str = "Skip state-only ops";

/// Name of the switch that makes the canvas scroll to what each step paints.
pub const FOLLOW_LABEL: &str = "Follow step";

/// What [`FOLLOW_LABEL`] does, in its tooltip.
pub const FOLLOW_HINT: &str = "Scroll the canvas to what each step paints when it is out of view";

/// Name of the switch that outlines what is not painted yet.
pub const UNPAINTED_LABEL: &str = "Unpainted boxes";

/// What [`UNPAINTED_LABEL`] does, in its tooltip.
pub const UNPAINTED_HINT: &str = "Outline what the later operators of the page will paint";

/// What the A | B switch of a comparison does, in its tooltip.
pub const SIDE_HINT: &str = "The side the stepper walks: its page is cut at the step, the other drawn whole";

/// Shown when no page has captured operators.
pub const NO_OPERATORS: &str = "This document has no operators.";

/// Operators shown before the current one.
pub const CARDS_BEFORE: u32 = 4;

/// Operators shown after the current one.
pub const CARDS_AFTER: u32 = 2;

/// Most operators the track marks; documents with more are left unmarked to keep a frame cheap.
const MAX_MARKED: u32 = 20_000;

const CARD_H: f32 = 58.0;
const GAP: f32 = 6.0;
const PLAY_SIZE: f32 = 32.0;

/// The name of the card of operator `ix`, e.g. `op #211 Do`.
pub fn card_label(ix: u32, operator: &str) -> String {
    format!("op #{ix} {operator}")
}

/// The position text of the stepper, e.g. `211 / 1120 ops`: how many operators of the whole
/// document have run, of how many.
pub fn position_text(count: u32, total: u32) -> String {
    format!("{count} / {total} ops")
}

/// Where the stepper is, after its position, e.g. `page 3 · op #42` (the page one-based).
pub fn at_text(at: OpPosition) -> String {
    format!("page {} · op #{}", u64::from(at.page) + 1, at.ix)
}

/// An operator in a few words: its index, the operator and what it works on, e.g.
/// `#355 Tj “DRAFT”` or `#211 Do /Im1`.
pub(crate) fn summary(captured: &CapturedPage, op: &OpRecord) -> String {
    let detail = text_of(captured, op).map_or_else(|| op.operands.join(" "), |text| format!("“{text}”"));
    let mut line = format!("#{} {} {detail}", op.ix, op.operator);
    if line.chars().count() > 28 {
        line = line.chars().take(27).collect::<String>() + "…";
    }
    line.trim_end().to_owned()
}

/// What the user did in the tab this frame.
enum Act {
    /// An operator of the stepper's page (a card).
    To(u32),
    /// A paint-order position of the whole document (the track).
    Index(u32),
    /// The start (`false`) or end of the page.
    PageEdge(bool),
    Next(bool),
    Play,
    Skip(bool),
    Follow(bool),
    Unpainted(bool),
    /// A comparison's side to step.
    Side(Side),
}

/// One operator card.
struct Card {
    ix: u32,
    operator: String,
    note: String,
}

pub(super) fn ui(ui: &mut Ui, state: &mut AppState) {
    let (Some(index), Some(last)) = (
        state.step_index().filter(|_| state.canvas.view.stepper),
        state.last_step_index(),
    ) else {
        ui.label(RichText::new(NO_OPERATORS).color(tokens::TEXT_3));
        return;
    };
    let at = state.canvas.view.step;
    let ticks = ticks(state, last);
    let pages = page_marks(state);
    let cards = cards(state, at);
    let mut act = None;
    if !controls(ui, state, &mut act) {
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), tokens::CONTROL_H),
            Layout::right_to_left(Align::Center),
            |ui| switches(ui, state, Labels::Short, &mut act),
        );
    }
    ui.add_space(2.0);
    if let Some(picked) = StepTrack::new(index, last).ticks(&ticks).pages(&pages).show(ui) {
        act = Some(Act::Index(picked));
    }
    ui.add_space(4.0);
    card_row(ui, &cards, at.ix, &mut act);
    match act {
        Some(Act::To(ix)) => state.step_to(OpPosition::new(at.page, ix)),
        Some(Act::Index(picked)) => state.step_to_index(picked),
        Some(Act::PageEdge(forward)) => {
            state.step_to_page_edge(forward);
        }
        Some(Act::Next(forward)) => {
            state.step_next(forward);
        }
        Some(Act::Play) => state.toggle_play(),
        Some(Act::Skip(on)) => state.canvas.view.skip_state_only = on,
        Some(Act::Follow(on)) => state.canvas.view.follow_step = on,
        Some(Act::Unpainted(on)) => state.canvas.view.unpainted = on,
        Some(Act::Side(side)) => state.set_stepped_side(side),
        None => {}
    }
}

/// The number of pages of the document the stepper walks.
fn stepped_pages(state: &AppState) -> PageIx {
    let count = state.stepped_doc().map_or(0, pdfxray_core::Document::page_count);
    PageIx::try_from(count).unwrap_or(PageIx::MAX)
}

/// The marks of the track, at paint-order positions of the whole document: a dim tick per
/// operator that paints nothing and a red one per operator that paints untagged content. The
/// red ones are the open document's checks, so a comparison's stepper on B has none.
fn ticks(state: &mut AppState, last: u32) -> Vec<Tick> {
    if last >= MAX_MARKED {
        return Vec::new();
    }
    let flagged = state.stepped_side() == Side::A;
    let mut ticks = Vec::new();
    for page in 0..stepped_pages(state) {
        let Some(doc) = state.stepped_doc() else {
            break;
        };
        let (Some(start), Some(page_last)) = (
            doc.paint_order_index(OpPosition::new(page, 0)),
            state.last_op(page),
        ) else {
            continue;
        };
        ticks.extend(
            (0..=page_last)
                .filter(|ix| doc.is_state_only(page, *ix))
                .map(|ix| Tick {
                    ix: start + ix,
                    kind: TickKind::StateOnly,
                }),
        );
        if !flagged {
            continue;
        }
        if let Some(stats) = state.page_stats(page) {
            ticks.extend(stats.coverage.untagged_ops.iter().map(|&ix| Tick {
                ix: start + ix,
                kind: TickKind::Flagged,
            }));
        }
    }
    ticks
}

/// Where each page starts on the track, for a document of more than one page that has
/// operators.
fn page_marks(state: &AppState) -> Vec<PageMark> {
    let Some(doc) = state.stepped_doc() else {
        return Vec::new();
    };
    let pages = stepped_pages(state);
    if pages < 2 {
        return Vec::new();
    }
    (0..pages)
        .filter_map(|page| {
            let ix = doc.paint_order_index(OpPosition::new(page, 0))?;
            Some(PageMark { ix, page })
        })
        .collect()
}

/// The cards around `at`, all on its page.
fn cards(state: &AppState, at: OpPosition) -> Vec<Card> {
    let Some(captured) = state.stepped_doc().and_then(|doc| doc.captured(at.page)) else {
        return Vec::new();
    };
    let last = captured.ops.last().map_or(0, |op| op.ix);
    (at.ix.saturating_sub(CARDS_BEFORE)..=(at.ix + CARDS_AFTER).min(last))
        .filter_map(|ix| {
            let op = captured.ops.get(usize_from_u32(ix)).filter(|op| op.ix == ix)?;
            let text = text_of(captured, op);
            Some(Card {
                ix,
                operator: op.operator.clone(),
                note: explain::operator_with_text(&op.operator, &op.operands, text.as_deref()),
            })
        })
        .collect()
}

/// The buttons and the position, then the switches when they fit; returns whether they did.
fn controls(ui: &mut Ui, state: &AppState, act: &mut Option<Act>) -> bool {
    let view = &state.canvas.view;
    let can_back = state.next_target(false).is_some();
    let can_forward = state.next_target(true).is_some();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_enabled_ui(state.page_start_target().is_some(), |ui| {
            let start = ToolButton::new(PAGE_START_LABEL)
                .icon(Icon::StepBack)
                .icon_only(true)
                .show(ui);
            if start.clicked() {
                *act = Some(Act::PageEdge(false));
            }
            let hint = page_edge_hint(false, state.page_edge_crosses(false));
            tooltip::with_shortcut(start, &hint, "Home");
        });
        ui.add_enabled_ui(can_back, |ui| {
            let back = ToolButton::new(PREVIOUS_LABEL)
                .icon(Icon::ChevronLeft)
                .icon_only(true)
                .show(ui);
            if back.clicked() {
                *act = Some(Act::Next(false));
            }
            tooltip::with_shortcut(back, "Previous operator", "←");
        });
        if play_button(ui, view.playing).clicked() {
            *act = Some(Act::Play);
        }
        ui.add_enabled_ui(can_forward, |ui| {
            let forward = ToolButton::new(NEXT_LABEL)
                .icon(Icon::ChevronRight)
                .icon_only(true)
                .show(ui);
            if forward.clicked() {
                *act = Some(Act::Next(true));
            }
            tooltip::with_shortcut(forward, "Next operator", "→");
        });
        ui.add_enabled_ui(state.page_end_target().is_some(), |ui| {
            let end = ToolButton::new(PAGE_END_LABEL)
                .icon(Icon::StepForward)
                .icon_only(true)
                .show(ui);
            if end.clicked() {
                *act = Some(Act::PageEdge(true));
            }
            let hint = page_edge_hint(true, state.page_edge_crosses(true));
            tooltip::with_shortcut(end, &hint, "End");
        });
        ui.add_space(10.0);
        let count = state.step_index().map_or(0, |index| index + 1);
        let from = ui.cursor().min.x;
        position(ui, state.op_total(), count, view.step);
        // Keep the widest position's room, so the switches do not move as the stepper does.
        let used = ui.cursor().min.x - from;
        ui.add_space((position_room(ui, state.op_total(), stepped_pages(state)) - used).max(0.0));
        // The switches go at the end of the row, their labels shortened when the full ones do
        // not fit after the position; when neither fits they go on a row of their own: they
        // never run over the position.
        let room = ui.available_width() - 12.0;
        let labels = [Labels::Full, Labels::Short]
            .into_iter()
            .find(|labels| switches_width(ui, state, *labels) <= room);
        if let Some(labels) = labels {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                switches(ui, state, labels, act);
            });
        }
        labels.is_some()
    })
    .inner
}

/// Which labels the switches show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Labels {
    /// Their names.
    Full,
    /// [`SHORT_LABELS`]; the names stay their accessible names and tooltips.
    Short,
}

/// The switches' names, in the order they are drawn right to left: skip, follow, unpainted.
const NAMES: [&str; 3] = [SKIP_LABEL, FOLLOW_LABEL, UNPAINTED_LABEL];

/// What the switches show where their names do not fit: skip, follow, unpainted.
pub const SHORT_LABELS: [&str; 3] = ["Skip state-only", "Follow", "Unpainted"];

/// What each switch does, in its tooltip, in the order of [`NAMES`].
const HINTS: [Option<&str>; 3] = [None, Some(FOLLOW_HINT), Some(UNPAINTED_HINT)];

/// The width the switches take with `labels`, the side of a comparison included.
fn switches_width(ui: &Ui, state: &AppState, labels: Labels) -> f32 {
    let shown = if labels == Labels::Full {
        NAMES
    } else {
        SHORT_LABELS
    };
    let switches: f32 = shown.iter().map(|label| switch_width(ui, label)).sum::<f32>() + 2.0 * 8.0;
    let side = if state.in_diff() {
        12.0 + segmented_mini_width(ui, &side_segments())
    } else {
        0.0
    };
    // The spacing between the items of the row, one per item.
    switches + side + ui.spacing().item_spacing.x * 4.0
}

/// The segments of a comparison's A | B switch.
fn side_segments() -> [Segment<'static, Side>; 2] {
    [Segment::new(Side::A, "A"), Segment::new(Side::B, "B")]
}

/// The switches, right to left, and before them the side of a comparison.
fn switches(ui: &mut Ui, state: &AppState, labels: Labels, act: &mut Option<Act>) {
    let view = &state.canvas.view;
    let mut values = [view.skip_state_only, view.follow_step, view.unpainted];
    for (n, name) in NAMES.iter().enumerate() {
        if n > 0 {
            ui.add_space(8.0);
        }
        let label = if labels == Labels::Full {
            name
        } else {
            &SHORT_LABELS[n]
        };
        let response = switch_named(ui, &mut values[n], label, name);
        let changed = response.changed();
        match (HINTS[n], labels) {
            (Some(hint), _) => {
                tooltip::text(response, hint);
            }
            (None, Labels::Short) => {
                tooltip::text(response, *name);
            }
            (None, Labels::Full) => {}
        }
        if changed {
            *act = Some(match n {
                0 => Act::Skip(values[n]),
                1 => Act::Follow(values[n]),
                _ => Act::Unpainted(values[n]),
            });
        }
    }
    if state.in_diff() {
        ui.add_space(12.0);
        let mut side = state.stepped_side();
        let toggle = ui
            .scope(|ui| segmented_mini(ui, &mut side, &side_segments()))
            .response;
        if side != state.stepped_side() {
            *act = Some(Act::Side(side));
        }
        tooltip::text(toggle, SIDE_HINT);
    }
}

/// `211 / 1120 ops` (the count large, the rest dim), then where that is: `page 3 · op #42`.
fn position(ui: &mut Ui, total: u32, count: u32, at: OpPosition) {
    let (count_job, at_text) = position_parts(total, count, at);
    ui.label(count_job);
    ui.add_space(4.0);
    ui.label(at_text);
}

/// The two parts of [`position`]: the count and where it is.
fn position_parts(total: u32, count: u32, at: OpPosition) -> (LayoutJob, RichText) {
    let mut job = LayoutJob::default();
    let big = egui::TextFormat::simple(fonts::mono_medium(tokens::mono::LARGE + 2.0), tokens::TEXT);
    let dim = egui::TextFormat::simple(fonts::mono(tokens::mono::LARGE), tokens::TEXT_3);
    job.append(&count.to_string(), 0.0, big);
    job.append(&format!(" / {total} ops"), 0.0, dim);
    let at = RichText::new(at_text(at))
        .font(fonts::mono(tokens::mono::LARGE))
        .color(tokens::TEXT_2);
    (job, at)
}

/// The widest [`position`] of a document of `total` operators over `pages` pages: the count at
/// its largest, the last page and an operator number as long as the total. The row keeps this
/// room, so it does not change its layout as the stepper moves.
fn position_room(ui: &Ui, total: u32, pages: PageIx) -> f32 {
    let widest = OpPosition::new(pages.saturating_sub(1), total);
    let (job, at) = position_parts(total, total, widest);
    let painter = ui.painter();
    let count = painter.layout_job(job).size().x;
    let at = painter
        .layout_no_wrap(
            at.text().to_owned(),
            fonts::mono(tokens::mono::LARGE),
            tokens::TEXT_2,
        )
        .size()
        .x;
    count + 4.0 + at + 2.0 * ui.spacing().item_spacing.x
}

/// The round accent button: a triangle to play, two bars to pause.
fn play_button(ui: &mut Ui, playing: bool) -> egui::Response {
    let label = if playing { PAUSE_LABEL } else { PLAY_LABEL };
    let (rect, response) = ui.allocate_exact_size(vec2(PLAY_SIZE, PLAY_SIZE), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let fill = if response.hovered() {
        tokens::ACCENT.gamma_multiply(0.22)
    } else {
        tokens::ACCENT_DIM
    };
    painter.circle(
        rect.center(),
        PLAY_SIZE / 2.0 - 1.0,
        fill,
        Stroke::new(1.5, tokens::ACCENT),
    );
    let c = rect.center();
    if playing {
        for dx in [-3.5, 3.5] {
            painter.rect_filled(
                Rect::from_center_size(pos2(c.x + dx, c.y), vec2(3.0, 11.0)),
                1.0,
                tokens::ACCENT,
            );
        }
    } else {
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(c.x - 3.5, c.y - 6.0),
                pos2(c.x - 3.5, c.y + 6.0),
                pos2(c.x + 6.5, c.y),
            ],
            tokens::ACCENT,
            Stroke::NONE,
        ));
    }
    // The button already has an accent outline: the ring stands off it.
    focus::ring(ui, &response, rect.expand(3.0), PLAY_SIZE / 2.0 + 3.0);
    tooltip::with_shortcut(response, label, "space")
}

/// The cards, one slot each: the current operator always has the fifth slot, so cards do not
/// jump when stepping; near the ends the slots outside the page stay empty.
fn card_row(ui: &mut Ui, cards: &[Card], step: u32, act: &mut Option<Act>) {
    let slots = CARDS_BEFORE + 1 + CARDS_AFTER;
    let width = ((ui.available_width() - GAP * f32_from_u32(slots - 1)) / f32_from_u32(slots)).max(40.0);
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), CARD_H), Sense::hover());
    let first = i64::from(step) - i64::from(CARDS_BEFORE);
    for card in cards {
        let slot = i64::from(card.ix) - first;
        let x = row.min.x + f32_from_i64(slot) * (width + GAP);
        let rect = Rect::from_min_size(pos2(x, row.min.y), vec2(width, CARD_H));
        if card_ui(ui, rect, card, card.ix == step, card.ix > step) {
            *act = Some(Act::To(card.ix));
        }
    }
}

fn card_ui(ui: &mut Ui, rect: Rect, card: &Card, current: bool, future: bool) -> bool {
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("paint-card", card.ix)),
        Sense::click(),
    );
    let name = card_label(card.ix, &card.operator);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, current, name.clone()));
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(&card.note);
    let dim = if future { 0.5 } else { 1.0 };
    let painter = ui.painter();
    let (fill, stroke) = if current {
        (tokens::ACCENT_DIM, Stroke::new(1.5, tokens::ACCENT))
    } else if response.hovered() {
        (tokens::SURFACE_3, Stroke::new(1.0, tokens::BORDER_STRONG))
    } else {
        (tokens::SURFACE_2, Stroke::new(1.0, tokens::BORDER))
    };
    painter.rect(
        rect,
        tokens::RADIUS_MD,
        fill.gamma_multiply(if current { 1.0 } else { dim }),
        Stroke::new(
            stroke.width,
            stroke.color.gamma_multiply(if current { 1.0 } else { dim }),
        ),
        StrokeKind::Inside,
    );
    focus::ring(ui, &response, rect, tokens::RADIUS_MD);
    let ink = |color: Color32| color.gamma_multiply(dim);
    let top = rect.min + vec2(8.0, 7.0);
    let operator_ink = if current {
        tokens::ACCENT
    } else {
        ink(tokens::TEXT)
    };
    painter.text(
        top,
        egui::Align2::LEFT_TOP,
        &card.operator,
        fonts::mono_medium(tokens::mono::LARGE),
        operator_ink,
    );
    painter.text(
        pos2(rect.max.x - 8.0, top.y + 1.0),
        egui::Align2::RIGHT_TOP,
        format!("#{}", card.ix),
        fonts::mono(tokens::mono::SMALL),
        ink(tokens::TEXT_3),
    );
    let mut job = LayoutJob::simple(
        card.note.clone(),
        fonts::ui(tokens::text::SMALL),
        ink(tokens::TEXT_3),
        rect.width() - 16.0,
    );
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    let galley = painter.layout_job(job);
    painter.galley(rect.min + vec2(8.0, 25.0), galley, ink(tokens::TEXT_3));
    response.clicked()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::test_support::install_two_overlapping_pages;

    #[test]
    fn the_marks_of_later_pages_sit_at_their_place_in_the_whole_document() {
        let mut state = AppState::default();
        install_two_overlapping_pages(&mut state);
        state.set_stepper(true);
        let last = state.last_step_index().unwrap();

        let mut marks = ticks(&mut state, last);
        marks.sort_by_key(|t| (t.ix, t.kind == TickKind::Flagged));
        let by_kind = |kind| {
            marks
                .iter()
                .filter(|t| t.kind == kind)
                .map(|t| t.ix)
                .collect::<Vec<_>>()
        };

        // Page 1 runs 4 operators, so page 2's operators 0..6 are positions 4..10.
        assert_eq!(last, 9);
        assert_eq!(by_kind(TickKind::StateOnly), [0, 1, 2, 4, 5, 7, 8]);
        assert_eq!(by_kind(TickKind::Flagged), [3, 6, 9], "every fill is untagged");
        // smoke: each mark is the operator at that place of the document
        let doc = &state.document.as_ref().unwrap().doc;
        for tick in &marks {
            let at = doc.op_at(tick.ix).unwrap();
            assert_eq!(
                doc.is_state_only(at.page, at.ix),
                tick.kind == TickKind::StateOnly,
                "{tick:?} -> {at:?}"
            );
        }
    }
}
