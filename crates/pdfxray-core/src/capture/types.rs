//! Public data model of a captured page.

use std::sync::Arc;

use pdfxray_raw::ObjRef;
use serde::{Serialize, Serializer};

use crate::{Point, Rect};

/// Serializes an `Arc<T>` as the `T` it points to (serde's `rc` feature is not enabled).
fn serialize_arc<T: Serialize, S: Serializer>(value: &Arc<T>, serializer: S) -> Result<S::Ok, S::Error> {
    value.as_ref().serialize(serializer)
}

/// Everything captured from one page.
///
/// `ops` holds every content-stream operator in **execution order** (operators inside Form
/// `XObjects` follow the `Do` that invoked them). `items` holds everything painted, in paint
/// order. All coordinates are in page space: PDF default user space, points, origin bottom-left,
/// before `/Rotate`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CapturedPage {
    /// Zero-based page index.
    pub page: u32,
    /// Every operator executed, in execution order.
    pub ops: Vec<OpRecord>,
    /// Every painted item, in paint order.
    pub items: Vec<PaintItem>,
    /// Every marked-content sequence, in order of its `BMC`/`BDC`.
    pub marked: Vec<MarkedSeq>,
    /// Every Form `XObject` invocation that was drawn, in execution order.
    pub xobjects: Vec<XObjectScope>,
    /// Whether the operator-to-paint attribution is complete for this page.
    pub alignment: Alignment,
    /// Observations that did not make the page degraded.
    pub notes: Vec<CaptureNote>,
}

/// One executed content-stream operator.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpRecord {
    /// Execution index, global per page (including operators inside Form `XObjects`).
    pub ix: u32,
    /// The operator token, e.g. `Tj` or `BDC`.
    pub operator: String,
    /// The operands in PDF syntax, e.g. `/F1`, `18`, `(Hello)`, `[(A) -20 (B)]`.
    pub operands: Vec<String>,
    /// The stream the operator was read from.
    pub stream: StreamRef,
    /// Index of the operator within the content it was parsed from: the page's concatenated
    /// `/Contents`, or the Form `XObject` stream.
    pub local_ix: u32,
    /// Byte range `(start, end)` of the instruction (operands and operator) inside the decoded
    /// bytes of `stream`. For page content this is relative to the individual `/Contents`
    /// stream named by [`StreamRef::PageContent`].
    pub span: Option<(usize, usize)>,
    /// Depth of the marked-content stack after this operator.
    pub mc_depth: u32,
    /// Innermost drawn Form `XObject` scope the operator belongs to, as an index into
    /// [`CapturedPage::xobjects`].
    pub xobject: Option<u32>,
    /// Index of the first item this operator painted, if it painted any. Items of one operator
    /// are contiguous in [`CapturedPage::items`].
    pub first_item: Option<u32>,
    /// Number of items this operator painted.
    pub item_count: u32,
    /// Graphics state after the operator executed.
    #[serde(serialize_with = "serialize_arc")]
    pub gstate: Arc<GState>,
}

/// Where a stream of operators comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum StreamRef {
    /// The `index`-th stream of the page's `/Contents`.
    PageContent {
        /// Position in the `/Contents` array (0 for a single stream).
        index: u32,
    },
    /// A Form `XObject` invoked with `Do`.
    FormXObject {
        /// Object reference of the form, when it is an indirect object.
        obj: Option<ObjRef>,
        /// Resource name used by the invoking `Do`.
        name: String,
        /// The stream containing the invoking `Do`.
        parent: Box<StreamRef>,
    },
}

/// Something that was painted, in paint order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintItem {
    /// Paint order, global per page.
    pub ix: u32,
    /// [`OpRecord::ix`] of the operator that produced it.
    pub op: u32,
    /// What was painted.
    pub kind: PaintKind,
    /// Bounding box in page space. Strokes include half the line width.
    pub bbox: Rect,
    /// Bounding box of the clip in effect when it was painted, if any.
    pub clip: Option<Rect>,
    /// Graphics state in effect when the operator started.
    #[serde(serialize_with = "serialize_arc")]
    pub gstate: Arc<GState>,
    /// Open marked-content sequences, outermost first, as indices into [`CapturedPage::marked`].
    /// Sequences opened in an invoking stream are included for items inside Form `XObjects`.
    pub mc_stack: Vec<u32>,
    /// Enclosing Form `XObject` scopes, outermost first, as indices into
    /// [`CapturedPage::xobjects`].
    pub xobject_stack: Vec<u32>,
}

