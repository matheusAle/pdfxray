//! Facts about painted content: text, paths, shadings, images and Type 3 glyphs.
//!
//! # Conventions
//!
//! * **Page space** is the page's *default user space*: the y-up coordinate system that the
//!   content stream's operands live in once the current transformation matrix is applied. It is
//!   not rotated by `/Rotate`, not shifted by the crop box and not scaled by `/UserUnit`.
//! * **Operator index** (`op_index`) is the zero-based position of an operator among all operators
//!   of one content stream, in stream order. Operands do not count, and an inline image
//!   (`BI ... ID ... EI`) counts as exactly one operator. For content painted from a Form
//!   `XObject` the index counts within the form's own stream and `stream` names the form.
//! * **Bounding boxes** are `[llx, lly, urx, ury]` in page space. A path's box is the box of the
//!   path geometry itself; it is *not* grown by the line width, cap or join.
//! * **Rectangles clip**: every clip in these fixtures is an axis-aligned rectangle, so the clip is
//!   reported as one rectangle, the intersection of all active clips.

use serde::{Deserialize, Serialize};

use super::Rect;

/// A colour as the content stream set it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorFacts {
    /// Colour space name: `DeviceGray`, `DeviceRGB` or `DeviceCMYK`.
    pub space: String,
    /// The components, in the order the colour operator listed them.
    pub components: Vec<f64>,
}

impl ColorFacts {
    /// A `DeviceGray` colour.
    #[must_use]
    pub fn gray(g: f64) -> Self {
        Self {
            space: "DeviceGray".to_owned(),
            components: vec![g],
        }
    }

    /// A `DeviceRGB` colour.
    #[must_use]
    pub fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self {
            space: "DeviceRGB".to_owned(),
            components: vec![r, g, b],
        }
    }

    /// A `DeviceCMYK` colour.
    #[must_use]
    pub fn cmyk(c: f64, m: f64, y: f64, k: f64) -> Self {
        Self {
            space: "DeviceCMYK".to_owned(),
            components: vec![c, m, y, k],
        }
    }
}

/// The line parameters in force when a path was stroked, exactly as the operators set them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineFacts {
    /// `w` operand, in user space units at the time of painting.
    pub width: f64,
    /// `J` operand: 0 butt, 1 round, 2 projecting square.
    pub cap: u8,
    /// `j` operand: 0 miter, 1 round, 2 bevel.
    pub join: u8,
    /// `d` array; empty means solid.
    pub dash_array: Vec<f64>,
    /// `d` phase.
    pub dash_phase: f64,
}

impl LineFacts {
    /// The initial line parameters of a page: width 1, butt cap, miter join, solid.
    #[must_use]
    pub const fn initial_solid() -> Self {
        Self {
            width: 1.0,
            cap: 0,
            join: 0,
            dash_array: Vec::new(),
            dash_phase: 0.0,
        }
    }
}

/// One path-painting or shading operator and the graphics state it ran in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintOpFacts {
    /// Zero-based page index.
    pub page: u32,
    /// Operator index of the painting operator in the page content stream.
    pub op_index: u32,
    /// The painting operator: `f`, `f*`, `S`, `B`, `sh`, ...
    pub operator: String,
    /// Geometry box of the painted path, in page space, before clipping. For `sh` this is the
    /// current clip rectangle, which is what the shading fills.
    pub bbox: Rect,
    /// `bbox` intersected with the clip; `None` when nothing is visible.
    pub visible_bbox: Option<Rect>,
    /// Fill colour; present for operators that fill (`f`, `B`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<ColorFacts>,
    /// Stroke colour; present for operators that stroke (`S`, `B`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<ColorFacts>,
    /// Non-stroking alpha (`/ca`); present for operators that fill and for `sh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_alpha: Option<f64>,
    /// Stroking alpha (`/CA`); present for operators that stroke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_alpha: Option<f64>,
    /// Blend mode name, `Normal` unless an `ExtGState` set another.
    pub blend_mode: String,
    /// Intersection of all active rectangular clips, `None` when nothing clips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Rect>,
    /// Line parameters; present for operators that stroke.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<LineFacts>,
    /// Name of the shading resource painted by `sh`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shading: Option<String>,
}

