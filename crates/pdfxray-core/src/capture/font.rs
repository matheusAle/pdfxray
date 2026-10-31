//! What the walker needs to know about a font: vertical metrics for glyph boxes, widths of
//! Type 3 glyphs, and how strings split into character codes.

use hayro_interpret::hayro_cmap::{CMap, CMapName};
use hayro_interpret::{CMapResolverFn, InterpreterSettings};
use std::collections::HashMap;

use hayro_syntax::content::UntypedIter;
use hayro_syntax::object::{Array, Dict, Name, Object, Stream};
use kurbo::Affine;

/// Ascent/descent in glyph space (1000 units per em) used when a font gives none.
const DEFAULT_ASCENT: f64 = 800.0;
const DEFAULT_DESCENT: f64 = -200.0;

/// Facts about one font dictionary.
#[derive(Debug, Clone)]
pub(super) struct FontInfo {
    /// Top of the glyph box, 1/1000 em above the baseline.
    pub ascent: f64,
    /// Bottom of the glyph box, 1/1000 em (negative below the baseline).
    pub descent: f64,
    /// Widths of a Type 3 font's glyphs, in 1/1000 em.
    pub type3_widths: Option<Type3Widths>,
    /// The `d1` boxes of a Type 3 font's glyph procedures by character code, in 1/1000 em.
    type3_boxes: HashMap<u32, kurbo::Rect>,
    /// The `/Encoding` `CMap` of a composite font.
    cmap: Option<CMap>,
}

/// `/Widths` of a Type 3 font converted to 1/1000 em.
#[derive(Debug, Clone)]
pub(super) struct Type3Widths {
    first_char: i64,
    widths: Vec<f64>,
}

impl Type3Widths {
    /// Width of `code`, 1/1000 em.
    pub(super) fn width(&self, code: u32) -> Option<f64> {
        let idx = usize::try_from(i64::from(code) - self.first_char).ok()?;
        self.widths.get(idx).copied()
    }
}

impl FontInfo {
    /// The Helvetica the engine substitutes when a font cannot be resolved.
    pub(super) fn fallback() -> Self {
        let (ascent, descent) = standard_metrics("Helvetica").unwrap_or((DEFAULT_ASCENT, DEFAULT_DESCENT));
        Self {
            ascent,
            descent,
            type3_widths: None,
            type3_boxes: HashMap::new(),
            cmap: None,
        }
    }

