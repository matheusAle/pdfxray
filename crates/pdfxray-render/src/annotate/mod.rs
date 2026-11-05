//! The annotation layer: tag boxes, the chips that name them, reading-order badges and the path
//! between them, laid out once for the app and the exported image alike.
//!
//! [`layout`] takes what to annotate ([`Scene`]s) and where ([`Grid`]) and returns a resolved
//! shape list: every rect, label position and nudge already decided, with the [`NodeId`] each
//! shape stands for. The app draws that list with its own painter and binds its buttons to the
//! rects it was given; [`crate::render_page`] rasterises the same list. Text is measured here
//! only (the private `text` module), so a chip is nudged to the same place in both.
//!
//! A [`Scene`] is generic: a box per element in any colour, with an optional chip saying anything,
//! and an optional numbered path. [`tag_scene`] builds the one the canvas shows (role colours,
//! role chips, the reading order); [`change_scenes`] builds a diff's (status colours, the role
//! as chip text, a [`Legend`]), both sides laid out together so their chips clear each other.

mod changes;
mod place;
mod tags;
pub(crate) mod text;

use pdfxray_core::{Document, HeaderScope, NodeId, PageIx, Rect, ScopeSource, TableScope};

use crate::RenderError;

use crate::overlay::{Edge, Ink, Line, Overlay};
use crate::transform::{PixelPoint, PixelRect, Transform};
use place::{badge_spot, clear_beside, inside};

pub use changes::{ChangeShow, change_legend, change_scenes, status_name, status_rgba};
pub use place::clear_of;
pub use tags::{
    ARTIFACT_CHIP, ArtifactBox, TagShow, TagView, UNFOCUSED_BOX, UNFOCUSED_PATH, artifact_boxes,
    backtrack_note, chip_text, tag_scene,
};
pub use text::{Face, Font, measure};

/// The chip, badge and note text size, in annotation units (the app's points).
const LABEL_SIZE: f32 = 11.0;
/// The note text size, mono.
const NOTE_SIZE: f32 = 10.0;
/// Radius of a reading-order badge.
pub const BADGE_RADIUS: f32 = 9.0;
/// Space between a box's edge and its badge.
const BADGE_GAP: f32 = 2.0;
/// Width of the reading-order path.
const PATH_WIDTH: f32 = 2.0;
/// Dash and gap of the segment into a detour.
const DETOUR_DASH: (f32, f32) = (5.0, 4.0);
/// Corner radius of a chip or a note.
pub const CHIP_RADIUS: f32 = 4.0;
/// Space a chip leaves around its name: left and right together, top and bottom together.
const CHIP_PAD: (f32, f32) = (8.0, 3.0);
/// How far left of its box a chip at the top-left corner sits.
const CHIP_OFFSET: f32 = 3.0;
/// Space a note leaves around its text.
const NOTE_PAD: (f32, f32) = (14.0, 8.0);
/// Side of the arrow in a header chip.
const ARROW: f32 = 9.0;
/// Stroke of that arrow.
const ARROW_STROKE: f32 = 1.7;
/// Space between two parts of a header chip: its name, the arrow and a mark.
const TAIL_GAP: f32 = 3.0;
/// Strength of the arrow of a header whose direction is inferred.
const INFERRED_ARROW: f32 = 0.6;
/// Width of the amber ring of a header whose scope conflicts.
pub const CONFLICT_RING: f32 = 1.5;
/// Size of the `!` of a conflict.
const MARK_SIZE: f32 = 9.0;

/// The colours of the annotation layer other than the boxes': the design's tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The reading-order path and its badges.
    pub accent: [u8; 4],
    /// A backtrack: its segment, its badge and its note; a conflicting header scope.
    pub warn: [u8; 4],
    /// The ground under the labels: chip text, the ring around chips and badges, note fill.
    pub ground: [u8; 4],
    /// The number on a badge.
    pub on_accent: [u8; 4],
    /// Text on the ground: the legend's labels.
    pub text: [u8; 4],
}

impl Palette {
    /// The design's colours (`13-foundations`), the app's tokens.
    pub const DESIGN: Self = Self {
        accent: [0x3D, 0xD6, 0xF5, 255],
        warn: [0xFB, 0xBF, 0x24, 255],
        ground: [0x0B, 0x0D, 0x10, 255],
        on_accent: [0x05, 0x21, 0x2A, 255],
        text: [0xE7, 0xEA, 0xEE, 255],
    };
}

