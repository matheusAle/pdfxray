//! Overlay shapes painted on top of the rasterised page, and the one compositor that paints them.
//!
//! Every shape goes through `vello_cpu`, the rasteriser hayro draws the page with, so a debug
//! outline, a tag box and a badge's ring are anti-aliased alike in one image.

use hayro::vello_cpu::color::{AlphaColor, Srgb};
use hayro::vello_cpu::kurbo::{
    self, BezPath, Circle, Point as KPoint, Rect as KRect, RoundedRect, Shape as _,
};
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use pdfxray_core::Rect;

use crate::annotate::text::{self, Font};
use crate::transform::{PixelPoint, PixelRect, Transform};

/// A colour and how strongly it is laid on: `rgba` (straight) times `opacity`.
///
/// The opacity is kept apart from the colour so that a faded colour is computed once, the same
/// way, by every painter: [`Ink::premultiplied`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    /// Straight RGBA.
    pub rgba: [u8; 4],
    /// 0 to 1, applied on top of the alpha of `rgba`.
    pub opacity: f32,
}

impl Ink {
    /// `rgba` at full strength.
    pub const fn new(rgba: [u8; 4]) -> Self {
        Self { rgba, opacity: 1.0 }
    }

    /// The same colour at `factor` of its strength.
    #[must_use]
    pub fn fade(self, factor: f32) -> Self {
        Self {
            opacity: self.opacity * factor,
            ..self
        }
    }

    /// Premultiplied RGBA8: each premultiplied channel times the opacity, rounded half up.
    pub fn premultiplied(self) -> [u8; 4] {
        let [r, g, b, a] = self.rgba;
        let alpha = f32::from(a) / 255.0;
        let channel = |c: u8, by: f32| {
            let scaled = (f32::from(c) * by).clamp(0.0, 255.0);
            // In 0..=255 after the clamp.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let out = (scaled + 0.5) as u8;
            out
        };
        let premultiplied = [channel(r, alpha), channel(g, alpha), channel(b, alpha), a];
        let opacity = self.opacity.clamp(0.0, 1.0);
        premultiplied.map(|c| channel(c, opacity))
    }

    fn color(self) -> AlphaColor<Srgb> {
        let [r, g, b, a] = self.rgba;
        AlphaColor::from_rgba8(r, g, b, a).multiply_alpha(self.opacity.clamp(0.0, 1.0))
    }
}

/// Which side of a shape's edge its outline is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Entirely inside the shape.
    Inside,
    /// Centred on the edge.
    Centre,
    /// Entirely outside the shape.
    Outside,
}

/// An outline or a line: width in output units, ink, and for a closed shape which side of its
/// edge it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// Width in output units.
    pub width: f32,
    /// Colour.
    pub ink: Ink,
    /// Side of the edge, for closed shapes; lines are centred whatever it says.
    pub edge: Edge,
}

impl Line {
    /// A line of `width` in `ink`, centred.
    pub const fn new(width: f32, ink: Ink) -> Self {
        Self {
            width,
            ink,
            edge: Edge::Centre,
        }
    }

    /// The same line on `edge` of a shape.
    #[must_use]
    pub const fn on(self, edge: Edge) -> Self {
        Self { edge, ..self }
    }
}

/// A shape drawn over the page bitmap, in output units (bitmap pixels when it is rasterised
/// here): a debug outline placed with [`Overlay::outline`], or an annotation shape laid out by
/// [`crate::annotate::layout`] on a [`crate::annotate::Grid`].
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    /// A box: a debug outline, a tag box, the body of a chip or a note, a legend swatch.
    Frame {
        /// The box.
        rect: PixelRect,
        /// Corner radius.
        radius: f32,
        /// Interior; `None` leaves it untouched.
        fill: Option<Ink>,
        /// Outline; `None` draws none.
        stroke: Option<Line>,
    },
    /// One line of text, clipped to `rect`.
    Text {
        /// The area the layout gave the text, which it is clipped to.
        rect: PixelRect,
        /// The top-left corner of the line: the baseline is the font's ascent below it.
        origin: PixelPoint,
        /// The text.
        text: String,
        /// Face and size.
        font: Font,
        /// Colour.
        ink: Ink,
    },
    /// An open line through `points`.
    Polyline {
        /// At least two points.
        points: Vec<PixelPoint>,
        /// Width and ink.
        line: Line,
    },
    /// A dashed straight line.
    Dashed {
        /// Start.
        from: PixelPoint,
        /// End.
        to: PixelPoint,
        /// Width and ink.
        line: Line,
        /// Length of a dash.
        dash: f32,
        /// Length of a gap.
        gap: f32,
    },
    /// A numbered disc.
    Badge {
        /// Centre.
        center: PixelPoint,
        /// Radius.
        radius: f32,
        /// Interior.
        fill: Ink,
        /// The ring around it, centred on the edge.
        ring: Line,
        /// The number.
        label: String,
        /// Face and size of the number.
        font: Font,
        /// Colour of the number.
        ink: Ink,
    },
}

