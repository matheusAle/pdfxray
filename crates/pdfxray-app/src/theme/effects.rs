//! Shared surfaces and effects: frosted frames, the page's shadows, the faint grid and the
//! primary button's gradient.
//!
//! egui cannot blur what is behind a surface, so a "frosted" surface is a solid tinted fill at
//! the design's alpha with a hairline and a soft shadow. Shadows are egui [`Shadow`]s: one layer
//! per call, so the page's two-layer shadow is two of them.

use egui::epaint::{Mesh, Shadow, Vertex, WHITE_UV};
use egui::{Color32, CornerRadius, Frame, Margin, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use pdfxray_core::numeric::u8_from_f32_round;

use super::tokens;

/// Radius of floating canvas cards.
pub const CARD_RADIUS: f32 = 10.0;
/// Distance between the lines of the grid behind the pages.
pub const GRID_STEP: f32 = 24.0;

/// Popovers, menus and tooltips: `0 18 44 #000000CC`.
pub const POPOVER_SHADOW: Shadow = Shadow {
    offset: [0, 18],
    blur: 44,
    spread: 0,
    color: Color32::from_black_alpha(0xCC),
};
/// Floating cards: lighter than a popover, they sit lower.
pub const CARD_SHADOW: Shadow = Shadow {
    offset: [0, 10],
    blur: 28,
    spread: 0,
    color: Color32::from_black_alpha(0x99),
};
/// The two layers of a page's shadow, far one first: `0 28 70 #000000B8` and `0 2 6 #00000066`.
pub const PAGE_SHADOWS: [Shadow; 2] = [
    Shadow {
        offset: [0, 28],
        blur: 70,
        spread: 0,
        color: Color32::from_black_alpha(0xB8),
    },
    Shadow {
        offset: [0, 2],
        blur: 6,
        spread: 0,
        color: Color32::from_black_alpha(0x66),
    },
];
/// The glow under the primary button: `0 4 14 #3DD6F533`.
pub const GLOW: Shadow = Shadow {
    offset: [0, 4],
    blur: 14,
    spread: 0,
    color: Color32::from_rgba_unmultiplied_const(0x3D, 0xD6, 0xF5, 0x33),
};
/// The 1 px highlight along the top of the primary button: `#FFFFFF40`.
const HIGHLIGHT: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x40);
/// The grid's lines: `#FFFFFF05`.
const GRID_LINE: Color32 = Color32::from_rgba_unmultiplied_const(0xFF, 0xFF, 0xFF, 0x05);

/// The frame of popovers and menus: [`tokens::POPOVER_FILL`], a 1 px `#FFFFFF17` hairline and a
/// deep soft shadow.
pub fn frosted_panel() -> Frame {
    frosted_panel_at(1.0)
}

/// [`frosted_panel`] at `opacity` (0..=1) of its own: the fill, hairline and shadow fade
/// together while it opens.
pub fn frosted_panel_at(opacity: f32) -> Frame {
    frosted(opacity, tokens::POPOVER_FILL, POPOVER_SHADOW)
}

/// The frame of tooltips: [`frosted_panel_at`] with the opaque [`tokens::TOOLTIP_FILL`] and
/// [`ON_PAGE_SHADOW`]. A tooltip is small and often rests on the white page, where a popover's
/// deep shadow reads as a smudge, or on dense rows that must not read through it.
pub fn frosted_tooltip_at(opacity: f32) -> Frame {
    frosted(opacity, tokens::TOOLTIP_FILL, ON_PAGE_SHADOW)
}