/// Where annotations go: the mapping from page space to output units, and the output's units.
///
/// Output units are the app's screen points, or the pixels of an exported bitmap. Sizes of the
/// annotation layer (a chip's text, a badge's radius) are in *annotation units*, `unit` output
/// units each, so a chip is as big on the screen whatever the zoom, and an export can draw them
/// as big as it likes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Page space to output units.
    pub to_out: Transform,
    /// Output units per annotation unit.
    pub unit: f32,
    /// Output units per device pixel: text rows are whole device pixels tall.
    pub pixel: f32,
    /// Where annotations may go, in output units: a note is kept inside it.
    pub bounds: PixelRect,
}

impl Grid {
    /// The grid of a bitmap of `width` x `height` pixels drawn with `page_to_pixel`, with
    /// annotations `unit` pixels per annotation unit.
    pub fn bitmap(page_to_pixel: Transform, width: u32, height: u32, unit: f32) -> Self {
        Self {
            to_out: page_to_pixel,
            unit,
            pixel: 1.0,
            bounds: PixelRect {
                x0: 0.0,
                y0: 0.0,
                x1: f64::from(width),
                y1: f64::from(height),
            },
        }
    }

    /// The grid of page `page` of `doc` drawn at `scale` on its own bitmap, annotations `unit`
    /// pixels per annotation unit: what an export of one page lays its annotations out on.
    pub fn page(doc: &Document, page: PageIx, scale: f32, unit: f32) -> Result<Self, RenderError> {
        let (width, height, to_pixel) = crate::page_geometry(doc, page, scale)?;
        Ok(Self::bitmap(to_pixel, width, height, unit))
    }

    fn u(&self, v: f32) -> f32 {
        v * self.unit
    }

    fn ud(&self, v: f32) -> f64 {
        f64::from(v * self.unit)
    }

    fn font(&self, face: Face, size: f32) -> Font {
        Font::new(face, self.u(size))
    }

    fn rect(&self, rect: Rect) -> PixelRect {
        self.to_out.rect_to_pixel(rect)
    }
}

/// What a shape stands for, so the app can bind interaction to it and animate the path.
#[derive(Debug, Clone, PartialEq)]
pub enum Anchor {
    /// Nothing to bind: a box, a legend.
    None,
    /// The box of an element.
    Box(NodeId),
    /// The name on a chip naming an element: the [`Overlay::Text`] whose rect is the whole
    /// chip, the button.
    Chip(NodeId),
    /// The rest of that chip: its body, a header's arrow and marks, a conflict's ring.
    Part(NodeId),
    /// The badge of step `step` (zero-based) of the path, numbering `node`.
    Badge {
        /// Zero-based position on the path.
        step: usize,
        /// The element it numbers.
        node: NodeId,
    },
    /// The segment of the path into step `step`, and the note on it.
    Segment(usize),
}

/// One resolved shape and what it stands for.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The shape, in output units.
    pub shape: Overlay,
    /// What it stands for.
    pub anchor: Anchor,
    /// The index in the laid-out scenes of the scene it belongs to; `None` for the legend. Both
    /// sides of a comparison are laid out together and the ids of their elements can be equal
    /// (object numbers are reused from file to file), so this is what says which side a shape is.
    pub scene: Option<usize>,
}

/// The overlays of `annotations`, in order: what [`crate::RenderSpec::overlays`] rasterises.
pub fn overlays(annotations: &[Annotation]) -> Vec<Overlay> {
    annotations.iter().map(|a| a.shape.clone()).collect()
}

/// How a box is outlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A faint fill and a 1.5-unit outline: an element.
    Solid,
    /// A 1-unit outline at 55 %: a container.
    Container,
    /// A 1-unit outline: an artifact sequence.
    Thin,
}

/// Which corner of a box its chip is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    /// Left of the box, level with its top edge.
    TopLeft,
    /// Above the box, level with its right edge.
    TopRight,
}

/// A chip naming a box.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    /// What it says.
    pub text: String,
    /// Where it sits.
    pub corner: Corner,
    /// The scope of a table header cell: an arrow and marks after the name, an outlined chip
    /// when the scope is inferred and an amber ring when it conflicts.
    pub scope: Option<HeaderScope>,
}

/// An element's box: where, in what colour, how strongly, and its chip.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// The element.
    pub node: NodeId,
    /// Its box, page space.
    pub rect: Rect,
    /// The colour of the box and the chip: a role, a diff status.
    pub color: [u8; 4],
    /// How the box is outlined.
    pub look: Look,
    /// Whether the box is drawn; a mark can have a chip without a box.
    pub outlined: bool,
    /// How strongly the box is drawn, 0 to 1.
    pub strength: f32,
    /// The chip, if it has one.
    pub chip: Option<Chip>,
}