/// One text-showing operator (`Tj`, `TJ`, `'` or `"`).
///
/// *Effective size* is the `Tf` size multiplied by the length of the text matrix' and CTM's
/// y axis, that is by the vertical scale of the combined transform; the fixtures only use
/// uniform scales so the number is unambiguous. *Origin* is where the first glyph's origin lands
/// in page space: the text-space point `(0, rise)` mapped through the text matrix and the CTM,
/// after a leading `TJ` adjustment (`-n/1000 * size * Tz/100` along x) has been applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextOpFacts {
    /// Zero-based page index.
    pub page: u32,
    /// Operator index of the showing operator in the page content stream.
    pub op_index: u32,
    /// `Tj`, `TJ`, `'` or `"`.
    pub operator: String,
    /// Shown characters, one char per byte (Latin-1); `TJ` numbers are left out.
    pub text: String,
    /// Resource name of the font, without the slash.
    pub font: String,
    /// `/BaseFont` of the font, absent for Type 3 fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_font: Option<String>,
    /// Operand of the last `Tf`.
    pub font_size: f64,
    /// Size in page space units, see the type documentation.
    pub effective_size: f64,
    /// `Tr` value: 0 fill, 1 stroke, 2 fill and stroke, 3 invisible.
    pub render_mode: u8,
    /// `Tc` in force.
    pub char_spacing: f64,
    /// `Tw` in force.
    pub word_spacing: f64,
    /// `Tz` in force, in percent.
    pub horizontal_scaling: f64,
    /// `Ts` in force.
    pub rise: f64,
    /// `TL` in force.
    pub leading: f64,
    /// Page-space origin of the first glyph, see the type documentation.
    pub origin: [f64; 2],
}

/// How an image got onto the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    /// An image `XObject` painted with `Do`.
    Xobject,
    /// An inline image (`BI ... ID ... EI`).
    Inline,
}

/// One painted image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageFacts {
    /// Zero-based page index the image ends up on.
    pub page: u32,
    /// Form `XObject` whose stream paints the image; `None` when the page stream paints it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<u32>,
    /// Operator index of the `Do` or `BI` operator within the painting stream.
    pub op_index: u32,
    /// `XObject` or inline.
    pub kind: ImageKind,
    /// Resource name used by `Do`; absent for inline images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Object number of the image `XObject`; absent for inline images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<u32>,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Bits per colour component.
    pub bits_per_component: u8,
    /// Colour space name.
    pub color_space: String,
    /// Page-space box of the image's unit square after the CTM (and form matrices).
    pub bbox: Rect,
}

/// Which painting operator a Type 3 glyph procedure used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphPaint {
    /// The procedure fills a path.
    Fill,
    /// The procedure strokes a path.
    Stroke,
}

/// One glyph of a Type 3 font as shown on the page.
///
/// Glyph space is mapped to page space by the font matrix, the font size, the text matrix and the
/// CTM; `origin` is the glyph origin in page space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Type3GlyphFacts {
    /// Zero-based page index.
    pub page: u32,
    /// Operator index of the showing operator.
    pub op_index: u32,
    /// Position of the glyph within the shown string.
    pub glyph_index: u32,
    /// Character code.
    pub code: u8,
    /// Glyph name from the font's `/Encoding /Differences`.
    pub glyph_name: String,
    /// Object number of the glyph procedure stream.
    pub proc_object: u32,
    /// Glyph origin in page space.
    pub origin: [f64; 2],
    /// The `d1` bounding box mapped to page space.
    pub declared_bbox: Rect,
    /// Geometry box of the path the procedure paints, in page space.
    pub path_bbox: Rect,
    /// Whether the procedure fills or strokes.
    pub paint: GlyphPaint,
}
