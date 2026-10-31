//! Facts about a font dictionary that the glyph inspector shows: names, embedding, encoding,
//! vertical metrics and the width table.

use pdfxray_raw::{Dict, Object};
use serde::Serialize;

use super::describe::Lens;
use super::format::subset_prefix;
use crate::capture::standard_metrics;
use crate::{Document, ObjRef, ResKind};

/// Everything the inspector knows about one font.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FontFacts {
    /// Resource name the content selected the font by (`F1`), when known.
    pub resource: Option<String>,
    /// The font dictionary's object, when it is an indirect object.
    pub obj: Option<ObjRef>,
    /// `/BaseFont` without the slash, subset tag included (`ABCDEF+SourceSerif4-Bold`).
    pub base_font: Option<String>,
    /// `/Subtype` of the font: `Type1`, `TrueType`, `Type0`, `Type3` ...
    pub subtype: Option<String>,
    /// `/Subtype` of the descendant of a Type 0 font: `CIDFontType0` or `CIDFontType2`.
    pub cid_subtype: Option<String>,
    /// True when a font program is embedded (never for Type 3, whose glyphs are procedures).
    pub embedded: bool,
    /// The descriptor key holding the program: `FontFile`, `FontFile2` or `FontFile3`.
    pub font_file: Option<String>,
    /// The font program stream.
    pub font_file_obj: Option<ObjRef>,
    /// The six-letter subset tag of the base font (`ABCDEF`).
    pub subset_prefix: Option<String>,
    /// How character codes are interpreted: `/WinAnsiEncoding`, `/Identity-H`, `custom (...)`.
    pub encoding: Option<String>,
    /// True when the font has a `/ToUnicode` map.
    pub has_to_unicode: bool,
    /// Top of the glyph box in em (from the descriptor, the Type 3 box or the standard 14 table).
    pub ascent_em: Option<f64>,
    /// Bottom of the glyph box in em (negative below the baseline).
    pub descent_em: Option<f64>,
    /// The font's bounding box in thousandths of an em.
    pub font_bbox: Option<[f64; 4]>,
    #[serde(skip)]
    widths: Widths,
}

/// The width tables of the font kinds, in thousandths of an em.
#[derive(Debug, Clone, PartialEq)]
enum Widths {
    /// No table: the font relies on built-in metrics.
    None,
    /// `/FirstChar` + `/Widths` of a simple font or a Type 3 font.
    Simple {
        first_char: i64,
        widths: Vec<f64>,
        missing: f64,
    },
    /// `/DW` + `/W` of a CID font whose codes are CIDs (Identity encodings).
    Cid {
        default: f64,
        ranges: Vec<(i64, i64, f64)>,
    },
}

impl FontFacts {
    /// The advance width of character `code` in thousandths of an em, from the font's own
    /// width table. `None` when the font has no table (standard 14 fonts) or its codes are not
    /// CIDs.
    pub fn width_units(&self, code: u32) -> Option<f64> {
        let code = i64::from(code);
        match &self.widths {
            Widths::None => None,
            Widths::Simple {
                first_char,
                widths,
                missing,
            } => {
                let at = usize::try_from(code.checked_sub(*first_char)?).ok()?;
                Some(widths.get(at).copied().unwrap_or(*missing))
            }
            Widths::Cid { default, ranges } => Some(
                ranges
                    .iter()
                    .find(|(first, last, _)| (*first..=*last).contains(&code))
                    .map_or(*default, |(.., w)| *w),
            ),
        }
    }

    /// The codes the `/Widths` table of a simple or Type 3 font gives a width above zero, in code
    /// order: the characters the font says it has. Empty for fonts without such a table.
    pub fn defined_codes(&self) -> Vec<u32> {
        let Widths::Simple {
            first_char, widths, ..
        } = &self.widths
        else {
            return Vec::new();
        };
        (*first_char..)
            .zip(widths)
            .filter(|(_, w)| **w > 0.0)
            .filter_map(|(code, _)| u32::try_from(code).ok())
            .collect()
    }