/// One stop of a numbered path.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The element.
    pub node: NodeId,
    /// Its box, page space: the badge goes beside it.
    pub rect: Rect,
    /// The number on the badge.
    pub label: String,
    /// The path goes back up into this step: the segment and badge take the warning colour.
    pub backtrack: bool,
    /// The segment into this step is dashed.
    pub dashed: bool,
    /// A note on the segment into this step.
    pub note: Option<String>,
    /// How strongly the segment into this step is drawn, 0 to 1.
    pub strength: f32,
}

/// What to annotate on one page (or one side of a comparison).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// The boxes, in drawing order.
    pub marks: Vec<Mark>,
    /// The numbered path, if any.
    pub path: Vec<Step>,
}

impl Scene {
    /// The same scene with every box moved by `to`: one side's page space onto another's, so
    /// both sides of a comparison are laid out on one [`Grid`].
    #[must_use]
    pub fn mapped(mut self, to: impl Fn(Rect) -> Rect) -> Self {
        for mark in &mut self.marks {
            mark.rect = to(mark.rect);
        }
        for step in &mut self.path {
            step.rect = to(step.rect);
        }
        self
    }
}

/// A key drawn as its own block at the top-left corner of the bounds: what each colour means.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Legend {
    /// The rows: a swatch and what it means.
    pub rows: Vec<([u8; 4], String)>,
}

/// Lays `scenes` out on `grid` in `palette`: every box, then every chip, the path with its notes,
/// the badges and the legend, in that drawing order. Chips are placed in scene order, each moved
/// down clear of those before; badges then take a spot clear of the chips, and notes clear of
/// both.
pub fn layout(grid: &Grid, palette: &Palette, scenes: &[Scene], legend: Option<&Legend>) -> Vec<Annotation> {
    let mut placed: Vec<PixelRect> = Vec::new();
    let mut out = Vec::new();
    for (index, scene) in scenes.iter().enumerate() {
        let from = out.len();
        for mark in scene.marks.iter().filter(|m| m.outlined) {
            out.push(outline(grid, mark));
        }
        of_scene(&mut out[from..], index);
    }
    for (index, scene) in scenes.iter().enumerate() {
        let from = out.len();
        for mark in &scene.marks {
            if let Some(chip) = &mark.chip {
                chip_shapes(grid, palette, mark, chip, &mut placed, &mut out);
            }
        }
        of_scene(&mut out[from..], index);
    }
    let mut badges = Vec::new();
    for (index, scene) in scenes.iter().enumerate() {
        let (from, badges_from) = (out.len(), badges.len());
        let points: Vec<PixelPoint> = scene
            .path
            .iter()
            .map(|step| {
                let at = badge_spot(grid, grid.rect(step.rect), &placed);
                placed.push(PixelRect::around(at, grid.ud(BADGE_RADIUS)));
                at
            })
            .collect();
        for (n, pair) in points.windows(2).enumerate() {
            let step = &scene.path[n + 1];
            segment(
                grid,
                palette,
                step,
                n + 1,
                (pair[0], pair[1]),
                &mut placed,
                &mut out,
            );
        }
        for (n, (step, center)) in scene.path.iter().zip(points).enumerate() {
            let fill = if step.backtrack {
                palette.warn
            } else {
                palette.accent
            };
            badges.push(Annotation {
                shape: Overlay::Badge {
                    center,
                    radius: grid.u(BADGE_RADIUS),
                    fill: Ink::new(fill),
                    ring: Line::new(grid.u(1.5), Ink::new(palette.ground)),
                    label: step.label.clone(),
                    font: grid.font(Face::Bold, LABEL_SIZE),
                    ink: Ink::new(palette.on_accent),
                },
                anchor: Anchor::Badge {
                    step: n,
                    node: step.node.clone(),
                },
                scene: None,
            });
        }
        of_scene(&mut out[from..], index);
        of_scene(&mut badges[badges_from..], index);
    }
    out.extend(badges);
    if let Some(legend) = legend {
        legend_shapes(grid, palette, legend, &mut out);
    }
    out
}

/// Marks `shapes` as laid out for scene `index`.
fn of_scene(shapes: &mut [Annotation], index: usize) {
    for shape in shapes {
        shape.scene = Some(index);
    }
}

/// The box of `mark`.
fn outline(grid: &Grid, mark: &Mark) -> Annotation {
    let ink = Ink::new(mark.color).fade(mark.strength);
    let (fill, stroke) = match mark.look {
        Look::Solid => (Some(ink.fade(0.06)), Line::new(grid.u(1.5), ink)),
        Look::Container => (None, Line::new(grid.u(1.0), ink.fade(0.55))),
        Look::Thin => (None, Line::new(grid.u(1.0), ink)),
    };
    Annotation {
        shape: Overlay::Frame {
            rect: grid.rect(mark.rect),
            radius: 0.0,
            fill,
            stroke: Some(stroke.on(Edge::Outside)),
        },
        anchor: Anchor::Box(mark.node.clone()),
        scene: None,
    }
}

