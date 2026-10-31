//! A tiny content-stream interpreter that re-derives text, paint, image and Type 3 facts from the
//! raw bytes. It shares no code with the generator: where the catalog states a hand-computed
//! constant, this module computes the same value from the operators.
//!
//! It only understands what the fixtures use: rectangular clips, straight path segments, the
//! operators listed in `Interp::op`, Type 1 fonts (no advance widths) and Type 3 fonts (with
//! advance widths).

use std::collections::BTreeMap;

use pdfxray_fixtures::{
    ColorFacts, GlyphPaint, ImageFacts, ImageKind, LineFacts, PaintOpFacts, Rect, TextOpFacts,
    Type3GlyphFacts,
};

use crate::common::{latin1, page_objects, ref_after, stream_data, text_of};
use crate::pdfobj::{Obj, parse_object};

// ---------------------------------------------------------------------------------------------
// Operators

/// An operand of a content-stream operator.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Operand {
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Arr(Vec<Operand>),
    Dict(Vec<(String, Operand)>),
}

impl Operand {
    fn num(&self) -> f64 {
        match self {
            Operand::Num(n) => *n,
            other => panic!("number expected, got {other:?}"),
        }
    }

    fn name(&self) -> &str {
        match self {
            Operand::Name(n) => n,
            other => panic!("name expected, got {other:?}"),
        }
    }

    fn dict_get(&self, key: &str) -> Option<&Operand> {
        match self {
            Operand::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// One operator with its operands. `index` counts operators of the stream from zero; an inline
/// image is the single operator `BI` whose only operand is its dictionary.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Op {
    pub(crate) index: u32,
    pub(crate) name: String,
    pub(crate) operands: Vec<Operand>,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    at: usize,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t' | 0 | 12)
}

fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%')
}

impl Lexer<'_> {
    fn skip_space(&mut self) {
        while self.at < self.bytes.len() && is_space(self.bytes[self.at]) {
            self.at += 1;
        }
    }

    fn word(&mut self) -> String {
        let start = self.at;
        while self.at < self.bytes.len() && !is_space(self.bytes[self.at]) && !is_delim(self.bytes[self.at]) {
            self.at += 1;
        }
        latin1(&self.bytes[start..self.at])
    }

    /// An operand, or `Err(operator)` when the next token is an operator keyword.
    fn operand(&mut self) -> Result<Operand, String> {
        self.skip_space();
        match self.bytes[self.at] {
            b'/' => {
                self.at += 1;
                Ok(Operand::Name(self.word()))
            }
            b'(' => {
                self.at += 1;
                let mut out = Vec::new();
                let mut depth = 1;
                while depth > 0 {
                    let b = self.bytes[self.at];
                    self.at += 1;
                    match b {
                        b'\\' => {
                            out.push(self.bytes[self.at]);
                            self.at += 1;
                        }
                        b'(' => {
                            depth += 1;
                            out.push(b);
                        }
                        b')' => {
                            depth -= 1;
                            if depth > 0 {
                                out.push(b);
                            }
                        }
                        other => out.push(other),
                    }
                }
                Ok(Operand::Str(out))
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    if self.bytes[self.at] == b']' {
                        self.at += 1;
                        return Ok(Operand::Arr(items));
                    }
                    items.push(self.operand().expect("operand inside array"));
                }
            }
            b'<' if self.bytes.get(self.at + 1) == Some(&b'<') => {
                self.at += 2;
                let mut entries = Vec::new();
                loop {
                    self.skip_space();
                    if self.bytes[self.at..].starts_with(b">>") {
                        self.at += 2;
                        return Ok(Operand::Dict(entries));
                    }
                    let Ok(Operand::Name(key)) = self.operand() else {
                        panic!("dictionary key expected")
                    };
                    entries.push((key, self.operand().expect("dictionary value")));
                }
            }
            _ => {
                let word = self.word();
                word.parse::<f64>().map(Operand::Num).map_err(|_| word)
            }
        }
    }
}

