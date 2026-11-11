//! The scrubbable track of the paint-order stepper: a thin bar with a knob at the current
//! operator. Clicking or dragging it picks the operator under the pointer.
//!
//! Where a page starts, a hairline crosses the track, with the page number (`p3`) above it when
//! the next page mark leaves room for it.

use egui::{Align2, Color32, CursorIcon, Rect, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use pdfxray_core::PageIx;
use pdfxray_core::numeric::{f32_from_u32, u32_from_f32_round};

use super::focus;
use crate::theme::{fonts, tokens};

/// Accessible name of the track.
pub const LABEL: &str = "Paint order position";

/// Space left at each end so the knob is not clipped.
const PAD: f32 = 6.0;

/// Height of the band above the track that holds the page numbers.
const LABEL_BAND: f32 = 13.0;

/// The least room to the next page mark that a page number needs.
pub const LABEL_ROOM: f32 = 24.0;

/// What a mark on the track says about its operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickKind {
    /// The operator changes no pixels (graphics state, text state, marked content ...): a dim
    /// tick under the track.
    StateOnly,
    /// The operator paints something the checks flag, e.g. untagged content: a red mark above
    /// the track.
    Flagged,
}

/// One mark on the track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    /// Position of the operator on the track (the paint order of the whole document for the
    /// stepper).
    pub ix: u32,
    /// What it says.
    pub kind: TickKind,
}

/// Where a page starts on the track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageMark {
    /// Position of the page's first operator on the track.
    pub ix: u32,
    /// The page, zero-based.
    pub page: PageIx,
}

/// The text of the number by a page mark, one-based: `p3`.
pub fn page_text(page: PageIx) -> String {
    format!("p{}", u64::from(page) + 1)
}

/// The track of operators `0..=last` with the knob at `step`.
#[derive(Debug, Clone, Copy)]
#[must_use = "call `show`"]
pub struct StepTrack<'a> {
    step: u32,
    last: u32,
    ticks: &'a [Tick],
    pages: &'a [PageMark],
    height: f32,
    name: &'a str,
}

impl<'a> StepTrack<'a> {
    /// A track for operators up to `last`, the knob at `step`.
    pub const fn new(step: u32, last: u32) -> Self {
        Self {
            step,
            last,
            ticks: &[],
            pages: &[],
            height: 24.0,
            name: LABEL,
        }
    }

    /// The track's accessible name, when there is more than one: [`LABEL`] otherwise.
    pub const fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// Marks where each page starts, in track order; a band above the track holds their
    /// numbers.
    pub const fn pages(mut self, pages: &'a [PageMark]) -> Self {
        self.pages = pages;
        self
    }

    /// Marks operators on the track.
    pub const fn ticks(mut self, ticks: &'a [Tick]) -> Self {
        self.ticks = ticks;
        self
    }

    /// Height of the whole control, marks included.
    pub const fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// The x position of `step` inside `rect`, the control's rectangle.
    pub fn x_of(rect: Rect, step: u32, last: u32) -> f32 {
        let inner = rect.shrink2(vec2(PAD, 0.0));
        inner.min.x + inner.width() * fraction(step, last)
    }

    /// The operator under screen position `x` inside `rect`.
    pub fn step_at(rect: Rect, x: f32, last: u32) -> u32 {
        let inner = rect.shrink2(vec2(PAD, 0.0));
        let fraction = ((x - inner.min.x) / inner.width().max(1.0)).clamp(0.0, 1.0);
        // The product is between 0 and `last`.
        let step = u32_from_f32_round(fraction * f32_from_u32(last));
        step.min(last)
    }

