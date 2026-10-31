//! What an object looks like: the facts a preview of it needs, by kind.
//!
//! [`Document::visual`] tells whether an indirect object can be pictured (an image, a font, a
//! Form `XObject`, a colour space, a shading, a pattern or a page) and gives the numbers drawing
//! it needs: an image's pixel size, the codes a font is shown with, the area of a form, a shading
//! or a pattern worth looking at, the colours to paint in a colour space. The core never
//! rasterises; `pdfxray-render` draws what is described here.

use std::collections::BTreeSet;

use pdfxray_raw::{Dict, Object};
use serde::Serialize;

use super::describe::{ColorSpaceInfo, Lens};
use super::format::subset_prefix;
use crate::capture::{Alignment, PaintKind};
use crate::content::char_code_of;
use crate::{CharCode, Document, FontFacts, Matrix, ObjRef, PageIx, Point, Rect};

/// Most glyphs a font preview lists.
pub const MAX_PREVIEW_GLYPHS: usize = 96;

/// The text of a font specimen.
pub const SPECIMEN: &str = "Aa Bb Cc 0123";

/// Most swatches of a colour space: an `/Indexed` palette has up to 256 entries.
const MAX_SWATCHES: usize = 256;

/// Most inks of a `DeviceN` space that get swatches.
const MAX_INKS: usize = 8;

/// Tiles of a tiling pattern shown across and down.
const TILES: f64 = 3.0;

/// Steps of a tint or grey ramp, both ends included.
const RAMP_STEPS: u32 = 8;

/// An object that can be pictured, and what drawing it needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Visual {
    /// The object.
    pub obj: ObjRef,
    /// What it is, with its facts.
    pub kind: VisualKind,
}

/// What kind of picture an object has.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum VisualKind {
    /// An image `XObject`: its pixels.
    Image(ImageVisual),
    /// A font: a specimen drawn with its own program.
    Font(Box<FontVisual>),
    /// A Form `XObject`: what it paints.
    Form(FormVisual),
    /// A colour space: swatches of colours in it.
    ColorSpace(ColorSpaceVisual),
    /// A shading: the shading over the area it covers.
    Shading(ShadingVisual),
    /// A pattern: a rectangle filled with it.
    Pattern(PatternVisual),
    /// A page dictionary: the page.
    Page(PageIx),
}

/// The facts of an image `XObject`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageVisual {
    /// `/Width` in pixels.
    pub width: u32,
    /// `/Height` in pixels.
    pub height: u32,
    /// `/BitsPerComponent`; 1 for an image mask. `None` when absent (JPEG 2000 data brings its
    /// own).
    pub bits_per_component: Option<u32>,
    /// The colour space in one line (`DeviceRGB`, `ICCBased (N=3)`); `None` for an image mask
    /// and when absent (JPEG 2000 data brings its own).
    pub color_space: Option<String>,
    /// A stencil mask: the samples say where the fill colour paints.
    pub image_mask: bool,
    /// The image has an `/SMask` (a soft mask of its own).
    pub soft_mask: bool,
}

/// Where a font's glyphs come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FontProgram {
    /// A font program is embedded: the preview draws with it.
    Embedded,
    /// A Type 3 font: its glyphs are content-stream procedures in the font dictionary.
    Procedures,
    /// No program is embedded: a viewer draws the text with a font of its own, so there is
    /// nothing of the file's to show.
    Missing,
}

/// The facts of a font and the codes its specimen shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FontVisual {
    /// What the inspector knows about the font.
    pub facts: FontFacts,
    /// Where its glyphs come from.
    pub program: FontProgram,
    /// [`SPECIMEN`] as codes, for a simple font whose program holds every character (embedded,
    /// not a subset, not Type 3); empty otherwise.
    pub sample: Vec<CharCode>,
    /// The codes to show one by one, in code order, at most [`MAX_PREVIEW_GLYPHS`]: the ones the
    /// document shows with this font, or when it shows none, the ones `/Widths` defines.
    pub glyphs: Vec<CharCode>,
    /// How many distinct codes the document shows with this font.
    pub used: usize,
}

/// The facts of a Form `XObject`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormVisual {
    /// `/BBox` after `/Matrix`: where the form paints in the space of whatever invokes it.
    pub area: Rect,
    /// A page that paints the form, when the form has no `/Resources` of its own: its names
    /// resolve in the resources of the page.
    pub borrows_from: Option<PageIx>,
}