/// The width the parts after a header chip's name take: the arrow and the marks.
fn tail_width(scope: &HeaderScope) -> f32 {
    let marks = u8::from(scope.source == ScopeSource::Conflict) + u8::from(!scope.named_by.is_empty());
    TAIL_GAP + ARROW + f32::from(marks) * (TAIL_GAP + ARROW)
}

/// Places the chip of `mark` clear of `placed`, records it there and pushes its shapes: the body,
/// the name, a header's arrow and marks, and a conflict's ring.
fn chip_shapes(
    grid: &Grid,
    palette: &Palette,
    mark: &Mark,
    chip: &Chip,
    placed: &mut Vec<PixelRect>,
    out: &mut Vec<Annotation>,
) {
    let part = Anchor::Part(mark.node.clone());
    let font = grid.font(Face::Bold, LABEL_SIZE);
    let (name_w, name_h) = text::measure(&chip.text, font, grid.pixel);
    let tail = chip.scope.as_ref().map_or(0.0, tail_width);
    let size = (
        f64::from(name_w + grid.u(CHIP_PAD.0 + tail)),
        f64::from(name_h + grid.u(CHIP_PAD.1)),
    );
    let rect = grid.rect(mark.rect);
    let wanted = match chip.corner {
        Corner::TopLeft => PixelRect::from_min_size(
            PixelPoint::new(rect.x0 - size.0 - grid.ud(CHIP_OFFSET), rect.y0),
            size,
        ),
        Corner::TopRight => PixelRect::from_min_size(
            PixelPoint::new(rect.x1 - size.0, rect.y0 - size.1 - grid.ud(1.0)),
            size,
        ),
    };
    let at = clear_of(placed, wanted, grid.ud(1.0));
    placed.push(at);
    // An inferred scope is an outlined chip: the colour on the page.
    let outlined = chip
        .scope
        .as_ref()
        .is_some_and(|s| s.source == ScopeSource::Inferred);
    let fill = Ink::new(mark.color);
    let ink = if outlined { fill } else { Ink::new(palette.ground) };
    let body = if outlined {
        (None, Line::new(grid.u(1.0), fill).on(Edge::Inside))
    } else {
        (
            Some(fill),
            Line::new(grid.u(1.0), Ink::new(palette.ground)).on(Edge::Outside),
        )
    };
    out.push(Annotation {
        shape: Overlay::Frame {
            rect: at,
            radius: grid.u(CHIP_RADIUS),
            fill: body.0,
            stroke: Some(body.1),
        },
        anchor: part.clone(),
        scene: None,
    });
    out.push(Annotation {
        shape: Overlay::Text {
            rect: at,
            origin: PixelPoint::new(
                at.x0 + grid.ud(CHIP_PAD.0 / 2.0),
                at.y0 + grid.ud(CHIP_PAD.1 / 2.0),
            ),
            text: chip.text.clone(),
            font,
            ink,
        },
        anchor: Anchor::Chip(mark.node.clone()),
        scene: None,
    });
    let mut push = |shape| {
        out.push(Annotation {
            shape,
            anchor: part.clone(),
            scene: None,
        });
    };
    if let Some(scope) = &chip.scope {
        let x = at.x0 + grid.ud(CHIP_PAD.0 / 2.0) + f64::from(name_w);
        for shape in tail_shapes(grid, palette, x, at.center().y, scope, ink) {
            push(shape);
        }
        if scope.source == ScopeSource::Conflict {
            push(Overlay::Frame {
                rect: at,
                radius: grid.u(CHIP_RADIUS),
                fill: None,
                stroke: Some(Line::new(grid.u(CONFLICT_RING), Ink::new(palette.warn)).on(Edge::Outside)),
            });
        }
    }
}