fn frosted(opacity: f32, fill: Color32, shadow: Shadow) -> Frame {
    let shadow = Shadow {
        color: shadow.color.gamma_multiply(opacity),
        ..shadow
    };
    Frame::new()
        .fill(fill.gamma_multiply(opacity))
        .stroke(Stroke::new(1.0, tokens::POPOVER_HAIRLINE.gamma_multiply(opacity)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
        .shadow(shadow)
}

/// Bars that float over the page itself (the selection action bar): a popover's deep shadow
/// would read as a smudge on white paper, so they sit close with a small one.
pub const ON_PAGE_SHADOW: Shadow = Shadow {
    offset: [0, 4],
    blur: 12,
    spread: 0,
    color: Color32::from_black_alpha(0x59),
};

/// [`frosted_panel`] with [`ON_PAGE_SHADOW`], for bars drawn over a page.
pub fn on_page_bar() -> Frame {
    frosted_panel().shadow(ON_PAGE_SHADOW)
}

/// The frame of a floating canvas card: `#12161B` at 90 %, radius 10, a hairline and a shadow.
pub fn floating_card() -> Frame {
    Frame::new()
        .fill(tokens::CARD_FILL)
        .stroke(Stroke::new(1.0, tokens::POPOVER_HAIRLINE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .shadow(CARD_SHADOW)
}

/// Paints the two-layer shadow of a page whose rectangle is `rect`.
pub fn page_shadow(painter: &Painter, rect: Rect) {
    for shadow in PAGE_SHADOWS {
        painter.add(shadow.as_shape(rect, 0.0));
    }
}

/// Everything the shadow of a page at `rect` can darken, the page included.
pub fn page_shadow_reach(rect: Rect) -> Rect {
    PAGE_SHADOWS
        .iter()
        .fold(rect, |reach, shadow| reach.union(rect + shadow.margin()))
}

/// The segments of the faint grid inside `rect`: a vertical line every [`GRID_STEP`] from
/// `rect.min + offset` (the offset is the pan of the canvas), then the horizontal ones. A line
/// on the rect's far edge is not part of it.
pub fn grid_lines(rect: Rect, offset: Vec2) -> Vec<[Pos2; 2]> {
    let first = |from: f32, shift: f32| from + shift.rem_euclid(GRID_STEP);
    let mut lines = Vec::new();
    let mut x = first(rect.min.x, offset.x);
    while x < rect.max.x {
        lines.push([pos2(x, rect.min.y), pos2(x, rect.max.y)]);
        x += GRID_STEP;
    }
    let mut y = first(rect.min.y, offset.y);
    while y < rect.max.y {
        lines.push([pos2(rect.min.x, y), pos2(rect.max.x, y)]);
        y += GRID_STEP;
    }
    lines
}

/// Paints the faint 24 px grid (`#FFFFFF05`) inside `rect`, shifted by `offset` so it follows
/// the canvas when it pans.
pub fn grid(painter: &Painter, rect: Rect, offset: Vec2) {
    let stroke = Stroke::new(1.0, GRID_LINE);
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let lines = grid_lines(rect, offset)
        .into_iter()
        .map(|points| Shape::line_segment(points, stroke))
        .collect();
    painter.add(Shape::Vec(lines));
}

/// Segments per rounded corner of the gradient's outline.
const CORNER_STEPS: u8 = 6;

/// The outline of `rect` with rounded corners, clockwise from the top edge's left end.
fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
    // (corner centre, first angle of the arc), clockwise: top-left, top-right, bottom-right,
    // bottom-left.
    let corners = [
        (pos2(rect.min.x + r, rect.min.y + r), 180.0_f32),
        (pos2(rect.max.x - r, rect.min.y + r), 270.0),
        (pos2(rect.max.x - r, rect.max.y - r), 0.0),
        (pos2(rect.min.x + r, rect.max.y - r), 90.0),
    ];
    let mut points = Vec::with_capacity(4 * (usize::from(CORNER_STEPS) + 1));
    for (centre, start) in corners {
        for step in 0..=CORNER_STEPS {
            let angle = (start + 90.0 * f32::from(step) / f32::from(CORNER_STEPS)).to_radians();
            points.push(centre + vec2(angle.cos(), angle.sin()) * r);
        }
    }
    points
}

/// The vertex-coloured mesh of the primary gradient: [`tokens::PRIMARY_TOP`] on the top edge to
/// [`tokens::PRIMARY_BOTTOM`] on the bottom edge, the corners rounded by `radius`. A fan from the
/// centre: the colour is a linear function of y at every vertex, so it is exactly linear inside.
pub fn gradient_mesh(rect: Rect, radius: f32) -> Mesh {
    let colour_at = |y: f32| {
        let t = ((y - rect.min.y) / rect.height().max(f32::EPSILON)).clamp(0.0, 1.0);
        lerp_colour(tokens::PRIMARY_TOP, tokens::PRIMARY_BOTTOM, t)
    };
    let outline = rounded_outline(rect, radius);
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: rect.center(),
        uv: WHITE_UV,
        color: colour_at(rect.center().y),
    });
    for point in &outline {
        mesh.vertices.push(Vertex {
            pos: *point,
            uv: WHITE_UV,
            color: colour_at(point.y),
        });
    }
    let count = u32::try_from(outline.len()).unwrap_or(u32::MAX);
    for i in 0..count {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % count);
    }
    mesh
}