/// The facts of a colour space and the colours its preview paints.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColorSpaceVisual {
    /// `ICCBased`, `Indexed`, `Separation`, `DeviceN`, `Lab`, `CalRGB`, `CalGray`.
    pub family: String,
    /// The space in one line: `Indexed (Base=DeviceRGB, HiVal=255)`.
    pub summary: String,
    /// Colour components.
    pub components: usize,
    /// The object is an ICC profile stream rather than a colour space array: it is used as
    /// `[/ICCBased <object>]`.
    pub icc_profile: bool,
    /// The colours to paint, each as the operands of `scn` in this space.
    pub swatches: Vec<Vec<f64>>,
    /// Swatches per row.
    pub columns: usize,
}

/// The facts of a shading.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShadingVisual {
    /// `/ShadingType`.
    pub shading_type: Option<i64>,
    /// The shading in one line: `axial (type 2), DeviceRGB`.
    pub summary: String,
    /// `/ColorSpace` in one line (`DeviceRGB`), when present.
    pub color_space: Option<String>,
    /// The area worth showing, in shading space: `/BBox`, the domain of a function-based
    /// shading, the axis or the circles. `None` for a mesh without `/BBox`, whose extent is only
    /// known by decoding the mesh.
    pub area: Option<Rect>,
    /// The axis of an axial shading (type 2), from where `t0` paints to where `t1` does: the
    /// direction a gradient bar of it runs.
    pub axis: Option<(Point, Point)>,
    /// `/Extend` of an axial or radial shading: whether it goes on before `t0` and after `t1`.
    pub extend: [bool; 2],
    /// `/FunctionType` of `/Function`, of the first one when it is an array.
    pub function_type: Option<i64>,
    /// The colours of an axial or radial shading along `t`: where its function starts, where a
    /// stitching function switches, and where it ends. Empty for the other types.
    pub stops: Vec<ShadingStop>,
}

/// A colour of a shading at one value of its parameter `t`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShadingStop {
    /// The value of `t`, within `/Domain`.
    pub t: f64,
    /// Where `t` lies from the start (0) to the end (1) of `/Domain`.
    pub position: f64,
    /// The function's output there, in the components of the shading's colour space. Empty when
    /// the function is not one that can be evaluated without sampling (only exponential and
    /// stitching functions are).
    pub color: Vec<f64>,
}

/// The facts of a pattern.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatternVisual {
    /// `/PatternType`: 1 tiling, 2 shading.
    pub pattern_type: Option<i64>,
    /// The pattern in one line: `Tiling, coloured`.
    pub summary: String,
    /// An uncoloured tiling pattern (`/PaintType 2`): the colour comes from the painter.
    pub uncoloured: bool,
    /// The area worth showing in the space of whatever paints the pattern: three tiles across
    /// and down, or the area of the shading, after `/Matrix`.
    pub area: Option<Rect>,
}

impl Document {
    /// What object `r` looks like, as of the newest revision. `None` when it is nothing a
    /// picture can show (a catalog, a number, a content stream, ...).
    pub fn visual(&self, r: ObjRef) -> Option<Visual> {
        let kind = self.visual_kind(r)?;
        Some(Visual { obj: r, kind })
    }

    fn visual_kind(&self, r: ObjRef) -> Option<VisualKind> {
        if let Some(page) = self.pages.iter().position(|p| p.obj == Some(r)) {
            return Some(VisualKind::Page(PageIx::try_from(page).ok()?));
        }
        let object = self.raw.object(r)?;
        let lens = Lens::new(&self.raw);
        match &object.value.value {
            Object::Stream(stream) => {
                let d = &stream.dict;
                if lens.int(d, "PatternType").is_some() {
                    return pattern(&lens, d).map(VisualKind::Pattern);
                }
                if lens.int(d, "ShadingType").is_some() {
                    return Some(VisualKind::Shading(shading(&lens, d)));
                }
                match lens.name(d, "Subtype").as_deref() {
                    Some("Image") => image(&lens, d).map(VisualKind::Image),
                    Some("Form") => self.form(&lens, d, r).map(VisualKind::Form),
                    None if is_icc_profile(&lens, d) => icc_profile(&lens, d).map(VisualKind::ColorSpace),
                    _ => None,
                }
            }
            Object::Dict(d) => {
                if lens.name(d, "Type").as_deref() == Some("Font") {
                    return self.font(r).map(|f| VisualKind::Font(Box::new(f)));
                }
                if lens.int(d, "PatternType").is_some() {
                    return pattern(&lens, d).map(VisualKind::Pattern);
                }
                lens.int(d, "ShadingType")
                    .map(|_| VisualKind::Shading(shading(&lens, d)))
            }
            value @ Object::Array(_) => color_space(&lens.color_space(value, 0)).map(VisualKind::ColorSpace),
            _ => None,
        }
    }

