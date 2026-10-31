//! A segmented control: one of a few mutually exclusive options in a pill.

use egui::{Color32, FontId, Frame, Margin, Role, Sense, Stroke, Ui, WidgetInfo, vec2};
use pdfxray_core::numeric::f32_from_usize;

use super::focus;
use crate::theme::{fonts, tokens};

/// One option of a [`segmented`] control.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Segment<'a, T> {
    /// The value the control takes when this option is picked.
    pub value: T,
    /// Text of the option; also its accessible name.
    pub label: &'a str,
    /// A kind dot in front of the text.
    pub dot: Option<Color32>,
}

impl<'a, T> Segment<'a, T> {
    /// An option without a dot.
    pub const fn new(value: T, label: &'a str) -> Self {
        Self {
            value,
            label,
            dot: None,
        }
    }

    /// Adds a kind dot.
    pub const fn dot(mut self, color: Color32) -> Self {
        self.dot = Some(color);
        self
    }
}

const DOT: f32 = 6.0;
const GAP: f32 = 6.0;

/// Sizes of a control.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    /// Height of one option.
    height: f32,
    /// Space left and right of an option's text.
    pad: f32,
    /// Space between the pill's edge and the options.
    inset: i8,
    /// Corner radius of the pill.
    radius: u8,
    /// The font of an option, selected or not.
    font: fn(bool) -> FontId,
    /// Ink of an option that is neither selected nor hovered.
    idle_ink: Color32,
    /// Fill of the pill.
    fill: Color32,
    /// The pill has a hairline.
    outline: bool,
}

const REGULAR: Metrics = Metrics {
    height: 24.0,
    pad: 10.0,
    inset: 3,
    radius: 8,
    font: |selected| {
        if selected {
            fonts::medium(tokens::text::BODY)
        } else {
            fonts::ui(tokens::text::BODY)
        }
    },
    idle_ink: tokens::TEXT_2,
    fill: tokens::CANVAS,
    outline: true,
};

const COMPACT: Metrics = Metrics {
    height: 18.0,
    pad: 7.0,
    inset: 2,
    radius: 5,
    font: |selected| {
        if selected {
            fonts::mono_medium(tokens::mono::SMALL)
        } else {
            fonts::mono(tokens::mono::SMALL)
        }
    },
    idle_ink: tokens::TEXT_3,
    fill: tokens::CANVAS,
    outline: true,
};

/// Size of the text of a [`segmented_mini`] option.
const MINI_TEXT: f32 = 10.5;

const MINI: Metrics = Metrics {
    height: 16.0,
    pad: 6.0,
    inset: 2,
    radius: 5,
    font: |_| fonts::ui(MINI_TEXT),
    idle_ink: tokens::TEXT_3,
    fill: tokens::SURFACE_2,
    outline: false,
};

/// Draws the control for `current`; returns true when the user picked another option.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, current: &mut T, segments: &[Segment<'_, T>]) -> bool {
    draw(ui, current, segments, REGULAR)
}

/// The small mono variant of [`segmented`], used by the units switch in the status bar.
pub fn segmented_compact<T: Copy + PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    segments: &[Segment<'_, T>],
) -> bool {
    draw(ui, current, segments, COMPACT)
}

/// The smallest variant of [`segmented`]: 16-point options on a surface-2 pill without a
/// hairline, as in the header of a preview card (Fit · 1:1).
pub fn segmented_mini<T: Copy + PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    segments: &[Segment<'_, T>],
) -> bool {
    draw(ui, current, segments, MINI)
}

/// How wide [`segmented`] draws `segments`, so a row can keep room for it at its right end.
pub fn segmented_width<T>(ui: &Ui, segments: &[Segment<'_, T>]) -> f32 {
    width(ui, segments, REGULAR)
}

/// How wide [`segmented_mini`] draws `segments`, so a row can keep room for it at its right end.
pub fn segmented_mini_width<T>(ui: &Ui, segments: &[Segment<'_, T>]) -> f32 {
    width(ui, segments, MINI)
}

/// The width of the control: each option as wide as its label in the wider of its two fonts
/// (selected or not), the gaps between them and the pill's inset.
fn width<T>(ui: &Ui, segments: &[Segment<'_, T>], metrics: Metrics) -> f32 {
    let label_w = |label: &str, selected: bool| {
        ui.painter()
            .layout_no_wrap(label.to_owned(), (metrics.font)(selected), Color32::WHITE)
            .size()
            .x
    };
    let labels: f32 = segments
        .iter()
        .map(|segment| {
            let dot = if segment.dot.is_some() { DOT + GAP } else { 0.0 };
            metrics.pad * 2.0 + label_w(segment.label, false).max(label_w(segment.label, true)) + dot
        })
        .sum();
    let gaps = SEGMENT_GAP * f32_from_usize(segments.len().saturating_sub(1));
    labels + gaps + 2.0 * f32::from(metrics.inset)
}

/// Space between two options of a control.
const SEGMENT_GAP: f32 = 2.0;

fn draw<T: Copy + PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    segments: &[Segment<'_, T>],
    metrics: Metrics,
) -> bool {
    let mut changed = false;
    let stroke = if metrics.outline {
        Stroke::new(1.0, tokens::HAIRLINE)
    } else {
        Stroke::NONE
    };
    Frame::new()
        .fill(metrics.fill)
        .stroke(stroke)
        .corner_radius(metrics.radius)
        .inner_margin(Margin::same(metrics.inset))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = SEGMENT_GAP;
            ui.horizontal(|ui| {
                for segment in segments {
                    let selected = *current == segment.value;
                    if segment_ui(ui, segment, selected, metrics).clicked() && !selected {
                        *current = segment.value;
                        changed = true;
                    }
                }
            });
        });
    changed
}

fn segment_ui<T>(ui: &mut Ui, segment: &Segment<'_, T>, selected: bool, metrics: Metrics) -> egui::Response {
    let font = (metrics.font)(selected);
    let galley = ui
        .painter()
        .layout_no_wrap(segment.label.to_owned(), font, Color32::WHITE);
    let mut width = metrics.pad * 2.0 + galley.size().x;
    if segment.dot.is_some() {
        width += DOT + GAP;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(width, metrics.height), Sense::click());
    let label = segment.label;
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, selected, label));
    if ui.is_rect_visible(rect) {
        let (fill, ink) = if selected {
            // Selected is neutral (white on a white 8 % fill), not the accent.
            (tokens::NEUTRAL_ACTIVE, tokens::ACTIVE_INK)
        } else if response.hovered() {
            (tokens::NEUTRAL_HOVER, tokens::TEXT)
        } else {
            (Color32::TRANSPARENT, metrics.idle_ink)
        };
        // The option's corners follow the pill's, inset.
        let radius = f32::from(metrics.radius) - f32::from(metrics.inset);
        ui.painter().rect_filled(rect, radius, fill);
        focus::ring(ui, &response, rect, radius);
        let mut x = rect.min.x + metrics.pad;
        if let Some(color) = segment.dot {
            let dot =
                egui::Rect::from_center_size(egui::pos2(x + DOT / 2.0, rect.center().y), vec2(DOT, DOT));
            ui.painter().rect_filled(dot, 2.0, color);
            x += DOT + GAP;
        }
        let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley_with_override_text_color(pos, galley, ink);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
