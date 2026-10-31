//! What the canvas draws for the node views: the hover outline, the tree preview, glyph boxes,
//! untagged content in red, and the tags that name the selection and the baseline.

use egui::{Align2, Color32, Painter, Pos2, Stroke, StrokeKind, vec2};
use pdfxray_core::{NodeId, PageIx, Rect};

use super::geometry::PageGeometry;
use crate::selection::selection_rect;
use crate::state::{AppState, RailItem};
use crate::theme::{fonts, tokens};

/// How a box is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    /// The node under the pointer: a thin outline.
    Hover,
    /// The node a tree row is previewing.
    Preview,
    /// A glyph of the selected text operator.
    Glyph,
    /// The selected glyph.
    GlyphSelected,
    /// Content that is neither tagged nor an artifact.
    Untagged,
}

/// One box on the page.
#[derive(Debug, Clone, Copy)]
pub(super) struct Mark {
    pub(super) rect: Rect,
    pub(super) style: Style,
}

/// A tag over a box: the text, whether it is a warning, and the box it names.
#[derive(Debug, Clone)]
pub(super) struct Tag {
    pub(super) text: String,
    pub(super) warn: bool,
    pub(super) rect: Rect,
}

/// Everything extra to draw on one page.
#[derive(Debug, Default)]
pub(super) struct Marks {
    pub(super) boxes: Vec<Mark>,
    /// The selection's tag.
    pub(super) selection: Option<Tag>,
    /// `baseline y 676`, with the page-space line it sits on: `(x0, x1, y, text)`.
    pub(super) baseline: Option<(f64, f64, f64, String)>,
}

/// Collects what to draw on `page` this frame. `preview` is the node a tree row asks to preview
/// (the canvas takes it out of the state once per frame); when it is on this page it is drawn and
/// recorded in [`crate::CanvasState::drawn_preview`].
pub(super) fn collect(state: &mut AppState, page: PageIx, preview: Option<&NodeId>) -> Marks {
    let mut marks = Marks::default();
    let rect_on_page = |state: &AppState, id: &NodeId| {
        let doc = &state.document.as_ref()?.doc;
        selection_rect(doc, id).filter(|s| s.page == page).map(|s| s.rect)
    };

    if state.nav.rail == RailItem::Content && state.canvas.view.xray {
        let untagged: Vec<Rect> = state
            .page_stats(page)
            .map(|stats| stats.untagged_boxes.iter().map(|(_, rect)| *rect).collect())
            .unwrap_or_default();
        marks.boxes.extend(untagged.into_iter().map(|rect| Mark {
            rect,
            style: Style::Untagged,
        }));
    }

    // The glyph boxes belong to the selected operator, which is on one page.
    let selection_here = matches!(
        state.selection.primary,
        Some(NodeId::Glyph { page: p, .. } | NodeId::Op { page: p, .. }) if p == page
    );
    let (glyphs, chosen) = if selection_here {
        state.selected_glyph_boxes()
    } else {
        (Vec::new(), None)
    };
    for (n, rect) in glyphs.iter().enumerate() {
        marks.boxes.push(Mark {
            rect: *rect,
            style: if Some(n) == chosen {
                Style::GlyphSelected
            } else {
                Style::Glyph
            },
        });
    }
    if let (Some(n), Some(NodeId::Glyph { .. })) = (chosen, state.selection.primary.clone())
        && let Some(selected) = glyphs.get(n)
        && let Some(info) = state
            .selection
            .primary
            .as_ref()
            .and_then(|id| state.document.as_ref()?.doc.glyph_info(id))
    {
        let lengths = state.lengths();
        let x0 = glyphs.iter().map(|g| g.x0).fold(selected.x0, f64::min);
        let x1 = glyphs.iter().map(|g| g.x1).fold(selected.x1, f64::max);
        marks.baseline = Some((
            x0,
            x1,
            info.origin.y,
            format!("baseline y {}", lengths.format(info.origin.y)),
        ));
    }

    if let Some(id) = state.canvas.hover.clone()
        && Some(&id) != state.selection.primary.as_ref()
        && let Some(rect) = rect_on_page(state, &id)
    {
        marks.boxes.push(Mark {
            rect,
            style: Style::Hover,
        });
    }
    if let Some(id) = preview
        && let Some(rect) = rect_on_page(state, id)
    {
        marks.boxes.push(Mark {
            rect,
            style: Style::Preview,
        });
        state.canvas.drawn_preview = Some(id.clone());
    }

    if let Some(label) = state.selection_label()
        && let Some(rect) = state
            .selection
            .primary
            .clone()
            .and_then(|id| rect_on_page(state, &id))
    {
        // While stepping, the operator the stepper stopped at says it is painting now.
        let painting_now = super::paint_order::painting_now(state);
        marks.selection = Some(Tag {
            warn: label.warn && painting_now.is_none(),
            text: painting_now.unwrap_or(label.text),
            rect,
        });
    }
    marks
}

