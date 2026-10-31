//! The scrubbable track of the paint-order stepper: a thin bar with a knob at the current
//! operator. Clicking or dragging it picks the operator under the pointer.

use egui::{Color32, CursorIcon, Rect, Sense, Stroke, Ui, WidgetInfo, pos2, vec2};
use pdfxray_core::numeric::{f32_from_u32, u32_from_f32_round};

use crate::theme::tokens;

/// Accessible name of the track.
pub const LABEL: &str = "Paint order position";

/// Space left at each end so the knob is not clipped.
const PAD: f32 = 6.0;

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
    /// Execution index of the operator.
    pub ix: u32,
    /// What it says.
    pub kind: TickKind,
}

/// The track of operators `0..=last` with the knob at `step`.
#[derive(Debug, Clone, Copy)]
#[must_use = "call `show`"]
pub struct StepTrack<'a> {
    step: u32,
    last: u32,
    ticks: &'a [Tick],
    height: f32,
}

impl<'a> StepTrack<'a> {
    /// A track for operators up to `last`, the knob at `step`.
    pub const fn new(step: u32, last: u32) -> Self {
        Self {
            step,
            last,
            ticks: &[],
            height: 24.0,
        }
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
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), self.height), Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::slider(true, f64::from(self.step), LABEL));
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect);
        }
        let picked = (response.clicked() || response.dragged() || response.drag_started())
            .then(|| response.interact_pointer_pos())
            .flatten()
            .map(|pos| Self::step_at(rect, pos.x, self.last))?;
        (picked != self.step).then_some(picked)
    }

    fn paint(&self, ui: &Ui, rect: Rect) {
        let painter = ui.painter();
        let y = rect.center().y;
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