/// The kind of a [`PaintItem`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PaintKind {
    /// A filled path.
    Fill,
    /// A stroked path.
    Stroke,
    /// A path that is filled and stroked in one operator (`B`, `b`, ...).
    FillStroke,
    /// A clipping path or text clip taking effect. Not painted, but part of the paint order.
    Clip,
    /// One glyph of a text-showing operator.
    Glyph {
        /// Position of the glyph within its operator.
        glyph_index: u32,
        /// Character code read from the string. `0` when the code could not be determined.
        char_code: u32,
        /// Unicode text of the glyph when the font provides it.
        unicode: Option<String>,
        /// Glyph origin on the baseline, page space.
        origin: Point,
        /// Length of the glyph's advance vector (font advance width scaled by font size and
        /// horizontal scaling) in page space; spacing parameters and `TJ` adjustments excluded.
        advance: f64,
    },
    /// An image, either an image `XObject` or an inline image.
    Image {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// True for `BI ... EI`.
        inline: bool,
    },
    /// A shading painted with `sh`; its box is the clip in effect (or the page box).
    Shading,
}

/// A marked-content sequence (`BMC`/`BDC` ... `EMC`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarkedSeq {
    /// Index in [`CapturedPage::marked`].
    pub ix: u32,
    /// The tag, e.g. `P` or `Artifact`.
    pub tag: String,
    /// `/MCID` from the property list.
    pub mcid: Option<i32>,
    /// Property list entries as `(key, value in PDF syntax)`, sorted by key.
    pub properties: Vec<(String, String)>,
    /// Name in `/Resources /Properties` when the list was given by name.
    pub property_resource: Option<String>,
    /// Decoded `/ActualText`.
    pub actual_text: Option<String>,
    /// Decoded `/Alt`.
    pub alt: Option<String>,
    /// Decoded `/Lang`.
    pub lang: Option<String>,
    /// Present when the tag is `Artifact`.
    pub artifact: Option<ArtifactInfo>,
    /// [`OpRecord::ix`] of the opening operator.
    pub begin_op: u32,
    /// [`OpRecord::ix`] of the closing `EMC`; `None` when the sequence was never closed.
    pub end_op: Option<u32>,
    /// The enclosing sequence, if any.
    pub parent: Option<u32>,
    /// The stream containing the opening operator.
    pub stream: StreamRef,
    /// Union of the bounding boxes of all items painted inside, including nested sequences.
    pub bbox: Option<Rect>,
}

/// Artifact property list (`/Artifact <</Type ... /Subtype ...>> BDC`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArtifactInfo {
    /// `/Type`: `Pagination`, `Layout`, `Page` or `Background`; `None` for a bare artifact.
    pub kind: Option<String>,
    /// `/Subtype`, e.g. `Header`, `Footer`, `Watermark`.
    pub subtype: Option<String>,
    /// `/BBox`.
    pub bbox: Option<Rect>,
    /// `/Attached` edges, e.g. `Top`, `Bottom`.
    pub attached: Vec<String>,
}

/// One drawn invocation of a Form `XObject`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct XObjectScope {
    /// Index in [`CapturedPage::xobjects`].
    pub ix: u32,
    /// Resource name used by `Do`.
    pub name: String,
    /// Object reference of the form.
    pub obj: Option<ObjRef>,
    /// Enclosing scope.
    pub parent: Option<u32>,
    /// [`OpRecord::ix`] of the invoking `Do`.
    pub op: u32,
    /// The form's `/Matrix` (identity when absent).
    pub matrix: [f64; 6],
    /// The form's `/BBox` in form space.
    pub bbox: Option<[f64; 4]>,
    /// CTM inside the form (`/Matrix` concatenated onto the CTM at the `Do`).
    pub ctm: [f64; 6],
    /// Bounding box of `/BBox` in page space.
    pub page_bbox: Option<Rect>,
    /// The stream holding the form's operators.
    pub stream: StreamRef,
    /// First operator executed inside the form.
    pub first_op: Option<u32>,
    /// Last operator executed inside the form.
    pub last_op: Option<u32>,
}