    fn font(&self, r: ObjRef) -> Option<FontVisual> {
        let facts = self.font_facts(None, None, Some(r))?;
        let program = if facts.subtype.as_deref() == Some("Type3") {
            FontProgram::Procedures
        } else if facts.embedded {
            FontProgram::Embedded
        } else {
            FontProgram::Missing
        };
        let whole = program == FontProgram::Embedded
            && facts.subtype.as_deref() != Some("Type0")
            && facts.base_font.as_deref().and_then(subset_prefix).is_none();
        let sample = if whole {
            SPECIMEN
                .bytes()
                .map(|b| char_code_of(u32::from(b), true, Some(&facts)))
                .collect()
        } else {
            Vec::new()
        };
        let used = self.codes_shown_with(r);
        let codes: Vec<u32> = if used.is_empty() {
            facts.defined_codes()
        } else {
            used.iter().copied().collect()
        };
        let glyphs = codes
            .into_iter()
            .take(MAX_PREVIEW_GLYPHS)
            .map(|code| char_code_of(code, true, Some(&facts)))
            .collect();
        Some(FontVisual {
            program,
            sample,
            glyphs,
            used: used.len(),
            facts,
        })
    }

    /// The character codes the pages show with the font object `font`.
    fn codes_shown_with(&self, font: ObjRef) -> BTreeSet<u32> {
        let mut codes = BTreeSet::new();
        for page in (0..self.page_count()).filter_map(|p| PageIx::try_from(p).ok()) {
            let Some(cap) = self.captured(page) else {
                continue;
            };
            let exact = matches!(cap.alignment, Alignment::Exact);
            for item in &cap.items {
                if let PaintKind::Glyph { char_code, .. } = item.kind
                    && item.gstate.text.font_ref == Some(font)
                    && (exact || char_code != 0)
                {
                    codes.insert(char_code);
                }
            }
        }
        codes
    }

    fn form(&self, lens: &Lens<'_>, d: &Dict, r: ObjRef) -> Option<FormVisual> {
        let bbox = rect_of(lens, d, "BBox")?;
        let area = transformed(bbox, &matrix_of(lens, d))?;
        let borrows_from = if d.get(b"Resources").is_some() {
            None
        } else {
            (0..self.page_count())
                .filter_map(|p| PageIx::try_from(p).ok())
                .find(|&p| {
                    self.captured(p)
                        .is_some_and(|cap| cap.xobjects.iter().any(|x| x.obj == Some(r)))
                })
        };
        Some(FormVisual { area, borrows_from })
    }
}

fn image(lens: &Lens<'_>, d: &Dict) -> Option<ImageVisual> {
    let dimension = |key: &str| {
        lens.int(d, key)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
    };
    let image_mask = lens.obj(d, "ImageMask") == Some(Object::Bool(true));
    Some(ImageVisual {
        width: dimension("Width")?,
        height: dimension("Height")?,
        bits_per_component: if image_mask {
            Some(1)
        } else {
            lens.int(d, "BitsPerComponent")
                .and_then(|v| u32::try_from(v).ok())
        },
        color_space: if image_mask {
            None
        } else {
            lens.obj(d, "ColorSpace")
                .map(|cs| lens.color_space(&cs, 0).summary())
        },
        image_mask,
        soft_mask: d.get(b"SMask").is_some(),
    })
}

/// An ICC profile stream: `/N` of 1, 3 or 4 and nothing that makes it another kind of stream.
fn is_icc_profile(lens: &Lens<'_>, d: &Dict) -> bool {
    matches!(lens.int(d, "N"), Some(1 | 3 | 4)) && d.get(b"Type").is_none() && d.get(b"Length1").is_none()
}