    pub(super) fn from_dict(dict: &Dict<'_>, settings: &InterpreterSettings) -> Self {
        let subtype = dict
            .get::<Name<'_>>(b"Subtype")
            .map(|n| n.as_str().to_owned())
            .unwrap_or_default();
        let base_font = dict
            .get::<Name<'_>>(b"BaseFont")
            .map(|n| n.as_str().to_owned())
            .unwrap_or_default();

        let mut info = Self {
            ascent: DEFAULT_ASCENT,
            descent: DEFAULT_DESCENT,
            type3_widths: None,
            type3_boxes: HashMap::new(),
            cmap: None,
        };

        match subtype.as_str() {
            "Type3" => info.read_type3(dict),
            "Type0" => {
                info.cmap = dict
                    .get::<Object<'_>>(b"Encoding")
                    .and_then(|enc| read_encoding(&enc, &settings.cmap_resolver));
                let descendant = dict
                    .get::<Array<'_>>(b"DescendantFonts")
                    .and_then(|a| a.iter::<Dict<'_>>().next());
                let metrics = descendant.as_ref().and_then(|d| descriptor_metrics(d));
                info.apply_metrics(metrics, &base_font);
            }
            _ => info.apply_metrics(descriptor_metrics(dict), &base_font),
        }
        info
    }

    fn apply_metrics(&mut self, from_descriptor: Option<(f64, f64)>, base_font: &str) {
        if let Some((ascent, descent)) = from_descriptor.or_else(|| standard_metrics(base_font)) {
            self.ascent = ascent;
            self.descent = descent;
        }
    }

    /// The box a Type 3 glyph procedure declares with `d1`, for character code `code`.
    pub(super) fn type3_box(&self, code: u32) -> Option<kurbo::Rect> {
        self.type3_boxes.get(&code).copied()
    }

    fn read_type3(&mut self, dict: &Dict<'_>) {
        let matrix = dict
            .get::<[f64; 6]>(b"FontMatrix")
            .unwrap_or([0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
        if let Some(bbox) = dict.get::<[f64; 4]>(b"FontBBox") {
            let (y0, y1) = (bbox[1].min(bbox[3]), bbox[1].max(bbox[3]));
            if y1 > y0 {
                self.ascent = y1 * matrix[3] * 1000.0;
                self.descent = y0 * matrix[3] * 1000.0;
            }
        }
        let first_char = dict.get::<i64>(b"FirstChar").unwrap_or(0);
        if let Some(widths) = dict.get::<Array<'_>>(b"Widths") {
            let scale = matrix[0] * 1000.0;
            self.type3_widths = Some(Type3Widths {
                first_char,
                widths: widths.iter::<f64>().map(|w| w * scale).collect(),
            });
        }
        self.type3_boxes = type3_glyph_boxes(dict, matrix);
    }

    /// Splits `bytes` into `(character code, byte length)` pairs the way the engine does.
    pub(super) fn read_codes(&self, bytes: &[u8]) -> Vec<(u32, usize)> {
        let Some(cmap) = &self.cmap else {
            return bytes.iter().map(|&b| (u32::from(b), 1)).collect();
        };
        let mut codes = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let mut code = 0u32;
            let mut found = None;
            for n in 0..4.min(bytes.len() - offset) {
                code = (code << 8) | u32::from(bytes[offset + n]);
                let len = u8::try_from(n + 1).unwrap_or(4);
                if cmap.lookup_cid_code(code, len).is_some() {
                    found = Some((code, n + 1));
                    break;
                }
            }
            let (code, len) = found.unwrap_or((0, 1));
            codes.push((code, len));
            offset += len;
        }
        codes
    }
}

/// Ascent and descent from a font's `/FontDescriptor` (or its `/FontBBox`), 1/1000 em.
fn descriptor_metrics(font: &Dict<'_>) -> Option<(f64, f64)> {
    let descriptor = font.get::<Dict<'_>>(b"FontDescriptor")?;
    let ascent = descriptor.get::<f64>(b"Ascent").unwrap_or(0.0);
    let descent = descriptor.get::<f64>(b"Descent").unwrap_or(0.0);
    if ascent > 0.0 && descent != 0.0 {
        return Some((ascent, -descent.abs()));
    }
    let bbox = descriptor.get::<[f64; 4]>(b"FontBBox")?;
    let (y0, y1) = (bbox[1].min(bbox[3]), bbox[1].max(bbox[3]));
    (y1 > y0).then_some((y1, y0))
}

/// Ascender/descender of the standard 14 font families (Adobe core AFM values).
pub(crate) fn standard_metrics(base_font: &str) -> Option<(f64, f64)> {
    let name = base_font.split_once('+').map_or(base_font, |(_, rest)| rest);
    let lower = name.to_ascii_lowercase();
    let table: [(&str, (f64, f64)); 6] = [
        ("helvetica", (718.0, -207.0)),
        ("arial", (718.0, -207.0)),
        ("times", (683.0, -217.0)),
        ("courier", (629.0, -157.0)),
        ("symbol", (1010.0, -293.0)),
        ("zapfdingbats", (820.0, -143.0)),
    ];
    table
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map(|(_, m)| *m)
}

fn read_encoding(object: &Object<'_>, resolver: &CMapResolverFn) -> Option<CMap> {
    match object {
        Object::Name(n) => match CMapName::from_bytes(n) {
            CMapName::IdentityH => Some(CMap::identity_h()),
            CMapName::IdentityV => Some(CMap::identity_v()),
            other => {
                let data = resolver(other)?;
                let resolver = resolver.clone();
                CMap::parse(data, move |n| resolver(n))
            }
        },
        Object::Stream(s) => {
            let decoded = s.decoded().ok()?;
            let resolver = resolver.clone();
            CMap::parse(&decoded, move |n| resolver(n))
        }
        _ => None,
    }
}

/// The `d1` boxes of the glyph procedures named by the font's `/Encoding /Differences`,
/// converted from glyph space to 1/1000 em with the font matrix.
fn type3_glyph_boxes(font: &Dict<'_>, matrix: [f64; 6]) -> HashMap<u32, kurbo::Rect> {
    let mut boxes = HashMap::new();
    let (Some(procs), Some(differences)) = (
        font.get::<Dict<'_>>(b"CharProcs"),
        font.get::<Dict<'_>>(b"Encoding")
            .and_then(|e| e.get::<Array<'_>>(b"Differences")),
    ) else {
        return boxes;
    };
    let to_thousandths = Affine::scale(1000.0) * Affine::new(matrix);
    let mut code = 0i64;
    for item in differences.iter::<Object<'_>>() {
        match item {
            Object::Number(n) => code = n.as_i64(),
            Object::Name(name) => {
                let declared = procs
                    .get::<Stream<'_>>(name.as_ref())
                    .and_then(|s| declared_box(&s));
                if let (Some(rect), Ok(code)) = (declared, u32::try_from(code)) {
                    boxes.insert(code, to_thousandths.transform_rect_bbox(rect));
                }
                code += 1;
            }
            _ => {}
        }
    }
    boxes
}

/// The `llx lly urx ury` of the `d1` operator that opens a Type 3 glyph procedure.
fn declared_box(proc_stream: &Stream<'_>) -> Option<kurbo::Rect> {
    let data = proc_stream.decoded().ok()?;
    let mut iter = UntypedIter::new(&data);
    for _ in 0..4 {
        let inst = iter.next()?;
        let operator: &[u8] = inst.operator;
        if operator == b"d1" {
            let v: Vec<f64> = inst
                .operands()
                .filter_map(|o| match o {
                    Object::Number(n) => Some(n.as_f64()),
                    _ => None,
                })
                .collect();
            return match v.as_slice() {
                [_, _, x0, y0, x1, y1] => Some(kurbo::Rect::new(*x0, *y0, *x1, *y1)),
                _ => None,
            };
        }
    }
    None
}