/// A color value with its color space.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColorInfo {
    /// Color space family: `DeviceGray`, `DeviceRGB`, `DeviceCMYK`, `ICCBased`, `Separation`,
    /// `Pattern`, ...
    pub space: String,
    /// Resource name when the space was selected with `cs`/`CS` by name.
    pub resource: Option<String>,
    /// Current color components.
    pub components: Vec<f64>,
    /// Pattern resource name (`scn`/`SCN` with a trailing name).
    pub pattern: Option<String>,
}

impl ColorInfo {
    pub(super) fn device(space: &str, components: &[f64]) -> Self {
        Self {
            space: space.to_owned(),
            resource: None,
            components: components.to_vec(),
            pattern: None,
        }
    }
}

/// The text state parameters (ISO 32000 9.3) plus the text matrices.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextState {
    /// Font resource name given to `Tf`.
    pub font: Option<String>,
    /// Object reference of the font dictionary, when indirect.
    pub font_ref: Option<ObjRef>,
    /// Font size (`Tf`).
    pub font_size: f64,
    /// Character spacing (`Tc`).
    pub char_spacing: f64,
    /// Word spacing (`Tw`).
    pub word_spacing: f64,
    /// Horizontal scaling in percent (`Tz`, 100 by default).
    pub horizontal_scaling: f64,
    /// Leading (`TL`).
    pub leading: f64,
    /// Text rise (`Ts`).
    pub rise: f64,
    /// Text rendering mode (`Tr`), 0 to 7.
    pub render_mode: u8,
    /// Text matrix.
    pub tm: [f64; 6],
    /// Text line matrix.
    pub tlm: [f64; 6],
}

/// The graphics state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GState {
    /// Current transformation matrix `[a b c d e f]` from the current space to page space.
    pub ctm: [f64; 6],
    /// Non-stroking color.
    pub fill: ColorInfo,
    /// Stroking color.
    pub stroke: ColorInfo,
    /// Line width (`w`).
    pub line_width: f64,
    /// Line cap (`J`).
    pub line_cap: u8,
    /// Line join (`j`).
    pub line_join: u8,
    /// Miter limit (`M`).
    pub miter_limit: f64,
    /// Dash array (`d`).
    pub dash_array: Vec<f64>,
    /// Dash phase (`d`).
    pub dash_phase: f64,
    /// Flatness (`i`).
    pub flatness: f64,
    /// Rendering intent (`ri`).
    pub rendering_intent: Option<String>,
    /// Text state.
    pub text: TextState,
    /// Non-stroking alpha (`ca`).
    pub alpha_fill: f64,
    /// Stroking alpha (`CA`).
    pub alpha_stroke: f64,
    /// Blend mode (`BM`), `Normal` by default.
    pub blend_mode: String,
    /// True while a soft mask (`/SMask` dictionary) is active.
    pub soft_mask: bool,
    /// Resource name of the last `gs`.
    pub ext_gstate: Option<String>,
    /// Bounding box in page space of the clip in effect.
    pub clip: Option<Rect>,
}

pub(super) const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

impl Default for TextState {
    fn default() -> Self {
        Self {
            font: None,
            font_ref: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            leading: 0.0,
            rise: 0.0,
            render_mode: 0,
            tm: IDENTITY,
            tlm: IDENTITY,
        }
    }
}

impl Default for GState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            fill: ColorInfo::device("DeviceGray", &[0.0]),
            stroke: ColorInfo::device("DeviceGray", &[0.0]),
            line_width: 1.0,
            line_cap: 0,
            line_join: 0,
            miter_limit: 10.0,
            dash_array: Vec::new(),
            dash_phase: 0.0,
            flatness: 0.0,
            rendering_intent: None,
            text: TextState::default(),
            alpha_fill: 1.0,
            alpha_stroke: 1.0,
            blend_mode: "Normal".to_owned(),
            soft_mask: false,
            ext_gstate: None,
            clip: None,
        }
    }
}

/// Whether the attribution of painted items to operators is complete.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Alignment {
    /// Every operator was executed by the engine and every device callback was attributed.
    Exact,
    /// Something prevented complete attribution; the operator list is still complete but some
    /// items may be missing or carry uncertain details.
    Degraded {
        /// What went wrong, for the first problem found.
        reason: String,
    },
}

/// An observation made while capturing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureNote {
    /// The operator the note is about.
    pub op: Option<u32>,
    /// Human-readable description.
    pub message: String,
}