fn icc_profile(lens: &Lens<'_>, d: &Dict) -> Option<ColorSpaceVisual> {
    let n = lens.int(d, "N")?;
    let mut params = vec![("N", n.to_string())];
    if let Some(alt) = lens.obj(d, "Alternate") {
        params.push(("Alternate", lens.color_space(&alt, 1).summary()));
    }
    let info = ColorSpaceInfo {
        family: "ICCBased".to_owned(),
        components: usize::try_from(n).ok(),
        params,
    };
    let mut visual = color_space(&info)?;
    visual.icc_profile = true;
    Some(visual)
}

fn color_space(info: &ColorSpaceInfo) -> Option<ColorSpaceVisual> {
    let components = info.components?;
    let param = |key: &str| {
        info.params
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    };
    let ramp = || -> Vec<Vec<f64>> {
        (0..=RAMP_STEPS)
            .map(|i| vec![f64::from(i) / f64::from(RAMP_STEPS)])
            .collect()
    };
    let swatches: Vec<Vec<f64>> = match (info.family.as_str(), components) {
        ("Pattern", _) | (_, 0) => return None,
        ("Indexed" | "I", _) => {
            let hival = param("HiVal").and_then(|h| h.parse::<usize>().ok())?;
            (0..=hival.min(MAX_SWATCHES - 1))
                .map(|i| vec![f64::from(u32::try_from(i).unwrap_or(0))])
                .collect()
        }
        ("Separation", _) | (_, 1) => ramp(),
        ("DeviceN", n) => {
            let inks = n.min(MAX_INKS);
            [1.0, 0.5]
                .iter()
                .flat_map(|tint| {
                    (0..inks).map(move |ink| {
                        let mut values = vec![0.0; n];
                        values[ink] = *tint;
                        values
                    })
                })
                .collect()
        }
        ("Lab", _) => [
            [100.0, 0.0, 0.0],
            [70.0, 0.0, 0.0],
            [40.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [55.0, 70.0, 0.0],
            [55.0, -70.0, 0.0],
            [55.0, 0.0, 70.0],
            [55.0, 0.0, -70.0],
        ]
        .iter()
        .map(|c| c.to_vec())
        .collect(),
        (_, 3) => [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
        ]
        .iter()
        .map(|c| c.to_vec())
        .collect(),
        (_, 4) => [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 0.0, 0.0],
            [0.0, 1.0, 1.0, 0.0],
            [1.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.4],
        ]
        .iter()
        .map(|c| c.to_vec())
        .collect(),
        _ => return None,
    };
    let columns = match info.family.as_str() {
        "Indexed" | "I" => 16,
        "DeviceN" => components.min(MAX_INKS),
        _ => swatches.len(),
    }
    .min(swatches.len());
    Some(ColorSpaceVisual {
        family: info.family.clone(),
        summary: info.summary(),
        components,
        icc_profile: false,
        swatches,
        columns,
    })
}

fn shading(lens: &Lens<'_>, d: &Dict) -> ShadingVisual {
    let shading_type = lens.int(d, "ShadingType");
    let numbers = |key: &str| -> Option<Vec<f64>> {
        Some(lens.array(d, key)?.iter().filter_map(Object::as_f64).collect())
    };
    let axis = (shading_type == Some(2))
        .then(|| numbers("Coords"))
        .flatten()
        .and_then(|c| <[f64; 4]>::try_from(c).ok())
        .map(|[x0, y0, x1, y1]| (Point::new(x0, y0), Point::new(x1, y1)));
    let along_t = matches!(shading_type, Some(2 | 3));
    let extend = lens
        .array(d, "Extend")
        .filter(|_| along_t)
        .map_or([false; 2], |e| {
            [0, 1].map(|i| matches!(e.get(i).map(|v| lens.resolve(v)), Some(Object::Bool(true))))
        });
    let function = lens.obj(d, "Function");
    let function_type = function.as_ref().and_then(|f| match f {
        Object::Array(items) => items
            .first()
            .and_then(|first| function_dict(&lens.resolve(&first.value)))
            .and_then(|first| lens.int(&first, "FunctionType")),
        other => function_dict(other).and_then(|f| lens.int(&f, "FunctionType")),
    });
    let stops = match (&function, along_t) {
        (Some(function), true) => {
            let domain = numbers("Domain")
                .and_then(|v| <[f64; 2]>::try_from(v).ok())
                .unwrap_or([0.0, 1.0]);
            shading_stops(lens, function, domain)
        }
        _ => Vec::new(),
    };
    ShadingVisual {
        shading_type,
        summary: lens.shading_summary(d),
        color_space: lens
            .obj(d, "ColorSpace")
            .map(|cs| lens.color_space(&cs, 0).summary()),
        area: shading_area(lens, d),
        axis,
        extend,
        function_type,
        stops,
    }
}