    /// Lays the track out across the available width. Returns the operator the user picked this
    /// frame, if it is not the current one.
    pub fn show(self, ui: &mut Ui) -> Option<u32> {
        let band = if self.pages.is_empty() { 0.0 } else { LABEL_BAND };
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), self.height + band),
            Sense::click_and_drag(),
        );
        response.widget_info(|| WidgetInfo::slider(true, f64::from(self.step), self.name));
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect);
            focus::row_ring(ui, &response, rect, tokens::RADIUS_SM);
        }
        let picked = (response.clicked() || response.dragged() || response.drag_started())
            .then(|| response.interact_pointer_pos())
            .flatten()
            .map(|pos| Self::step_at(rect, pos.x, self.last))?;
        (picked != self.step).then_some(picked)
    }

    /// The page marks whose number is shown in `rect`: those with [`LABEL_ROOM`] to the next
    /// mark and clear of the number before.
    pub fn labelled(rect: Rect, pages: &[PageMark], last: u32) -> Vec<PageMark> {
        let mut shown = Vec::new();
        let mut free_from = f32::NEG_INFINITY;
        for (n, mark) in pages.iter().enumerate() {
            let x = Self::x_of(rect, mark.ix, last);
            let next = pages
                .get(n + 1)
                .map_or(rect.max.x - PAD, |next| Self::x_of(rect, next.ix, last));
            if next - x >= LABEL_ROOM && x >= free_from {
                shown.push(*mark);
                free_from = x + LABEL_ROOM;
            }
        }
        shown
    }

    fn paint(&self, ui: &Ui, rect: Rect) {
        let painter = ui.painter();
        let band = if self.pages.is_empty() { 0.0 } else { LABEL_BAND };
        let track = Rect::from_min_max(pos2(rect.min.x, rect.min.y + band), rect.max);
        let y = track.center().y;
        let bar = Rect::from_min_max(pos2(rect.min.x + PAD, y - 2.0), pos2(rect.max.x - PAD, y + 2.0));
        painter.rect_filled(bar, 2.0, tokens::SURFACE_3);
        let knob = Self::x_of(rect, self.step, self.last);
        painter.rect_filled(
            Rect::from_min_max(bar.min, pos2(knob, bar.max.y)),
            2.0,
            tokens::KIND_CONTENT,
        );
        for tick in self.ticks {
            let x = Self::x_of(rect, tick.ix, self.last);
            let (span, stroke) = match tick.kind {
                TickKind::StateOnly => (
                    [pos2(x, y + 5.0), pos2(x, y + 9.0)],
                    Stroke::new(1.0, tokens::TEXT_3.gamma_multiply(0.45)),
                ),
                TickKind::Flagged => (
                    [pos2(x, y - 9.0), pos2(x, y - 4.0)],
                    Stroke::new(2.0, tokens::ERR),
                ),
            };
            painter.line_segment(span, stroke);
        }
        for mark in self.pages {
            let x = Self::x_of(rect, mark.ix, self.last);
            painter.line_segment(
                [pos2(x, track.min.y + 2.0), pos2(x, track.max.y - 2.0)],
                Stroke::new(1.0, tokens::TRACK_PAGE_MARK),
            );
        }
        for mark in Self::labelled(rect, self.pages, self.last) {
            painter.text(
                pos2(Self::x_of(rect, mark.ix, self.last), track.min.y - 4.0),
                Align2::LEFT_BOTTOM,
                page_text(mark.page),
                fonts::mono(tokens::mono::TINY),
                tokens::TRACK_PAGE_TEXT,
            );
        }
        painter.line_segment(
            [pos2(knob, y - 8.0), pos2(knob, y + 8.0)],
            Stroke::new(1.5, tokens::ACCENT),
        );
        painter.circle_filled(pos2(knob, y), 4.5, tokens::ACCENT);
        painter.circle_stroke(
            pos2(knob, y),
            4.5,
            Stroke::new(1.5, Color32::from_black_alpha(90)),
        );
    }
}

/// How far along `step` is, 0 to 1; 0 when there is only one operator.
fn fraction(step: u32, last: u32) -> f32 {
    if last == 0 {
        0.0
    } else {
        // Operator counts are far below f32's exact integer range.
        f32_from_u32(step.min(last)) / f32_from_u32(last)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(100.0, 0.0), vec2(412.0, 24.0))
    }

    #[test]
    fn a_position_on_the_track_is_the_operator_there() {
        // The usable width is 400: 6 pt of room at each end.
        assert_eq!(StepTrack::step_at(rect(), 106.0, 80), 0);
        assert_eq!(StepTrack::step_at(rect(), 306.0, 80), 40);
        assert_eq!(StepTrack::step_at(rect(), 506.0, 80), 80);
        assert_eq!(StepTrack::step_at(rect(), 206.0, 80), 20);
        // Beyond the ends it stops at the first and last operator.
        assert_eq!(StepTrack::step_at(rect(), 0.0, 80), 0);
        assert_eq!(StepTrack::step_at(rect(), 900.0, 80), 80);
    }

    #[test]
    fn a_page_number_shows_only_with_room_to_the_next_mark() {
        // 400 pt for 80 operators: 5 pt each.
        let marks = [
            PageMark { ix: 0, page: 0 },
            PageMark { ix: 4, page: 1 },
            PageMark { ix: 10, page: 2 },
            PageMark { ix: 40, page: 3 },
        ];

        let shown: Vec<PageIx> = StepTrack::labelled(rect(), &marks, 80)
            .iter()
            .map(|m| m.page)
            .collect();

        // p1 is 20 pt from p2, p2 30 pt from p3: p1 has no room, p2 and p3 and p4 do.
        assert_eq!(shown, [1, 2, 3]);
        assert_eq!(page_text(2), "p3");
        // Smoke: labels never overlap, even where the marks leave room one by one.
        let crowded: Vec<PageMark> = (0..20).map(|n| PageMark { ix: n * 4, page: n }).collect();
        let xs: Vec<f32> = StepTrack::labelled(rect(), &crowded, 80)
            .iter()
            .map(|m| StepTrack::x_of(rect(), m.ix, 80))
            .collect();
        assert!(xs.windows(2).all(|w| w[1] - w[0] >= LABEL_ROOM), "{xs:?}");
    }

    #[test]
    fn the_knob_sits_where_a_click_would_pick_the_same_operator() {
        for step in [0, 1, 17, 79, 80] {
            let x = StepTrack::x_of(rect(), step, 80);
            assert_eq!(StepTrack::step_at(rect(), x, 80), step);
        }
        assert!((StepTrack::x_of(rect(), 40, 80) - 306.0).abs() < 1e-3);
        // A single operator has nowhere to go.
        assert!((StepTrack::x_of(rect(), 0, 0) - 106.0).abs() < 1e-3);
        assert_eq!(StepTrack::step_at(rect(), 400.0, 0), 0);
    }
}