fn lerp_colour(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |from: u8, to: u8| {
        let value = f32::from(from) + (f32::from(to) - f32::from(from)) * t;
        // A channel between two u8s, rounded.
        u8_from_f32_round(value)
    };
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

/// Paints the primary button's fill in `rect`: the soft cyan glow, the vertical gradient
/// `#6FE3F9` to `#2CC6E8` and a 1 px `#FFFFFF40` highlight along the top.
pub fn primary_gradient(painter: &Painter, rect: Rect, radius: f32) {
    painter.add(GLOW.as_shape(rect, radius_of(radius)));
    painter.add(Shape::mesh(gradient_mesh(rect, radius)));
    let inset = radius.min(rect.width() / 2.0);
    painter.line_segment(
        [
            pos2(rect.min.x + inset, rect.min.y + 0.5),
            pos2(rect.max.x - inset, rect.min.y + 0.5),
        ],
        Stroke::new(1.0, HIGHLIGHT),
    );
}

fn radius_of(radius: f32) -> CornerRadius {
    // Radii are small whole numbers.
    CornerRadius::same(u8_from_f32_round(radius))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use egui::{Context, RawInput, Rect, Shape, pos2, vec2};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn the_gradient_runs_from_the_top_colour_to_the_bottom_colour() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(80.0, 30.0));
        let mesh = gradient_mesh(rect, 7.0);

        let at = |y: f32| -> Vec<Color32> {
            mesh.vertices
                .iter()
                .filter(|v| (v.pos.y - y).abs() < 1e-3)
                .map(|v| v.color)
                .collect()
        };
        let top = at(rect.min.y);
        let bottom = at(rect.max.y);
        assert!(!top.is_empty() && !bottom.is_empty());
        assert!(top.iter().all(|c| *c == tokens::PRIMARY_TOP), "{top:?}");
        assert!(bottom.iter().all(|c| *c == tokens::PRIMARY_BOTTOM), "{bottom:?}");
        // Smoke: the first vertex is the centre, half way between the two.
        let middle = mesh.vertices[0].color;
        assert!(middle.g() < tokens::PRIMARY_TOP.g() && middle.g() > tokens::PRIMARY_BOTTOM.g());
        // Every vertex lies inside the rectangle; the corners are cut by the radius.
        assert!(mesh.vertices.iter().all(|v| rect.expand(1e-3).contains(v.pos)));
        assert!(
            !mesh
                .vertices
                .iter()
                .any(|v| v.pos.distance(rect.left_top()) < 1.0),
            "the sharp corner is rounded off"
        );
        // Colours never leave the two ends.
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.color.g() <= tokens::PRIMARY_TOP.g() && v.color.g() >= tokens::PRIMARY_BOTTOM.g())
        );
        // Every triangle refers to existing vertices.
        assert!(mesh.is_valid());
    }

    #[test]
    fn the_grid_has_a_line_every_24_px_and_follows_the_pan() {
        let rect = Rect::from_min_size(pos2(100.0, 50.0), vec2(240.0, 120.0));
        let lines = grid_lines(rect, vec2(0.0, 0.0));
        let vertical = lines.iter().filter(|[a, b]| a.x == b.x).count();
        let horizontal = lines.len() - vertical;
        // 240 / 24 and 120 / 24: the far edge is not drawn.
        assert_eq!((vertical, horizontal), (10, 5));
        assert!(lines.iter().all(|[a, b]| rect.contains(*a) && rect.contains(*b)));

        // A pan of 5 px shifts every line by 5; one of 29 px is the same as 5 (mod 24).
        let panned = grid_lines(rect, vec2(5.0, 0.0));
        let xs: Vec<f32> = panned
            .iter()
            .filter(|[a, b]| a.x == b.x)
            .map(|[a, _]| a.x)
            .collect();
        assert_eq!(xs[0], 105.0);
        assert_eq!(xs[1] - xs[0], GRID_STEP);
        assert_eq!(grid_lines(rect, vec2(29.0, 0.0)), panned);
        // A negative pan wraps the other way and still stays inside.
        assert!(
            grid_lines(rect, vec2(-5.0, -5.0))
                .iter()
                .all(|[a, _]| a.x >= 100.0 && a.y >= 50.0)
        );
        // Smoke: a bigger rect has more lines.
        let bigger = Rect::from_min_size(rect.min, vec2(480.0, 120.0));
        assert!(grid_lines(bigger, Vec2::ZERO).len() > lines.len());
    }

    #[test]
    fn the_page_shadow_is_two_layers_far_one_first() {
        let ctx = Context::default();
        let rect = Rect::from_min_size(pos2(40.0, 40.0), vec2(200.0, 260.0));
        let mut output = ctx.run_ui(RawInput::default(), |ui| page_shadow(ui.painter(), rect));
        output.textures_delta.clear();
        let shadows: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                Shape::Rect(shape) => Some((shape.rect, shape.blur_width, shape.fill)),
                _ => None,
            })
            .collect();

        assert_eq!(shadows.len(), 2);
        assert_eq!(shadows[0].1, 70.0);
        assert_eq!(shadows[1].1, 6.0);
        // Each is the page's rectangle moved down by its offset.
        assert_eq!(shadows[0].0, rect.translate(vec2(0.0, 28.0)));
        assert_eq!(shadows[1].0, rect.translate(vec2(0.0, 2.0)));
        assert!(shadows[0].2.a() > shadows[1].2.a() - 0x20);
        // The far shadow reaches furthest: half its blur around it, 28 lower.
        assert_eq!(
            page_shadow_reach(rect),
            Rect::from_min_max(pos2(5.0, 33.0), pos2(275.0, 363.0))
        );
    }

    #[test]
    fn frames_carry_the_tints_and_fade_with_their_opacity() {
        let popover = frosted_panel();
        assert_eq!(popover.fill, tokens::POPOVER_FILL);
        assert_eq!(popover.stroke.color, tokens::POPOVER_HAIRLINE);
        assert_eq!(popover.shadow.offset, [0, 18]);
        let card = floating_card();
        assert_eq!(card.fill, tokens::CARD_FILL);
        assert_eq!(card.corner_radius, CornerRadius::same(10));

        let half = frosted_panel_at(0.5);
        assert!(half.fill.a() < popover.fill.a());
        assert_eq!(frosted_panel_at(0.0).fill, Color32::TRANSPARENT);
        assert_eq!(frosted_panel_at(1.0).fill, popover.fill);
    }

    #[test]
    fn tooltips_take_the_light_shadow_and_panels_are_nearly_opaque() {
        let tooltip = frosted_tooltip_at(1.0);

        assert_eq!(tooltip.shadow, ON_PAGE_SHADOW);
        assert_eq!(tooltip.fill, tokens::TOOLTIP_FILL);
        assert_eq!(tooltip.fill.a(), 0xFF);
        assert_eq!(frosted_panel().shadow, POPOVER_SHADOW);
        // It fades with the rest while it opens.
        assert!(frosted_tooltip_at(0.5).shadow.color.a() < ON_PAGE_SHADOW.color.a());
        // The page or the inspector must not read through a panel: egui has no blur to hide it.
        assert!(tokens::POPOVER_FILL.a() >= 0xF0, "{}", tokens::POPOVER_FILL.a());
        assert!(tooltip.shadow.blur < POPOVER_SHADOW.blur);
    }
}