/// Splits a content stream into operators.
pub(crate) fn parse_ops(bytes: &[u8]) -> Vec<Op> {
    let mut lexer = Lexer { bytes, at: 0 };
    let mut ops = Vec::new();
    let mut operands = Vec::new();
    loop {
        lexer.skip_space();
        if lexer.at >= bytes.len() {
            break;
        }
        match lexer.operand() {
            Ok(operand) => operands.push(operand),
            Err(name) if name == "BI" => {
                // Inline image: key/value pairs up to ID, then data up to a whitespace-delimited EI.
                let mut entries = Vec::new();
                loop {
                    lexer.skip_space();
                    if bytes[lexer.at..].starts_with(b"ID") {
                        lexer.at += 2;
                        break;
                    }
                    let Ok(Operand::Name(key)) = lexer.operand() else {
                        panic!("inline image key expected")
                    };
                    entries.push((key, lexer.operand().expect("inline image value")));
                }
                let rest = &bytes[lexer.at..];
                let end = rest
                    .windows(4)
                    .position(|w| is_space(w[0]) && &w[1..3] == b"EI" && (is_space(w[3])))
                    .or_else(|| rest.ends_with(b"EI").then(|| rest.len() - 3))
                    .expect("EI terminates the inline image");
                lexer.at += end + 3;
                ops.push(Op {
                    index: u32::try_from(ops.len()).unwrap(),
                    name: "BI".to_owned(),
                    operands: vec![Operand::Dict(entries)],
                });
                operands.clear();
            }
            Err(name) => {
                ops.push(Op {
                    index: u32::try_from(ops.len()).unwrap(),
                    name,
                    operands: std::mem::take(&mut operands),
                });
            }
        }
    }
    ops
}

