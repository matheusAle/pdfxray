//! Failed checks on the page: a red outline around the node of each finding and a pill above it
//! naming the rule, for the Validation entry.

use egui::{Color32, CursorIcon, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use super::geometry::PageGeometry;
use crate::findings::{FAIL, PageBadge, pill_position};
use crate::theme::{fonts, tokens};

/// Draws `badges` over the page and returns the finding of the pill that was clicked.
pub(super) fn draw(ui: &Ui, geo: &PageGeometry, badges: &[PageBadge]) -> Option<u32> {
    let painter = ui.painter().with_clip_rect(ui.clip_rect());
    let mut placed: Vec<Rect> = Vec::new();
    let mut clicked = None;
    // The current finding is drawn last, so it is on top.
    let ordered = badges
        .iter()
        .filter(|badge| !badge.current)
        .chain(badges.iter().filter(|badge| badge.current));
    for badge in ordered {
        let rect = geo.rect_to_screen(badge.rect);
        let width = if badge.current { 2.5 } else { 1.5 };
        painter.rect(
            rect,
            0.0,
            FAIL.gamma_multiply(0.07),
            Stroke::new(width, FAIL),
            StrokeKind::Outside,
        );
        let galley =
            painter.layout_no_wrap(badge.text.clone(), fonts::medium(tokens::text::SMALL), tokens::BG);
        let size = galley.size() + vec2(12.0, 6.0);
        let pill = pill_position(rect.left_top(), geo.rect.min.y, &placed, size);
        placed.push(pill);
        let response = ui
            .interact(
                pill,
                ui.make_persistent_id(("badge", badge.finding)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, true, badge.text.clone()));
        // Under the pointer the pill lightens.
        let fill = if response.hovered() {
            FAIL.lerp_to_gamma(Color32::WHITE, 0.2)
        } else {
            FAIL
        };
        painter.rect_filled(pill, tokens::RADIUS_SM, fill);
        painter.galley(pill.min + vec2(6.0, 3.0), galley, tokens::BG);
        if response.clicked() {
            clicked = Some(badge.finding);
        }
    }
    clicked
}