/// Paints `overlays` over `pixmap`, source over.
pub(crate) fn paint(pixmap: &mut Pixmap, overlays: &[Overlay]) {
    if overlays.is_empty() {
        return;
    }
    let mut ctx = RenderContext::new(pixmap.width(), pixmap.height());
    for overlay in overlays {
        overlay.paint(&mut ctx);
    }
    ctx.flush();
    ctx.render_with(
        pixmap,
        &mut Resources::default(),
        RasterizerSettings {
            target_init: TargetInit::SrcOver,
            ..RasterizerSettings::default()
        },
    );
}

fn krect(r: PixelRect) -> KRect {
    KRect::new(r.x0, r.y0, r.x1, r.y1)
}

fn kpoint(p: PixelPoint) -> KPoint {
    KPoint::new(p.x, p.y)
}

/// Fills `shape` with `ink`.
fn fill(ctx: &mut RenderContext, shape: &impl kurbo::Shape, ink: Ink) {
    ctx.set_paint(ink.color());
    ctx.fill_path(&shape.to_path(0.1));
}

/// Strokes `path` with `line`, centred.
fn stroke(ctx: &mut RenderContext, path: &BezPath, line: Line) {
    if line.width <= 0.0 {
        return;
    }
    ctx.set_paint(line.ink.color());
    ctx.set_stroke(
        kurbo::Stroke::new(f64::from(line.width))
            .with_caps(kurbo::Cap::Butt)
            .with_join(kurbo::Join::Miter),
    );
    ctx.stroke_path(path);
}

/// The outline of `r`, its corners rounded by `radius`; a square-cornered box is the plain
/// rectangle, so a debug outline is the four edges and nothing else.
fn frame_path(r: KRect, radius: f64) -> BezPath {
    if radius > 0.0 {
        RoundedRect::from_rect(r, radius).to_path(0.1)
    } else {
        r.to_path(0.1)
    }
}

impl Overlay {
    /// The page-space `rect` drawn with `to_pixel`: outlined `width` output units wide in
    /// `stroke`, centred on its edge (zero draws no outline), and filled with `fill`. Colours are
    /// straight RGBA. The debug overlays (page boxes, highlights, the x-ray) are made of these.
    pub fn outline(
        to_pixel: &Transform,
        rect: Rect,
        stroke: [u8; 4],
        fill: Option<[u8; 4]>,
        width: f32,
    ) -> Self {
        Self::Frame {
            rect: to_pixel.rect_to_pixel(rect),
            radius: 0.0,
            fill: fill.map(Ink::new),
            stroke: (width > 0.0).then(|| Line::new(width, Ink::new(stroke))),
        }
    }