/// The arrow and the marks of a header chip whose name ends at `x`, centred on `y`, in `ink`.
fn tail_shapes(
    grid: &Grid,
    palette: &Palette,
    x: f64,
    y: f64,
    scope: &HeaderScope,
    ink: Ink,
) -> Vec<Overlay> {
    let arrow_ink = if scope.source == ScopeSource::Inferred {
        ink.fade(INFERRED_ARROW)
    } else {
        ink
    };
    let side = grid.ud(ARROW);
    let mut left = x + grid.ud(TAIL_GAP);
    let mut shapes = arrow(
        grid,
        PixelRect::from_min_size(PixelPoint::new(left, y - side / 2.0), (side, side)),
        scope.scope,
        arrow_ink,
    );
    left += side + grid.ud(TAIL_GAP);
    if scope.source == ScopeSource::Conflict {
        let mark = PixelRect::from_min_size(PixelPoint::new(left, y - side / 2.0), (side, side));
        shapes.push(Overlay::Frame {
            rect: mark,
            radius: grid.u(2.0),
            fill: Some(Ink::new(palette.warn)),
            stroke: None,
        });
        shapes.push(centred(
            grid,
            mark,
            "!",
            grid.font(Face::Bold, MARK_SIZE),
            Ink::new(palette.ground),
        ));
        left += side + grid.ud(TAIL_GAP);
    }
    if !scope.named_by.is_empty() {
        let slot = PixelRect::from_min_size(PixelPoint::new(left, y - side / 2.0), (side, side));
        shapes.push(centred(
            grid,
            slot,
            "#",
            grid.font(Face::MonoBold, NOTE_SIZE),
            ink,
        ));
    }
    shapes
}

/// `text` centred on the centre of `slot`.
fn centred(grid: &Grid, slot: PixelRect, text: &str, font: Font, ink: Ink) -> Overlay {
    let (w, h) = text::measure(text, font, grid.pixel);
    let c = slot.center();
    let origin = PixelPoint::new(c.x - f64::from(w) / 2.0, c.y - f64::from(h) / 2.0);
    // Clipped to the slot and the line, whichever reaches further: a mark is never cut.
    let line = PixelRect::from_min_size(origin, (f64::from(w), f64::from(h)));
    Overlay::Text {
        rect: PixelRect {
            x0: slot.x0.min(line.x0),
            y0: slot.y0.min(line.y0),
            x1: slot.x1.max(line.x1),
            y1: slot.y1.max(line.y1),
        },
        origin,
        text: text.to_owned(),
        font,
        ink,
    }
}

/// An arrow in `rect`: → for `Row`, ↓ for `Column`, and for `Both` one to the right and one down
/// from the top-left corner.
fn arrow(grid: &Grid, rect: PixelRect, scope: TableScope, ink: Ink) -> Vec<Overlay> {
    let line = Line::new(grid.u(ARROW_STROKE), ink);
    let head = grid.ud(ARROW) * 0.38;
    let inset = grid.ud(ARROW_STROKE) / 2.0;
    let p = PixelPoint::new;
    let right = |from: PixelPoint, to: PixelPoint| {
        [
            Overlay::Polyline {
                points: vec![from, to],
                line,
            },
            Overlay::Polyline {
                points: vec![p(to.x - head, to.y - head), to, p(to.x - head, to.y + head)],
                line,
            },
        ]
    };
    let down = |from: PixelPoint, to: PixelPoint| {
        [
            Overlay::Polyline {
                points: vec![from, to],
                line,
            },
            Overlay::Polyline {
                points: vec![p(to.x - head, to.y - head), to, p(to.x + head, to.y - head)],
                line,
            },
        ]
    };
    let c = rect.center();
    match scope {
        TableScope::Row => right(p(rect.x0 + inset, c.y), p(rect.x1 - inset, c.y)).to_vec(),
        TableScope::Column => down(p(c.x, rect.y0 + inset), p(c.x, rect.y1 - inset)).to_vec(),
        TableScope::Both => {
            let corner = p(rect.x0 + inset, rect.y0 + inset);
            let mut shapes = right(corner, p(rect.x1 - inset, corner.y)).to_vec();
            shapes.extend(down(corner, p(corner.x, rect.y1 - inset)));
            shapes
        }
    }
}

/// The segment from `from` to `to` into `step` (number `at` on the path), and its note.
#[allow(clippy::too_many_arguments)]
fn segment(
    grid: &Grid,
    palette: &Palette,
    step: &Step,
    at: usize,
    (from, to): (PixelPoint, PixelPoint),
    placed: &mut Vec<PixelRect>,
    out: &mut Vec<Annotation>,
) {
    let color = if step.backtrack {
        palette.warn
    } else {
        palette.accent
    };
    let line = Line::new(grid.u(PATH_WIDTH), Ink::new(color).fade(step.strength));
    let shape = if step.dashed {
        Overlay::Dashed {
            from,
            to,
            line,
            dash: grid.u(DETOUR_DASH.0),
            gap: grid.u(DETOUR_DASH.1),
        }
    } else {
        Overlay::Polyline {
            points: vec![from, to],
            line,
        }
    };
    out.push(Annotation {
        shape,
        anchor: Anchor::Segment(at),
        scene: None,
    });
    if let Some(note) = &step.note {
        for shape in note_shapes(grid, palette, placed, (from, to), note) {
            out.push(Annotation {
                shape,
                anchor: Anchor::Segment(at),
                scene: None,
            });
        }
    }
}