/// Most levels of stitching functions followed when evaluating one.
const MAX_FUNCTION_DEPTH: u8 = 4;

/// The dictionary of a function: the object itself, or the dictionary of its stream.
fn function_dict(f: &Object) -> Option<Dict> {
    match f {
        Object::Dict(d) => Some(d.clone()),
        Object::Stream(s) => Some(s.dict.clone()),
        _ => None,
    }
}

/// The stops of `function` over `domain`: its two ends and, for a stitching function, its
/// `/Bounds` inside them.
fn shading_stops(lens: &Lens<'_>, function: &Object, [t0, t1]: [f64; 2]) -> Vec<ShadingStop> {
    let bounds: Vec<f64> = function_dict(function)
        .filter(|f| lens.int(f, "FunctionType") == Some(3))
        .and_then(|f| lens.array(&f, "Bounds"))
        .map(|b| b.iter().filter_map(Object::as_f64).collect())
        .unwrap_or_default();
    let (lo, hi) = (t0.min(t1), t0.max(t1));
    let inner = bounds.into_iter().filter(|b| *b > lo && *b < hi);
    let span = t1 - t0;
    std::iter::once(t0)
        .chain(inner)
        .chain(std::iter::once(t1))
        .map(|t| ShadingStop {
            t,
            position: if span == 0.0 { 0.0 } else { (t - t0) / span },
            color: evaluate(lens, function, t, 0).unwrap_or_default(),
        })
        .collect()
}

/// The output of `function` at `x`, for the kinds that need no sampling: exponential (type 2),
/// stitching (type 3) and an array of one-output functions.
fn evaluate(lens: &Lens<'_>, function: &Object, x: f64, depth: u8) -> Option<Vec<f64>> {
    if depth > MAX_FUNCTION_DEPTH {
        return None;
    }
    let numbers = |d: &Dict, key: &str| -> Option<Vec<f64>> {
        Some(lens.array(d, key)?.iter().filter_map(Object::as_f64).collect())
    };
    if let Object::Array(items) = function {
        let mut out = Vec::new();
        for item in items {
            out.extend(evaluate(lens, &lens.resolve(&item.value), x, depth + 1)?);
        }
        return Some(out);
    }
    let d = function_dict(function)?;
    let domain = numbers(&d, "Domain")
        .and_then(|v| <[f64; 2]>::try_from(v).ok())
        .unwrap_or([0.0, 1.0]);
    let x = x.clamp(domain[0].min(domain[1]), domain[0].max(domain[1]));
    match lens.int(&d, "FunctionType")? {
        2 => {
            let c0 = numbers(&d, "C0").unwrap_or_else(|| vec![0.0]);
            let c1 = numbers(&d, "C1").unwrap_or_else(|| vec![1.0]);
            let n = lens.obj(&d, "N").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let k = x.powf(n);
            (c0.len() == c1.len()).then(|| c0.iter().zip(&c1).map(|(a, b)| a + k * (b - a)).collect())
        }
        3 => {
            let functions = lens.array(&d, "Functions")?;
            let bounds = numbers(&d, "Bounds").unwrap_or_default();
            let encode = numbers(&d, "Encode").unwrap_or_default();
            let k = bounds.iter().take_while(|b| x >= **b).count();
            let lo = if k == 0 { domain[0] } else { bounds[k - 1] };
            let hi = bounds.get(k).copied().unwrap_or(domain[1]);
            let (e0, e1) = (
                encode.get(2 * k).copied().unwrap_or(0.0),
                encode.get(2 * k + 1).copied().unwrap_or(1.0),
            );
            let inner = if (hi - lo).abs() < f64::EPSILON {
                e0
            } else {
                e0 + (x - lo) * (e1 - e0) / (hi - lo)
            };
            evaluate(lens, &lens.resolve(functions.get(k)?), inner, depth + 1)
        }
        _ => None,
    }
}

