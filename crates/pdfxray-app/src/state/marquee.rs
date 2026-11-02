//! The marquee: a rectangle dragged on a page that selects everything of the Pick target it
//! touches (see [`pdfxray_core::Document::hit_rect`]).

use egui::Modifiers;
use pdfxray_core::{NodeId, PageIx, Point, Rect};

use super::AppState;
use super::selection::Combine;

/// A marquee being drawn on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct Marquee {
    /// The page it is drawn on; it never leaves it.
    pub page: PageIx,
    /// Where the drag started, in page space.
    pub start: Point,
    /// Where the pointer is now, in page space.
    pub end: Point,
    /// What it does with what it touches: replace, add (Shift) or subtract (⌥).
    pub how: Combine,
    /// What it touches now, in reading order.
    pub catch: Vec<NodeId>,
}

impl Marquee {
    /// The rectangle, in page space.
    pub fn rect(&self) -> Rect {
        Rect::new(self.start.x, self.start.y, self.end.x, self.end.y)
    }
}

/// What a marquee started with `modifiers` does: Shift adds, ⌥ subtracts, anything else (⌘ too)
/// replaces the selection.
pub fn combine_for(modifiers: Modifiers) -> Combine {
    if modifiers.alt {
        Combine::Subtract
    } else if modifiers.shift {
        Combine::Add
    } else {
        Combine::Replace
    }
}

impl AppState {
    /// Starts a marquee at `point` of `page`.
    pub fn start_marquee(&mut self, page: PageIx, point: Point, modifiers: Modifiers) {
        self.canvas.marquee = Some(Marquee {
            page,
            start: point,
            end: point,
            how: combine_for(modifiers),
            catch: Vec::new(),
        });
        self.drag_marquee(point);
    }

    /// Moves the marquee's free corner to `point` and works out what it touches.
    pub fn drag_marquee(&mut self, point: Point) {
        let layer = self.canvas.layer_mode;
        let Some(marquee) = self.canvas.marquee.as_mut() else {
            return;
        };
        let moved = marquee.end != point;
        marquee.end = point;
        if !moved && !marquee.catch.is_empty() {
            return;
        }
        marquee.catch = self
            .document
            .as_ref()
            .map(|loaded| loaded.doc.hit_rect(marquee.page, marquee.rect(), layer))
            .unwrap_or_default();
    }

    /// The marquee was let go: what it touches joins, replaces or leaves the selection. A
    /// replacing marquee over nothing clears the selection.
    pub fn finish_marquee(&mut self) {
        let Some(marquee) = self.canvas.marquee.take() else {
            return;
        };
        if marquee.how == Combine::Replace && marquee.catch.is_empty() {
            self.clear_selection();
            return;
        }
        self.select_nodes(marquee.catch, marquee.how);
    }

    /// The text of the chip that trails the cursor: how many nodes the marquee touches and what
    /// it picks, e.g. `4 · Tags`.
    pub fn marquee_count(&self) -> Option<String> {
        let marquee = self.canvas.marquee.as_ref()?;
        Some(format!(
            "{} · {}",
            marquee.catch.len(),
            self.canvas.layer_mode.title()
        ))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn shift_adds_alt_subtracts_and_anything_else_replaces() {
        assert_eq!(combine_for(Modifiers::SHIFT), Combine::Add);
        assert_eq!(combine_for(Modifiers::ALT), Combine::Subtract);
        assert_eq!(combine_for(Modifiers::COMMAND), Combine::Replace);
        // Smoke: no modifier replaces.
        assert_eq!(combine_for(Modifiers::NONE), Combine::Replace);
    }

    #[test]
    fn the_rectangle_is_normalised_whichever_way_it_is_dragged() {
        let marquee = Marquee {
            page: 0,
            start: Point::new(50.0, 10.0),
            end: Point::new(20.0, 40.0),
            how: Combine::Replace,
            catch: Vec::new(),
        };
        assert_eq!(marquee.rect(), Rect::new(20.0, 10.0, 50.0, 40.0));
    }
}