/// Draws the boxes and tags over the page. Returns where the baseline's tag is, for what floats
/// under the selection to stay clear of it.
pub(super) fn draw(painter: &Painter, geo: &PageGeometry, marks: &Marks) -> Option<egui::Rect> {
    for mark in &marks.boxes {
        let rect = geo.rect_to_screen(mark.rect);
        match mark.style {
            Style::Hover => {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.5, tokens::ACCENT), StrokeKind::Outside);
            }
            Style::Preview => {
                painter.rect(
                    rect,
                    0.0,
                    tokens::ACCENT.gamma_multiply(0.08),
                    Stroke::new(1.5, tokens::ACCENT),
                    StrokeKind::Outside,
                );
            }
            Style::Glyph => {
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, tokens::KIND_CONTENT.gamma_multiply(0.9)),
                    StrokeKind::Outside,
                );
            }
            Style::GlyphSelected => {
                painter.rect(
                    rect,
                    0.0,
                    tokens::ACCENT.gamma_multiply(0.18),
                    Stroke::new(1.5, tokens::ACCENT),
                    StrokeKind::Outside,
                );
            }
            Style::Untagged => {
                painter.rect(
                    rect,
                    0.0,
                    tokens::ERR.gamma_multiply(0.08),
                    Stroke::new(1.5, tokens::ERR),
                    StrokeKind::Outside,
                );
            }
        }
    }
    let baseline = marks.baseline.as_ref().map(|(x0, x1, y, text)| {
        let from = geo.page_to_screen(pdfxray_core::Point::new(*x0, *y));
        let to = geo.page_to_screen(pdfxray_core::Point::new(*x1, *y));
        painter.line_segment([from, to], Stroke::new(1.0, tokens::PINK));
        pill(
            painter,
            to + vec2(0.0, 4.0),
            Align2::RIGHT_TOP,
            text,
            tokens::PINK,
            tokens::BG,
        )
    });
    if let Some(tag) = &marks.selection {
        let rect = geo.rect_to_screen(tag.rect);
        let fill = if tag.warn { tokens::ERR } else { tokens::ACCENT };
        pill(
            painter,
            rect.left_top() - vec2(0.0, 2.0),
            Align2::LEFT_BOTTOM,
            &tag.text,
            fill,
            tokens::BG,
        );
    }
    baseline
}

/// A tag of mono text on a filled pill, anchored at `at`. It moves inside the painter's clip
/// rectangle when the anchor is at its edge. Returns where it was drawn.
pub(super) fn pill(
    painter: &Painter,
    at: Pos2,
    anchor: Align2,
    text: &str,
    fill: Color32,
    ink: Color32,
) -> egui::Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), fonts::mono_semibold(tokens::mono::SMALL), ink);
    let size = galley.size() + vec2(10.0, 4.0);
    let rect = keep_inside(anchor.anchor_size(at, size), painter.clip_rect());
    painter.rect_filled(rect, tokens::RADIUS_SM, fill);
    painter.galley(rect.min + vec2(5.0, 2.0), galley, ink);
    // Keeps the tag readable on a busy page: a hairline in the page's dark.
    painter.rect_stroke(
        rect,
        tokens::RADIUS_SM,
        Stroke::new(1.0, tokens::BG),
        StrokeKind::Outside,
    );
    rect
}

/// `rect` moved inside `clip` when it sticks out of it at the top or at a side. Its start wins
/// over its end when it is wider than `clip`: that is the part that is read first.
fn keep_inside(rect: egui::Rect, clip: egui::Rect) -> egui::Rect {
    let dy = (clip.min.y - rect.min.y).max(0.0);
    let to_left = (clip.max.x - rect.max.x).min(0.0);
    let dx = to_left + (clip.min.x - (rect.min.x + to_left)).max(0.0);
    rect.translate(vec2(dx, dy))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use egui::{Rect as UiRect, pos2};
    use pretty_assertions::assert_eq;

    use super::*;

    const PAGE: fn() -> UiRect = || UiRect::from_min_max(pos2(100.0, 50.0), pos2(400.0, 500.0));

    #[test]
    fn a_tag_that_sticks_out_to_the_left_starts_at_the_edge_of_the_page() {
        // A baseline tag hangs from the right end of a short run at the left edge.
        let tag = UiRect::from_min_size(pos2(40.0, 200.0), vec2(90.0, 16.0));

        let kept = keep_inside(tag, PAGE());

        assert_eq!(kept, UiRect::from_min_size(pos2(100.0, 200.0), vec2(90.0, 16.0)));
    }

    #[test]
    fn a_tag_that_sticks_out_to_the_right_or_the_top_comes_back_in() {
        let right = UiRect::from_min_size(pos2(350.0, 200.0), vec2(90.0, 16.0));
        let top = UiRect::from_min_size(pos2(200.0, 40.0), vec2(90.0, 16.0));

        assert_eq!(keep_inside(right, PAGE()).max.x, 400.0);
        assert_eq!(keep_inside(top, PAGE()).min.y, 50.0);
        // Smoke: one that is inside stays where it is.
        let inside = UiRect::from_min_size(pos2(200.0, 200.0), vec2(90.0, 16.0));
        assert_eq!(keep_inside(inside, PAGE()), inside);
    }

    #[test]
    fn a_tag_wider_than_the_page_keeps_its_start() {
        let wide = UiRect::from_min_size(pos2(60.0, 200.0), vec2(400.0, 16.0));

        assert_eq!(keep_inside(wide, PAGE()).min.x, 100.0);
    }
}