    fn paint(&self, ctx: &mut RenderContext) {
        match self {
            Self::Frame {
                rect,
                radius,
                fill: interior,
                stroke: outline,
            } => {
                let r = krect(*rect);
                let radius = f64::from(*radius);
                if let Some(interior) = interior {
                    fill(ctx, &frame_path(r, radius), *interior);
                }
                if let Some(line) = outline {
                    // Outside or inside the edge: the centre line moves by half the width.
                    let half = f64::from(line.width) / 2.0;
                    let by = match line.edge {
                        Edge::Inside => -half,
                        Edge::Centre => 0.0,
                        Edge::Outside => half,
                    };
                    let corner = if radius > 0.0 { (radius + by).max(0.0) } else { 0.0 };
                    stroke(ctx, &frame_path(r.inflate(by, by), corner), *line);
                }
            }
            Self::Text {
                rect,
                origin,
                text,
                font,
                ink,
            } => {
                ctx.push_clip_layer(&krect(*rect).to_path(0.1));
                ctx.set_paint(ink.color());
                ctx.fill_path(&text::outlines(text, *font, kpoint(*origin)));
                ctx.pop_layer();
            }
            Self::Polyline { points, line } => {
                let mut path = BezPath::new();
                for (n, p) in points.iter().enumerate() {
                    if n == 0 {
                        path.move_to(kpoint(*p));
                    } else {
                        path.line_to(kpoint(*p));
                    }
                }
                stroke(ctx, &path, *line);
            }
            Self::Dashed {
                from,
                to,
                line,
                dash,
                gap,
            } => stroke(ctx, &dashes(*from, *to, *dash, *gap), *line),
            Self::Badge {
                center,
                radius,
                fill: interior,
                ring,
                label,
                font,
                ink,
            } => {
                let disc = Circle::new(kpoint(*center), f64::from(*radius));
                fill(ctx, &disc, *interior);
                stroke(ctx, &disc.to_path(0.1), *ring);
                let (w, h) = text::measure(label, *font, 0.0);
                let origin = KPoint::new(center.x - f64::from(w) / 2.0, center.y - f64::from(h) / 2.0);
                ctx.set_paint(ink.color());
                ctx.fill_path(&text::outlines(label, *font, origin));
            }
        }
    }
}

/// The dashes of the line from `from` to `to`: `dash` drawn, `gap` left, from the start.
fn dashes(from: PixelPoint, to: PixelPoint, dash: f32, gap: f32) -> BezPath {
    let (a, b) = (kpoint(from), kpoint(to));
    let length = a.distance(b);
    let mut path = BezPath::new();
    let (dash, period) = (f64::from(dash.max(0.1)), f64::from(dash.max(0.1) + gap.max(0.0)));
    let mut at = 0.0;
    while at < length {
        path.move_to(a.lerp(b, at / length));
        path.line_to(a.lerp(b, (at + dash).min(length) / length));
        at += period;
    }
    path
}