    /// Bytes per character code: 2 for a Type 0 font with an Identity encoding, 1 for simple
    /// fonts, `None` for a Type 0 font whose `CMap` decides.
    pub fn code_bytes(&self) -> Option<usize> {
        if self.subtype.as_deref() != Some("Type0") {
            return Some(1);
        }
        matches!(self.encoding.as_deref(), Some("/Identity-H" | "/Identity-V")).then_some(2)
    }
}

impl Document {
    /// The facts of the font selected as `resource` among the fonts of `owner`'s resources, or
    /// the font dictionary `font_ref` when the content knows its object.
    pub(crate) fn font_facts(
        &self,
        resource: Option<&str>,
        owner: Option<ObjRef>,
        font_ref: Option<ObjRef>,
    ) -> Option<FontFacts> {
        let by_name = || {
            let entry = self
                .scope_of_owner(owner?)?
                .entry(ResKind::Font, resource?)
                .map(|e| (e.target, e.value.clone()))?;
            Some(entry)
        };
        let (obj, value) = match font_ref {
            Some(r) => (Some(r), Object::Ref(r)),
            None => by_name()?,
        };
        let resolved = self.raw.resolve(&value).into_owned();
        let dict = resolved
            .as_dict()
            .or_else(|| resolved.as_stream().map(|s| &s.dict))?;
        Some(read_font(&Lens::new(&self.raw), dict, resource, obj))
    }
}

fn read_font(lens: &Lens<'_>, font: &Dict, resource: Option<&str>, obj: Option<ObjRef>) -> FontFacts {
    let subtype = lens.name(font, "Subtype");
    let base_font = lens.name(font, "BaseFont");
    let descendant = lens.descendant(font).map(|(_, d)| d);
    let type3 = subtype.as_deref() == Some("Type3");
    let program = if type3 { None } else { lens.program(font) };
    let carrier = descendant.as_ref().unwrap_or(font);
    let descriptor = lens.descriptor(font).map(|(_, d)| d);
    let metrics = metrics(lens, font, descriptor.as_ref(), base_font.as_deref(), type3);
    FontFacts {
        resource: resource.map(ToOwned::to_owned),
        obj,
        subset_prefix: base_font
            .as_deref()
            .and_then(subset_prefix)
            .map(ToOwned::to_owned),
        base_font,
        cid_subtype: descendant.as_ref().and_then(|d| lens.name(d, "Subtype")),
        embedded: program.is_some(),
        font_file: program.as_ref().map(|p| p.key.key().to_owned()),
        font_file_obj: program.and_then(|p| p.target),
        encoding: encoding(lens, font),
        has_to_unicode: lens
            .obj(font, "ToUnicode")
            .is_some_and(|v| v.as_stream().is_some()),
        ascent_em: metrics.map(|(ascent, _)| ascent / 1000.0),
        descent_em: metrics.map(|(_, descent)| descent / 1000.0),
        font_bbox: bbox(lens, descriptor.as_ref().unwrap_or(font)),
        widths: widths(lens, font, carrier, subtype.as_deref()),
        subtype,
    }
}

/// `(ascent, descent)` in thousandths of an em.
fn metrics(
    lens: &Lens<'_>,
    font: &Dict,
    descriptor: Option<&Dict>,
    base_font: Option<&str>,
    type3: bool,
) -> Option<(f64, f64)> {
    if type3 {
        let scale = type3_scale_y(lens, font);
        let b = bbox(lens, font)?;
        let (y0, y1) = (b[1].min(b[3]), b[1].max(b[3]));
        return (y1 > y0).then_some((y1 * scale, y0 * scale));
    }
    if let Some(d) = descriptor {
        let number = |key: &str| lens.obj(d, key).and_then(|v| v.as_f64());
        let (ascent, descent) = (number("Ascent").unwrap_or(0.0), number("Descent").unwrap_or(0.0));
        if ascent > 0.0 && descent != 0.0 {
            return Some((ascent, -descent.abs()));
        }
        if let Some([_, y0, _, y1]) = bbox(lens, d) {
            let (lo, hi) = (y0.min(y1), y0.max(y1));
            if hi > lo {
                return Some((hi, lo));
            }
        }
    }
    standard_metrics(base_font?)
}

