//! Where the pages go in the scrolled content, and which one fills most of the view.

use egui::{Rect as UiRect, Vec2, pos2, vec2};
use pdfxray_core::PageIx;

/// Space around the pages and between two pages, in points.
pub const PAGE_GAP: f32 = 16.0;

/// Where each page goes in the scrolled content, at the canvas zoom.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    /// One rectangle per page, relative to the top-left corner of the content.
    pub pages: Vec<UiRect>,
    /// Size of the whole content.
    pub size: Vec2,
}

/// Lays `sizes` (page sizes on screen) out top to bottom, [`PAGE_GAP`] apart and centred
/// across. A document shorter than the view is centred in it.
pub fn layout_pages(sizes: &[Vec2], view: Vec2) -> PageLayout {
    let widest = sizes.iter().map(|s| s.x).fold(0.0, f32::max);
    let width = view.x.max(widest + PAGE_GAP * 2.0);
    let total = sizes.iter().map(|s| s.y + PAGE_GAP).sum::<f32>() + PAGE_GAP;
    let height = view.y.max(total);
    let mut y = PAGE_GAP + (height - total) / 2.0;
    let pages = sizes
        .iter()
        .map(|size| {
            let rect = UiRect::from_min_size(pos2((width - size.x) / 2.0, y), *size);
            y += size.y + PAGE_GAP;
            rect
        })
        .collect();
    PageLayout {
        pages,
        size: vec2(width, height),
    }
}

/// The page whose rectangle covers most of `view` (both relative to the content).
pub fn most_visible(layout: &PageLayout, view: UiRect) -> Option<PageIx> {
    layout
        .pages
        .iter()
        .enumerate()
        .map(|(page, rect)| (page, rect.intersect(view).height().max(0.0)))
        .filter(|(_, seen)| *seen > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .and_then(|(page, _)| PageIx::try_from(page).ok())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn pages_stack_top_to_bottom_a_gap_apart_and_centred_across() {
        let layout = layout_pages(&[vec2(100.0, 200.0), vec2(60.0, 50.0)], vec2(150.0, 100.0));

        assert_eq!(
            layout.pages,
            [
                UiRect::from_min_size(pos2(25.0, PAGE_GAP), vec2(100.0, 200.0)),
                UiRect::from_min_size(pos2(45.0, PAGE_GAP * 2.0 + 200.0), vec2(60.0, 50.0)),
            ]
        );
        // Smoke: the content is as tall as the pages and gaps, and as wide as the view.
        assert_eq!(layout.size, vec2(150.0, 200.0 + 50.0 + PAGE_GAP * 3.0));
    }

    #[test]
    fn a_document_shorter_than_the_view_is_centred_in_it() {
        let layout = layout_pages(&[vec2(100.0, 100.0)], vec2(300.0, 400.0));

        assert_eq!(
            layout.pages,
            [UiRect::from_min_size(pos2(100.0, 150.0), vec2(100.0, 100.0))]
        );
        assert_eq!(layout.size, vec2(300.0, 400.0));
    }

    #[test]
    fn the_current_page_is_the_one_that_fills_most_of_the_view() {
        let layout = layout_pages(&[vec2(100.0, 300.0); 3], vec2(200.0, 200.0));
        // Pages start at 16, 332 and 648; a view from 250 to 450 shows 66 pt of page 1 and
        // 118 pt of page 2.
        let view = UiRect::from_min_size(pos2(0.0, 250.0), vec2(200.0, 200.0));

        assert_eq!(most_visible(&layout, view), Some(1));
        // Smoke: a view past the last page shows none.
        let past = UiRect::from_min_size(pos2(0.0, 2000.0), vec2(200.0, 200.0));
        assert_eq!(most_visible(&layout, past), None);
    }
}