/// The area of a shading worth showing, in shading space.
fn shading_area(lens: &Lens<'_>, d: &Dict) -> Option<Rect> {
    if let Some(bbox) = rect_of(lens, d, "BBox") {
        return Some(bbox);
    }
    let numbers = |key: &str| -> Option<Vec<f64>> {
        Some(lens.array(d, key)?.iter().filter_map(Object::as_f64).collect())
    };
    let area = match lens.int(d, "ShadingType")? {
        1 => {
            let domain = numbers("Domain").unwrap_or_else(|| vec![0.0, 1.0, 0.0, 1.0]);
            let [x0, x1, y0, y1] = <[f64; 4]>::try_from(domain).ok()?;
            transformed(Rect::new(x0, y0, x1, y1), &matrix_of(lens, d))?
        }
        2 => {
            let [x0, y0, x1, y1] = <[f64; 4]>::try_from(numbers("Coords")?).ok()?;
            // A box around the axis, at least half as deep as the axis is long.
            let half = (x1 - x0).hypot(y1 - y0) / 4.0;
            let r = Rect::new(x0, y0, x1, y1);
            let (cx, cy) = (f64::midpoint(r.x0, r.x1), f64::midpoint(r.y0, r.y1));
            Rect::new(
                r.x0.min(cx - half),
                r.y0.min(cy - half),
                r.x1.max(cx + half),
                r.y1.max(cy + half),
            )
        }
        3 => {
            let [x0, y0, r0, x1, y1, r1] = <[f64; 6]>::try_from(numbers("Coords")?).ok()?;
            let (r0, r1) = (r0.abs(), r1.abs());
            Rect::new(
                (x0 - r0).min(x1 - r1),
                (y0 - r0).min(y1 - r1),
                (x0 + r0).max(x1 + r1),
                (y0 + r0).max(y1 + r1),
            )
        }
        _ => return None,
    };
    usable(area)
}

fn pattern(lens: &Lens<'_>, d: &Dict) -> Option<PatternVisual> {
    let pattern_type = lens.int(d, "PatternType");
    let matrix = matrix_of(lens, d);
    let area = match pattern_type {
        Some(1) => {
            let bbox = rect_of(lens, d, "BBox")?;
            let step = |key: &str, fallback: f64| {
                lens.obj(d, key)
                    .and_then(|v| v.as_f64())
                    .map(f64::abs)
                    .filter(|s| *s > 0.0)
                    .unwrap_or(fallback)
            };
            let (x_step, y_step) = (step("XStep", bbox.width()), step("YStep", bbox.height()));
            let tiles = Rect::new(
                bbox.x0,
                bbox.y0,
                bbox.x0 + TILES * x_step,
                bbox.y0 + TILES * y_step,
            );
            transformed(tiles, &matrix)
        }
        Some(2) => {
            let shading = lens.obj(d, "Shading")?;
            let area = shading_area(lens, shading.as_dict()?);
            area.and_then(|a| transformed(a, &matrix))
        }
        _ => None,
    };
    Some(PatternVisual {
        pattern_type,
        summary: lens.pattern_summary(d),
        uncoloured: pattern_type == Some(1) && lens.int(d, "PaintType") == Some(2),
        area,
    })
}

/// The rectangle `key` holds, normalised; `None` unless four numbers span an area.
fn rect_of(lens: &Lens<'_>, d: &Dict, key: &str) -> Option<Rect> {
    let numbers: Vec<f64> = lens.array(d, key)?.iter().filter_map(Object::as_f64).collect();
    let [x0, y0, x1, y1] = <[f64; 4]>::try_from(numbers).ok()?;
    usable(Rect::new(x0, y0, x1, y1))
}

/// `/Matrix` of `d`, the identity when absent or malformed.
fn matrix_of(lens: &Lens<'_>, d: &Dict) -> Matrix {
    lens.array(d, "Matrix")
        .map(|m| m.iter().filter_map(Object::as_f64).collect::<Vec<f64>>())
        .and_then(|m| <[f64; 6]>::try_from(m).ok())
        .map_or(Matrix::IDENTITY, Matrix::from)
}

/// The box around `r` after `m`.
fn transformed(r: Rect, m: &Matrix) -> Option<Rect> {
    let corners =
        [(r.x0, r.y0), (r.x1, r.y0), (r.x0, r.y1), (r.x1, r.y1)].map(|(x, y)| m.apply(Point::new(x, y)));
    let xs = corners.iter().map(|p| p.x);
    let ys = corners.iter().map(|p| p.y);
    usable(Rect::new(
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ))
}

/// `r` when it is finite and spans an area.
fn usable(r: Rect) -> Option<Rect> {
    let finite = [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite());
    (finite && r.width() > 0.0 && r.height() > 0.0).then_some(r)
}