#[cfg(test)]
mod tests {
    use kurbo::Affine;
    use pdfxray_core::numeric::usize_from_u32;
    use pretty_assertions::assert_eq;

    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    /// A white `w` x `h` bitmap with `overlays` painted over it, as straight RGBA rows.
    fn painted(w: u16, h: u16, overlays: &[Overlay]) -> Vec<u8> {
        let mut pixmap = Pixmap::new(w, h);
        pixmap.data_as_u8_slice_mut().fill(255);
        paint(&mut pixmap, overlays);
        pixmap.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha)
    }

    fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let at = usize_from_u32((y * w + x) * 4);
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    }

    fn rect(rect: Rect, fill: Option<[u8; 4]>, width: f32) -> Overlay {
        Overlay::outline(&Transform::new(Affine::IDENTITY).unwrap(), rect, RED, fill, width)
    }

    #[test]
    fn fill_covers_exactly_the_rectangle() {
        let img = painted(6, 6, &[rect(Rect::new(1.0, 2.0, 3.0, 4.0), Some(RED), 0.0)]);

        let red: Vec<(u32, u32)> = (0..6)
            .flat_map(|y| (0..6).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&img, 6, x, y) == RED)
            .collect();
        assert_eq!(red, vec![(1, 2), (2, 2), (1, 3), (2, 3)]);
        // Smoke: everything else is still white.
        assert_eq!(pixel(&img, 6, 0, 0), WHITE);
    }

    #[test]
    fn stroke_is_centred_on_the_edge_and_leaves_the_interior() {
        let img = painted(10, 10, &[rect(Rect::new(2.0, 2.0, 8.0, 8.0), None, 2.0)]);

        assert_eq!(
            [
                pixel(&img, 10, 1, 5),
                pixel(&img, 10, 2, 5),
                pixel(&img, 10, 5, 8),
                pixel(&img, 10, 5, 5),
                pixel(&img, 10, 0, 5),
            ],
            [RED, RED, RED, WHITE, WHITE]
        );
    }

    #[test]
    fn half_covered_pixel_blends_half() {
        let black = Some([0, 0, 0, 255]);
        let img = painted(2, 1, &[rect(Rect::new(0.0, 0.0, 0.5, 1.0), black, 0.0)]);

        let [r, g, b, a] = pixel(&img, 2, 0, 0);
        assert!((126..=129).contains(&r) && r == g && g == b, "{r} {g} {b}");
        assert_eq!(a, 255);
        assert_eq!(pixel(&img, 2, 1, 0), WHITE);
    }

    #[test]
    fn rectangles_outside_the_bitmap_are_ignored() {
        let img = painted(
            3,
            3,
            &[rect(Rect::new(-50.0, -50.0, -10.0, -10.0), Some(RED), 3.0)],
        );

        assert_eq!(img, WHITE.repeat(9));
    }

    #[test]
    fn an_outside_outline_leaves_the_frame_and_an_inside_one_stays_in_it() {
        let frame = |edge| Overlay::Frame {
            rect: PixelRect {
                x0: 4.0,
                y0: 4.0,
                x1: 12.0,
                y1: 12.0,
            },
            radius: 0.0,
            fill: None,
            stroke: Some(Line::new(2.0, Ink::new(RED)).on(edge)),
        };

        let outside = painted(16, 16, &[frame(Edge::Outside)]);
        let inside = painted(16, 16, &[frame(Edge::Inside)]);

        assert_eq!(
            [pixel(&outside, 16, 3, 8), pixel(&outside, 16, 4, 8)],
            [RED, WHITE]
        );
        assert_eq!([pixel(&inside, 16, 3, 8), pixel(&inside, 16, 4, 8)], [WHITE, RED]);
    }

    #[test]
    fn text_is_clipped_to_its_rect_and_a_badge_paints_its_number() {
        let font = Font::new(text::Face::Bold, 40.0);
        let text = |x1: f64| Overlay::Text {
            rect: PixelRect {
                x0: 0.0,
                y0: 0.0,
                x1,
                y1: 60.0,
            },
            origin: PixelPoint::new(2.0, 2.0),
            text: "HH".to_owned(),
            font,
            ink: Ink::new(RED),
        };
        let reds = |img: &[u8], w: u32, xs: std::ops::Range<u32>| {
            xs.flat_map(|x| (0..60).map(move |y| (x, y)))
                .filter(|&(x, y)| pixel(img, w, x, y) == RED)
                .count()
        };

        let whole = painted(80, 60, &[text(80.0)]);
        let clipped = painted(80, 60, &[text(20.0)]);

        assert!(reds(&whole, 80, 30..80) > 0);
        assert_eq!(reds(&clipped, 80, 21..80), 0);
        // Smoke: the clipped text still paints left of the clip.
        assert!(reds(&clipped, 80, 0..20) > 0);
        let badge = painted(
            40,
            40,
            &[Overlay::Badge {
                center: PixelPoint::new(20.0, 20.0),
                radius: 15.0,
                fill: Ink::new(RED),
                ring: Line::new(1.5, Ink::new(WHITE)),
                label: "8".to_owned(),
                font: Font::new(text::Face::Bold, 20.0),
                ink: Ink::new([0, 0, 0, 255]),
            }],
        );
        assert_eq!(pixel(&badge, 40, 20, 8), RED);
        assert_ne!(pixel(&badge, 40, 20, 20), RED);
    }

    #[test]
    fn ink_fades_like_a_premultiplied_colour() {
        let ink = Ink::new([0xB4, 0x8C, 0xFF, 255]);

        assert_eq!(ink.premultiplied(), [0xB4, 0x8C, 0xFF, 255]);
        assert_eq!(
            ink.fade(0.15).premultiplied(),
            [27, 21, 38, 38],
            "0xB4 * 0.15 = 27, 0xFF * 0.15 = 38.25"
        );
        // Smoke: fading twice multiplies.
        assert!((ink.fade(0.5).fade(0.5).opacity - 0.25).abs() < f32::EPSILON);
    }
}