/// The amber note on `segment`, beside it, clear of `placed` and inside the grid's bounds.
fn note_shapes(
    grid: &Grid,
    palette: &Palette,
    placed: &mut Vec<PixelRect>,
    segment: (PixelPoint, PixelPoint),
    text: &str,
) -> [Overlay; 2] {
    let font = grid.font(Face::Mono, NOTE_SIZE);
    let (w, h) = text::measure(text, font, grid.pixel);
    let size = (
        f64::from(w + grid.u(NOTE_PAD.0)),
        f64::from(h + grid.u(NOTE_PAD.1)),
    );
    let at = clear_beside(grid, segment.0, segment.1, size);
    let rect = inside(
        grid.bounds,
        clear_of(placed, PixelRect::centred(at, size), grid.ud(1.0)),
    );
    placed.push(rect);
    let warn = Ink::new(palette.warn);
    [
        Overlay::Frame {
            rect,
            radius: grid.u(CHIP_RADIUS),
            fill: Some(Ink::new(palette.ground).fade(0.92)),
            stroke: Some(Line::new(grid.u(1.0), warn.fade(0.6)).on(Edge::Inside)),
        },
        Overlay::Text {
            rect,
            origin: PixelPoint::new(
                rect.x0 + grid.ud(NOTE_PAD.0 / 2.0),
                rect.y0 + grid.ud(NOTE_PAD.1 / 2.0),
            ),
            text: text.to_owned(),
            font,
            ink: warn,
        },
    ]
}

/// The legend: a block at the top-left corner of the bounds, a swatch and a label per row.
/// The measured parts of a legend on a grid.
struct LegendMetrics {
    font: Font,
    pad: f64,
    swatch: f64,
    gap: f64,
    row_h: f64,
    rows: Vec<(f32, f32)>,
    size: (f64, f64),
}

impl LegendMetrics {
    fn of(grid: &Grid, legend: &Legend) -> Self {
        let font = grid.font(Face::Bold, LABEL_SIZE);
        let pad = grid.ud(8.0);
        let swatch = grid.ud(10.0);
        let rows: Vec<(f32, f32)> = legend
            .rows
            .iter()
            .map(|(_, label)| text::measure(label, font, grid.pixel))
            .collect();
        let row_h = rows.iter().map(|&(_, h)| f64::from(h)).fold(swatch, f64::max);
        let text_w = rows.iter().map(|&(w, _)| f64::from(w)).fold(0.0, f64::max);
        let gap = grid.ud(6.0);
        let n = f64::from(u32::try_from(legend.rows.len()).unwrap_or(u32::MAX));
        let size = (
            pad * 2.0 + swatch + gap + text_w,
            pad * 2.0 + row_h * n + gap * (n - 1.0),
        );
        Self {
            font,
            pad,
            swatch,
            gap,
            row_h,
            rows,
            size,
        }
    }
}

/// The room `legend` takes at the top-left corner of `grid`'s bounds, margins included: what an
/// export sets aside so the legend is a block of its own, clear of the page. Zero for no rows.
pub fn legend_extent(grid: &Grid, legend: &Legend) -> (f64, f64) {
    if legend.rows.is_empty() {
        return (0.0, 0.0);
    }
    let m = LegendMetrics::of(grid, legend);
    (m.size.0 + m.pad * 2.0, m.size.1 + m.pad * 2.0)
}