/// The vertical scale of a Type 3 font matrix expressed in thousandths of an em.
fn type3_scale_y(lens: &Lens<'_>, font: &Dict) -> f64 {
    lens.array(font, "FontMatrix")
        .and_then(|m| m.get(3).and_then(Object::as_f64))
        .unwrap_or(0.001)
        * 1000.0
}

fn bbox(lens: &Lens<'_>, d: &Dict) -> Option<[f64; 4]> {
    let items = lens.array(d, "FontBBox")?;
    let numbers: Vec<f64> = items.iter().filter_map(Object::as_f64).collect();
    <[f64; 4]>::try_from(numbers).ok()
}

fn encoding(lens: &Lens<'_>, font: &Dict) -> Option<String> {
    Some(match lens.obj(font, "Encoding")? {
        Object::Name(n) => format!("/{}", n.to_string_lossy()),
        Object::Dict(d) => {
            let base = lens.name(&d, "BaseEncoding");
            let changes = lens
                .array(&d, "Differences")
                .map_or(0, |a| a.iter().filter(|o| matches!(o, Object::Name(_))).count());
            match base {
                Some(b) => format!("custom (/{b} + {changes} differences)"),
                None => format!("custom ({changes} differences)"),
            }
        }
        Object::Stream(_) => "embedded CMap".to_owned(),
        _ => return None,
    })
}

fn widths(lens: &Lens<'_>, font: &Dict, carrier: &Dict, subtype: Option<&str>) -> Widths {
    match subtype {
        Some("Type0") => cid_widths(lens, font, carrier),
        Some("Type3") => simple_widths(lens, font, None, type3_scale_x(lens, font)),
        _ => simple_widths(lens, font, lens.descriptor(font).map(|(_, d)| d), 1.0),
    }
}

fn type3_scale_x(lens: &Lens<'_>, font: &Dict) -> f64 {
    lens.array(font, "FontMatrix")
        .and_then(|m| m.first().and_then(Object::as_f64))
        .unwrap_or(0.001)
        * 1000.0
}

fn simple_widths(lens: &Lens<'_>, font: &Dict, descriptor: Option<Dict>, scale: f64) -> Widths {
    let Some(list) = lens.array(font, "Widths") else {
        return Widths::None;
    };
    let missing = descriptor
        .and_then(|d| lens.obj(&d, "MissingWidth"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    Widths::Simple {
        first_char: lens.int(font, "FirstChar").unwrap_or(0),
        widths: list.iter().map(|w| w.as_f64().unwrap_or(0.0) * scale).collect(),
        missing: missing * scale,
    }
}

/// `/DW` and `/W` of the descendant, usable when the codes are CIDs (an Identity encoding).
fn cid_widths(lens: &Lens<'_>, font: &Dict, descendant: &Dict) -> Widths {
    let identity = matches!(
        lens.obj(font, "Encoding"),
        Some(Object::Name(n)) if n == "Identity-H" || n == "Identity-V"
    );
    if !identity {
        return Widths::None;
    }
    let default = lens
        .obj(descendant, "DW")
        .and_then(|v| v.as_f64())
        .unwrap_or(1000.0);
    let mut ranges = Vec::new();
    if let Some(list) = lens.array(descendant, "W") {
        let mut items = list.into_iter().peekable();
        while let Some(first) = items.next().and_then(|o| o.as_int()) {
            match items.next() {
                Some(Object::Array(widths)) => {
                    for (offset, w) in (0_i64..).zip(&widths) {
                        let w = w.value.as_f64().unwrap_or(0.0);
                        ranges.push((first + offset, first + offset, w));
                    }
                }
                Some(last) => {
                    let (Some(last), Some(w)) = (last.as_int(), items.next().and_then(|o| o.as_f64())) else {
                        break;
                    };
                    ranges.push((first, last, w));
                }
                None => break,
            }
        }
    }
    Widths::Cid { default, ranges }
}
