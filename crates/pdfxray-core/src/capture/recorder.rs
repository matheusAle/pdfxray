//! The recording `Device` handed to the engine.

use hayro_interpret::font::{Glyph, GlyphRun};
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::{BlendMode, ClipPath, Device, DrawMode, DrawProps, Image, ImageDrawProps, SoftMask};
use kurbo::{Affine, BezPath, Shape};

use super::geom::{from_kurbo, union};
use super::ops::MARKER_TAG;
use crate::Rect;

/// What the engine reported while executing one operator.
#[derive(Debug, Clone)]
pub(super) enum Event {
    /// A path was filled or stroked.
    Path {
        /// Page-space bounding box (stroke includes half the line width).
        bbox: Rect,
    },
    /// A run of glyphs was shown (one event per fill/stroke pass).
    Glyphs {
        /// The glyphs of the run.
        glyphs: Vec<RunGlyph>,
    },
    /// An image was drawn.
    Image {
        /// Page-space bounding box.
        bbox: Rect,
        /// Pixel width as decoded.
        width: u32,
        /// Pixel height as decoded.
        height: u32,
    },
}

/// One glyph of a run.
#[derive(Debug, Clone)]
pub(super) struct RunGlyph {
    /// Maps glyph space (1000 units per em) to page space (the engine's glyph transform alone
    /// stops at user space; the CTM is applied here).
    pub transform: Affine,
    /// Unicode text the font maps the glyph to.
    pub unicode: Option<String>,
    /// Advance width in 1/1000 em, when the engine knows it.
    pub advance: Option<f64>,
    /// Bounding box of the outline in glyph space, when it has one.
    pub outline_bbox: Option<kurbo::Rect>,
}

/// Events grouped by the operator that caused them.
#[derive(Debug, Default, Clone)]
pub(super) struct Group {
    /// Draw events attributed to the operator.
    pub events: Vec<Event>,
    /// True when the engine reported anything at all (including clips and groups).
    pub touched: bool,
}

impl Group {
    /// Union of the page-space boxes of the path events.
    pub(super) fn path_box(&self) -> Option<Rect> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Path { bbox } => Some(*bbox),
                _ => None,
            })
            .reduce(union)
    }

    /// Page-space box and pixel size of the first image drawn.
    pub(super) fn image(&self) -> Option<(Rect, u32, u32)> {
        self.events.iter().find_map(|e| match e {
            Event::Image { bbox, width, height } => Some((*bbox, *width, *height)),
            _ => None,
        })
    }
}

/// Collects engine callbacks into per-operator [`Group`]s using the injected markers.
pub(super) struct Recorder {
    /// One group per operator of the instrumented stream.
    pub groups: Vec<Group>,
    /// Operators whose draw events are discarded (Form `XObject` invocations are captured by a
    /// separate run); `touched` is still recorded.
    discard: Vec<bool>,
    current: Option<usize>,
    /// Number of operators the engine reached (highest marker seen plus one).
    pub reached: usize,
    /// Draw events seen before the first marker.
    pub stray: usize,
}

impl Recorder {
    pub(super) fn new(op_count: usize, discard: Vec<bool>) -> Self {
        Self {
            groups: vec![Group::default(); op_count],
            discard,
            current: None,
            reached: 0,
            stray: 0,
        }
    }

    fn touch(&mut self) -> Option<&mut Group> {
        if let Some(i) = self.current {
            let group = self.groups.get_mut(i)?;
            group.touched = true;
            Some(group)
        } else {
            self.stray += 1;
            None
        }
    }

    fn record(&mut self, event: Event) {
        let discard = self
            .current
            .is_some_and(|i| self.discard.get(i).copied().unwrap_or(false));
        if let Some(group) = self.touch()
            && !discard
        {
            group.events.push(event);
        }
    }
}

/// Bounding box of `path` under `transform`; strokes grow by half the line width, scaled per
/// axis by how far the transform stretches a unit circle.
fn path_bbox(path: &BezPath, transform: Affine, stroke_width: Option<f64>) -> Option<Rect> {
    let mut moved = path.clone();
    moved.apply_affine(transform);
    let b = moved.bounding_box();
    let [a, bb, c, d, _, _] = transform.as_coeffs();
    let (dx, dy) = stroke_width.map_or((0.0, 0.0), |w| {
        let half = w.max(0.0) / 2.0;
        (half * a.hypot(c), half * bb.hypot(d))
    });
    from_kurbo(kurbo::Rect::new(b.x0 - dx, b.y0 - dy, b.x1 + dx, b.y1 + dy))
}

impl<'a> Device<'a> for Recorder {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, draw_mode: &DrawMode) {
        let stroke = match draw_mode {
            DrawMode::Stroke(s) | DrawMode::FillAndStroke(_, s) => Some(f64::from(s.line_width)),
            DrawMode::Fill(_) | DrawMode::Invisible => None,
        };
        match path_bbox(path, props.transform, stroke) {
            Some(bbox) => self.record(Event::Path { bbox }),
            None => {
                self.touch();
            }
        }
    }

    fn push_clip_path(&mut self, _: &ClipPath) {
        self.touch();
    }

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {
        self.touch();
    }

    fn draw_glyph_run(&mut self, glyph_run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        let glyphs = glyph_run
            .glyphs()
            .iter()
            .map(|g| {
                let unicode = g.as_unicode().map(|u| match u {
                    BfString::Char(c) => c.to_string(),
                    BfString::String(s) => s,
                });
                let (advance, outline_bbox) = match &**g {
                    Glyph::Outline(o) => {
                        let outline = o.outline();
                        let bbox = (!outline.elements().is_empty()).then(|| outline.bounding_box());
                        (o.advance_width().map(f64::from), bbox)
                    }
                    Glyph::Type3(_) => (None, None),
                };
                RunGlyph {
                    transform: props.transform * g.transform(),
                    unicode,
                    advance,
                    outline_bbox,
                }
            })
            .collect();
        self.record(Event::Glyphs { glyphs });
    }

    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let (width, height) = (image.width(), image.height());
        let pixels = kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        match from_kurbo(props.transform.transform_rect_bbox(pixels)) {
            Some(bbox) => self.record(Event::Image { bbox, width, height }),
            None => {
                self.touch();
            }
        }
    }

    fn pop_clip(&mut self) {
        self.touch();
    }

    fn pop_transparency_group(&mut self) {
        self.touch();
    }

    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>) {
        if tag == MARKER_TAG
            && let Some(i) = mcid.and_then(|m| usize::try_from(m).ok())
        {
            self.current = Some(i);
            self.reached = self.reached.max(i + 1);
        }
    }

    fn end_marked_content(&mut self) {}
}