// ---------------------------------------------------------------------------------------------
// Geometry helpers

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` applied first, then `b`.
fn mul(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: Matrix, x: f64, y: f64) -> [f64; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

fn bounds(points: &[[f64; 2]]) -> Rect {
    let xs = points.iter().map(|p| p[0]);
    let ys = points.iter().map(|p| p[1]);
    [
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ]
}

fn transformed_rect(m: Matrix, r: Rect) -> Rect {
    bounds(&[
        apply(m, r[0], r[1]),
        apply(m, r[2], r[1]),
        apply(m, r[2], r[3]),
        apply(m, r[0], r[3]),
    ])
}

fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

/// Rounds away floating point noise so derived values compare equal to decimal constants.
fn round(x: f64) -> f64 {
    (x * 1e6).round() / 1e6 + 0.0
}

fn round_rect(r: Rect) -> Rect {
    r.map(round)
}

// ---------------------------------------------------------------------------------------------
// Interpreter

#[derive(Clone)]
struct TextState {
    font: Option<String>,
    size: f64,
    tc: f64,
    tw: f64,
    tz: f64,
    tl: f64,
    ts: f64,
    tr: u8,
}

#[derive(Clone)]
struct GState {
    ctm: Matrix,
    fill: ColorFacts,
    stroke: ColorFacts,
    fill_alpha: f64,
    stroke_alpha: f64,
    blend: String,
    clip: Option<Rect>,
    line: LineFacts,
    text: TextState,
}

/// Everything the interpreter observed.
#[derive(Default)]
pub(crate) struct Capture {
    pub(crate) text: Vec<TextOpFacts>,
    pub(crate) paints: Vec<PaintOpFacts>,
    pub(crate) images: Vec<ImageFacts>,
    pub(crate) glyphs: Vec<Type3GlyphFacts>,
}

pub(crate) struct Interp<'a> {
    pdf: &'a [u8],
    page: u32,
    pub(crate) capture: Capture,
}

/// A resource dictionary resolved to a direct object.
struct Resources(Obj);

impl Resources {
    /// `(object number if indirect, resolved object)` of `category`/`name`.
    fn lookup(&self, pdf: &[u8], category: &str, name: &str) -> Option<(Option<u32>, Obj)> {
        let category = resolve(pdf, self.0.get(category)?);
        let entry = category.get(name)?;
        Some((entry.reference(), resolve(pdf, entry)))
    }
}

fn resolve(pdf: &[u8], obj: &Obj) -> Obj {
    match obj {
        Obj::Ref(n) => parse_object(pdf, *n),
        other => other.clone(),
    }
}

impl<'a> Interp<'a> {
    pub(crate) fn new(pdf: &'a [u8], page: u32) -> Self {
        Self {
            pdf,
            page,
            capture: Capture::default(),
        }
    }

    /// Interprets the page `obj`'s content stream with its effective resources.
    pub(crate) fn run_page(&mut self, page_obj: u32) {
        let (node, _) = crate::common::inherited(self.pdf, page_obj, "/Resources").expect("resources");
        let holder = parse_object(self.pdf, node);
        let resources = Resources(resolve(self.pdf, holder.get("Resources").expect("/Resources")));
        let contents = ref_after(&text_of(self.pdf, page_obj), "/Contents ").expect("/Contents");
        let ops = parse_ops(&stream_data(self.pdf, contents));
        self.run(&ops, &resources, IDENTITY, None);
    }

    fn run(&mut self, ops: &[Op], resources: &Resources, ctm: Matrix, stream: Option<u32>) {
        let initial = GState {
            ctm,
            fill: ColorFacts::gray(0.0),
            stroke: ColorFacts::gray(0.0),
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            blend: "Normal".to_owned(),
            clip: None,
            line: LineFacts::initial_solid(),
            text: TextState {
                font: None,
                size: 0.0,
                tc: 0.0,
                tw: 0.0,
                tz: 100.0,
                tl: 0.0,
                ts: 0.0,
                tr: 0,
            },
        };
        let mut run = Run {
            gs: initial,
            stack: Vec::new(),
            tm: IDENTITY,
            tlm: IDENTITY,
            path: Vec::new(),
            pending_clip: false,
        };
        for op in ops {
            self.op(op, &mut run, resources, stream);
        }
    }

    fn op(&mut self, op: &Op, run: &mut Run, resources: &Resources, stream: Option<u32>) {
        let n = |i: usize| op.operands[i].num();
        match op.name.as_str() {
            "q" => run.stack.push(run.gs.clone()),
            "Q" => run.gs = run.stack.pop().expect("balanced q/Q"),
            "cm" => run.gs.ctm = mul([n(0), n(1), n(2), n(3), n(4), n(5)], run.gs.ctm),
            "w" => run.gs.line.width = n(0),
            "J" => run.gs.line.cap = n(0) as u8,
            "j" => run.gs.line.join = n(0) as u8,
            "d" => {
                let Operand::Arr(items) = &op.operands[0] else {
                    panic!("dash array")
                };
                run.gs.line.dash_array = items.iter().map(Operand::num).collect();
                run.gs.line.dash_phase = n(1);
            }
            "g" => run.gs.fill = ColorFacts::gray(n(0)),
            "G" => run.gs.stroke = ColorFacts::gray(n(0)),
            "rg" => run.gs.fill = ColorFacts::rgb(n(0), n(1), n(2)),
            "RG" => run.gs.stroke = ColorFacts::rgb(n(0), n(1), n(2)),
            "k" => run.gs.fill = ColorFacts::cmyk(n(0), n(1), n(2), n(3)),
            "K" => run.gs.stroke = ColorFacts::cmyk(n(0), n(1), n(2), n(3)),
            "gs" => {
                let (_, dict) = resources
                    .lookup(self.pdf, "ExtGState", op.operands[0].name())
                    .expect("ExtGState resource");
                if let Some(v) = dict.get("CA") {
                    run.gs.stroke_alpha = num_of(v);
                }
                if let Some(v) = dict.get("ca") {
                    run.gs.fill_alpha = num_of(v);
                }
                if let Some(bm) = dict.get("BM").and_then(Obj::name) {
                    bm.clone_into(&mut run.gs.blend);
                }
            }
            // Paths: only straight segments and rectangles, transformed when constructed.
            "m" | "l" => run.path.push(apply(run.gs.ctm, n(0), n(1))),
            "re" => {
                let (left, bottom, width, height) = (n(0), n(1), n(2), n(3));
                run.path.extend([
                    apply(run.gs.ctm, left, bottom),
                    apply(run.gs.ctm, left + width, bottom),
                    apply(run.gs.ctm, left + width, bottom + height),
                    apply(run.gs.ctm, left, bottom + height),
                ]);
            }
            "W" | "W*" => run.pending_clip = true,
            "n" | "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => {
                self.paint(op, run, stream);
            }
            "sh" => {
                let name = op.operands[0].name().to_owned();
                let clip = run.gs.clip.expect("sh runs inside a clip in the fixtures");
                if stream.is_none() {
                    self.capture.paints.push(PaintOpFacts {
                        page: self.page,
                        op_index: op.index,
                        operator: "sh".to_owned(),
                        bbox: round_rect(clip),
                        visible_bbox: Some(round_rect(clip)),
                        fill: None,
                        stroke: None,
                        fill_alpha: Some(round(run.gs.fill_alpha)),
                        stroke_alpha: None,
                        blend_mode: run.gs.blend.clone(),
                        clip: Some(round_rect(clip)),
                        line: None,
                        shading: Some(name),
                    });
                }
            }
            "BT" => {
                run.tm = IDENTITY;
                run.tlm = IDENTITY;
            }
            "Tf" => {
                run.gs.text.font = Some(op.operands[0].name().to_owned());
                run.gs.text.size = n(1);
            }
            "Tc" => run.gs.text.tc = n(0),
            "Tw" => run.gs.text.tw = n(0),
            "Tz" => run.gs.text.tz = n(0),
            "TL" => run.gs.text.tl = n(0),
            "Ts" => run.gs.text.ts = n(0),
            "Tr" => run.gs.text.tr = n(0) as u8,
            "Td" => {
                run.tlm = mul([1.0, 0.0, 0.0, 1.0, n(0), n(1)], run.tlm);
                run.tm = run.tlm;
            }
            "TD" => {
                run.gs.text.tl = -n(1);
                run.tlm = mul([1.0, 0.0, 0.0, 1.0, n(0), n(1)], run.tlm);
                run.tm = run.tlm;
            }
            "Tm" => {
                run.tm = [n(0), n(1), n(2), n(3), n(4), n(5)];
                run.tlm = run.tm;
            }
            "T*" => next_line(run),
            "Tj" => self.show(op, run, resources, &op.operands[..1], stream),
            "TJ" => {
                let Operand::Arr(items) = &op.operands[0] else {
                    panic!("TJ array")
                };
                self.show(op, run, resources, items, stream);
            }
            "'" => {
                next_line(run);
                self.show(op, run, resources, &op.operands[..1], stream);
            }
            "\"" => {
                run.gs.text.tw = n(0);
                run.gs.text.tc = n(1);
                next_line(run);
                self.show(op, run, resources, &op.operands[2..3], stream);
            }
            "Do" => self.do_xobject(op, run, resources, stream),
            "BI" => {
                let dict = &op.operands[0];
                let get = |long: &str, short: &str| dict.dict_get(short).or_else(|| dict.dict_get(long));
                let space = match get("ColorSpace", "CS").map(Operand::name) {
                    Some("RGB" | "DeviceRGB") => "DeviceRGB",
                    Some("G" | "DeviceGray") => "DeviceGray",
                    Some("CMYK" | "DeviceCMYK") => "DeviceCMYK",
                    other => panic!("inline colour space {other:?}"),
                };
                self.capture.images.push(ImageFacts {
                    page: self.page,
                    stream,
                    op_index: op.index,
                    kind: ImageKind::Inline,
                    name: None,
                    object: None,
                    width: get("Width", "W").expect("W").num() as u32,
                    height: get("Height", "H").expect("H").num() as u32,
                    bits_per_component: get("BitsPerComponent", "BPC").expect("BPC").num() as u8,
                    color_space: space.to_owned(),
                    bbox: round_rect(transformed_rect(run.gs.ctm, [0.0, 0.0, 1.0, 1.0])),
                });
            }
            // Marked content and everything else does not change the facts captured here.
            _ => {}
        }
    }

    fn paint(&mut self, op: &Op, run: &mut Run, stream: Option<u32>) {
        let name = op.name.as_str();
        let (fills, strokes) = match name {
            "f" | "F" | "f*" => (true, false),
            "S" | "s" => (false, true),
            "B" | "B*" | "b" | "b*" => (true, true),
            _ => (false, false),
        };
        if (fills || strokes) && stream.is_none() {
            let bbox = bounds(&run.path);
            let visible = run.gs.clip.map_or(Some(bbox), |c| intersect(bbox, c));
            self.capture.paints.push(PaintOpFacts {
                page: self.page,
                op_index: op.index,
                operator: name.to_owned(),
                bbox: round_rect(bbox),
                visible_bbox: visible.map(round_rect),
                fill: fills.then(|| run.gs.fill.clone()),
                stroke: strokes.then(|| run.gs.stroke.clone()),
                fill_alpha: fills.then(|| round(run.gs.fill_alpha)),
                stroke_alpha: strokes.then(|| round(run.gs.stroke_alpha)),
                blend_mode: run.gs.blend.clone(),
                clip: run.gs.clip.map(round_rect),
                line: strokes.then(|| run.gs.line.clone()),
                shading: None,
            });
        }
        if run.pending_clip {
            let bbox = bounds(&run.path);
            run.gs.clip = Some(
                run.gs
                    .clip
                    .map_or(bbox, |c| intersect(c, bbox).expect("clips overlap")),
            );
            run.pending_clip = false;
        }
        run.path.clear();
    }

    fn do_xobject(&mut self, op: &Op, run: &mut Run, resources: &Resources, stream: Option<u32>) {
        let name = op.operands[0].name();
        let (object, dict) = resources
            .lookup(self.pdf, "XObject", name)
            .unwrap_or_else(|| panic!("XObject {name}"));
        match dict.get("Subtype").and_then(Obj::name) {
            Some("Image") => {
                let space = dict.get("ColorSpace").and_then(Obj::name).expect("colour space");
                self.capture.images.push(ImageFacts {
                    page: self.page,
                    stream,
                    op_index: op.index,
                    kind: ImageKind::Xobject,
                    name: Some(name.to_owned()),
                    object,
                    width: dict.get("Width").and_then(Obj::int).expect("width") as u32,
                    height: dict.get("Height").and_then(Obj::int).expect("height") as u32,
                    bits_per_component: dict.get("BitsPerComponent").and_then(Obj::int).expect("bpc") as u8,
                    color_space: space.to_owned(),
                    bbox: round_rect(transformed_rect(run.gs.ctm, [0.0, 0.0, 1.0, 1.0])),
                });
            }
            Some("Form") => {
                let form = object.expect("forms are indirect");
                let matrix = dict.get("Matrix").map_or(IDENTITY.to_vec(), Obj::numbers);
                let m = [matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]];
                let own = dict.get("Resources").map(|r| Resources(resolve(self.pdf, r)));
                let ops = parse_ops(&stream_data(self.pdf, form));
                let inner = own.as_ref().unwrap_or(resources);
                self.run(&ops, inner, mul(m, run.gs.ctm), Some(form));
            }
            other => panic!("unsupported XObject subtype {other:?}"),
        }
    }

    /// A showing operator: `items` are the string (and TJ number) operands.
    fn show(
        &mut self,
        op: &Op,
        run: &mut Run,
        resources: &Resources,
        items: &[Operand],
        stream: Option<u32>,
    ) {
        let ts = run.gs.text.clone();
        let font_name = ts.font.clone().expect("Tf before showing text");
        let (_, font) = resources
            .lookup(self.pdf, "Font", &font_name)
            .unwrap_or_else(|| panic!("font {font_name}"));
        let th = ts.tz / 100.0;

        // A leading TJ adjustment shifts the first glyph along x.
        let shift = match items.first() {
            Some(Operand::Num(adjust)) => -adjust / 1000.0 * ts.size * th,
            _ => 0.0,
        };
        let trm_base = mul(run.tm, run.gs.ctm);
        let origin = apply(trm_base, shift, ts.ts);
        let text: Vec<u8> = items
            .iter()
            .filter_map(|i| match i {
                Operand::Str(s) => Some(s.clone()),
                _ => None,
            })
            .flatten()
            .collect();

        if stream.is_none() {
            self.capture.text.push(TextOpFacts {
                page: self.page,
                op_index: op.index,
                operator: op.name.clone(),
                text: latin1(&text),
                font: font_name.clone(),
                base_font: font.get("BaseFont").and_then(Obj::name).map(str::to_owned),
                font_size: round(ts.size),
                effective_size: round(ts.size * trm_base[1].hypot(trm_base[3])),
                render_mode: ts.tr,
                char_spacing: round(ts.tc),
                word_spacing: round(ts.tw),
                horizontal_scaling: round(ts.tz),
                rise: round(ts.ts),
                leading: round(ts.tl),
                origin: origin.map(round),
            });
        }

        if font.get("Subtype").and_then(Obj::name) == Some("Type3") {
            self.type3_glyphs(op, run, &font, &text);
        }
    }

    fn type3_glyphs(&mut self, op: &Op, run: &mut Run, font: &Obj, text: &[u8]) {
        let ts = run.gs.text.clone();
        let fm = font.get("FontMatrix").expect("FontMatrix").numbers();
        let fm = [fm[0], fm[1], fm[2], fm[3], fm[4], fm[5]];
        let first = font.get("FirstChar").and_then(Obj::int).expect("FirstChar");
        let widths = font.get("Widths").expect("Widths").numbers();
        let encoding = resolve(self.pdf, font.get("Encoding").expect("Encoding"));
        let differences = encoding.get("Differences").expect("Differences").array().to_vec();
        let char_procs = resolve(self.pdf, font.get("CharProcs").expect("CharProcs"));
        let th = ts.tz / 100.0;
        for (glyph_index, &code) in text.iter().enumerate() {
            // Differences: `start /name /name ...`
            let start = differences[0].int().expect("start code");
            let name = differences[1..][usize::try_from(i64::from(code) - start).unwrap()]
                .name()
                .expect("glyph name")
                .to_owned();
            let proc_object = char_procs
                .get(&name)
                .and_then(Obj::reference)
                .expect("glyph proc");
            // Glyph space -> text space (font matrix, size, scaling, rise) -> page space.
            let glyph_to_page = mul(
                mul(fm, [ts.size * th, 0.0, 0.0, ts.size, 0.0, ts.ts]),
                mul(run.tm, run.gs.ctm),
            );
            let proc_ops = parse_ops(&stream_data(self.pdf, proc_object));
            let d1 = proc_ops.iter().find(|o| o.name == "d1").expect("d1");
            let declared = transformed_rect(
                glyph_to_page,
                [
                    d1.operands[2].num(),
                    d1.operands[3].num(),
                    d1.operands[4].num(),
                    d1.operands[5].num(),
                ],
            );
            let mut inner = Run {
                gs: run.gs.clone(),
                stack: Vec::new(),
                tm: IDENTITY,
                tlm: IDENTITY,
                path: Vec::new(),
                pending_clip: false,
            };
            inner.gs.ctm = glyph_to_page;
            // Run the procedure's painting operators and read the path box back from the capture.
            let mut sub = Interp {
                pdf: self.pdf,
                page: self.page,
                capture: Capture::default(),
            };
            let resources = Resources(Obj::Dict(Vec::new()));
            for proc_op in &proc_ops {
                sub.op(proc_op, &mut inner, &resources, None);
            }
            let painted = sub.capture.paints.first().expect("glyph paints a path");
            self.capture.glyphs.push(Type3GlyphFacts {
                page: self.page,
                op_index: op.index,
                glyph_index: u32::try_from(glyph_index).unwrap(),
                code,
                glyph_name: name,
                proc_object,
                origin: apply(glyph_to_page, 0.0, 0.0).map(round),
                declared_bbox: round_rect(declared),
                path_bbox: painted.bbox,
                paint: if painted.operator.starts_with('S') {
                    GlyphPaint::Stroke
                } else {
                    GlyphPaint::Fill
                },
            });
            let width = widths[usize::try_from(i64::from(code) - first).unwrap()];
            let space = if code == b' ' { ts.tw } else { 0.0 };
            let tx = (width * fm[0] * ts.size + ts.tc + space) * th;
            run.tm = mul([1.0, 0.0, 0.0, 1.0, tx, 0.0], run.tm);
        }
    }
}

struct Run {
    gs: GState,
    stack: Vec<GState>,
    tm: Matrix,
    tlm: Matrix,
    path: Vec<[f64; 2]>,
    pending_clip: bool,
}

fn next_line(run: &mut Run) {
    run.tlm = mul([1.0, 0.0, 0.0, 1.0, 0.0, -run.gs.text.tl], run.tlm);
    run.tm = run.tlm;
}

fn num_of(obj: &Obj) -> f64 {
    match obj {
        Obj::Num(n) => *n,
        other => panic!("number expected, got {other:?}"),
    }
}

/// Facts the interpreter derives for every page of the file.
pub(crate) fn capture_pages(pdf: &[u8]) -> Capture {
    let mut all = Capture::default();
    for (index, page) in page_objects(pdf).into_iter().enumerate() {
        let mut interp = Interp::new(pdf, u32::try_from(index).unwrap());
        interp.run_page(page);
        all.text.extend(interp.capture.text);
        all.paints.extend(interp.capture.paints);
        all.images.extend(interp.capture.images);
        all.glyphs.extend(interp.capture.glyphs);
    }
    all
}

// ---------------------------------------------------------------------------------------------
// Marked content

/// What marked-content analysis of one content stream finds.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct MarkedContentReport {
    /// `[start, end)` runs of painting operators outside any marked-content sequence.
    pub(crate) untagged: Vec<[u32; 2]>,
    /// MCID to number of sequences that use it.
    pub(crate) mcid_uses: BTreeMap<i32, u32>,
    /// `(operator index, tag)` of sequences opened inside an artifact.
    pub(crate) nested_in_artifact: Vec<(u32, String)>,
}

fn is_painting(name: &str) -> bool {
    matches!(
        name,
        "f" | "F"
            | "f*"
            | "S"
            | "s"
            | "B"
            | "B*"
            | "b"
            | "b*"
            | "sh"
            | "Do"
            | "BI"
            | "Tj"
            | "TJ"
            | "'"
            | "\""
    )
}

pub(crate) fn analyze_marked_content(ops: &[Op]) -> MarkedContentReport {
    let mut report = MarkedContentReport::default();
    // Each open sequence: is it an artifact?
    let mut stack: Vec<bool> = Vec::new();
    let mut run: Option<[u32; 2]> = None;
    for op in ops {
        match op.name.as_str() {
            "BMC" | "BDC" => {
                if let Some(r) = run.take() {
                    report.untagged.push(r);
                }
                let tag = op.operands[0].name().to_owned();
                if stack.iter().any(|artifact| *artifact) {
                    report.nested_in_artifact.push((op.index, tag.clone()));
                }
                if let Some(Operand::Num(mcid)) = op.operands.get(1).and_then(|d| d.dict_get("MCID")) {
                    *report.mcid_uses.entry(*mcid as i32).or_default() += 1;
                }
                stack.push(tag == "Artifact");
            }
            "EMC" => {
                if let Some(r) = run.take() {
                    report.untagged.push(r);
                }
                stack.pop();
            }
            name if is_painting(name) && stack.is_empty() => {
                run = Some(run.map_or([op.index, op.index + 1], |r| [r[0], op.index + 1]));
            }
            _ => {}
        }
    }
    if let Some(r) = run {
        report.untagged.push(r);
    }
    report
}