fn legend_shapes(grid: &Grid, palette: &Palette, legend: &Legend, out: &mut Vec<Annotation>) {
    if legend.rows.is_empty() {
        return;
    }
    let LegendMetrics {
        font,
        pad,
        swatch,
        gap,
        row_h,
        rows,
        size,
    } = LegendMetrics::of(grid, legend);
    let block = PixelRect::from_min_size(PixelPoint::new(grid.bounds.x0 + pad, grid.bounds.y0 + pad), size);
    let mut push = |shape| {
        out.push(Annotation {
            shape,
            anchor: Anchor::None,
            scene: None,
        });
    };
    push(Overlay::Frame {
        rect: block,
        radius: grid.u(CHIP_RADIUS),
        fill: Some(Ink::new(palette.ground).fade(0.92)),
        stroke: Some(Line::new(grid.u(1.0), Ink::new(palette.ground)).on(Edge::Outside)),
    });
    let mut y = block.y0 + pad;
    for ((color, label), &(w, h)) in legend.rows.iter().zip(&rows) {
        let x = block.x0 + pad;
        push(Overlay::Frame {
            rect: PixelRect::from_min_size(PixelPoint::new(x, y + (row_h - swatch) / 2.0), (swatch, swatch)),
            radius: grid.u(2.0),
            fill: Some(Ink::new(*color)),
            stroke: None,
        });
        let origin = PixelPoint::new(x + swatch + gap, y + (row_h - f64::from(h)) / 2.0);
        push(Overlay::Text {
            rect: PixelRect::from_min_size(origin, (f64::from(w), f64::from(h))),
            origin,
            text: label.clone(),
            font,
            ink: Ink::new(palette.text),
        });
        y += row_h + gap;
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use kurbo::Affine;
    use pdfxray_core::{HeaderPlace, RoleClass};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::xray::role_rgba;

    /// Page space as output units, y down, on a 600 x 400 output.
    fn grid() -> Grid {
        Grid {
            to_out: Transform::new(Affine::IDENTITY).unwrap(),
            unit: 1.0,
            pixel: 1.0,
            bounds: PixelRect {
                x0: 0.0,
                y0: 0.0,
                x1: 600.0,
                y1: 400.0,
            },
        }
    }

    fn mark(n: u32, rect: Rect, chip: Option<&str>) -> Mark {
        Mark {
            node: NodeId::Page(n),
            rect,
            color: role_rgba(RoleClass::Paragraph),
            look: Look::Solid,
            outlined: true,
            strength: 1.0,
            chip: chip.map(|text| Chip {
                text: text.to_owned(),
                corner: Corner::TopLeft,
                scope: None,
            }),
        }
    }

    fn step(n: u32, rect: Rect, backtrack: bool) -> Step {
        Step {
            node: NodeId::Page(n),
            rect,
            label: (n + 1).to_string(),
            backtrack,
            dashed: false,
            note: backtrack.then(|| "↰ goes back up".to_owned()),
            strength: 1.0,
        }
    }

    fn rects_of(annotations: &[Annotation], anchor: impl Fn(&Anchor) -> bool) -> Vec<PixelRect> {
        annotations
            .iter()
            .filter(|a| anchor(&a.anchor))
            .filter_map(|a| match &a.shape {
                Overlay::Text { rect, .. } => Some(*rect),
                Overlay::Badge { center, radius, .. } => Some(PixelRect::around(*center, f64::from(*radius))),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn boxes_go_under_chips_under_the_path_under_the_badges() {
        let scene = Scene {
            marks: vec![
                mark(0, Rect::new(100.0, 100.0, 200.0, 120.0), Some("P")),
                mark(1, Rect::new(100.0, 140.0, 200.0, 160.0), Some("H1")),
            ],
            path: vec![
                step(0, Rect::new(100.0, 100.0, 200.0, 120.0), false),
                step(1, Rect::new(100.0, 140.0, 200.0, 160.0), false),
            ],
        };

        let out = layout(&grid(), &Palette::DESIGN, &[scene], None);

        let kinds: Vec<&str> = out
            .iter()
            .map(|a| match a.anchor {
                Anchor::None => "none",
                Anchor::Box(_) => "box",
                Anchor::Chip(_) => "chip",
                Anchor::Part(_) => "part",
                Anchor::Segment(_) => "segment",
                Anchor::Badge { .. } => "badge",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "box", "box", "part", "chip", "part", "chip", "segment", "badge", "badge"
            ]
        );
        // Smoke: the chip's name is the button and carries its element.
        assert_eq!(
            out.iter()
                .filter_map(|a| match (&a.anchor, &a.shape) {
                    (Anchor::Chip(node), Overlay::Text { text, .. }) => Some((node.clone(), text.as_str())),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [(NodeId::Page(0), "P"), (NodeId::Page(1), "H1")]
        );
    }

    #[test]
    fn a_chip_left_of_its_box_moves_down_clear_of_the_one_above_and_no_badge_covers_either() {
        // Two boxes a hair apart: the second chip would sit on the first.
        let a = Rect::new(100.0, 100.0, 200.0, 110.0);
        let b = Rect::new(100.0, 112.0, 200.0, 122.0);
        let scene = Scene {
            marks: vec![mark(0, a, Some("P")), mark(1, b, Some("P"))],
            path: vec![step(0, a, false), step(1, b, true)],
        };

        let out = layout(&grid(), &Palette::DESIGN, &[scene], None);

        let chips = rects_of(&out, |a| matches!(a, Anchor::Chip(_)));
        let (w, h) = measure("P", Font::new(Face::Bold, LABEL_SIZE), 1.0);
        let first = PixelRect::from_min_size(
            PixelPoint::new(100.0 - f64::from(w + 8.0) - 3.0, 100.0),
            (f64::from(w + 8.0), f64::from(h + 3.0)),
        );
        // The second wants the top of its own box, which the first covers: one step down.
        let second = first.translate(0.0, 12.0);
        assert_eq!(chips, [first, second.translate(0.0, second.height() + 1.0)]);
        let discs = rects_of(&out, |a| matches!(a, Anchor::Badge { .. }));
        let notes = rects_of(&out, |a| matches!(a, Anchor::Segment(_)));
        assert_eq!((discs.len(), notes.len()), (2, 1));
        for disc in discs.iter().chain(&notes) {
            assert!(
                chips.iter().all(|chip| !chip.intersects(disc)),
                "{disc:?} on {chips:?}"
            );
        }
        // Smoke: the note stays inside the bounds.
        let bounds = grid().bounds;
        assert!(
            notes[0].x0 >= bounds.x0 && notes[0].x1 <= bounds.x1,
            "{:?}",
            notes[0]
        );
    }

    #[test]
    fn the_tail_makes_room_for_the_arrow_and_each_mark() {
        let mut s = HeaderScope {
            scope: TableScope::Row,
            source: ScopeSource::Declared,
            place: HeaderPlace::FirstColumn,
            governs: Vec::new(),
            named_by: Vec::new(),
        };
        assert_eq!(tail_width(&s), TAIL_GAP + ARROW);
        s.source = ScopeSource::Conflict;
        assert_eq!(tail_width(&s), 2.0 * (TAIL_GAP + ARROW));
        s.named_by = vec![NodeId::Document];
        assert_eq!(tail_width(&s), 3.0 * (TAIL_GAP + ARROW));

        // Smoke: a conflicting header chip is wider by its tail and rung in amber.
        let mut header = mark(0, Rect::new(100.0, 100.0, 200.0, 120.0), Some("TH"));
        if let Some(chip) = header.chip.as_mut() {
            chip.scope = Some(s.clone());
        }
        let out = layout(
            &grid(),
            &Palette::DESIGN,
            &[Scene {
                marks: vec![header],
                ..Scene::default()
            }],
            None,
        );
        let chip = rects_of(&out, |a| matches!(a, Anchor::Chip(_)))[0];
        let (w, _) = measure("TH", Font::new(Face::Bold, LABEL_SIZE), 1.0);
        assert!((chip.width() - f64::from(w + 8.0 + tail_width(&s))).abs() < 1e-4);
        let ring = out.iter().any(|a| {
            matches!(&a.shape, Overlay::Frame { stroke: Some(line), fill: None, .. }
                if line.ink.rgba == Palette::DESIGN.warn && line.width == CONFLICT_RING)
        });
        assert!(ring);
    }

    #[test]
    fn a_mark_colours_its_box_and_chip_and_a_unit_scales_them() {
        let scene = Scene {
            marks: vec![Mark {
                color: [255, 0, 0, 255],
                ..mark(0, Rect::new(100.0, 100.0, 200.0, 120.0), Some("P"))
            }],
            path: Vec::new(),
        };
        let big = Grid { unit: 2.0, ..grid() };

        let out = layout(&big, &Palette::DESIGN, &[scene], None);

        let Overlay::Frame {
            stroke: Some(line), ..
        } = &out[0].shape
        else {
            panic!("{:?}", out[0]);
        };
        assert_eq!((line.ink.rgba, line.width), ([255, 0, 0, 255], 3.0));
        let chip = rects_of(&out, |a| matches!(a, Anchor::Chip(_)))[0];
        let (w, h) = measure("P", Font::new(Face::Bold, LABEL_SIZE * 2.0), 1.0);
        assert_eq!(
            (chip.width(), chip.height()),
            (f64::from(w + 16.0), f64::from(h + 6.0))
        );
        // Smoke: the chip's body takes the mark's colour too.
        assert!(out.iter().any(|a| matches!(&a.shape,
            Overlay::Frame { fill: Some(ink), radius, .. } if ink.rgba == [255, 0, 0, 255] && *radius == 8.0)));
    }

    #[test]
    fn a_legend_is_a_block_of_swatches_and_labels_in_the_corner() {
        let legend = Legend {
            rows: vec![
                ([255, 0, 0, 255], "removed".to_owned()),
                ([0, 200, 0, 255], "added".to_owned()),
            ],
        };

        let out = layout(&grid(), &Palette::DESIGN, &[], Some(&legend));

        let labels: Vec<&str> = out
            .iter()
            .filter_map(|a| match &a.shape {
                Overlay::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["removed", "added"]);
        let Overlay::Frame { rect: block, .. } = out[0].shape else {
            panic!("{:?}", out[0]);
        };
        assert_eq!((block.x0, block.y0), (8.0, 8.0));
        // Smoke: every row sits inside the block.
        assert!(
            rects_of(&out, |_| true)
                .iter()
                .all(|r| r.x1 <= block.x1 && r.y1 <= block.y1)
        );
    }
}
